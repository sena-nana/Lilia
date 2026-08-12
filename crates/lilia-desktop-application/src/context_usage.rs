use lilia_contracts::{ChatContextUsage, TaskId};
use mutsuki_agent_contracts::{AgentEvent, AgentEventEnvelope};

use crate::{DesktopApplication, DesktopApplicationError};

const NATIVE_BACKEND: &str = "native-agentkit";

impl DesktopApplication {
    pub fn task_context_usage(
        &self,
        task_id: &TaskId,
    ) -> Result<Option<ChatContextUsage>, DesktopApplicationError> {
        let runtime = self.authority().shared_runtime();
        let mut latest = None;
        for binding in self.authority().list_session_bindings(task_id)? {
            let Ok(session) = runtime
                .inner()
                .session_snapshot(binding.agent_session.as_str())
            else {
                continue;
            };
            if let Some(candidate) = latest_context_usage(task_id, &session.events) {
                if latest
                    .as_ref()
                    .is_none_or(|current: &ContextUsageCandidate| candidate.key > current.key)
                {
                    latest = Some(candidate);
                }
            }
        }
        Ok(latest.map(|candidate| candidate.usage))
    }
}

#[derive(Clone, Debug)]
struct ContextUsageCandidate {
    key: (u64, u64, u8),
    usage: ChatContextUsage,
}

fn latest_context_usage(
    task_id: &TaskId,
    events: &[AgentEventEnvelope],
) -> Option<ContextUsageCandidate> {
    events
        .iter()
        .filter_map(|event| {
            let updated_at = event.meta.timestamp_unix_ms;
            let (priority, used_tokens, limit_tokens, used_percent, source) = match &event.event {
                AgentEvent::ContextUsageUpdated { usage, .. } => {
                    let used_tokens = usage.input_tokens.saturating_add(usage.reserved_tokens);
                    let limit_tokens = (usage.limit_tokens > 0).then_some(usage.limit_tokens);
                    let used_percent = limit_tokens.map(|limit| {
                        ((used_tokens as f64 / limit as f64) * 100.0).clamp(0.0, 100.0)
                    });
                    (2, used_tokens, limit_tokens, used_percent, NATIVE_BACKEND)
                }
                AgentEvent::Usage { usage, .. } => {
                    let used_tokens = if usage.total_tokens > 0 {
                        usage.total_tokens
                    } else {
                        usage.input_tokens.saturating_add(usage.output_tokens)
                    };
                    (1, used_tokens, None, None, "native-agentkit-usage")
                }
                _ => return None,
            };
            Some(ContextUsageCandidate {
                key: (updated_at, event.sequence, priority),
                usage: ChatContextUsage {
                    task_id: task_id.as_str().to_owned(),
                    backend: NATIVE_BACKEND.to_owned(),
                    used_tokens,
                    limit_tokens,
                    used_percent,
                    source: source.to_owned(),
                    updated_at,
                    unavailable_reason: None,
                },
            })
        })
        .max_by_key(|candidate| candidate.key)
}

#[cfg(test)]
mod tests {
    use mutsuki_agent_contracts::{AgentEventMeta, AgentUsage, ContextUsageSnapshot};
    use serde_json::json;

    use super::*;

    fn envelope(sequence: u64, timestamp: u64, event: AgentEvent) -> AgentEventEnvelope {
        AgentEventEnvelope {
            session_id: "session-1".to_owned(),
            sequence,
            meta: AgentEventMeta {
                event_id: format!("event-{sequence}"),
                timestamp_unix_ms: timestamp,
                ..AgentEventMeta::default()
            },
            event,
        }
    }

    #[test]
    fn exact_context_usage_has_a_ratio_and_wins_same_timestamp_usage() {
        let task_id = TaskId::new("task-1").unwrap();
        let events = vec![
            envelope(
                2,
                100,
                AgentEvent::Usage {
                    turn_id: "turn-1".to_owned(),
                    usage: AgentUsage {
                        input_tokens: 4_000,
                        output_tokens: 96,
                        total_tokens: 4_096,
                    },
                },
            ),
            envelope(
                2,
                100,
                AgentEvent::ContextUsageUpdated {
                    turn_id: "turn-1".to_owned(),
                    usage: ContextUsageSnapshot {
                        input_tokens: 3_584,
                        reserved_tokens: 512,
                        limit_tokens: 8_192,
                        breakdown: json!({}),
                    },
                },
            ),
        ];

        let usage = latest_context_usage(&task_id, &events).unwrap().usage;

        assert_eq!(usage.used_tokens, 4_096);
        assert_eq!(usage.limit_tokens, Some(8_192));
        assert_eq!(usage.used_percent, Some(50.0));
        assert_eq!(usage.source, NATIVE_BACKEND);
    }

    #[test]
    fn usage_fallback_never_invents_a_context_limit() {
        let task_id = TaskId::new("task-1").unwrap();
        let events = vec![envelope(
            1,
            101,
            AgentEvent::Usage {
                turn_id: "turn-1".to_owned(),
                usage: AgentUsage {
                    input_tokens: 21,
                    output_tokens: 13,
                    total_tokens: 34,
                },
            },
        )];

        let usage = latest_context_usage(&task_id, &events).unwrap().usage;

        assert_eq!(usage.used_tokens, 34);
        assert_eq!(usage.limit_tokens, None);
        assert_eq!(usage.used_percent, None);
        assert_eq!(usage.source, "native-agentkit-usage");
    }
}
