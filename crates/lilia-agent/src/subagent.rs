use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NativeSubagentDefinition {
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub description: String,
    pub instruction: String,
    #[serde(default)]
    pub enabled: bool,
}

impl NativeSubagentDefinition {
    pub fn validate(&self) -> Result<(), &'static str> {
        if self.id.trim().is_empty() || self.id.len() > 128 {
            return Err("subagent id must contain 1-128 characters");
        }
        if self.name.trim().is_empty() || self.name.chars().count() > 80 {
            return Err("subagent name must contain 1-80 characters");
        }
        if self.description.chars().count() > 500 {
            return Err("subagent description exceeds 500 characters");
        }
        if self.instruction.trim().is_empty() || self.instruction.chars().count() > 16_000 {
            return Err("subagent instruction must contain 1-16000 characters");
        }
        if self
            .id
            .chars()
            .chain(self.name.chars())
            .any(char::is_control)
            || self
                .description
                .chars()
                .chain(self.instruction.chars())
                .any(|character| character.is_control() && !matches!(character, '\n' | '\r' | '\t'))
        {
            return Err("subagent fields must not contain control characters");
        }
        Ok(())
    }
}
