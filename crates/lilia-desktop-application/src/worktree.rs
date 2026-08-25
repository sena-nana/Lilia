use std::ffi::OsString;
use std::path::{Path, PathBuf};

use lilia_contracts::{ProjectId, TaskId};

use crate::{DesktopApplication, DesktopApplicationError, DesktopEventKind};

pub use lilia_feature_worktree::*;

impl DesktopApplication {
    pub fn set_initial_worktree_intent(
        &self,
        task_id: &TaskId,
        selection: Option<&DesktopInitialWorktreeSelection>,
    ) -> Result<(), DesktopApplicationError> {
        let worktrees = self
            .inner
            .worktrees
            .lock()
            .map_err(|_| DesktopApplicationError::StateUnavailable("worktrees"))?;
        if let Some(selection) = selection {
            worktrees.save_initial_intent(task_id, selection)?;
        } else {
            worktrees.clear_initial_intent(task_id)?;
        }
        Ok(())
    }

    pub fn initial_worktree_intent(
        &self,
        task_id: &TaskId,
    ) -> Result<Option<DesktopInitialWorktreeSelection>, DesktopApplicationError> {
        Ok(self
            .inner
            .worktrees
            .lock()
            .map_err(|_| DesktopApplicationError::StateUnavailable("worktrees"))?
            .initial_intent(task_id)?)
    }

    pub fn retry_initial_worktree(
        &self,
        task_id: &TaskId,
    ) -> Result<bool, DesktopApplicationError> {
        let Some(selection) = self.initial_worktree_intent(task_id)? else {
            return Ok(false);
        };
        if self.task_worktree(task_id)?.is_none() {
            match selection {
                DesktopInitialWorktreeSelection::Create => {
                    self.create_task_worktree(task_id, None)?;
                }
                DesktopInitialWorktreeSelection::Existing(path) => {
                    self.attach_task_worktree(task_id, &path)?;
                }
            }
        }
        self.set_initial_worktree_intent(task_id, None)?;
        Ok(true)
    }

    pub(crate) fn ensure_initial_worktree_ready(
        &self,
        task_id: &TaskId,
    ) -> Result<(), DesktopApplicationError> {
        if self.initial_worktree_intent(task_id)?.is_some() {
            return Err(DesktopWorktreeError::InitialPreparationPending(task_id.clone()).into());
        }
        Ok(())
    }

    pub fn task_workspace_path(
        &self,
        task_id: &TaskId,
    ) -> Result<Option<String>, DesktopApplicationError> {
        if let Some(worktree) = self.task_worktree(task_id)? {
            return Ok(Some(worktree.worktree_path));
        }
        let task = self.get_task(task_id)?;
        let Some(project_id) = task.project_id else {
            return Ok(None);
        };
        let project = self.get_project(&project_id)?;
        Ok(project.workspace_path.or_else(|| {
            project
                .git_workspace
                .and_then(|workspace| workspace.worktree_path)
        }))
    }

    pub fn task_worktree(
        &self,
        task_id: &TaskId,
    ) -> Result<Option<DesktopTaskWorktree>, DesktopApplicationError> {
        self.get_task(task_id)?;
        Ok(self
            .inner
            .worktrees
            .lock()
            .map_err(|_| DesktopApplicationError::StateUnavailable("worktrees"))?
            .active_for_task(task_id)?)
    }

    pub fn list_task_repository_worktrees(
        &self,
        task_id: &TaskId,
    ) -> Result<Vec<DesktopWorktreeListItem>, DesktopApplicationError> {
        let (_, _, base) = self.task_repository(task_id)?;
        let base = canonical_path(&base, "base repository")?;
        let base_text = normalized_path(&base);
        let bound_paths = self
            .inner
            .worktrees
            .lock()
            .map_err(|_| DesktopApplicationError::StateUnavailable("worktrees"))?
            .active_bound_paths()?;
        Ok(list_git_worktrees(&base)?
            .into_iter()
            .map(|item| {
                let item_path = canonical_path(Path::new(&item.path), "worktree")
                    .map(|path| normalized_path(&path))
                    .unwrap_or(item.path);
                DesktopWorktreeListItem {
                    is_main: item_path == base_text,
                    is_task_bound: bound_paths.contains(&item_path),
                    path: item_path,
                    head: item.head,
                    branch: item.branch,
                    bare: item.bare,
                    detached: item.detached,
                    prunable: item.prunable,
                    locked: item.locked,
                }
            })
            .collect())
    }

