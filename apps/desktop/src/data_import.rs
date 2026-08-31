use std::path::{Path, PathBuf};

use crate::application::{DesktopImportPlan, DesktopImportPlanStatus, DesktopImportReport};
use lilia_kernel::JobId;
use sha2::{Digest, Sha256};

pub const LEGACY_INSTANCE_IDENTITY: &str = "liliacode.native-preview";

pub fn legacy_instance_identity(home: &Path) -> Result<String, String> {
    let absolute = if home.is_absolute() {
        home.to_path_buf()
    } else {
        std::env::current_dir()
            .map_err(|error| format!("failed to resolve imported data home: {error}"))?
            .join(home)
    };
    let mut normalized = absolute.to_string_lossy().replace('\\', "/");
    if cfg!(windows) {
        normalized.make_ascii_lowercase();
    }
    let digest = Sha256::digest(normalized.as_bytes());
    Ok(format!("{LEGACY_INSTANCE_IDENTITY}.{digest:x}"))
}

#[derive(Debug, Default)]
pub struct NativeDataImportState {
    pub source_home: Option<PathBuf>,
    pub plan: Option<DesktopImportPlan>,
    pub report: Option<DesktopImportReport>,
    pub credentials_confirmed: bool,
    pub restart_required: bool,
    pub busy: bool,
    pub error: Option<String>,
    active_job: Option<JobId>,
}

impl NativeDataImportState {
    /// Clears the previous plan before the host starts preparing a new one.
    /// Preparation can fail before a job exists, which is why it is separate
    /// from [`Self::begin`].
    pub fn reset_for_plan(&mut self, source_home: PathBuf) {
        self.source_home = Some(source_home);
        self.plan = None;
        self.report = None;
        self.credentials_confirmed = false;
    }

    pub fn begin(&mut self, job_id: JobId) {
        self.active_job = Some(job_id);
        self.busy = true;
        self.error = None;
    }

    pub fn finish_plan(
        &mut self,
        job_id: JobId,
        result: Result<DesktopImportPlan, String>,
    ) -> bool {
        if self.active_job != Some(job_id) {
            return false;
        }
        self.finish_operation();
        match result {
            Ok(plan) => {
                self.plan = Some(plan);
                self.error = None;
            }
            Err(error) => {
                self.plan = None;
                self.error = Some(error);
            }
        }
        true
    }

    pub fn finish_execute(&mut self, job_id: JobId, report: DesktopImportReport) -> bool {
        if self.active_job != Some(job_id) {
            return false;
        }
        self.finish_operation();
        self.error = None;
        self.report = Some(report);
        true
    }

    pub fn set_restart_required(&mut self, required: bool) {
        self.restart_required = required;
    }

    pub fn set_error(&mut self, message: impl Into<String>) {
        self.error = Some(message.into());
    }

    /// Reports a step that failed before or instead of producing a result.
    pub fn fail(&mut self, message: impl Into<String>) {
        self.finish_operation();
        self.error = Some(message.into());
    }

    pub fn toggle_credentials(&mut self) {
        if self.has_credentials() && !self.busy && self.report.is_none() {
            self.credentials_confirmed = !self.credentials_confirmed;
        }
    }

    pub fn reset(&mut self) {
        if !self.busy && !self.restart_required {
            *self = Self::default();
        }
    }

    pub fn can_execute(&self) -> bool {
        !self.busy
            && self.report.is_none()
            && self
                .plan
                .as_ref()
                .is_some_and(|plan| plan.status != DesktopImportPlanStatus::Blocked)
    }

    pub fn has_credentials(&self) -> bool {
        self.plan
            .as_ref()
            .is_some_and(|plan| !plan.credential_entries.is_empty())
    }

