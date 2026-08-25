use lilia_contracts::{
    ProductConversationStatus, ProductTaskPriority, ProductTaskStatus, ProjectArchiveState,
};
use serde::Serialize;
use serde_json::Value as JsonValue;
use sha2::{Digest, Sha256};

use crate::application::{
    DesktopApplication, DesktopApplicationError, DesktopGoalStatus, DesktopTodoGuideStatus,
    DesktopTodoPriority, DesktopTodoSource, MemoryScope, MilestoneStatus, ProjectQuery, TaskQuery,
};

const SNAPSHOT_SCHEMA_VERSION: u32 = 9;

#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DesktopEquivalenceSnapshot {
    pub schema_version: u32,
    pub fixture_id: String,
    pub projects: Vec<DesktopEquivalenceProjectFact>,
    pub tasks: Vec<DesktopEquivalenceTaskFact>,
    pub conversations: Vec<DesktopEquivalenceConversationFact>,
    pub timeline: Vec<DesktopEquivalenceTimelineFact>,
    pub composers: Vec<DesktopEquivalenceComposerFact>,
    pub goals: Vec<DesktopEquivalenceGoalFact>,
    pub todos: Vec<DesktopEquivalenceTodoFact>,
    pub roadmap: Vec<DesktopEquivalenceMilestoneFact>,
    pub memories: Vec<DesktopEquivalenceMemoryFact>,
    pub memory_settings: DesktopEquivalenceMemorySettingsFact,
    pub conversation_suggestions: DesktopEquivalenceConversationSuggestionSettingsFact,
    pub automations: Vec<DesktopEquivalenceAutomationFact>,
    pub skills_registry_revision: u64,
    pub skills: Vec<DesktopEquivalenceSkillFact>,
    pub plugins_registry_revision: u64,
    pub plugins: Vec<DesktopEquivalencePluginFact>,
    pub hook_sources: Vec<DesktopEquivalenceHookSourceFact>,
    pub mcp_registry_revision: u64,
    pub mcp_servers: Vec<DesktopEquivalenceMcpServerFact>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DesktopEquivalenceProjectFact {
    pub id: String,
    pub name: String,
    pub has_workspace_path: bool,
    pub pinned: bool,
    pub sort_order: i64,
    pub archive: ProjectArchiveState,
    pub revision: u64,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DesktopEquivalenceTaskFact {
    pub id: String,
    pub project_id: Option<String>,
    pub parent_id: Option<String>,
    pub title: String,
    pub description: Option<String>,
    pub status: ProductTaskStatus,
    pub priority: ProductTaskPriority,
    pub pinned: bool,
    pub sort_order: i64,
    pub archived: bool,
    pub depends_on: Vec<String>,
    pub revision: u64,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DesktopEquivalenceConversationFact {
    pub id: String,
    pub project_id: Option<String>,
    pub task_id: Option<String>,
    pub title: String,
    pub status: ProductConversationStatus,
    pub archived: bool,
    pub binding_ids: Vec<String>,
    pub timeline_cursor: u64,
    pub revision: u64,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DesktopEquivalenceTimelineFact {
    pub id: String,
    pub task_id: String,
    pub agent_session: String,
    pub sequence: u64,
    pub turn_id: Option<String>,
    pub kind: String,
    pub status: String,
    pub title: String,
    pub summary: Option<String>,
    pub payload: JsonValue,
    pub projected: bool,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DesktopEquivalenceComposerFact {
    pub task_id: String,
    pub revision: u64,
    pub content_bytes: usize,
    pub content_sha256: String,
    pub attachment_count: usize,
    pub conversation_task_ids: Vec<String>,
    pub model: Option<String>,
    pub reasoning_effort: Option<String>,
    pub permission: crate::application::DesktopExecutionPermission,
    pub plan_mode: bool,
    pub goal_mode: bool,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DesktopEquivalenceGoalFact {
    pub task_id: String,
    pub objective_sha256: String,
    pub status: DesktopGoalStatus,
    pub token_budget: Option<u64>,
    pub tokens_used: u64,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DesktopEquivalenceTodoFact {
    pub task_id: String,
    pub text_sha256: String,
    pub done: bool,
    pub order: i64,
    pub source: DesktopTodoSource,
    pub priority: DesktopTodoPriority,
    pub guide_status: Option<DesktopTodoGuideStatus>,
    pub attachment_count: usize,
    pub conversation_task_ids: Vec<String>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DesktopEquivalenceMilestoneFact {
    pub project_id: String,
    pub title: String,
    pub description: String,
    pub status: MilestoneStatus,
    pub due_date: Option<i64>,
    pub order: i64,
    pub task_ids: Vec<String>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DesktopEquivalenceMemoryFact {
    pub scope: MemoryScope,
    pub project_id: Option<String>,
    pub title: String,
    pub body_sha256: String,
    pub tags: Vec<String>,
    pub enabled: bool,
    pub source_task_id: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DesktopEquivalenceMemorySettingsFact {
    pub enabled: bool,
    pub baseline_injection_enabled: bool,
    pub cooldown_turns: u64,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DesktopEquivalenceConversationSuggestionSettingsFact {
    pub enabled: bool,
    pub source: crate::application::DesktopConversationSuggestionSource,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DesktopEquivalenceAutomationFact {
    pub name: String,
    pub enabled: bool,
    pub published: bool,
    pub scope: DesktopEquivalenceAutomationScopeFact,
    pub nodes: Vec<DesktopEquivalenceAutomationNodeFact>,
    pub edges: Vec<DesktopEquivalenceAutomationEdgeFact>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DesktopEquivalenceAutomationScopeFact {
    pub project_ids: Vec<String>,
    pub include_inbox: bool,
    pub task_statuses: Vec<String>,
    pub backends: Vec<String>,
    pub event_kinds: Vec<String>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DesktopEquivalenceAutomationNodeFact {
    pub kind: String,
    pub title: String,
    pub config_sha256: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DesktopEquivalenceAutomationEdgeFact {
    pub source_kind: String,
    pub source_title: String,
    pub target_kind: String,
    pub target_title: String,
    pub source_handle: Option<String>,
    pub target_handle: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DesktopEquivalenceMcpServerFact {
    pub server_id: String,
    pub transport: String,
    pub enabled: bool,
    pub registered: bool,
    pub editable: bool,
    pub configuration_sha256: String,
    pub credentials: Vec<DesktopEquivalenceMcpCredentialFact>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DesktopEquivalenceSkillFact {
    pub skill_id: String,
    pub scope: String,
    pub description_sha256: String,
    pub enabled: bool,
    pub editable: bool,
    pub runtime_available: bool,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DesktopEquivalencePluginFact {
    pub plugin_id: String,
    pub version: String,
    pub description_sha256: String,
    pub enabled: bool,
    pub runtime_available: bool,
    pub package_sha256: String,
    pub skill_count: usize,
    pub hook_count: usize,
    pub mcp_server_count: usize,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DesktopEquivalenceHookSourceFact {
    pub scope: String,
    pub revision: u64,
    pub enabled: bool,
    pub handlers: Vec<DesktopEquivalenceHookHandlerFact>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DesktopEquivalenceHookHandlerFact {
    pub id: String,
    pub event: String,
    pub matcher: Option<String>,
    pub configuration_sha256: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DesktopEquivalenceMcpCredentialFact {
    pub kind: String,
    pub name: String,
    pub present: bool,
}

impl DesktopApplication {
    pub fn debug_equivalence_snapshot(
        &self,
        fixture_id: &str,
    ) -> Result<DesktopEquivalenceSnapshot, DesktopApplicationError> {
        let fixture_id = validate_fixture_id(fixture_id)?;
        let prefix = format!("{fixture_id}-");
        let mut projects = self
            .query_projects(ProjectQuery {
                include_archived: true,
            })?
            .into_iter()
            .filter(|project| project.id.as_str().starts_with(&prefix))
            .map(|project| DesktopEquivalenceProjectFact {
                id: project.id.as_str().to_owned(),
                name: project.name,
                has_workspace_path: project.workspace_path.is_some(),
                pinned: project.pinned,
                sort_order: project.sort_order,
                archive: project.archive,
                revision: project.revision.get(),
            })
            .collect::<Vec<_>>();
        projects.sort_by(|left, right| left.id.cmp(&right.id));

        let mut product_tasks = self
            .query_tasks(TaskQuery::default().including_archived())?
            .into_iter()
            .filter(|task| task.id.as_str().starts_with(&prefix))
            .collect::<Vec<_>>();
        product_tasks.sort_by(|left, right| left.id.cmp(&right.id));

        let tasks = product_tasks
            .iter()
            .map(|task| {
                let mut depends_on = task
                    .depends_on
                    .iter()
                    .map(|task_id| task_id.as_str().to_owned())
                    .collect::<Vec<_>>();
                depends_on.sort();
                DesktopEquivalenceTaskFact {
                    id: task.id.as_str().to_owned(),
                    project_id: task
                        .project_id
                        .as_ref()
                        .map(|project_id| project_id.as_str().to_owned()),
                    parent_id: task
                        .parent_id
                        .as_ref()
                        .map(|task_id| task_id.as_str().to_owned()),
                    title: task.title.clone(),
                    description: task.description.clone(),
                    status: task.status,
                    priority: task.priority,
                    pinned: task.pinned,
                    sort_order: task.sort_order,
                    archived: task.archived,
                    depends_on,
                    revision: task.revision.get(),
                }
            })
            .collect::<Vec<_>>();

        let mut conversations = self
            .authority()
            .client()?
            .products()
            .list_conversations()?
            .into_iter()
            .filter(|conversation| {
                conversation.id.as_str().starts_with(&prefix)
                    || conversation
                        .task_id
                        .as_ref()
                        .is_some_and(|task_id| task_id.as_str().starts_with(&prefix))
            })
            .map(|conversation| {
                let mut binding_ids = conversation
                    .binding_ids
                    .iter()
                    .map(|binding_id| binding_id.as_str().to_owned())
                    .collect::<Vec<_>>();
                binding_ids.sort();
                DesktopEquivalenceConversationFact {
                    id: conversation.id.as_str().to_owned(),
                    project_id: conversation
                        .project_id
                        .as_ref()
                        .map(|project_id| project_id.as_str().to_owned()),
                    task_id: conversation
                        .task_id
                        .as_ref()
                        .map(|task_id| task_id.as_str().to_owned()),
                    title: conversation.title,
                    status: conversation.status,
                    archived: conversation.archived,
                    binding_ids,
                    timeline_cursor: conversation.timeline_cursor,
                    revision: conversation.revision.get(),
                }
            })
            .collect::<Vec<_>>();
        conversations.sort_by(|left, right| left.id.cmp(&right.id));

        let mut timeline = product_tasks
            .iter()
            .flat_map(|task| {
                self.authority()
                    .projection_timeline_for_task(&task.id)
                    .into_iter()
                    .map(|event| {
                        let goal_event = event.kind == "goal";
                        DesktopEquivalenceTimelineFact {
                            id: event.id.as_str().to_owned(),
                            task_id: event.task_id.as_str().to_owned(),
                            agent_session: event.agent_session.as_str().to_owned(),
                            sequence: event.sequence,
                            turn_id: event.turn_id,
                            kind: event.kind.clone(),
                            status: event.status,
                            title: event.title,
                            summary: if goal_event { None } else { event.summary },
                            payload: normalized_timeline_payload(&event.kind, event.payload),
                            projected: event.projected,
                        }
                    })
            })
            .collect::<Vec<_>>();
        timeline.sort_by(|left, right| {
            left.task_id
                .cmp(&right.task_id)
                .then_with(|| left.sequence.cmp(&right.sequence))
                .then_with(|| left.id.cmp(&right.id))
        });

        let mut composers = product_tasks
            .iter()
            .map(|task| self.composer_state(&task.id))
            .collect::<Result<Vec<_>, _>>()?
            .into_iter()
            .map(|composer| {
                let mut conversation_task_ids = composer
                    .conversation_references
                    .iter()
                    .map(|reference| reference.task_id.clone())
                    .collect::<Vec<_>>();
                conversation_task_ids.sort();
                DesktopEquivalenceComposerFact {
                    task_id: composer.task_id.as_str().to_owned(),
                    revision: composer.revision,
                    content_bytes: composer.content.len(),
                    content_sha256: format!("{:x}", Sha256::digest(composer.content.as_bytes())),
                    attachment_count: composer.attachments.len(),
                    conversation_task_ids,
                    model: composer.model,
                    reasoning_effort: composer.reasoning_effort,
                    permission: composer.permission,
                    plan_mode: composer.plan_mode,
                    goal_mode: composer.goal_mode,
                }
            })
            .collect::<Vec<_>>();
        composers.sort_by(|left, right| left.task_id.cmp(&right.task_id));

        let mut goals = Vec::new();
        let mut todos = Vec::new();
        for task in &product_tasks {
            if let Some(goal) = self.task_goal(&task.id)? {
                goals.push(DesktopEquivalenceGoalFact {
                    task_id: task.id.as_str().to_owned(),
                    objective_sha256: format!("{:x}", Sha256::digest(goal.objective.as_bytes())),
                    status: goal.status,
                    token_budget: goal.token_budget,
                    tokens_used: goal.tokens_used,
                });
            }
            for todo in self.list_task_todos(&task.id)? {
                let mut conversation_task_ids = todo
                    .conversation_references
                    .into_iter()
                    .map(|reference| reference.task_id)
                    .collect::<Vec<_>>();
                conversation_task_ids.sort();
                todos.push(DesktopEquivalenceTodoFact {
                    task_id: task.id.as_str().to_owned(),
                    text_sha256: format!("{:x}", Sha256::digest(todo.text.as_bytes())),
                    done: todo.done,
                    order: todo.order,
                    source: todo.source,
                    priority: todo.priority,
                    guide_status: todo.guide_status,
                    attachment_count: todo.attachments.len(),
                    conversation_task_ids,
                });
            }
        }
        goals.sort_by(|left, right| left.task_id.cmp(&right.task_id));
        todos.sort_by(|left, right| {
            left.task_id
                .cmp(&right.task_id)
                .then_with(|| left.order.cmp(&right.order))
                .then_with(|| left.text_sha256.cmp(&right.text_sha256))
        });

        let mut roadmap = Vec::new();
        for project in &projects {
            let project_id = lilia_contracts::ProjectId::new(project.id.clone())?;
            let project_roadmap = self.project_roadmap(&project_id)?;
            for milestone in project_roadmap.milestones {
                let mut task_ids = project_roadmap
                    .links
                    .iter()
                    .filter(|link| link.milestone_id == milestone.id)
                    .map(|link| link.task_id.clone())
                    .collect::<Vec<_>>();
                task_ids.sort();
                roadmap.push(DesktopEquivalenceMilestoneFact {
                    project_id: milestone.project_id,
                    title: milestone.title,
                    description: milestone.description,
                    status: milestone.status,
                    due_date: milestone.due_date,
                    order: milestone.order,
                    task_ids,
                });
            }
        }
        roadmap.sort_by(|left, right| {
            left.project_id
                .cmp(&right.project_id)
                .then_with(|| left.order.cmp(&right.order))
                .then_with(|| left.title.cmp(&right.title))
        });

        let project_ids = projects
            .iter()
            .map(|project| project.id.as_str())
            .collect::<std::collections::BTreeSet<_>>();
        let mut memory_records = std::collections::BTreeMap::new();
        for memory in self.list_memories(None)? {
            memory_records.insert(memory.id.clone(), memory);
        }
        for project in &projects {
            let project_id = lilia_contracts::ProjectId::new(project.id.clone())?;
            for memory in self.list_memories(Some(&project_id))? {
                memory_records.insert(memory.id.clone(), memory);
            }
        }
        let mut memories = memory_records
            .into_values()
            .filter(|memory| {
                memory.scope == MemoryScope::User
                    || memory.id.starts_with(&prefix)
                    || memory
                        .project_id
                        .as_deref()
                        .is_some_and(|project_id| project_ids.contains(project_id))
            })
            .map(|memory| {
                let mut tags = memory.tags;
                tags.sort();
                DesktopEquivalenceMemoryFact {
                    scope: memory.scope,
                    project_id: memory.project_id,
                    title: memory.title,
                    body_sha256: format!("{:x}", Sha256::digest(memory.body.as_bytes())),
                    tags,
                    enabled: memory.enabled,
                    source_task_id: memory.source_task_id,
                }
            })
            .collect::<Vec<_>>();
        memories.sort_by(|left, right| {
            left.project_id
                .cmp(&right.project_id)
                .then_with(|| left.title.cmp(&right.title))
        });

        let memory_settings = self.memory_settings()?;
        let memory_settings = DesktopEquivalenceMemorySettingsFact {
            enabled: memory_settings.enabled,
            baseline_injection_enabled: memory_settings.baseline_injection_enabled,
            cooldown_turns: memory_settings.cooldown_turns,
        };
        let conversation_suggestions = self.conversation_suggestion_settings()?;
        let conversation_suggestions = DesktopEquivalenceConversationSuggestionSettingsFact {
            enabled: conversation_suggestions.enabled,
            source: conversation_suggestions.source,
        };

        let mut automations = self
            .list_automation_workflows()?
            .into_iter()
            .filter(|workflow| workflow.name.starts_with(&prefix))
            .map(|workflow| {
                let node_endpoints = workflow
                    .draft
                    .nodes
                    .iter()
                    .map(|node| (node.id.as_str(), (node.kind.as_str(), node.title.as_str())))
                    .collect::<std::collections::BTreeMap<_, _>>();
                let mut nodes = workflow
                    .draft
                    .nodes
                    .iter()
                    .map(|node| DesktopEquivalenceAutomationNodeFact {
                        kind: node.kind.clone(),
                        title: node.title.clone(),
                        config_sha256: format!(
                            "{:x}",
                            Sha256::digest(node.config.to_string().as_bytes())
                        ),
                    })
                    .collect::<Vec<_>>();
                nodes.sort_by(|left, right| {
                    left.kind
                        .cmp(&right.kind)
                        .then_with(|| left.title.cmp(&right.title))
                        .then_with(|| left.config_sha256.cmp(&right.config_sha256))
                });
                let mut edges = workflow
                    .draft
                    .edges
                    .iter()
                    .filter_map(|edge| {
                        let (source_kind, source_title) =
                            node_endpoints.get(edge.source.as_str())?;
                        let (target_kind, target_title) =
                            node_endpoints.get(edge.target.as_str())?;
                        Some(DesktopEquivalenceAutomationEdgeFact {
                            source_kind: (*source_kind).to_owned(),
                            source_title: (*source_title).to_owned(),
                            target_kind: (*target_kind).to_owned(),
                            target_title: (*target_title).to_owned(),
                            source_handle: edge.source_handle.clone(),
                            target_handle: edge.target_handle.clone(),
                        })
                    })
                    .collect::<Vec<_>>();
                edges.sort_by(|left, right| {
                    left.source_kind
                        .cmp(&right.source_kind)
                        .then_with(|| left.source_title.cmp(&right.source_title))
                        .then_with(|| left.target_kind.cmp(&right.target_kind))
                        .then_with(|| left.target_title.cmp(&right.target_title))
                        .then_with(|| left.source_handle.cmp(&right.source_handle))
                        .then_with(|| left.target_handle.cmp(&right.target_handle))
                });
                DesktopEquivalenceAutomationFact {
                    name: workflow.name,
                    enabled: workflow.enabled,
                    published: workflow.published_version_id.is_some(),
                    scope: DesktopEquivalenceAutomationScopeFact {
                        project_ids: sorted_strings(workflow.scope.project_ids),
                        include_inbox: workflow.scope.include_inbox,
                        task_statuses: sorted_strings(workflow.scope.task_statuses),
                        backends: sorted_strings(workflow.scope.backends),
                        event_kinds: sorted_strings(workflow.scope.event_kinds),
                    },
                    nodes,
                    edges,
                }
            })
            .collect::<Vec<_>>();
        automations.sort_by(|left, right| left.name.cmp(&right.name));

        let extensions = self.extensions_snapshot()?;
        let mut skills = extensions
            .skills
            .iter()
            .filter(|skill| skill.skill_id.starts_with(&prefix))
            .map(|skill| DesktopEquivalenceSkillFact {
                skill_id: skill.skill_id.clone(),
                scope: skill.scope.clone(),
                description_sha256: format!("{:x}", Sha256::digest(skill.description.as_bytes())),
                enabled: skill.enabled,
                editable: skill.editable,
                runtime_available: skill.runtime_available,
            })
            .collect::<Vec<_>>();
        skills.sort_by(|left, right| left.skill_id.cmp(&right.skill_id));
        let mut plugins = extensions
            .plugins
            .iter()
            .filter(|plugin| plugin.plugin_id.starts_with(&prefix))
            .map(|plugin| DesktopEquivalencePluginFact {
                plugin_id: plugin.plugin_id.clone(),
                version: plugin.version.clone(),
                description_sha256: format!("{:x}", Sha256::digest(plugin.description.as_bytes())),
                enabled: plugin.enabled,
                runtime_available: plugin.runtime_available,
                package_sha256: plugin.package_sha256.clone(),
                skill_count: plugin.skill_count,
                hook_count: plugin.hook_count,
                mcp_server_count: plugin.mcp_server_count,
            })
            .collect::<Vec<_>>();
        plugins.sort_by(|left, right| left.plugin_id.cmp(&right.plugin_id));
        let mut mcp_servers = extensions
            .mcp_servers
            .into_iter()
            .filter(|server| server.server_id.starts_with(&prefix))
            .map(|server| {
                let mut credentials = server
                    .credentials
                    .into_iter()
                    .map(|credential| DesktopEquivalenceMcpCredentialFact {
                        kind: credential.kind.key_segment().to_owned(),
                        name: credential.name,
                        present: credential.present,
                    })
                    .collect::<Vec<_>>();
                credentials.sort_by(|left, right| {
                    left.kind
                        .cmp(&right.kind)
                        .then_with(|| left.name.cmp(&right.name))
                });
                let configuration = serde_json::to_vec(&(
                    server.command.as_deref(),
                    server.args.as_slice(),
                    server.url.as_deref(),
                ))
                .expect("MCP string tuple serialization cannot fail");
                DesktopEquivalenceMcpServerFact {
                    server_id: server.server_id,
                    transport: server.transport,
                    enabled: server.enabled,
                    registered: server.registered,
                    editable: server.editable,
                    configuration_sha256: format!("{:x}", Sha256::digest(configuration)),
                    credentials,
                }
            })
            .collect::<Vec<_>>();
        mcp_servers.sort_by(|left, right| left.server_id.cmp(&right.server_id));
        let mut hook_sources = Vec::new();
        for source in self.hooks_overview(None)?.sources {
            if !source.exists {
                continue;
            }
            let document = self.read_hook_source(source.scope, source.project_cwd.as_deref())?;
            let mut handlers = document
                .handlers
                .into_iter()
                .filter(|handler| handler.id.starts_with(&prefix))
                .map(|handler| {
                    let configuration = serde_json::to_vec(&(
                        handler.handler_type.as_str(),
                        handler.command.as_deref(),
                        handler.command_windows.as_deref(),
                        handler.timeout_seconds,
                        handler.status_message.as_deref(),
                    ))
                    .expect("Hook string tuple serialization cannot fail");
                    DesktopEquivalenceHookHandlerFact {
                        id: handler.id,
                        event: handler.event,
                        matcher: handler.matcher,
                        configuration_sha256: format!("{:x}", Sha256::digest(configuration)),
                    }
                })
                .collect::<Vec<_>>();
            if handlers.is_empty() {
                continue;
            }
            handlers.sort_by(|left, right| left.id.cmp(&right.id));
            hook_sources.push(DesktopEquivalenceHookSourceFact {
                scope: match source.scope {
                    crate::application::DesktopHookScope::User => "user",
                    crate::application::DesktopHookScope::Project => "project",
                }
                .to_owned(),
                revision: source.revision,
                enabled: source.enabled,
                handlers,
            });
        }
        hook_sources.sort_by(|left, right| left.scope.cmp(&right.scope));

        Ok(DesktopEquivalenceSnapshot {
            schema_version: SNAPSHOT_SCHEMA_VERSION,
            fixture_id: fixture_id.to_owned(),
            projects,
            tasks,
            conversations,
            timeline,
            composers,
            goals,
            todos,
            roadmap,
            memories,
            memory_settings,
            conversation_suggestions,
            automations,
            skills_registry_revision: extensions.skills_registry_revision,
            skills,
            plugins_registry_revision: extensions.plugins_registry_revision,
            plugins,
            hook_sources,
            mcp_registry_revision: extensions.mcp_registry_revision,
            mcp_servers,
        })
    }
}

fn normalized_timeline_payload(kind: &str, payload: JsonValue) -> JsonValue {
    if kind != "goal" {
        return payload;
    }
    if payload.get("cleared").and_then(JsonValue::as_bool) == Some(true) {
        serde_json::json!({ "cleared": true })
    } else {
        serde_json::json!({ "goal": true })
    }
}

fn sorted_strings(mut values: Vec<String>) -> Vec<String> {
    values.sort();
    values.dedup();
    values
}

fn validate_fixture_id(fixture_id: &str) -> Result<&str, DesktopApplicationError> {
    let fixture_id = fixture_id.trim();
    if fixture_id.is_empty()
        || fixture_id.len() > 80
        || !fixture_id
            .chars()
            .all(|character| character.is_ascii_alphanumeric() || matches!(character, '-' | '_'))
    {
        return Err(DesktopApplicationError::InvalidInput {
            field: "fixture_id",
            message: "equivalence fixture id must be 1-80 ASCII letters, digits, '-' or '_'"
                .to_owned(),
        });
    }
    Ok(fixture_id)
}

#[cfg(test)]
mod tests {
    use std::path::Path;
    use std::sync::Arc;

    use lilia_contracts::{
        AgentSessionRef, ProjectId, ProjectionEventId, TaskId, TimelineProjectionCommand,
        TimelineProjectionEvent,
    };
    use lilia_service::ServiceAuthority;
    use serde_json::json;

    use super::*;
    use crate::application::{
        AutomationNode, AutomationNodePosition, AutomationSaveDraftInput, AutomationScopeFilter,
        DesktopApplicationConfig, DesktopComposerCommand, DesktopHost, DesktopHostAction,
        DesktopHostContext, DesktopHostError, DesktopHostResult, DesktopMcpServerUpsert,
        DesktopMcpTransport, DesktopProjectCreate, DesktopTaskCreate, DesktopTodoCreate,
        MemorySettings, MemoryUpsertInput,
    };

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

    fn application(home: &Path) -> DesktopApplication {
        let authority = ServiceAuthority::bootstrap_with_home(home).unwrap();
        DesktopApplication::from_authority(
            DesktopApplicationConfig::new(home, "liliacode.equivalence-snapshot").unwrap(),
            authority,
            Arc::new(NoopHost),
        )
        .unwrap()
    }

    #[test]
    fn snapshot_is_fixture_scoped_deterministic_and_secret_free() {
        let home = tempfile::tempdir().unwrap();
        let application = application(home.path());
        let fixture_id = "equivalence-p0-v1";
        let project_id = ProjectId::new(format!("{fixture_id}-project")).unwrap();
        let task_id = TaskId::new(format!("{fixture_id}-task")).unwrap();
        application
            .create_project(DesktopProjectCreate {
                id: project_id.clone(),
                name: "等价项目".to_owned(),
                workspace_path: Some("C:/private/equivalence".to_owned()),
            })
            .unwrap();
        application
            .create_task(DesktopTaskCreate {
                id: task_id.clone(),
                project_id: Some(project_id.clone()),
                parent_id: None,
                title: "等价任务".to_owned(),
            })
            .unwrap();
        application
            .create_task(DesktopTaskCreate {
                id: TaskId::new("foreign-task").unwrap(),
                project_id: None,
                parent_id: None,
                title: "不应导出".to_owned(),
            })
            .unwrap();
        application
            .execute_composer_command(
                &task_id,
                DesktopComposerCommand::SetContent("fixture secret text".to_owned()),
            )
            .unwrap();
        application
            .authority()
            .apply_projection(TimelineProjectionCommand::UpsertTimelineEvent {
                event: TimelineProjectionEvent {
                    id: ProjectionEventId::new(format!("{fixture_id}-event")),
                    task_id: task_id.clone(),
                    agent_session: AgentSessionRef::new(format!("{fixture_id}-session")).unwrap(),
                    sequence: 7,
                    turn_id: Some(format!("{fixture_id}-turn")),
                    kind: "assistant".to_owned(),
                    status: "completed".to_owned(),
                    title: "完成".to_owned(),
                    summary: Some("摘要".to_owned()),
                    payload: json!({ "markdown": "# fixed" }),
                    projected: true,
                },
            })
            .unwrap();
        let milestone = application
            .create_milestone(&project_id, "同语料里程碑")
            .unwrap();
        application
            .set_milestone_tasks(
                &project_id,
                &milestone.id,
                vec![task_id.as_str().to_owned()],
            )
            .unwrap();
        application
            .save_memory(MemoryUpsertInput {
                id: Some(format!("{fixture_id}-memory")),
                scope: MemoryScope::Project,
                project_id: Some(project_id.as_str().to_owned()),
                title: "同语料项目记忆".to_owned(),
                body: "fixture memory body".to_owned(),
                tags: vec!["中文".to_owned(), "equivalence".to_owned()],
                enabled: true,
                source_task_id: Some(task_id.as_str().to_owned()),
                expected_updated_at: None,
            })
            .unwrap();
        application
            .save_memory(MemoryUpsertInput {
                id: Some("runtime-generated-user-memory".to_owned()),
                scope: MemoryScope::User,
                project_id: None,
                title: "同语料用户记忆".to_owned(),
                body: "first line\nsecond line\nthird line".to_owned(),
                tags: vec!["user".to_owned(), "equivalence".to_owned()],
                enabled: true,
                source_task_id: None,
                expected_updated_at: None,
            })
            .unwrap();
        application
            .save_memory_settings(MemorySettings {
                enabled: false,
                baseline_injection_enabled: false,
                cooldown_turns: 10,
            })
            .unwrap();
        application
            .save_automation_draft(AutomationSaveDraftInput {
                id: None,
                name: format!("{fixture_id}-automation"),
                scope: AutomationScopeFilter {
                    include_inbox: true,
                    project_ids: vec![project_id.as_str().to_owned()],
                    ..AutomationScopeFilter::default()
                },
                nodes: vec![AutomationNode {
                    id: "random-node-id".to_owned(),
                    kind: "trigger".to_owned(),
                    title: "同语料触发".to_owned(),
                    position: AutomationNodePosition { x: 80.0, y: 120.0 },
                    config: json!({ "triggerKind": "manual" }),
                }],
                edges: Vec::new(),
            })
            .unwrap();
        application
            .set_task_goal(&task_id, "fixture goal objective", Some(200))
            .unwrap();
        application
            .create_task_todo(DesktopTodoCreate {
                task_id: task_id.clone(),
                text: "fixture todo body".to_owned(),
                priority: DesktopTodoPriority::High,
                attachments: Vec::new(),
                conversation_references: Vec::new(),
                workflow: None,
            })
            .unwrap();
        application
            .create_skill_package(crate::application::DesktopSkillCreate {
                expected_registry_revision: 0,
                scope: crate::application::DesktopSkillScope::User,
                project_cwd: None,
                skill_id: format!("{fixture_id}-skill"),
                description: "fixture secret Skill instructions".to_owned(),
            })
            .unwrap();
        application
            .upsert_mcp_server(DesktopMcpServerUpsert {
                expected_registry_revision: 0,
                server_id: format!("{fixture_id}-mcp"),
                transport: DesktopMcpTransport::Stdio,
                command: Some("fixture secret command".to_owned()),
                args: vec!["--private-argument".to_owned()],
                url: None,
                env_secret_names: Vec::new(),
                header_secret_names: Vec::new(),
                enabled: false,
            })
            .unwrap();
        application
            .create_hook_source(crate::application::DesktopHookScope::User, None)
            .unwrap();
        application
            .update_hook_source(
                crate::application::DesktopHookScope::User,
                None,
                crate::application::DesktopHookDocumentUpdate {
                    expected_revision: 1,
                    handlers: vec![crate::application::DesktopHookHandlerUpdate {
                        id: Some(format!("{fixture_id}-hook")),
                        event: "UserPromptSubmit".to_owned(),
                        matcher: Some("*fixture*".to_owned()),
                        handler_type: "command".to_owned(),
                        command: Some("fixture secret Hook command".to_owned()),
                        command_windows: None,
                        timeout_seconds: Some(5),
                        status_message: Some("fixture status".to_owned()),
                    }],
                },
            )
            .unwrap();

        let snapshot = application.debug_equivalence_snapshot(fixture_id).unwrap();

        assert_eq!(snapshot.schema_version, 9);
        assert_eq!(snapshot.projects.len(), 1);
        assert_eq!(snapshot.tasks.len(), 1);
        assert_eq!(snapshot.conversations.len(), 1);
        assert_eq!(snapshot.timeline.len(), 2);
        assert_eq!(snapshot.timeline[0].sequence, 7);
        assert_eq!(snapshot.composers.len(), 1);
        assert_eq!(snapshot.composers[0].content_bytes, 19);
        assert_eq!(snapshot.composers[0].content_sha256.len(), 64);
        assert_eq!(snapshot.goals.len(), 1);
        assert_eq!(snapshot.goals[0].objective_sha256.len(), 64);
        assert_eq!(snapshot.goals[0].token_budget, Some(200));
        assert_eq!(snapshot.todos.len(), 1);
        assert_eq!(snapshot.todos[0].text_sha256.len(), 64);
        assert_eq!(snapshot.todos[0].priority, DesktopTodoPriority::High);
        assert_eq!(snapshot.roadmap.len(), 1);
        assert_eq!(snapshot.roadmap[0].title, "同语料里程碑");
        assert_eq!(
            snapshot.roadmap[0].task_ids,
            vec![task_id.as_str().to_owned()]
        );
        assert_eq!(snapshot.memories.len(), 2);
        assert_eq!(snapshot.memories[0].scope, MemoryScope::User);
        assert_eq!(snapshot.memories[0].project_id, None);
        assert_eq!(snapshot.memories[0].title, "同语料用户记忆");
        assert_eq!(snapshot.memories[0].body_sha256.len(), 64);
        assert_eq!(snapshot.memories[1].scope, MemoryScope::Project);
        assert_eq!(snapshot.memories[1].title, "同语料项目记忆");
        assert_eq!(snapshot.memories[1].body_sha256.len(), 64);
        assert_eq!(
            snapshot.memories[1].tags,
            vec!["equivalence".to_owned(), "中文".to_owned()]
        );
        assert!(!snapshot.memory_settings.enabled);
        assert!(!snapshot.memory_settings.baseline_injection_enabled);
        assert_eq!(snapshot.memory_settings.cooldown_turns, 10);
        assert!(snapshot.conversation_suggestions.enabled);
        assert_eq!(
            snapshot.conversation_suggestions.source,
            crate::application::DesktopConversationSuggestionSource::AssistantAi
        );
        assert_eq!(snapshot.automations.len(), 1);
        assert_eq!(
            snapshot.automations[0].name,
            format!("{fixture_id}-automation")
        );
        assert_eq!(snapshot.automations[0].nodes.len(), 1);
        assert_eq!(snapshot.automations[0].nodes[0].kind, "trigger");
        assert_eq!(snapshot.automations[0].nodes[0].config_sha256.len(), 64);
        assert_eq!(snapshot.skills_registry_revision, 1);
        assert_eq!(snapshot.skills.len(), 1);
        assert_eq!(snapshot.skills[0].scope, "user");
        assert!(snapshot.skills[0].enabled);
        assert!(snapshot.skills[0].runtime_available);
        assert_eq!(snapshot.skills[0].description_sha256.len(), 64);
        assert_eq!(snapshot.hook_sources.len(), 1);
        assert_eq!(snapshot.hook_sources[0].scope, "user");
        assert_eq!(snapshot.hook_sources[0].revision, 2);
        assert!(!snapshot.hook_sources[0].enabled);
        assert_eq!(snapshot.hook_sources[0].handlers.len(), 1);
        assert_eq!(
            snapshot.hook_sources[0].handlers[0]
                .configuration_sha256
                .len(),
            64
        );
        assert_eq!(snapshot.mcp_registry_revision, 1);
        assert_eq!(snapshot.mcp_servers.len(), 1);
        assert_eq!(snapshot.mcp_servers[0].transport, "stdio");
        assert!(!snapshot.mcp_servers[0].enabled);
        assert_eq!(snapshot.mcp_servers[0].configuration_sha256.len(), 64);
        assert!(snapshot.projects[0].has_workspace_path);
        let serialized = serde_json::to_string(&snapshot).unwrap();
        assert!(!serialized.contains("fixture secret text"));
        assert!(!serialized.contains("fixture memory body"));
        assert!(!serialized.contains("fixture goal objective"));
        assert!(!serialized.contains("fixture todo body"));
        assert!(!serialized.contains("fixture secret Skill instructions"));
        assert!(!serialized.contains("fixture secret command"));
        assert!(!serialized.contains("--private-argument"));
        assert!(!serialized.contains("fixture secret Hook command"));
        assert!(validate_fixture_id("../private").is_err());
        drop(application);
        home.close().unwrap();
    }
}
