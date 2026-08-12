use serde_json::Value as JsonValue;

pub fn render_automation_template(template: &str, input: &JsonValue) -> String {
    let mut output = String::new();
    let mut remaining = template;
    while let Some(start) = remaining.find("${") {
        let (prefix, tail) = remaining.split_at(start);
        output.push_str(prefix);
        let Some(end) = tail.find('}') else {
            output.push_str(tail);
            return output;
        };
        if let Some(value) = automation_json_path(input, &tail[2..end]) {
            output.push_str(&automation_json_value_to_string(value));
        }
        remaining = &tail[end + 1..];
    }
    output.push_str(remaining);
    output
}

pub fn automation_json_path<'a>(value: &'a JsonValue, path: &str) -> Option<&'a JsonValue> {
    let mut current = value;
    for part in path.split('.') {
        current = current.get(part)?;
    }
    Some(current)
}

pub(crate) fn automation_json_value_to_string(value: &JsonValue) -> String {
    value
        .as_str()
        .map(str::to_owned)
        .unwrap_or_else(|| value.to_string())
}

pub(crate) fn automation_json_value_to_port(value: &JsonValue) -> String {
    automation_json_value_to_string(value)
        .trim()
        .chars()
        .map(|character| {
            if character.is_ascii_alphanumeric() || matches!(character, '_' | '-') {
                character
            } else {
                '_'
            }
        })
        .collect::<String>()
        .trim_matches('_')
        .to_owned()
}

pub(crate) fn automation_json_value_is_truthy(value: &JsonValue) -> bool {
    match value {
        JsonValue::Bool(value) => *value,
        JsonValue::Number(value) => value.as_i64().unwrap_or(0) != 0,
        JsonValue::String(value) => !value.trim().is_empty() && value != "false",
        JsonValue::Array(value) => !value.is_empty(),
        JsonValue::Object(value) => !value.is_empty(),
        JsonValue::Null => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn template_preserves_legacy_paths_and_unclosed_expression() {
        let input = serde_json::json!({
            "trigger": { "taskId": "task-1" },
            "nodes": { "one": { "output": { "count": 3 } } }
        });
        assert_eq!(
            render_automation_template(
                "task=${trigger.taskId}; count=${nodes.one.output.count}",
                &input
            ),
            "task=task-1; count=3"
        );
        assert_eq!(
            render_automation_template("keep ${trigger.taskId", &input),
            "keep ${trigger.taskId"
        );
    }
}