    fn finish_operation(&mut self) {
        self.active_job = None;
        self.busy = false;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::application::{
        DesktopCredentialImportEntry, DesktopImportItemKind, DesktopImportPlanItem,
        DesktopImportPlanItemStatus, DesktopImportReportItem, DesktopImportReportItemStatus,
        DesktopImportReportStatus, DesktopLegacyConfigurationImport,
    };

    fn plan(status: DesktopImportPlanStatus, credentials: &[&str]) -> DesktopImportPlan {
        DesktopImportPlan {
            id: "plan-1".to_owned(),
            source_home: PathBuf::from("source"),
            source_instance_identity: LEGACY_INSTANCE_IDENTITY.to_owned(),
            target_home: PathBuf::from("target"),
            target_instance_identity: "lilia".to_owned(),
            credential_entries: credentials
                .iter()
                .map(|value| DesktopCredentialImportEntry {
                    source_service: LEGACY_INSTANCE_IDENTITY.to_owned(),
                    source_account: (*value).to_owned(),
                    target_key: (*value).to_owned(),
                })
                .collect(),
            legacy_configuration: DesktopLegacyConfigurationImport::default(),
            status,
            items: vec![DesktopImportPlanItem {
                kind: DesktopImportItemKind::Credentials,
                status: DesktopImportPlanItemStatus::RequiresCredentialConfirmation,
                files: Vec::new(),
                error: None,
            }],
        }
    }

    fn report() -> DesktopImportReport {
        DesktopImportReport {
            plan_id: "plan-1".to_owned(),
            source_home: PathBuf::from("source"),
            target_home: PathBuf::from("target"),
            status: DesktopImportReportStatus::Completed,
            items: vec![DesktopImportReportItem {
                kind: DesktopImportItemKind::Credentials,
                status: DesktopImportReportItemStatus::SkippedCredentialDenied,
                files: Vec::new(),
                error: None,
            }],
        }
    }

    #[test]
    fn stale_operation_results_cannot_replace_the_current_plan() {
        let mut state = NativeDataImportState::default();
        let first = JobId::new(1);
        let second = JobId::new(2);
        state.reset_for_plan(PathBuf::from("first"));
        state.begin(first);
        state.reset_for_plan(PathBuf::from("second"));
        state.begin(second);

        assert!(!state.finish_plan(first, Ok(plan(DesktopImportPlanStatus::Ready, &[]))));
        assert!(state.finish_plan(second, Ok(plan(DesktopImportPlanStatus::Ready, &[]))));
        assert_eq!(state.source_home, Some(PathBuf::from("second")));
        assert!(!state.busy);
    }

    #[test]
    fn credentials_require_a_manifest_and_a_separate_toggle() {
        let mut state = NativeDataImportState::default();
        let operation = JobId::new(1);
        state.reset_for_plan(PathBuf::from("source"));
        state.begin(operation);
        state.finish_plan(
            operation,
            Ok(plan(DesktopImportPlanStatus::Ready, &["agentkit.provider"])),
        );

        assert!(!state.credentials_confirmed);
        state.toggle_credentials();
        assert!(state.credentials_confirmed);
    }

    #[test]
    fn completed_report_prevents_accidental_repeat_until_reset() {
        let mut state = NativeDataImportState::default();
        let plan_operation = JobId::new(1);
        state.reset_for_plan(PathBuf::from("source"));
        state.begin(plan_operation);
        state.finish_plan(
            plan_operation,
            Ok(plan(DesktopImportPlanStatus::Ready, &[])),
        );
        assert!(state.can_execute());
        let execute_operation = JobId::new(2);
        state.begin(execute_operation);
        assert!(state.finish_execute(execute_operation, report()));
        assert!(!state.can_execute());

        state.reset();
        assert!(state.plan.is_none());
        assert!(state.report.is_none());
    }

    #[test]
    fn blocked_plan_never_exposes_execute() {
        let mut state = NativeDataImportState::default();
        let operation = JobId::new(1);
        state.reset_for_plan(PathBuf::from("source"));
        state.begin(operation);
        state.finish_plan(operation, Ok(plan(DesktopImportPlanStatus::Blocked, &[])));
        assert!(!state.can_execute());
    }
}
