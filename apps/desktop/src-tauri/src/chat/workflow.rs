use crate::chat::types::{ChatRuntimeCommand, ChatWorkflow};
use crate::chat::workflow_contract;

pub(crate) fn workflow_kind(workflow: Option<&ChatWorkflow>) -> Option<String> {
    Some(workflow?.kind().to_owned())
}

pub(crate) fn runtime_command_kind(command: Option<&ChatRuntimeCommand>) -> Option<String> {
    let kind = match command? {
        ChatRuntimeCommand::SessionFork { .. } => {
            workflow_contract::session_fork_runtime_command_type()
        }
        ChatRuntimeCommand::SessionManagement { .. } => {
            workflow_contract::session_management_runtime_command_type()
        }
        ChatRuntimeCommand::RuntimeSettings { .. } => {
            workflow_contract::runtime_settings_command_type()
        }
        ChatRuntimeCommand::RemoteEnvironment { .. } => {
            workflow_contract::remote_environment_command_type()
        }
        ChatRuntimeCommand::SandboxDiagnostics { .. } => {
            workflow_contract::sandbox_diagnostics_command_type()
        }
        ChatRuntimeCommand::ProcessSession { .. } => {
            workflow_contract::process_session_command_type()
        }
    };
    Some(kind.to_owned())
}

pub(crate) fn automation_run_id(workflow: Option<&ChatWorkflow>) -> Option<String> {
    workflow?.automation_run_id().map(str::to_owned)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeSet;

    #[test]
    fn workflow_kind_names_cover_automation() {
        let workflow = ChatWorkflow::Automation {
            automation_run_id: "run-1".to_string(),
        };

        assert_eq!(
            workflow_kind(Some(&workflow)).as_deref(),
            Some("automation")
        );
        assert_eq!(automation_run_id(Some(&workflow)).as_deref(), Some("run-1"));
        assert_eq!(workflow_kind(None), None);
    }

    #[test]
    fn runtime_command_kinds_are_declared_in_contract_manifests() {
        let runtime_commands = [
            ChatRuntimeCommand::SessionFork {
                exclude_turns: None,
                source_turn_id: None,
                mode: None,
            },
            ChatRuntimeCommand::SessionManagement {
                action: "rename".to_string(),
                session_id: None,
                title: None,
                tag: None,
                archived: None,
                limit: None,
                cursor: None,
                search_term: None,
                include_system_messages: None,
            },
            ChatRuntimeCommand::RuntimeSettings {
                action: "update".to_string(),
            },
            ChatRuntimeCommand::RemoteEnvironment {
                action: "diagnose".to_string(),
                environment_id: None,
                environment: None,
            },
            ChatRuntimeCommand::SandboxDiagnostics {
                include_details: Some(true),
            },
            ChatRuntimeCommand::ProcessSession {
                action: "spawn".to_string(),
                process_id: None,
                command: Some("npm test".to_string()),
                cwd: None,
                stdin: None,
                rows: None,
                cols: None,
                env: None,
                tty: Some(true),
                permission_profile: None,
            },
        ];
        let runtime_command_kinds = BTreeSet::from([
            workflow_contract::session_fork_runtime_command_type().to_string(),
            workflow_contract::session_management_runtime_command_type().to_string(),
            workflow_contract::runtime_settings_command_type().to_string(),
            workflow_contract::remote_environment_command_type().to_string(),
            workflow_contract::sandbox_diagnostics_command_type().to_string(),
            workflow_contract::process_session_command_type().to_string(),
        ]);

        for command in &runtime_commands {
            let kind = runtime_command_kind(Some(command)).unwrap();
            let serialized = serde_json::to_value(command).unwrap();
            assert!(
                runtime_command_kinds.contains(&kind),
                "{kind} runtime command must be declared in contract manifests"
            );
            assert_eq!(
                serialized.get("type").and_then(serde_json::Value::as_str),
                Some(kind.as_str()),
                "{kind} runtime command serde type must match contract manifests"
            );
        }
        assert_eq!(runtime_command_kinds.len(), runtime_commands.len());
    }
}
