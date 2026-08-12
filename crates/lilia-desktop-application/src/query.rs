use lilia_contracts::ProjectId;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct ProjectQuery {
    pub include_archived: bool,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub enum DesktopTaskScope {
    #[default]
    All,
    Project(ProjectId),
    Inbox,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct TaskQuery {
    pub scope: DesktopTaskScope,
    pub include_archived: bool,
}

impl TaskQuery {
    pub fn for_project(project_id: ProjectId) -> Self {
        Self {
            scope: DesktopTaskScope::Project(project_id),
            include_archived: false,
        }
    }

    pub fn for_inbox() -> Self {
        Self {
            scope: DesktopTaskScope::Inbox,
            include_archived: false,
        }
    }

    pub fn for_project_or_inbox(project_id: Option<ProjectId>) -> Self {
        match project_id {
            Some(project_id) => Self::for_project(project_id),
            None => Self::for_inbox(),
        }
    }

    pub fn including_archived(mut self) -> Self {
        self.include_archived = true;
        self
    }
}
