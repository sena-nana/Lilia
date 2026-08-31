use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

const OTHER_ANSWER_VALUE: &str = "other";

#[derive(Clone, Copy, Debug, Default, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub(crate) enum AskUserMode {
    #[default]
    Confirm,
    Single,
    Multi,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub(crate) struct AskUserOption {
    id: Option<String>,
    pub(crate) label: String,
    pub(crate) description: Option<String>,
    pub(crate) preview: Option<String>,
    #[serde(default)]
    pub(crate) recommended: bool,
    #[serde(default)]
    pub(crate) danger: bool,
}

impl AskUserOption {
    pub(crate) fn id(&self, index: usize) -> String {
        self.id
            .clone()
            .filter(|id| !id.trim().is_empty())
            .unwrap_or_else(|| {
                if self.label.trim().is_empty() {
                    format!("opt-{index}")
                } else {
                    self.label.clone()
                }
            })
    }
}

#[derive(Clone, Debug, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub(crate) struct AskUserQuestion {
    pub(crate) id: String,
    pub(crate) header: Option<String>,
    pub(crate) question: String,
    #[serde(default)]
    pub(crate) mode: AskUserMode,
    #[serde(default)]
    pub(crate) options: Vec<AskUserOption>,
    pub(crate) confirm_label: Option<String>,
    pub(crate) cancel_label: Option<String>,
    #[serde(default)]
    pub(crate) danger: bool,
    pub(crate) skippable: Option<bool>,
    #[serde(default)]
    pub(crate) allow_other: bool,
    pub(crate) min_selections: Option<usize>,
    pub(crate) max_selections: Option<usize>,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub(crate) struct AskUserSpec {
    pub(crate) title: Option<String>,
    pub(crate) source: Option<String>,
    pub(crate) dismissable: Option<bool>,
    pub(crate) questions: Vec<AskUserQuestion>,
}

impl AskUserSpec {
    pub(crate) fn from_payload(payload: &Value) -> Option<Self> {
        let payload = payload.get("spec").unwrap_or(payload);
        let spec = serde_json::from_value::<Self>(payload.clone()).ok()?;
        (!spec.questions.is_empty()).then_some(spec)
    }

