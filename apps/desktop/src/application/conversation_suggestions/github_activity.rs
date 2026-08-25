use serde_json::Value as JsonValue;

use crate::ports::github::{github_client, github_request_headers};
use crate::application::{DesktopApplication, DesktopGitHubBindingMetadata};
use lilia_feature_suggestions::generation::{compact_line, truncate_chars};
use lilia_feature_suggestions::types::{
    GitHubActivitySample, GitHubRepoRef, GITHUB_ACTIVITY_LIMIT, GITHUB_EVENT_FETCH_LIMIT,
    SAMPLE_TEXT_LIMIT,
};

pub(crate) fn load_github_activity_context(
    app: &DesktopApplication,
    cwd: &str,
) -> Result<Option<(GitHubRepoRef, Vec<GitHubActivitySample>)>, String> {
    let Some(repo) = resolve_github_repo_from_cwd(cwd)? else {
        return Ok(None);
    };
    let (binding, token) = app
        .reconcile_github_binding(true)
        .map_err(|error| error.to_string())?;
    let Some(binding) = binding else {
        return Ok(None);
    };
    let Some(token) = token else {
        return Ok(None);
    };
    let activities = fetch_github_repo_activities(&repo, &binding, &token)?;
    if activities.is_empty() {
        return Ok(None);
    }
    Ok(Some((repo, activities)))
}

fn resolve_github_repo_from_cwd(cwd: &str) -> Result<Option<GitHubRepoRef>, String> {
    for key in ["remote.origin.url", "remote.upstream.url"] {
        if let Some(value) = git_config_value(cwd, key)? {
            if let Some(repo) = parse_github_repo_url(&value) {
                return Ok(Some(repo));
            }
        }
    }
    Ok(None)
}

fn git_config_value(cwd: &str, key: &str) -> Result<Option<String>, String> {
    let output = std::process::Command::new("git")
        .arg("-C")
        .arg(cwd)
        .arg("config")
        .arg("--get")
        .arg(key)
        .output()
        .map_err(|error| format!("读取 Git remote 失败：{error}"))?;
    if !output.status.success() {
        return Ok(None);
    }
    let value = String::from_utf8_lossy(&output.stdout).trim().to_string();
    Ok((!value.is_empty()).then_some(value))
}

pub(crate) fn parse_github_repo_url(input: &str) -> Option<GitHubRepoRef> {
    let trimmed = input.trim().trim_end_matches('/');
    let path = if let Some(rest) = trimmed.strip_prefix("https://github.com/") {
        rest
    } else if let Some(rest) = trimmed.strip_prefix("http://github.com/") {
        rest
    } else if let Some(rest) = trimmed.strip_prefix("git@github.com:") {
        rest
    } else {
        trimmed.strip_prefix("ssh://git@github.com/")?
    };
    let path = path.trim_end_matches(".git").trim_end_matches('/');
    let parts = path
        .split('/')
        .filter(|part| !part.trim().is_empty())
        .collect::<Vec<_>>();
    if parts.len() != 2 {
        return None;
    }
    let owner = parts[0].trim();
    let name = parts[1].trim();
    if owner.is_empty() || name.is_empty() {
        return None;
    }
    Some(GitHubRepoRef {
        owner: owner.to_string(),
        name: name.to_string(),
        full_name: format!("{owner}/{name}"),
    })
}

fn fetch_github_repo_activities(
    repo: &GitHubRepoRef,
    binding: &DesktopGitHubBindingMetadata,
    token: &str,
) -> Result<Vec<GitHubActivitySample>, String> {
    let client = github_client().map_err(|error| error.to_string())?;
    let url = format!(
        "https://api.github.com/repos/{}/{}/events",
        repo.owner, repo.name
    );
    let response = github_request_headers(
        client
            .get(url)
            .query(&[("per_page", GITHUB_EVENT_FETCH_LIMIT.to_string())]),
        Some(token),
    )
    .send()
    .map_err(|error| format!("读取 GitHub 仓库活动失败：{error}"))?;
    if response.status() == reqwest::StatusCode::UNAUTHORIZED
        || response.status() == reqwest::StatusCode::FORBIDDEN
    {
        return Err(format!("GitHub 绑定已失效（账号 {}）", binding.login));
    }
    if !response.status().is_success() {
        return Err(format!(
            "读取 GitHub 仓库活动失败：HTTP {}（{}）",
            response.status(),
            repo.full_name
        ));
    }
    let events = response
        .json::<Vec<JsonValue>>()
        .map_err(|error| format!("解析 GitHub 仓库活动失败：{error}"))?;
    Ok(normalize_github_events(repo, &events))
}

pub(crate) fn normalize_github_events(
    repo: &GitHubRepoRef,
    events: &[JsonValue],
) -> Vec<GitHubActivitySample> {
    events
        .iter()
        .filter_map(|event| normalize_github_event(repo, event))
        .take(GITHUB_ACTIVITY_LIMIT)
        .collect()
}

