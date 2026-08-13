use std::sync::TryLockError;

use crate::{
    DesktopApplication, DesktopApplicationError, DesktopEventKind, DesktopHostAction,
    DesktopHostResult, DesktopUpdateAction, DesktopUpdateResult, DesktopUpdateState,
};

const UPDATE_CHANNEL_FIELD: &str = "update.channel";
const UPDATE_VERSION_FIELD: &str = "update.version";

impl DesktopApplication {
    pub fn update_state(&self) -> Result<DesktopUpdateState, DesktopApplicationError> {
        self.inner
            .update_state
            .lock()
            .map(|state| state.clone())
            .map_err(|_| DesktopApplicationError::StateUnavailable("update"))
    }

    pub fn check_for_update(
        &self,
        channel: impl AsRef<str>,
    ) -> Result<DesktopUpdateState, DesktopApplicationError> {
        let channel = normalized_update_value(channel.as_ref(), UPDATE_CHANNEL_FIELD)?;
        let _operation = self.begin_update_operation()?;
        self.set_update_state(DesktopUpdateState::Checking)?;

        match self.inner.host.execute(
            &self.inner.host_context,
            DesktopHostAction::Update(DesktopUpdateAction::Check { channel }),
        ) {
            Ok(DesktopHostResult::Update(DesktopUpdateResult::UpToDate)) => {
                self.set_update_state(DesktopUpdateState::UpToDate)
            }
            Ok(DesktopHostResult::Update(DesktopUpdateResult::Available { version, notes })) => {
                self.set_update_state(DesktopUpdateState::Available { version, notes })
            }
            Ok(_) => self.fail_update(
                DesktopApplicationError::UnexpectedUpdateHostResult("check"),
                "更新检查没有返回可用结果。",
            ),
            Err(error) => {
                let message = error.message.clone();
                self.set_update_state(DesktopUpdateState::Failed { message })?;
                Err(error.into())
            }
        }
    }

    pub fn install_update(
        &self,
        version: impl AsRef<str>,
    ) -> Result<DesktopUpdateState, DesktopApplicationError> {
        let version = normalized_update_value(version.as_ref(), UPDATE_VERSION_FIELD)?;
        let _operation = self.begin_update_operation()?;
        let available = self.update_state()?;
        if !matches!(
            available,
            DesktopUpdateState::Available {
                version: ref available_version,
                ..
            } if available_version == &version
        ) {
            return Err(DesktopApplicationError::InvalidInput {
                field: UPDATE_VERSION_FIELD,
                message: "the requested version is not the currently available update".into(),
            });
        }

        self.set_update_state(DesktopUpdateState::Downloading {
            version: version.clone(),
            progress: None,
        })?;
        let mut last_progress = None;
        let mut progress_error = None;
        let mut publish_progress = |progress: Option<f32>| {
            if progress_error.is_some() {
                return;
            }
            let progress = progress
                .filter(|progress| progress.is_finite())
                .map(|progress| progress.clamp(0.0, 1.0));
            if progress.is_none() || progress == last_progress {
                return;
            }
            if progress
                .zip(last_progress)
                .is_some_and(|(next, last)| next < last)
            {
                return;
            }
            match self.set_update_state(DesktopUpdateState::Downloading {
                version: version.clone(),
                progress,
            }) {
                Ok(_) => last_progress = progress,
                Err(error) => progress_error = Some(error),
            }
        };
        let result = self.inner.host.execute_update(
            &self.inner.host_context,
            DesktopUpdateAction::Install {
                version: version.clone(),
            },
            &mut publish_progress,
        );
        if let Some(error) = progress_error {
            return Err(error);
        }
        match result {
            Ok(DesktopHostResult::Update(DesktopUpdateResult::InstallerLaunched {
                version: launched_version,
            })) if launched_version == version => {
                self.set_update_state(DesktopUpdateState::Installing {
                    version: version.clone(),
                })?;
                self.set_update_state(DesktopUpdateState::Restarting { version })
            }
            Ok(_) => self.fail_update(
                DesktopApplicationError::UnexpectedUpdateHostResult("install"),
                "更新安装器没有成功启动。",
            ),
            Err(error) => {
                let message = error.message.clone();
                self.set_update_state(DesktopUpdateState::Failed { message })?;
                Err(error.into())
            }
        }
    }

