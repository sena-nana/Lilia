//! Session title search shared by desktop consumers.
//!
//! Substring hits rank first, followed by character
//! bigram TF-IDF + cosine similarity for near matches. Corpus is always rebuilt
//! from Product Core task facts — never from UI caches.

use std::collections::{BTreeMap, HashMap, HashSet};

use lilia_contracts::TaskId;
use serde::{Deserialize, Serialize};

use crate::application::{DesktopApplication, DesktopApplicationError, ProjectQuery, TaskQuery};

const MAX_SESSION_SEARCH_RESULTS: usize = 100;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum DesktopSessionSearchKind {
    ProjectTask,
    Orphan,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DesktopSessionSearchResult {
    pub kind: DesktopSessionSearchKind,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub project_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub project_name: Option<String>,
    pub task_id: TaskId,
    pub title: String,
    pub route: String,
    pub score: f64,
    pub highlights: Vec<(usize, usize)>,
}

#[derive(Clone)]
struct SessionDoc {
    kind: DesktopSessionSearchKind,
    project_id: Option<String>,
    project_name: Option<String>,
    task_id: TaskId,
    title: String,
    route: String,
    title_tokens: Vec<String>,
    vector: HashMap<String, f64>,
}

impl DesktopApplication {
    pub fn search_sessions(
        &self,
        query: &str,
        limit: usize,
    ) -> Result<Vec<DesktopSessionSearchResult>, DesktopApplicationError> {
        let query = query.trim();
        if query.is_empty() {
            return Ok(Vec::new());
        }
        let docs = self.session_search_corpus()?;
        let mut merged = BTreeMap::new();
        for result in search_text(query, &docs) {
            merged.insert(result.route.clone(), result);
        }
        for result in search_vector(query, &docs) {
            merged
                .entry(result.route.clone())
                .and_modify(|existing| {
                    if result.score > existing.score {
                        existing.score = result.score;
                    }
                    if existing.highlights.is_empty() && !result.highlights.is_empty() {
                        existing.highlights = result.highlights.clone();
                    }
                })
                .or_insert(result);
        }
        let mut results = merged.into_values().collect::<Vec<_>>();
        results.sort_by(|left, right| {
            right
                .score
                .partial_cmp(&left.score)
                .unwrap_or(std::cmp::Ordering::Equal)
                .then_with(|| left.title.cmp(&right.title))
                .then_with(|| left.task_id.as_str().cmp(right.task_id.as_str()))
        });
        results.truncate(limit.clamp(1, MAX_SESSION_SEARCH_RESULTS));
        Ok(results)
    }

    fn session_search_corpus(&self) -> Result<Vec<SessionDoc>, DesktopApplicationError> {
        let project_names = self
            .query_projects(ProjectQuery::default())?
            .into_iter()
            .map(|project| (project.id, project.name))
            .collect::<BTreeMap<_, _>>();
        let mut docs = Vec::new();
        for task in self.query_tasks(TaskQuery::default())? {
            let project_name = task
                .project_id
                .as_ref()
                .and_then(|project_id| project_names.get(project_id))
                .cloned();
            let project_id = task.project_id.as_ref().map(|id| id.as_str().to_owned());
            let route = match &project_id {
                Some(project_id) => format!("/projects/{project_id}/tasks/{}", task.id.as_str()),
                None => format!("/chats/{}", task.id.as_str()),
            };
            let title_tokens = bigrams(&task.title);
            docs.push(SessionDoc {
                kind: if project_id.is_some() {
                    DesktopSessionSearchKind::ProjectTask
                } else {
                    DesktopSessionSearchKind::Orphan
                },
                project_id,
                project_name,
                task_id: task.id,
                title: task.title,
                route,
                title_tokens,
                vector: HashMap::new(),
            });
        }
        let idf = build_idf(&docs);
        for doc in &mut docs {
            doc.vector = tfidf_vec(&doc.title_tokens, &idf);
        }
        Ok(docs)
    }
}

fn search_text(query: &str, docs: &[SessionDoc]) -> Vec<DesktopSessionSearchResult> {
    let mut out = Vec::new();
    for doc in docs {
        let ranges = find_ranges(&doc.title, query);
        if ranges.is_empty() {
            continue;
        }
        let earliest = ranges.iter().map(|range| range.0).min().unwrap_or(0);
        let score =
            ranges.len() as f64 * 10.0 + (1.0 - earliest as f64 / doc.title.len().max(1) as f64);
        out.push(to_result(doc, score, ranges));
    }
    out
}

fn search_vector(query: &str, docs: &[SessionDoc]) -> Vec<DesktopSessionSearchResult> {
    let idf = build_idf(docs);
    let query_vec = tfidf_vec(&bigrams(query), &idf);
    if query_vec.is_empty() {
        return Vec::new();
    }
    let mut out = Vec::new();
    for doc in docs {
        let score = cosine(&query_vec, &doc.vector);
        if score <= 0.05 {
            continue;
        }
        out.push(to_result(doc, score, Vec::new()));
    }
    out
}

fn to_result(
    doc: &SessionDoc,
    score: f64,
    highlights: Vec<(usize, usize)>,
) -> DesktopSessionSearchResult {
    DesktopSessionSearchResult {
        kind: doc.kind,
        project_id: doc.project_id.clone(),
        project_name: doc.project_name.clone(),
        task_id: doc.task_id.clone(),
        title: doc.title.clone(),
        route: doc.route.clone(),
        score,
        highlights,
    }
}

fn find_ranges(title: &str, query: &str) -> Vec<(usize, usize)> {
    let title_lower = title.to_lowercase();
    let query_lower = query.to_lowercase().trim().to_owned();
    if query_lower.is_empty() {
        return Vec::new();
    }
    let mut ranges = Vec::new();
    let mut idx = 0;
    while let Some(found) = title_lower[idx..].find(&query_lower) {
        let start = idx + found;
        let end = start + query_lower.len();
        ranges.push((start, end));
        idx = end;
    }
    if !ranges.is_empty() {
        return ranges;
    }
    for token in query_lower
        .split_whitespace()
        .filter(|token| !token.is_empty())
    {
        let mut i = 0;
        while let Some(found) = title_lower[i..].find(token) {
            let start = i + found;
            let end = start + token.len();
            ranges.push((start, end));
            i = end;
        }
    }
    ranges
}

fn normalize(value: &str) -> String {
    value
        .to_lowercase()
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}

fn bigrams(value: &str) -> Vec<String> {
    let norm = normalize(value);
    if norm.is_empty() {
        return Vec::new();
    }
    let chars = norm.chars().collect::<Vec<_>>();
    if chars.len() == 1 {
        return vec![norm];
    }
    chars
        .windows(2)
        .map(|window| window.iter().collect::<String>())
        .collect()
}

fn build_idf(docs: &[SessionDoc]) -> HashMap<String, f64> {
    let n = docs.len().max(1) as f64;
    let mut df = HashMap::new();
    for doc in docs {
        let seen = doc.title_tokens.iter().cloned().collect::<HashSet<_>>();
        for token in seen {
            *df.entry(token).or_insert(0_u64) += 1;
        }
    }
    df.into_iter()
        .map(|(token, count)| (token, (1.0 + n / count as f64).ln()))
        .collect()
}

fn tfidf_vec(tokens: &[String], idf: &HashMap<String, f64>) -> HashMap<String, f64> {
    let mut tf = HashMap::new();
    for token in tokens {
        *tf.entry(token.clone()).or_insert(0_u64) += 1;
    }
    let total = tokens.len().max(1) as f64;
    tf.into_iter()
        .filter_map(|(token, count)| {
            let weight = (count as f64 / total) * idf.get(&token).copied().unwrap_or(0.0);
            (weight > 0.0).then_some((token, weight))
        })
        .collect()
}

fn cosine(left: &HashMap<String, f64>, right: &HashMap<String, f64>) -> f64 {
    if left.is_empty() || right.is_empty() {
        return 0.0;
    }
    let mut dot = 0.0;
    for (token, left_weight) in left {
        if let Some(right_weight) = right.get(token) {
            dot += left_weight * right_weight;
        }
    }
    let left_norm = left.values().map(|value| value * value).sum::<f64>().sqrt();
    let right_norm = right
        .values()
        .map(|value| value * value)
        .sum::<f64>()
        .sqrt();
    if left_norm == 0.0 || right_norm == 0.0 {
        0.0
    } else {
        dot / (left_norm * right_norm)
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use tempfile::TempDir;
    use uuid::Uuid;

    use super::*;
    use crate::application::{
        DesktopApplicationConfig, DesktopHost, DesktopHostAction, DesktopHostContext,
        DesktopHostError, DesktopHostResult, DesktopProjectCreate, DesktopTaskCreate,
    };

    #[derive(Default)]
    struct TestHost;

    impl DesktopHost for TestHost {
        fn execute(
            &self,
            _context: &DesktopHostContext,
            _action: DesktopHostAction,
        ) -> Result<DesktopHostResult, DesktopHostError> {
            Ok(DesktopHostResult::Completed)
        }
    }

    fn temp_app() -> (TempDir, DesktopApplication) {
        let dir = TempDir::new().unwrap();
        let config =
            DesktopApplicationConfig::new(dir.path(), format!("search-{}", Uuid::new_v4()))
                .unwrap();
        let app = DesktopApplication::bootstrap(config, Arc::new(TestHost)).unwrap();
        (dir, app)
    }

    #[test]
    fn substring_hit_ranks_above_unrelated_titles() {
        let (_dir, app) = temp_app();
        let project = app
            .create_project(DesktopProjectCreate::new("Alpha"))
            .unwrap();
        let matching = app
            .create_task(DesktopTaskCreate::new(
                Some(project.id.clone()),
                "Fix login timeout",
            ))
            .unwrap();
        app.create_task(DesktopTaskCreate::new(
            Some(project.id.clone()),
            "Unrelated gardening notes",
        ))
        .unwrap();

        let results = app.search_sessions("login", 10).unwrap();
        assert!(!results.is_empty());
        assert_eq!(results[0].task_id, matching.id);
        assert!(!results[0].highlights.is_empty());
        assert!(results.iter().all(|result| result.title.contains("login")
            || result.score > 0.0 && result.task_id == matching.id));
    }

    #[test]
    fn empty_query_returns_no_results() {
        let (_dir, app) = temp_app();
        assert!(app.search_sessions("   ", 10).unwrap().is_empty());
    }
}