    pub fn create_task_worktree(
        &self,
        task_id: &TaskId,
        parent_directory: Option<&Path>,
    ) -> Result<DesktopTaskWorktree, DesktopApplicationError> {
        let (task, project_id, base) = self.task_repository(task_id)?;
        if self.task_worktree(task_id)?.is_some() {
            return Err(DesktopWorktreeError::AlreadyBound(task_id.clone()).into());
        }
        ensure_git_repo(&base)?;
        let base = canonical_path(&base, "base repository")?;
        let base_branch = current_branch(&base)?;
        let preferred_parent = self.worktree_parent_directory_preference()?;
        let parent = parent_directory
            .map(Path::to_path_buf)
            .or(preferred_parent)
            .unwrap_or_else(|| {
                base.parent()
                    .map(Path::to_path_buf)
                    .unwrap_or_else(|| base.clone())
            });
        let parent = canonical_path(&parent, "worktree parent directory")?;
        let slug = task_title_slug(&task.title, task_id);
        let target = unique_worktree_target(&parent, &slug);
        let branch = unique_branch_name(&base, &slug)?;
        run_git(
            &base,
            &[
                OsString::from("worktree"),
                OsString::from("add"),
                OsString::from("-b"),
                OsString::from(&branch),
                target.as_os_str().to_owned(),
                OsString::from(&base_branch),
            ],
        )?;
        let worktree = match canonical_path(&target, "created worktree") {
            Ok(path) => path,
            Err(error) => {
                rollback_created_worktree(&base, &target, &branch);
                return Err(error.into());
            }
        };
        let base_text = normalized_path(&base);
        let worktree_text = normalized_path(&worktree);
        let saved = self
            .inner
            .worktrees
            .lock()
            .map_err(|_| DesktopApplicationError::StateUnavailable("worktrees"))?
            .save_active(
                task_id,
                project_id.as_ref(),
                &base_text,
                &worktree_text,
                &branch,
                &base_branch,
            );
        let saved = match saved {
            Ok(saved) => saved,
            Err(error) => {
                rollback_created_worktree(&base, &worktree, &branch);
                return Err(error.into());
            }
        };
        self.emit_event(DesktopEventKind::WorktreeChanged {
            task_id: task_id.clone(),
        });
        Ok(saved)
    }

    pub fn attach_task_worktree(
        &self,
        task_id: &TaskId,
        worktree_path: &Path,
    ) -> Result<DesktopTaskWorktree, DesktopApplicationError> {
        let (_, project_id, base) = self.task_repository(task_id)?;
        if self.task_worktree(task_id)?.is_some() {
            return Err(DesktopWorktreeError::AlreadyBound(task_id.clone()).into());
        }
        ensure_git_repo(&base)?;
        ensure_git_repo(worktree_path)?;
        let base = canonical_path(&base, "base repository")?;
        let worktree = canonical_path(worktree_path, "worktree")?;
        if base == worktree {
            return Err(DesktopWorktreeError::MainRepositoryCannotBeAttached.into());
        }
        let worktree_text = normalized_path(&worktree);
        let registered = list_git_worktrees(&base)?
            .into_iter()
            .find(|item| {
                canonical_path(Path::new(&item.path), "worktree").is_ok_and(|path| path == worktree)
            })
            .ok_or_else(|| DesktopWorktreeError::NotRegistered(worktree_text.clone()))?;
        let branch = registered
            .branch
            .filter(|branch| !branch.trim().is_empty())
            .ok_or_else(|| DesktopWorktreeError::Detached(worktree_text.clone()))?;
        let base_branch = current_branch(&base)?;
        let saved = self
            .inner
            .worktrees
            .lock()
            .map_err(|_| DesktopApplicationError::StateUnavailable("worktrees"))?
            .save_active(
                task_id,
                project_id.as_ref(),
                &normalized_path(&base),
                &worktree_text,
                &branch,
                &base_branch,
            )?;
        self.emit_event(DesktopEventKind::WorktreeChanged {
            task_id: task_id.clone(),
        });
        Ok(saved)
    }