    fn begin_update_operation(
        &self,
    ) -> Result<std::sync::MutexGuard<'_, ()>, DesktopApplicationError> {
        match self.inner.update_operation.try_lock() {
            Ok(operation) => Ok(operation),
            Err(TryLockError::WouldBlock) => Err(DesktopApplicationError::UpdateBusy),
            Err(TryLockError::Poisoned(_)) => Err(DesktopApplicationError::StateUnavailable(
                "update operation",
            )),
        }
    }

    fn set_update_state(
        &self,
        state: DesktopUpdateState,
    ) -> Result<DesktopUpdateState, DesktopApplicationError> {
        *self
            .inner
            .update_state
            .lock()
            .map_err(|_| DesktopApplicationError::StateUnavailable("update"))? = state.clone();
        self.emit_event(DesktopEventKind::UpdateStateChanged {
            state: state.clone(),
        });
        Ok(state)
    }

    fn fail_update(
        &self,
        error: DesktopApplicationError,
        message: &'static str,
    ) -> Result<DesktopUpdateState, DesktopApplicationError> {
        self.set_update_state(DesktopUpdateState::Failed {
            message: message.to_owned(),
        })?;
        Err(error)
    }
}

fn normalized_update_value(
    value: &str,
    field: &'static str,
) -> Result<String, DesktopApplicationError> {
    let value = value.trim();
    if value.is_empty() || value.len() > 128 || value.chars().any(char::is_control) {
        return Err(DesktopApplicationError::InvalidInput {
            field,
            message: "must be a non-empty value without control characters".into(),
        });
    }
    Ok(value.to_owned())
}

#[cfg(test)]
mod tests {
    use std::collections::VecDeque;
    use std::sync::atomic::{AtomicU64, Ordering};
    use std::sync::{Arc, Mutex};

    use lilia_service::ServiceAuthority;

    use super::*;
    use crate::{
        DesktopApplicationConfig, DesktopHost, DesktopHostContext, DesktopHostError,
        DesktopUpdateResult,
    };

    static NEXT_UPDATE_TEST: AtomicU64 = AtomicU64::new(1);

    struct ScriptedUpdateHost {
        responses: Mutex<VecDeque<Result<DesktopHostResult, DesktopHostError>>>,
        install_progress: Vec<Option<f32>>,
    }

    impl DesktopHost for ScriptedUpdateHost {
        fn execute(
            &self,
            _context: &DesktopHostContext,
            action: DesktopHostAction,
        ) -> Result<DesktopHostResult, DesktopHostError> {
            assert!(matches!(action, DesktopHostAction::Update(_)));
            self.responses
                .lock()
                .unwrap()
                .pop_front()
                .expect("scripted update response")
        }

        fn execute_update(
            &self,
            context: &DesktopHostContext,
            action: DesktopUpdateAction,
            on_download_progress: &mut dyn FnMut(Option<f32>),
        ) -> Result<DesktopHostResult, DesktopHostError> {
            for progress in &self.install_progress {
                on_download_progress(*progress);
            }
            self.execute(context, DesktopHostAction::Update(action))
        }
    }

    fn application(
        responses: impl IntoIterator<Item = Result<DesktopHostResult, DesktopHostError>>,
    ) -> DesktopApplication {
        application_with_progress(responses, Vec::new())
    }

    fn application_with_progress(
        responses: impl IntoIterator<Item = Result<DesktopHostResult, DesktopHostError>>,
        install_progress: Vec<Option<f32>>,
    ) -> DesktopApplication {
        let id = NEXT_UPDATE_TEST.fetch_add(1, Ordering::Relaxed);
        let authority = ServiceAuthority::bootstrap_in_memory_named(
            format!("desktop-update-test-{id}"),
            format!("desktop-update-test-client-{id}"),
        )
        .unwrap();
        let config = DesktopApplicationConfig::new(
            "C:/lilia/native-preview-update-test",
            "liliacode.native-preview.update-test",
        )
        .unwrap();
        DesktopApplication::from_authority(
            config,
            authority,
            Arc::new(ScriptedUpdateHost {
                responses: Mutex::new(responses.into_iter().collect()),
                install_progress,
            }),
        )
        .unwrap()
    }

