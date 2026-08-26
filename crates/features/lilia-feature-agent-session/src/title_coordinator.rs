//! Task title proposal coordinator.
//!
//! Owns freshness, generation and the apply/review decision. The host still
//! talks to the auxiliary model and writes the resulting title; this crate
//! never holds `Jobs`.

use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, OnceLock};

use lilia_contracts::TaskId;
use serde::{Deserialize, Serialize};

pub const TITLE_UPDATE_ACTION_KIND: &str = "title_update";
pub const TITLE_MAX_CHARS: usize = 18;
pub const TITLE_MIN_CHARS: usize = 2;
pub const TITLE_SOURCE_SETTINGS_KEY: &str = "desktop.task-title-source.v1";
pub const TITLE_SOURCE_SCHEMA_VERSION: u32 = 1;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DesktopTaskTitleSource {
    Auto,
    Manual,
}

impl DesktopTaskTitleSource {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Auto => "auto",
            Self::Manual => "manual",
        }
    }

    pub fn parse(value: &str) -> Self {
        match value.trim() {
            "manual" => Self::Manual,
            _ => Self::Auto,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DesktopTaskTitleState {
    pub id: TaskId,
    pub project_id: Option<String>,
    pub title: String,
    pub title_source: DesktopTaskTitleSource,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct DesktopTimelineUpperBound {
    pub turn_seq: i64,
    pub intra_turn_order: i64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct TitleJobVersion {
    upper_bound: DesktopTimelineUpperBound,
    generation: u64,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DesktopTitleUpdateJob {
    pub task: DesktopTaskTitleState,
    pub turn_id: Option<String>,
    version: TitleJobVersion,
}

impl DesktopTitleUpdateJob {
    pub fn upper_bound(&self) -> DesktopTimelineUpperBound {
        self.version.upper_bound
    }

    pub fn generation(&self) -> u64 {
        self.version.generation
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DesktopTitleUpdateReview {
    pub task: DesktopTaskTitleState,
    pub request_id: String,
    pub proposed_title: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum DesktopTitleUpdateDecision {
    Success(DesktopTaskTitleState),
    RequiresAction(DesktopTitleUpdateReview),
    Stale(DesktopTaskTitleState),
    Unchanged,
    Stopped,
}

#[derive(Default)]
struct TitleGenerationState {
    next_generation: u64,
    latest_by_task: HashMap<String, TitleJobVersion>,
}

struct DesktopTitleUpdateCoordinatorInner {
    generations: Mutex<TitleGenerationState>,
    emissions: Mutex<()>,
    stopped: AtomicBool,
}

/// Carries a finished turn's title update away from the turn worker that
/// produced it. The desktop host answers this by submitting `lilia.agent/title@1`;
/// a host that installs nothing simply goes untitled, because naming a task is
/// never worth holding up the turn that just ended.
pub trait DesktopTitleUpdateScheduler: Send + Sync + 'static {
    fn request(&self, task_id: TaskId, turn_id: Option<String>);
}

#[derive(Clone)]
pub struct DesktopTitleUpdateCoordinator {
    inner: Arc<DesktopTitleUpdateCoordinatorInner>,
}

impl Default for DesktopTitleUpdateCoordinator {
    fn default() -> Self {
        Self {
            inner: Arc::new(DesktopTitleUpdateCoordinatorInner {
                generations: Mutex::new(TitleGenerationState::default()),
                emissions: Mutex::new(()),
                stopped: AtomicBool::new(false),
            }),
        }
    }
}

impl DesktopTitleUpdateCoordinator {
    pub fn schedule(
        &self,
        task: DesktopTaskTitleState,
        turn_id: Option<String>,
        upper_bound: DesktopTimelineUpperBound,
    ) -> Result<Option<DesktopTitleUpdateJob>, String> {
        let mut state = self
            .inner
            .generations
            .lock()
            .map_err(|_| "title generation state lock poisoned".to_string())?;
        if self.inner.stopped.load(Ordering::Acquire) {
            return Ok(None);
        }
        state.next_generation = state
            .next_generation
            .checked_add(1)
            .ok_or_else(|| "title generation exhausted".to_string())?;
        let version = TitleJobVersion {
            upper_bound,
            generation: state.next_generation,
        };
        if state
            .latest_by_task
            .get(task.id.as_str())
            .is_some_and(|current| current.upper_bound > upper_bound)
        {
            return Ok(Some(DesktopTitleUpdateJob {
                task,
                turn_id,
                version,
            }));
        }
        state
            .latest_by_task
            .insert(task.id.as_str().to_owned(), version);
        Ok(Some(DesktopTitleUpdateJob {
            task,
            turn_id,
            version,
        }))
    }

    pub fn is_latest(&self, job: &DesktopTitleUpdateJob) -> bool {
        if self.inner.stopped.load(Ordering::Acquire) {
            return false;
        }
        self.inner.generations.lock().ok().is_some_and(|state| {
            state.latest_by_task.get(job.task.id.as_str()) == Some(&job.version)
        })
    }

    pub fn decide_proposal(
        &self,
        job: &DesktopTitleUpdateJob,
        proposed: &str,
        current: DesktopTaskTitleState,
    ) -> DesktopTitleUpdateDecision {
        let Ok(state) = self.inner.generations.lock() else {
            return DesktopTitleUpdateDecision::Stopped;
        };
        if self.inner.stopped.load(Ordering::Acquire) {
            return DesktopTitleUpdateDecision::Stopped;
        }
        if state.latest_by_task.get(job.task.id.as_str()) != Some(&job.version) {
            return DesktopTitleUpdateDecision::Stale(job.task.clone());
        }
        if current.title_source == DesktopTaskTitleSource::Manual {
            return DesktopTitleUpdateDecision::RequiresAction(DesktopTitleUpdateReview {
                task: current,
                request_id: uuid::Uuid::new_v4().to_string(),
                proposed_title: proposed.to_owned(),
            });
        }
        if current.title != job.task.title || current.title_source != job.task.title_source {
            return DesktopTitleUpdateDecision::Stale(current);
        }
        if proposed == compact_line(&current.title) {
            return DesktopTitleUpdateDecision::Unchanged;
        }
        DesktopTitleUpdateDecision::Success(current)
    }

    pub fn while_running(&self, action: impl FnOnce()) {
        let Ok(_emission) = self.inner.emissions.lock() else {
            return;
        };
        if self.inner.stopped.load(Ordering::Acquire) {
            return;
        }
        action();
    }

    pub fn shutdown(&self) {
        self.inner.stopped.store(true, Ordering::Release);
        if let Ok(mut state) = self.inner.generations.lock() {
            state.latest_by_task.clear();
        }
    }
}

pub fn normalize_title(input: String) -> Result<String, String> {
    let mut title = compact_line(&input);
    for prefix in ["标题：", "标题:", "Title:", "title:"] {
        if let Some(rest) = title.strip_prefix(prefix) {
            title = compact_line(rest);
        }
    }
    title = title
        .trim_matches(|ch: char| matches!(ch, '"' | '\'' | '`' | '“' | '”' | '‘' | '’'))
        .to_string();
    title = compact_line(&title);
    if title.chars().count() < TITLE_MIN_CHARS {
        return Err("generated title too short".to_string());
    }
    Ok(truncate_chars(&title, TITLE_MAX_CHARS)
        .trim_end_matches('…')
        .to_string())
}

pub fn title_event_id(task_id: &str, request_id: &str) -> String {
    format!("title-update:{task_id}:{request_id}")
}

pub fn compact_line(input: &str) -> String {
    input.split_whitespace().collect::<Vec<_>>().join(" ")
}

pub fn truncate_chars(input: &str, max: usize) -> String {
    let mut out = String::new();
    for (index, ch) in input.chars().enumerate() {
        if index >= max {
            out.push('…');
            return out;
        }
        out.push(ch);
    }
    out
}

static TITLE_SYSTEM_INSTRUCTION: OnceLock<String> = OnceLock::new();

pub fn title_system_instruction() -> &'static str {
    TITLE_SYSTEM_INSTRUCTION.get_or_init(|| {
        #[derive(Deserialize)]
        struct TitleSection {
            #[serde(rename = "systemInstruction")]
            system_instruction: String,
        }
        #[derive(Deserialize)]
        struct PromptText {
            title: TitleSection,
        }
        let contract: PromptText = serde_json::from_str(include_str!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../lilia-contracts/contracts/prompt-text.json"
        )))
        .expect("prompt-text.json must deserialize");
        contract.title.system_instruction
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn newest_generation_wins_for_decide() {
        let coordinator = DesktopTitleUpdateCoordinator::default();
        let task_id = TaskId::new("task-1").unwrap();
        let task = DesktopTaskTitleState {
            id: task_id.clone(),
            project_id: None,
            title: "初始标题".into(),
            title_source: DesktopTaskTitleSource::Auto,
        };
        let job_a = coordinator
            .schedule(
                task.clone(),
                Some("turn-a".into()),
                DesktopTimelineUpperBound {
                    turn_seq: 1,
                    intra_turn_order: 0,
                },
            )
            .unwrap()
            .unwrap();
        let job_b = coordinator
            .schedule(
                task.clone(),
                Some("turn-b".into()),
                DesktopTimelineUpperBound {
                    turn_seq: 2,
                    intra_turn_order: 0,
                },
            )
            .unwrap()
            .unwrap();
        assert!(coordinator.is_latest(&job_b));
        assert!(!coordinator.is_latest(&job_a));
        let decision = coordinator.decide_proposal(&job_a, "旧标题", task.clone());
        assert!(matches!(decision, DesktopTitleUpdateDecision::Stale(_)));
        let decision = coordinator.decide_proposal(&job_b, "新标题", task);
        assert!(matches!(decision, DesktopTitleUpdateDecision::Success(_)));
    }

    #[test]
    fn manual_title_requires_action() {
        let coordinator = DesktopTitleUpdateCoordinator::default();
        let task_id = TaskId::new("task-2").unwrap();
        let task = DesktopTaskTitleState {
            id: task_id,
            project_id: None,
            title: "手动标题".into(),
            title_source: DesktopTaskTitleSource::Manual,
        };
        let job = coordinator
            .schedule(
                task.clone(),
                None,
                DesktopTimelineUpperBound {
                    turn_seq: 1,
                    intra_turn_order: 0,
                },
            )
            .unwrap()
            .unwrap();
        let decision = coordinator.decide_proposal(&job, "建议标题", task);
        let DesktopTitleUpdateDecision::RequiresAction(review) = decision else {
            panic!("manual title should create a review");
        };
        assert_eq!(review.task.title, "手动标题");
        assert_eq!(review.proposed_title, "建议标题");
        assert!(!review.request_id.is_empty());
    }

    #[test]
    fn normalize_title_strips_wrappers() {
        assert_eq!(
            normalize_title("标题：`对话标题事件化实现进度需要继续确认更多内容`".into()).unwrap(),
            "对话标题事件化实现进度需要继续确认更"
        );
        assert!(normalize_title(" ".into()).is_err());
    }
}
