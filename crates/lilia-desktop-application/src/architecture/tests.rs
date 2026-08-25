use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;

use lilia_service::ServiceAuthority;
use mutsuki_agent_contracts::{AgentPermissionMode, InteractionKind, InteractionRequest};
use serde_json::json;

use super::*;
use crate::{
    DesktopApplication, DesktopApplicationConfig, DesktopEventKind, DesktopHost, DesktopHostAction,
    DesktopHostContext, DesktopHostError, DesktopHostResult, DesktopProjectCreate,
    DesktopTaskCreate,
};

static NEXT_ID: AtomicU64 = AtomicU64::new(1);

struct NoopHost;

impl DesktopHost for NoopHost {
    fn execute(
        &self,
        _context: &DesktopHostContext,
        _action: DesktopHostAction,
    ) -> Result<DesktopHostResult, DesktopHostError> {
        Ok(DesktopHostResult::Completed)
    }
}

fn application() -> DesktopApplication {
    let id = NEXT_ID.fetch_add(1, Ordering::Relaxed);
    let authority = ServiceAuthority::bootstrap_in_memory_named(
        format!("test:architecture-application:{id}"),
        format!("architecture-application-test:{id}"),
    )
    .unwrap();
    DesktopApplication::from_authority(
        DesktopApplicationConfig::new(
            "C:/lilia/architecture-application",
            format!("liliacode.architecture-test.{id}"),
        )
        .unwrap(),
        authority,
        Arc::new(NoopHost),
    )
    .unwrap()
}

#[test]
fn application_validates_product_ownership_and_emits_snapshot_invalidation() {
    let application = application();
    let project = application
        .create_project(DesktopProjectCreate::new("Native"))
        .unwrap();
    let task = application
        .create_task(DesktopTaskCreate::new(
            Some(project.id.clone()),
            "Architecture",
        ))
        .unwrap();
    let events = application.subscribe_events();
    let result = application
        .apply_project_architecture(ProjectArchitectureApplyInput {
            project_id: project.id.as_str().to_owned(),
            task_id: task.id.as_str().to_owned(),
            turn_id: Some("turn-1".to_owned()),
            backend: ArchitectureBackend::NativeAgentkit,
            permission: ArchitecturePermission::Ask,
            reason: "Add UI".to_owned(),
            changes: vec![ProjectArchitectureChange::UpsertNode {
                node: ProjectArchitectureNode {
                    id: "ui".to_owned(),
                    label: "UI".to_owned(),
                    node_type: "module".to_owned(),
                    summary: String::new(),
                    paths: Vec::new(),
                    tags: Vec::new(),
                },
            }],
            request_id: Some("request-1".to_owned()),
            expected_version: Some(0),
        })
        .unwrap();
    assert_eq!(result.graph.version, 1);
    assert!(matches!(
        events.recv().unwrap().kind,
        DesktopEventKind::ArchitectureChanged {
            project_id,
            version: 1
        } if project_id == project.id
    ));
    assert_eq!(
        application
            .project_architecture_changes(&project.id, 20)
            .unwrap()
            .len(),
        1
    );

    let other_project = application
        .create_project(DesktopProjectCreate::new("Other"))
        .unwrap();
    let error = application
        .apply_project_architecture(ProjectArchitectureApplyInput {
            project_id: other_project.id.as_str().to_owned(),
            task_id: task.id.as_str().to_owned(),
            turn_id: None,
            backend: ArchitectureBackend::NativeAgentkit,
            permission: ArchitecturePermission::Full,
            reason: "invalid".to_owned(),
            changes: vec![ProjectArchitectureChange::SetSummary {
                summary: "must not persist".to_owned(),
            }],
            request_id: Some("request-invalid".to_owned()),
            expected_version: Some(0),
        })
        .unwrap_err();
    assert!(error.to_string().contains("must belong"));
    assert_eq!(
        application
            .project_architecture(&other_project.id)
            .unwrap()
            .version,
        0
    );
}

#[test]
fn architecture_interaction_applies_authoritative_scope_and_resumes_the_same_turn() {
    let application = application();
    let project = application
        .create_project(DesktopProjectCreate::new("Native"))
        .unwrap();
    let task = application
        .create_task(DesktopTaskCreate::new(
            Some(project.id.clone()),
            "Architecture approval",
        ))
        .unwrap();
    let runtime = application.authority().shared_runtime();
    runtime
        .inner()
        .seed_debug_interaction(
            &task.id,
            "architecture-session",
            "architecture-turn",
            InteractionRequest {
                session_id: "architecture-session".to_owned(),
                turn_id: "architecture-turn".to_owned(),
                version: 1,
                interaction_id: "architecture-request".to_owned(),
                kind: InteractionKind::Custom,
                source_tool: Some("update_project_architecture".to_owned()),
                permission_mode: AgentPermissionMode::Ask,
                prompt: "Add the application service boundary".to_owned(),
                options: json!({
                    "reason": "Keep Native UI independent from persistence",
                    "changes": [{
                        "type": "upsert_node",
                        "node": {
                            "id": "desktop-application",
                            "label": "DesktopApplication",
                            "type": "service",
                            "summary": "Typed desktop application boundary",
                            "paths": ["crates/lilia-desktop-application"],
                            "tags": ["native"]
                        }
                    }]
                }),
                context: Some(json!({
                    "productTaskId": task.id.as_str(),
                    "productProjectId": project.id.as_str(),
                    "projectArchitectureVersion": 0
                })),
                details: None,
            },
        )
        .unwrap();
    assert_eq!(
        application
            .restore_task_runtime_from_projection(&task.id)
            .unwrap()
            .phase,
        "waiting_interaction"
    );

    let response = application
        .respond_task_architecture_interaction(
            &task.id,
            "architecture-request",
            crate::DesktopArchitectureInteractionDecision::Allow,
        )
        .unwrap();

    assert_eq!(
        response.decision,
        crate::DesktopArchitectureInteractionDecision::Allow
    );
    assert_eq!(response.graph.as_ref().map(|graph| graph.version), Some(1));
    assert_eq!(response.event.id.as_deref(), Some("architecture-request"));
    assert_eq!(response.event.permission, ArchitecturePermission::Ask);
    let graph = application.project_architecture(&project.id).unwrap();
    assert_eq!(graph.version, 1);
    assert_eq!(graph.nodes[0].id, "desktop-application");
}