    #[test]
    fn update_state_machine_publishes_check_download_install_and_restart() {
        let app = application([
            Ok(DesktopHostResult::Update(DesktopUpdateResult::Available {
                version: "0.2.0".into(),
                notes: Some("Native release".into()),
            })),
            Ok(DesktopHostResult::Update(
                DesktopUpdateResult::InstallerLaunched {
                    version: "0.2.0".into(),
                },
            )),
        ]);
        let events = app.subscribe_events();

        assert_eq!(
            app.check_for_update("preview").unwrap(),
            DesktopUpdateState::Available {
                version: "0.2.0".into(),
                notes: Some("Native release".into()),
            }
        );
        assert_eq!(
            app.install_update("0.2.0").unwrap(),
            DesktopUpdateState::Restarting {
                version: "0.2.0".into()
            }
        );

        let states = (0..5)
            .map(|_| events.recv().unwrap().kind)
            .map(|event| match event {
                DesktopEventKind::UpdateStateChanged { state } => state,
                other => panic!("unexpected event: {other:?}"),
            })
            .collect::<Vec<_>>();
        assert_eq!(
            states,
            vec![
                DesktopUpdateState::Checking,
                DesktopUpdateState::Available {
                    version: "0.2.0".into(),
                    notes: Some("Native release".into()),
                },
                DesktopUpdateState::Downloading {
                    version: "0.2.0".into(),
                    progress: None,
                },
                DesktopUpdateState::Installing {
                    version: "0.2.0".into(),
                },
                DesktopUpdateState::Restarting {
                    version: "0.2.0".into(),
                },
            ]
        );
    }

    #[test]
    fn install_rejects_versions_that_were_not_checked() {
        let app = application([]);
        let error = app.install_update("9.9.9").unwrap_err();
        assert!(matches!(
            error,
            DesktopApplicationError::InvalidInput {
                field: UPDATE_VERSION_FIELD,
                ..
            }
        ));
        assert_eq!(app.update_state().unwrap(), DesktopUpdateState::Idle);
    }

    #[test]
    fn install_publishes_monotonic_host_download_progress() {
        let app = application_with_progress(
            [
                Ok(DesktopHostResult::Update(DesktopUpdateResult::Available {
                    version: "0.2.0".into(),
                    notes: None,
                })),
                Ok(DesktopHostResult::Update(
                    DesktopUpdateResult::InstallerLaunched {
                        version: "0.2.0".into(),
                    },
                )),
            ],
            vec![Some(0.25), Some(0.75), Some(0.5), Some(0.75)],
        );
        let events = app.subscribe_events();
        app.check_for_update("preview").unwrap();
        app.install_update("0.2.0").unwrap();

        let states = (0..7)
            .map(|_| events.recv().unwrap().kind)
            .filter_map(|event| match event {
                DesktopEventKind::UpdateStateChanged { state } => Some(state),
                _ => None,
            })
            .collect::<Vec<_>>();
        assert_eq!(
            states,
            vec![
                DesktopUpdateState::Checking,
                DesktopUpdateState::Available {
                    version: "0.2.0".into(),
                    notes: None,
                },
                DesktopUpdateState::Downloading {
                    version: "0.2.0".into(),
                    progress: None,
                },
                DesktopUpdateState::Downloading {
                    version: "0.2.0".into(),
                    progress: Some(0.25),
                },
                DesktopUpdateState::Downloading {
                    version: "0.2.0".into(),
                    progress: Some(0.75),
                },
                DesktopUpdateState::Installing {
                    version: "0.2.0".into(),
                },
                DesktopUpdateState::Restarting {
                    version: "0.2.0".into(),
                },
            ]
        );
    }
}