fn normalize_github_event(repo: &GitHubRepoRef, event: &JsonValue) -> Option<GitHubActivitySample> {
    let id = event
        .get("id")
        .and_then(|value| value.as_str())?
        .to_string();
    let event_type = event.get("type").and_then(|value| value.as_str())?;
    let payload = event.get("payload")?;
    match event_type {
        "PullRequestEvent" => {
            normalize_numbered_github_event(repo, id, payload, "pull_request", "pull_request", "PR")
        }
        "IssuesEvent" => {
            normalize_numbered_github_event(repo, id, payload, "issue", "issue", "Issue")
        }
        "PushEvent" => normalize_push_event(repo, id, payload),
        _ => None,
    }
}

fn normalize_numbered_github_event(
    repo: &GitHubRepoRef,
    event_id: String,
    payload: &JsonValue,
    payload_key: &str,
    kind: &str,
    label: &str,
) -> Option<GitHubActivitySample> {
    let action = payload
        .get("action")
        .and_then(|value| value.as_str())
        .unwrap_or("updated");
    let subject = payload.get(payload_key)?;
    let number = subject.get("number").and_then(|value| value.as_i64())?;
    let title = compact_line(
        subject
            .get("title")
            .and_then(|value| value.as_str())
            .unwrap_or(""),
    );
    if title.is_empty() {
        return None;
    }
    let state = subject
        .get("state")
        .and_then(|value| value.as_str())
        .unwrap_or("unknown");
    let url = subject
        .get("html_url")
        .and_then(|value| value.as_str())
        .map(str::to_string);
    let title = format!("{label} #{number}: {title}");
    let fingerprint = format!(
        "{}|{}|{}|{}|{}|{}",
        event_id, kind, action, number, title, state
    );
    Some(GitHubActivitySample {
        id: format!("gh-{event_id}"),
        repo_full_name: repo.full_name.clone(),
        kind: kind.to_string(),
        action: action.to_string(),
        title: truncate_chars(&title, SAMPLE_TEXT_LIMIT),
        url,
        details: vec![format!("state: {state}")],
        fingerprint,
    })
}

fn normalize_push_event(
    repo: &GitHubRepoRef,
    event_id: String,
    payload: &JsonValue,
) -> Option<GitHubActivitySample> {
    let branch = payload
        .get("ref")
        .and_then(|value| value.as_str())
        .unwrap_or("")
        .trim_start_matches("refs/heads/");
    if branch.is_empty() {
        return None;
    }
    let commits = payload
        .get("commits")
        .and_then(|value| value.as_array())
        .cloned()
        .unwrap_or_default();
    let summary = commits
        .iter()
        .filter_map(|commit| {
            commit
                .get("message")
                .and_then(|value| value.as_str())
                .map(compact_line)
                .filter(|value| !value.is_empty())
        })
        .take(2)
        .collect::<Vec<_>>()
        .join(" / ");
    let title = if summary.is_empty() {
        format!("Push to {branch}")
    } else {
        format!("Push to {branch}: {summary}")
    };
    let fingerprint = format!("{event_id}|push|{branch}|{title}");
    Some(GitHubActivitySample {
        id: format!("gh-{event_id}"),
        repo_full_name: repo.full_name.clone(),
        kind: "push".to_string(),
        action: "pushed".to_string(),
        title: truncate_chars(&title, SAMPLE_TEXT_LIMIT),
        url: Some(format!(
            "https://github.com/{}/commits/{branch}",
            repo.full_name
        )),
        details: vec![format!("commits: {}", commits.len())],
        fingerprint,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn parse_https_and_ssh_github_urls() {
        let https = parse_github_repo_url("https://github.com/sena-nana/LiliaCode.git").unwrap();
        assert_eq!(https.full_name, "sena-nana/LiliaCode");
        let ssh = parse_github_repo_url("git@github.com:sena-nana/LiliaCode.git").unwrap();
        assert_eq!(ssh.full_name, "sena-nana/LiliaCode");
    }

    #[test]
    fn normalize_pull_request_events() {
        let repo = GitHubRepoRef {
            owner: "sena-nana".into(),
            name: "LiliaCode".into(),
            full_name: "sena-nana/LiliaCode".into(),
        };
        let activities = normalize_github_events(
            &repo,
            &[json!({
                "id": "1",
                "type": "PullRequestEvent",
                "payload": {
                    "action": "opened",
                    "pull_request": {
                        "number": 12,
                        "title": "Wire suggestions",
                        "state": "open",
                        "html_url": "https://github.com/sena-nana/LiliaCode/pull/12"
                    }
                }
            })],
        );
        assert_eq!(activities.len(), 1);
        assert_eq!(activities[0].kind, "pull_request");
        assert!(activities[0].title.contains("#12"));
    }
}