    pub fn clear_task_worktree(&self, task_id: &TaskId) -> Result<bool, DesktopApplicationError> {
        self.get_task(task_id)?;
        let changed = self
            .inner
            .worktrees
            .lock()
            .map_err(|_| DesktopApplicationError::StateUnavailable("worktrees"))?
            .mark_status(task_id, DesktopWorktreeStatus::Removed)?;
        if changed {
            self.emit_event(DesktopEventKind::WorktreeChanged {
                task_id: task_id.clone(),
            });
        }
        Ok(changed)
    }

    pub fn cleanup_task_worktree_and_archive(
        &self,
        task_id: &TaskId,
    ) -> Result<DesktopWorktreeMergeResult, DesktopApplicationError> {
        let worktree = self
            .task_worktree(task_id)?
            .ok_or_else(|| DesktopWorktreeError::NotBound(task_id.clone()))?;
        let base = PathBuf::from(&worktree.base_repo_path);
        let worktree_path = PathBuf::from(&worktree.worktree_path);
        ensure_git_repo(&base)?;
        ensure_git_repo(&worktree_path)?;
        ensure_clean(&base, "base repository")?;
        ensure_clean(&worktree_path, "worktree")?;
        if branch_unique_commit_count(&worktree_path, &worktree.base_branch)? > 0 {
            return Err(DesktopWorktreeError::UnmergedCommits.into());
        }
        remove_worktree_and_branch(&base, &worktree_path, &worktree.branch_name)?;
        self.finish_worktree_archive(
            task_id,
            DesktopWorktreeStatus::Removed,
            false,
            "Removed the worktree without unique commits and archived the task",
        )
    }

    pub fn merge_task_worktree_and_archive(
        &self,
        task_id: &TaskId,
    ) -> Result<DesktopWorktreeMergeResult, DesktopApplicationError> {
        let worktree = self
            .task_worktree(task_id)?
            .ok_or_else(|| DesktopWorktreeError::NotBound(task_id.clone()))?;
        let base = PathBuf::from(&worktree.base_repo_path);
        let worktree_path = PathBuf::from(&worktree.worktree_path);
        ensure_git_repo(&base)?;
        ensure_git_repo(&worktree_path)?;
        ensure_clean(&base, "base repository")?;
        ensure_clean(&worktree_path, "worktree")?;
        if branch_unique_commit_count(&worktree_path, &worktree.base_branch)? == 0 {
            return Err(DesktopWorktreeError::NoUniqueCommits.into());
        }
        if current_branch(&base)? != worktree.base_branch {
            run_git_text(&base, &["checkout", &worktree.base_branch])?;
        }
        run_git_text(&base, &["merge", "--no-ff", &worktree.branch_name])?;
        remove_worktree_and_branch(&base, &worktree_path, &worktree.branch_name)?;
        self.finish_worktree_archive(
            task_id,
            DesktopWorktreeStatus::Merged,
            true,
            "Merged the worktree branch, removed the worktree, and archived the task",
        )
    }

