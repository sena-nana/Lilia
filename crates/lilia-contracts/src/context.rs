use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ChatContextUsage {
    pub task_id: String,
    pub backend: String,
    pub used_tokens: u64,
    pub limit_tokens: Option<u64>,
    pub used_percent: Option<f64>,
    pub source: String,
    pub updated_at: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub unavailable_reason: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn context_usage_matches_the_frontend_wire_format() {
        let usage = ChatContextUsage {
            task_id: "task-1".to_owned(),
            backend: "native-agentkit".to_owned(),
            used_tokens: 4_096,
            limit_tokens: Some(8_192),
            used_percent: Some(50.0),
            source: "native-agentkit".to_owned(),
            updated_at: 100,
            unavailable_reason: None,
        };

        assert_eq!(
            serde_json::to_value(usage).unwrap(),
            serde_json::json!({
                "taskId": "task-1",
                "backend": "native-agentkit",
                "usedTokens": 4_096,
                "limitTokens": 8_192,
                "usedPercent": 50.0,
                "source": "native-agentkit",
                "updatedAt": 100,
            })
        );
    }
}