    pub(crate) fn is_dismissable(&self) -> bool {
        self.dismissable != Some(false)
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum AskUserAction {
    Select(String),
    SetFreeform(String),
    Submit,
    Reject,
    Skip,
    Back,
    Cancel,
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) enum AskUserOutcome {
    Pending,
    Completed { accepted: bool, response: Value },
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct AskUserDraft {
    question_index: usize,
    answers: BTreeMap<String, AskUserAnswer>,
    selected: Vec<String>,
    freeform: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
struct AskUserAnswer {
    question_id: String,
    value: Value,
    #[serde(skip_serializing_if = "Option::is_none")]
    notes: Option<String>,
}

impl AskUserDraft {
    pub(crate) fn question_index(&self) -> usize {
        self.question_index
    }

    pub(crate) fn question<'a>(&self, spec: &'a AskUserSpec) -> Option<&'a AskUserQuestion> {
        spec.questions.get(self.question_index)
    }

    pub(crate) fn selected(&self, option_id: &str) -> bool {
        self.selected.iter().any(|selected| selected == option_id)
    }

    pub(crate) fn other_selected(&self) -> bool {
        self.selected(OTHER_ANSWER_VALUE)
    }

    pub(crate) fn freeform(&self) -> &str {
        &self.freeform
    }

    pub(crate) fn can_submit(&self, spec: &AskUserSpec) -> bool {
        self.answer_for_current(spec).is_some()
    }

    pub(crate) fn apply(&mut self, spec: &AskUserSpec, action: AskUserAction) -> AskUserOutcome {
        let Some(question) = self.question(spec).cloned() else {
            return AskUserOutcome::Pending;
        };
        match action {
            AskUserAction::Select(option_id) => {
                if !question_accepts_option(&question, &option_id) {
                    return AskUserOutcome::Pending;
                }
                match question.mode {
                    AskUserMode::Confirm => {}
                    AskUserMode::Single => {
                        self.selected.clear();
                        self.selected.push(option_id.clone());
                        if option_id != OTHER_ANSWER_VALUE {
                            self.freeform.clear();
                        }
                    }
                    AskUserMode::Multi => {
                        if let Some(index) = self
                            .selected
                            .iter()
                            .position(|selected| selected == &option_id)
                        {
                            self.selected.remove(index);
                            if option_id == OTHER_ANSWER_VALUE {
                                self.freeform.clear();
                            }
                        } else {
                            if question
                                .max_selections
                                .is_some_and(|max| self.selected.len() >= max)
                            {
                                self.selected.remove(0);
                            }
                            self.selected.push(option_id);
                        }
                    }
                }
                AskUserOutcome::Pending
            }
            AskUserAction::SetFreeform(value) => {
                if question.allow_other && self.other_selected() {
                    self.freeform = value;
                }
                AskUserOutcome::Pending
            }
            AskUserAction::Submit => {
                let Some(answer) = self.answer_for_current(spec) else {
                    return AskUserOutcome::Pending;
                };
                self.answers.insert(answer.question_id.clone(), answer);
                self.advance(spec)
            }
            AskUserAction::Reject if question.mode == AskUserMode::Confirm => {
                self.answers.insert(
                    question.id.clone(),
                    AskUserAnswer {
                        question_id: question.id,
                        value: Value::String("no".to_owned()),
                        notes: normalized_freeform(&self.freeform),
                    },
                );
                self.advance(spec)
            }
            AskUserAction::Skip
                if spec.questions.len() > 1 && question.skippable != Some(false) =>
            {
                self.answers.remove(&question.id);
                self.advance(spec)
            }
            AskUserAction::Back if self.question_index > 0 => {
                if question.mode != AskUserMode::Confirm {
                    if let Some(answer) = self.answer_for_current(spec) {
                        self.answers.insert(answer.question_id.clone(), answer);
                    }
                }
                self.question_index -= 1;
                self.load_current_answer(spec);
                AskUserOutcome::Pending
            }
            AskUserAction::Cancel if spec.is_dismissable() => self.completed(true),
            _ => AskUserOutcome::Pending,
        }
    }

    fn answer_for_current(&self, spec: &AskUserSpec) -> Option<AskUserAnswer> {
        let question = self.question(spec)?;
        let (value, notes) = match question.mode {
            AskUserMode::Confirm => (Value::String("yes".to_owned()), None),
            AskUserMode::Single => {
                let selected = self.selected.first()?.clone();
                if selected == OTHER_ANSWER_VALUE {
                    let notes = normalized_freeform(&self.freeform)?;
                    (Value::String(selected), Some(notes))
                } else {
                    (Value::String(selected), None)
                }
            }
            AskUserMode::Multi => {
                let minimum = question.min_selections.unwrap_or(1);
                if self.selected.len() < minimum {
                    return None;
                }
                let notes = if self.other_selected() {
                    Some(normalized_freeform(&self.freeform)?)
                } else {
                    None
                };
                (
                    Value::Array(self.selected.iter().cloned().map(Value::String).collect()),
                    notes,
                )
            }
        };
        Some(AskUserAnswer {
            question_id: question.id.clone(),
            value,
            notes,
        })
    }

    fn advance(&mut self, spec: &AskUserSpec) -> AskUserOutcome {
        if self.question_index + 1 >= spec.questions.len() {
            return self.completed(false);
        }
        self.question_index += 1;
        self.load_current_answer(spec);
        AskUserOutcome::Pending
    }

    fn completed(&self, cancelled: bool) -> AskUserOutcome {
        AskUserOutcome::Completed {
            accepted: !cancelled,
            response: json!({
                "answers": self.answers,
                "cancelled": cancelled,
            }),
        }
    }

    fn load_current_answer(&mut self, spec: &AskUserSpec) {
        self.selected.clear();
        self.freeform.clear();
        let Some(question) = self.question(spec) else {
            return;
        };
        let Some(answer) = self.answers.get(&question.id) else {
            return;
        };
        match &answer.value {
            Value::String(value) if question.mode == AskUserMode::Single => {
                self.selected.push(value.clone());
            }
            Value::Array(values) if question.mode == AskUserMode::Multi => {
                self.selected
                    .extend(values.iter().filter_map(Value::as_str).map(str::to_owned));
            }
            _ => {}
        }
        self.freeform = answer.notes.clone().unwrap_or_default();
    }
}

fn question_accepts_option(question: &AskUserQuestion, option_id: &str) -> bool {
    (question.allow_other && option_id == OTHER_ANSWER_VALUE)
        || question
            .options
            .iter()
            .enumerate()
            .any(|(index, option)| option.id(index) == option_id)
}

fn normalized_freeform(value: &str) -> Option<String> {
    let value = value.trim();
    (!value.is_empty()).then(|| value.to_owned())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn flow_spec() -> AskUserSpec {
        AskUserSpec::from_payload(&json!({
            "spec": {
                "title": "Debug 多题提问",
                "dismissable": true,
                "questions": [
                    {
                        "id": "target",
                        "question": "先验证哪个入口？",
                        "mode": "single",
                        "options": [
                            { "id": "sidebar", "label": "侧栏" },
                            { "id": "timeline", "label": "时间线" }
                        ]
                    },
                    {
                        "id": "checks",
                        "question": "一起检查哪些状态？",
                        "mode": "multi",
                        "minSelections": 2,
                        "maxSelections": 2,
                        "allowOther": true,
                        "options": [
                            { "id": "success", "label": "完成态" },
                            { "id": "cancelled", "label": "取消态" }
                        ]
                    }
                ]
            }
        }))
        .unwrap()
    }

    #[test]
    fn multi_question_flow_builds_the_shared_ask_user_result_contract() {
        let spec = flow_spec();
        let mut draft = AskUserDraft::default();
        assert_eq!(
            draft.apply(&spec, AskUserAction::Select("timeline".to_owned())),
            AskUserOutcome::Pending
        );
        assert_eq!(
            draft.apply(&spec, AskUserAction::Submit),
            AskUserOutcome::Pending
        );
        assert_eq!(draft.question_index(), 1);

        draft.apply(&spec, AskUserAction::Select("success".to_owned()));
        assert!(!draft.can_submit(&spec));
        draft.apply(&spec, AskUserAction::Select("other".to_owned()));
        assert!(!draft.can_submit(&spec));
        draft.apply(
            &spec,
            AskUserAction::SetFreeform("  Windows 真机  ".to_owned()),
        );
        assert!(draft.can_submit(&spec));

        let AskUserOutcome::Completed { accepted, response } =
            draft.apply(&spec, AskUserAction::Submit)
        else {
            panic!("last question should complete the interaction")
        };
        assert!(accepted);
        assert_eq!(response["cancelled"], false);
        assert_eq!(response["answers"]["target"]["value"], "timeline");
        assert_eq!(
            response["answers"]["checks"]["value"],
            json!(["success", "other"])
        );
        assert_eq!(response["answers"]["checks"]["notes"], "Windows 真机");
    }

    #[test]
    fn back_restores_a_prior_choice_and_maximum_selection_replaces_the_oldest() {
        let spec = flow_spec();
        let mut draft = AskUserDraft::default();
        draft.apply(&spec, AskUserAction::Select("sidebar".to_owned()));
        draft.apply(&spec, AskUserAction::Submit);
        draft.apply(&spec, AskUserAction::Select("success".to_owned()));
        draft.apply(&spec, AskUserAction::Select("cancelled".to_owned()));
        draft.apply(&spec, AskUserAction::Select("other".to_owned()));
        assert!(!draft.selected("success"));
        assert!(draft.selected("cancelled"));
        assert!(draft.selected("other"));

        draft.apply(&spec, AskUserAction::Back);
        assert_eq!(draft.question_index(), 0);
        assert!(draft.selected("sidebar"));
    }

    #[test]
    fn nondismissable_question_rejects_cancel_without_losing_the_draft() {
        let mut spec = flow_spec();
        spec.dismissable = Some(false);
        let mut draft = AskUserDraft::default();
        draft.apply(&spec, AskUserAction::Select("timeline".to_owned()));

        assert_eq!(
            draft.apply(&spec, AskUserAction::Cancel),
            AskUserOutcome::Pending
        );
        assert!(draft.selected("timeline"));
    }
}