    fn task_repository(
        &self,
        task_id: &TaskId,
    ) -> Result<(lilia_contracts::ProductTask, Option<ProjectId>, PathBuf), DesktopApplicationError>
    {
        let task = self.get_task(task_id)?;
        let project_id = task
            .project_id
            .clone()
            .ok_or_else(|| DesktopWorktreeError::TaskHasNoProject(task_id.clone()))?;
        let project = self.get_project(&project_id)?;
        let workspace = project
            .workspace_path
            .as_deref()
            .or_else(|| {
                project
                    .git_workspace
                    .as_ref()
                    .and_then(|workspace| workspace.worktree_path.as_deref())
            })
            .filter(|path| !path.trim().is_empty())
            .ok_or_else(|| DesktopWorktreeError::ProjectHasNoWorkspace(project_id.clone()))?;
        Ok((task, Some(project_id), PathBuf::from(workspace)))
    }

    fn finish_worktree_archive(
        &self,
        task_id: &TaskId,
        status: DesktopWorktreeStatus,
        merged: bool,
        message: &str,
    ) -> Result<DesktopWorktreeMergeResult, DesktopApplicationError> {
        self.inner
            .worktrees
            .lock()
            .map_err(|_| DesktopApplicationError::StateUnavailable("worktrees"))?
            .mark_status(task_id, status)?;
        let task = self.get_task(task_id)?;
        let archived = !task.archived;
        if archived {
            self.set_task_archived(task_id, true)?;
        }
        self.emit_event(DesktopEventKind::WorktreeChanged {
            task_id: task_id.clone(),
        });
        Ok(DesktopWorktreeMergeResult {
            merged,
            removed: true,
            archived,
            message: message.to_owned(),
        })
    }
}

fn list_git_worktrees(base_repo_path: &Path) -> Result<Vec<GitWorktree>, DesktopWorktreeError> {
    ensure_git_repo(base_repo_path)?;
    let output = run_git_text(base_repo_path, &["worktree", "list", "--porcelain"])?;
    Ok(parse_worktree_porcelain(&output))
}

#[cfg(test)]
mod tests {
    use std::fs;
    use std::sync::Arc;

    use lilia_contracts::{ProductEntity, ProductTask, Project};
    use lilia_service::ServiceAuthority;
    use lilia_storage::Db;
    use uuid::Uuid;
    use tempfile::TempDir;

