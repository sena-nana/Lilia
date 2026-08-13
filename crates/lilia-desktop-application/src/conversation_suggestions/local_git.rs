use std::process::Command;

use super::generation::{compact_line, truncate_chars};
use super::types::{
    DesktopSuggestionLocalGitContextRef, LocalGitContextSample, LOCAL_GIT_COMMIT_LIMIT,
    LOCAL_GIT_FILE_LIMIT, SAMPLE_TEXT_LIMIT,
};

pub(crate) fn load_local_git_context(cwd: &str) -> Result<Option<LocalGitContextSample>, String> {
    if git_command_stdout(cwd, ["rev-parse", "--is-inside-work-tree"])?.as_deref() != Some("true") {
        return Ok(None);
    }
    let Some(branch) = local_git_branch(cwd)? else {
        return Ok(None);
    };
    let recent_commits = git_command_stdout(cwd, ["log", "-n", "3", "--pretty=format:%h %s"])?
        .unwrap_or_default()
        .lines()
        .map(compact_line)
        .filter(|line| !line.is_empty())
        .take(LOCAL_GIT_COMMIT_LIMIT)
        .collect::<Vec<_>>();
    let changed_files = git_command_stdout(
        cwd,
        ["status", "--porcelain=v1", "--untracked-files=normal"],
    )?
    .unwrap_or_default()
    .lines()
    .filter_map(parse_local_git_status_line)
    .take(LOCAL_GIT_FILE_LIMIT)
    .collect::<Vec<_>>();
    if recent_commits.is_empty() && changed_files.is_empty() {
        return Ok(None);
    }
    let status = if changed_files.is_empty() {
        "clean".to_string()
    } else {
        format!("dirty: {} changed files", changed_files.len())
    };
    let fingerprint = format!(
        "{}|{}|{}",
        branch,
        recent_commits.join(" / "),
        changed_files.join(" / ")
    );
    Ok(Some(LocalGitContextSample {
        context: DesktopSuggestionLocalGitContextRef {
            id: "local-git-current".to_string(),
            branch: truncate_chars(&compact_line(&branch), SAMPLE_TEXT_LIMIT),
            status,
            changed_files,
            recent_commits,
        },
        fingerprint,
    }))
}

fn local_git_branch(cwd: &str) -> Result<Option<String>, String> {
    let branch =
        git_command_stdout(cwd, ["rev-parse", "--abbrev-ref", "HEAD"])?.unwrap_or_default();
    if !branch.is_empty() && branch != "HEAD" {
        return Ok(Some(branch));
    }
    git_command_stdout(cwd, ["rev-parse", "--short", "HEAD"])
}

fn git_command_stdout<const N: usize>(
    cwd: &str,
    args: [&str; N],
) -> Result<Option<String>, String> {
    let output = Command::new("git")
        .arg("-C")
        .arg(cwd)
        .args(args)
        .output()
        .map_err(|error| format!("读取本地 Git 上下文失败：{error}"))?;
    if !output.status.success() {
        return Ok(None);
    }
    let value = String::from_utf8_lossy(&output.stdout).trim().to_string();
    Ok((!value.is_empty()).then_some(value))
}

fn parse_local_git_status_line(line: &str) -> Option<String> {
    if line.len() < 4 {
        return None;
    }
    let status = line.get(0..2)?.trim();
    let path = line.get(3..)?.trim();
    if path.is_empty() {
        return None;
    }
    let status = if status.is_empty() {
        "modified"
    } else {
        status
    };
    Some(format!("{status} {path}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_status_line_keeps_status_and_path() {
        assert_eq!(
            parse_local_git_status_line("M  apps/desktop/src/main.ts").as_deref(),
            Some("M apps/desktop/src/main.ts")
        );
        assert_eq!(parse_local_git_status_line("??").as_deref(), None);
    }
}