    use super::*;
    use crate::{
        DesktopApplicationConfig, DesktopHost, DesktopHostAction, DesktopHostContext,
        DesktopHostError, DesktopHostResult,
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

    fn initialize_repository(root: &Path) {
        fs::create_dir_all(root).unwrap();
        run_git_text(root, &["init", "-b", "main"]).unwrap();
        run_git_text(root, &["config", "user.email", "native@example.invalid"]).unwrap();
        run_git_text(root, &["config", "user.name", "Native Test"]).unwrap();
        run_git_text(root, &["config", "core.autocrlf", "false"]).unwrap();
        fs::write(root.join("README.md"), "native\n").unwrap();
        run_git_text(root, &["add", "README.md"]).unwrap();
        run_git_text(root, &["commit", "-m", "initial"]).unwrap();
    }

    fn application(repo: &Path) -> (DesktopApplication, TaskId) {
        let instance_id = Uuid::new_v4();
        let authority = ServiceAuthority::bootstrap_in_memory_named(
            format!("test:desktop-worktree:{instance_id}"),
            format!("desktop-worktree-test:{instance_id}"),
        )
        .unwrap();
        let project_id = ProjectId::new("worktree-project").unwrap();
        let mut project = Project::new(project_id.clone(), "Worktree project").unwrap();
        project.workspace_path = Some(normalized_path(repo));
        authority
            .client()
            .unwrap()
            .products()
            .create_entity(ProductEntity::Project(project))
            .unwrap();
        let task_id = TaskId::new("worktree-task").unwrap();
        authority
            .client()
            .unwrap()
            .products()
            .create_entity(ProductEntity::Task(
                ProductTask::new(task_id.clone(), Some(project_id), "Native worktree").unwrap(),
            ))
            .unwrap();
        let application = DesktopApplication::from_authority(
            DesktopApplicationConfig::new("C:/lilia/worktree-test", "liliacode.test").unwrap(),
            authority,
            Arc::new(NoopHost),
        )
        .unwrap();
        (application, task_id)
    }

    #[test]
    fn parses_porcelain_worktree_state() {
        let worktrees = parse_worktree_porcelain(
            "worktree D:/repo\nHEAD abc\nbranch refs/heads/main\n\nworktree D:/repo-wt\nHEAD def\nbranch refs/heads/lilia/task\nlocked reason\n",
        );

        assert_eq!(worktrees.len(), 2);
        assert_eq!(worktrees[0].branch.as_deref(), Some("main"));
        assert_eq!(worktrees[1].path, "D:/repo-wt");
        assert_eq!(worktrees[1].branch.as_deref(), Some("lilia/task"));
        assert!(worktrees[1].locked);
    }

    #[test]
    fn create_merge_remove_and_archive_uses_real_git_and_product_state() {
        let root = TempDir::new().unwrap();
        let repo = root.path().join("repo");
        initialize_repository(&repo);
        let (application, task_id) = application(&repo);

        let binding = application
            .create_task_worktree(&task_id, Some(root.path()))
            .unwrap();
        assert_eq!(
            application
                .task_workspace_path(&task_id)
                .unwrap()
                .as_deref(),
            Some(binding.worktree_path.as_str())
        );
        assert!(Path::new(&binding.worktree_path).is_dir());
        assert!(binding.branch_name.starts_with("lilia/native-worktree-"));
        let listed = application
            .list_task_repository_worktrees(&task_id)
            .unwrap();
        assert_eq!(listed.iter().filter(|item| item.is_main).count(), 1);
        assert_eq!(listed.iter().filter(|item| item.is_task_bound).count(), 1);

        let worktree_path = PathBuf::from(&binding.worktree_path);
        fs::write(worktree_path.join("native.txt"), "complete\n").unwrap();
        run_git_text(&worktree_path, &["add", "native.txt"]).unwrap();
        run_git_text(&worktree_path, &["commit", "-m", "native worktree"]).unwrap();

        let result = application
            .merge_task_worktree_and_archive(&task_id)
            .unwrap();

        assert!(result.merged);
        assert!(result.removed);
        assert!(result.archived);
        assert!(!worktree_path.exists());
        assert!(repo.join("native.txt").is_file());
        assert!(application.get_task(&task_id).unwrap().archived);
        assert_eq!(application.task_worktree(&task_id).unwrap(), None);
    }

    #[test]
    fn initial_worktree_intent_survives_reopen_until_explicitly_cleared() {
        let root = TempDir::new().unwrap();
        let database = root.path().join("worktrees.db");
        let task_id = TaskId::new("pending-worktree").unwrap();
        let selection = DesktopInitialWorktreeSelection::Existing(root.path().join("existing"));
        {
            let store = DesktopWorktreeStore::from_db(Db::open(&database).unwrap()).unwrap();
            store.save_initial_intent(&task_id, &selection).unwrap();
        }

        let store = DesktopWorktreeStore::from_db(Db::open(&database).unwrap()).unwrap();
        assert_eq!(store.initial_intent(&task_id).unwrap(), Some(selection));
        assert!(store.clear_initial_intent(&task_id).unwrap());
        assert_eq!(store.initial_intent(&task_id).unwrap(), None);
    }

    #[test]
    fn pending_initial_worktree_blocks_turns_and_retry_clears_the_gate() {
        let root = TempDir::new().unwrap();
        let repo = root.path().join("repo");
        initialize_repository(&repo);
        let (application, task_id) = application(&repo);
        application
            .set_initial_worktree_intent(&task_id, Some(&DesktopInitialWorktreeSelection::Create))
            .unwrap();

        assert!(matches!(
            application.start_composer_turn(&task_id),
            Err(DesktopApplicationError::Worktree(
                DesktopWorktreeError::InitialPreparationPending(ref pending)
            )) if pending == &task_id
        ));
        assert!(application.retry_initial_worktree(&task_id).unwrap());
        assert_eq!(application.initial_worktree_intent(&task_id).unwrap(), None);
        assert!(application.task_worktree(&task_id).unwrap().is_some());
    }
}
