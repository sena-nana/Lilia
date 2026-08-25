use std::path::{Path, PathBuf};

use lilia_desktop_application::{
    DesktopDatabaseKind, DesktopImportItemKind, DesktopImportPlan, DesktopImportPlanItemStatus,
    DesktopImportPlanStatus, DesktopImportReport, DesktopImportReportItemStatus,
    DesktopImportReportStatus,
};
use lilia_kernel::JobId;
use sha2::{Digest, Sha256};

pub const LEGACY_INSTANCE_IDENTITY: &str = "liliacode.native-preview";

pub fn legacy_instance_identity(home: &Path) -> Result<String, String> {
    let absolute = if home.is_absolute() {
        home.to_path_buf()
    } else {
        std::env::current_dir()
            .map_err(|error| format!("failed to resolve imported data home: {error}"))?
            .join(home)
    };
    let mut normalized = absolute.to_string_lossy().replace('\\', "/");
    if cfg!(windows) {
        normalized.make_ascii_lowercase();
    }
    let digest = Sha256::digest(normalized.as_bytes());
    Ok(format!("{LEGACY_INSTANCE_IDENTITY}.{digest:x}"))
}

#[derive(Debug, Default)]
pub struct NativeDataImportState {
    pub source_home: Option<PathBuf>,
    pub plan: Option<DesktopImportPlan>,
    pub report: Option<DesktopImportReport>,
    pub credentials_confirmed: bool,
    pub restart_required: bool,
    pub busy: bool,
    pub error: Option<String>,
    active_job: Option<JobId>,
}

impl NativeDataImportState {
    /// Clears the previous plan before the host starts preparing a new one.
    /// Preparation can fail before a job exists, which is why it is separate
    /// from [`Self::begin`].
    pub fn reset_for_plan(&mut self, source_home: PathBuf) {
        self.source_home = Some(source_home);
        self.plan = None;
        self.report = None;
        self.credentials_confirmed = false;
    }

    pub fn begin(&mut self, job_id: JobId) {
        self.active_job = Some(job_id);
        self.busy = true;
        self.error = None;
    }

    pub fn finish_plan(
        &mut self,
        job_id: JobId,
        result: Result<DesktopImportPlan, String>,
    ) -> bool {
        if self.active_job != Some(job_id) {
            return false;
        }
        self.finish_operation();
        match result {
            Ok(plan) => {
                self.plan = Some(plan);
                self.error = None;
            }
            Err(error) => {
                self.plan = None;
                self.error = Some(error);
            }
        }
        true
    }

    pub fn finish_execute(&mut self, job_id: JobId, report: DesktopImportReport) -> bool {
        if self.active_job != Some(job_id) {
            return false;
        }
        self.finish_operation();
        self.error = None;
        self.report = Some(report);
        true
    }

    pub fn set_restart_required(&mut self, required: bool) {
        self.restart_required = required;
    }

    pub fn set_error(&mut self, message: impl Into<String>) {
        self.error = Some(message.into());
    }

    /// Reports a step that failed before or instead of producing a result.
    pub fn fail(&mut self, message: impl Into<String>) {
        self.finish_operation();
        self.error = Some(message.into());
    }

    pub fn toggle_credentials(&mut self) {
        if self.has_credentials() && !self.busy && self.report.is_none() {
            self.credentials_confirmed = !self.credentials_confirmed;
        }
    }

    pub fn reset(&mut self) {
        if !self.busy && !self.restart_required {
            *self = Self::default();
        }
    }

    pub fn can_execute(&self) -> bool {
        !self.busy
            && self.report.is_none()
            && self
                .plan
                .as_ref()
                .is_some_and(|plan| plan.status != DesktopImportPlanStatus::Blocked)
    }

    pub fn has_credentials(&self) -> bool {
        self.plan
            .as_ref()
            .is_some_and(|plan| !plan.credential_entries.is_empty())
    }

    fn finish_operation(&mut self) {
        self.active_job = None;
        self.busy = false;
    }
}

pub fn item_label(kind: &DesktopImportItemKind) -> &'static str {
    match kind {
        DesktopImportItemKind::Database(DesktopDatabaseKind::ProductProjections) => "项目与任务",
        DesktopImportItemKind::Database(DesktopDatabaseKind::Product) => "产品数据",
        DesktopImportItemKind::Database(DesktopDatabaseKind::AgentRuntime) => "Agent 运行记录",
        DesktopImportItemKind::Database(DesktopDatabaseKind::LegacyDesktop) => "旧版设置",
        DesktopImportItemKind::Credentials => "登录凭据",
    }
}

pub fn plan_status_label(status: DesktopImportPlanStatus) -> &'static str {
    match status {
        DesktopImportPlanStatus::Ready => "可以导入",
        DesktopImportPlanStatus::Empty => "没有可导入的数据",
        DesktopImportPlanStatus::Partial => "部分内容可以导入",
        DesktopImportPlanStatus::Blocked => "暂时无法导入",
    }
}

pub fn plan_item_status_label(status: DesktopImportPlanItemStatus) -> &'static str {
    match status {
        DesktopImportPlanItemStatus::Ready => "准备就绪",
        DesktopImportPlanItemStatus::MissingSource => "未找到",
        DesktopImportPlanItemStatus::Conflict => "目标已有数据",
        DesktopImportPlanItemStatus::Incompatible => "版本不兼容",
        DesktopImportPlanItemStatus::SourceBusy => "旧版正在使用",
        DesktopImportPlanItemStatus::InspectionFailed => "检查失败",
        DesktopImportPlanItemStatus::RequiresCredentialConfirmation => "等待确认",
    }
}

pub fn report_status_label(status: DesktopImportReportStatus) -> &'static str {
    match status {
        DesktopImportReportStatus::Completed => "导入完成",
        DesktopImportReportStatus::NothingToImport => "没有内容需要导入",
        DesktopImportReportStatus::AwaitingCredentialConfirmation => "等待凭据确认",
        DesktopImportReportStatus::PartialFailure => "部分内容未完成",
        DesktopImportReportStatus::Failed => "导入未完成",
    }
}

pub fn report_item_status_label(status: &DesktopImportReportItemStatus) -> String {
    match status {
        DesktopImportReportItemStatus::Copied => "已复制".to_owned(),
        DesktopImportReportItemStatus::MissingSource => "未找到，已跳过".to_owned(),
        DesktopImportReportItemStatus::Conflict => "目标已有数据，未覆盖".to_owned(),
        DesktopImportReportItemStatus::AwaitingCredentialConfirmation => "等待确认".to_owned(),
        DesktopImportReportItemStatus::SkippedCredentialDenied => "未选择，已跳过".to_owned(),
        DesktopImportReportItemStatus::CredentialsImported {
            imported,
            skipped,
            failed,
        } => format!("已复制 {imported}，跳过 {skipped}，失败 {failed}"),
        DesktopImportReportItemStatus::Failed => "失败".to_owned(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use lilia_desktop_application::{
        DesktopCredentialImportEntry, DesktopImportPlanItem, DesktopImportReportItem,
        DesktopImportReportStatus, DesktopLegacyConfigurationImport,
    };

    fn plan(status: DesktopImportPlanStatus, credentials: &[&str]) -> DesktopImportPlan {
        DesktopImportPlan {
            id: "plan-1".to_owned(),
            source_home: PathBuf::from("source"),
            source_instance_identity: LEGACY_INSTANCE_IDENTITY.to_owned(),
            target_home: PathBuf::from("target"),
            target_instance_identity: "lilia".to_owned(),
            credential_entries: credentials
                .iter()
                .map(|value| DesktopCredentialImportEntry {
                    source_service: LEGACY_INSTANCE_IDENTITY.to_owned(),
                    source_account: (*value).to_owned(),
                    target_key: (*value).to_owned(),
                })
                .collect(),
            legacy_configuration: DesktopLegacyConfigurationImport::default(),
            status,
            items: vec![DesktopImportPlanItem {
                kind: DesktopImportItemKind::Credentials,
                status: DesktopImportPlanItemStatus::RequiresCredentialConfirmation,
                files: Vec::new(),
                error: None,
            }],
        }
    }

    fn report() -> DesktopImportReport {
        DesktopImportReport {
            plan_id: "plan-1".to_owned(),
            source_home: PathBuf::from("source"),
            target_home: PathBuf::from("target"),
            status: DesktopImportReportStatus::Completed,
            items: vec![DesktopImportReportItem {
                kind: DesktopImportItemKind::Credentials,
                status: DesktopImportReportItemStatus::SkippedCredentialDenied,
                files: Vec::new(),
                error: None,
            }],
        }
    }

    #[test]
    fn stale_operation_results_cannot_replace_the_current_plan() {
        let mut state = NativeDataImportState::default();
        let first = JobId::new(1);
        let second = JobId::new(2);
        state.reset_for_plan(PathBuf::from("first"));
        state.begin(first);
        state.reset_for_plan(PathBuf::from("second"));
        state.begin(second);

        assert!(!state.finish_plan(first, Ok(plan(DesktopImportPlanStatus::Ready, &[]))));
        assert!(state.finish_plan(second, Ok(plan(DesktopImportPlanStatus::Ready, &[]))));
        assert_eq!(state.source_home, Some(PathBuf::from("second")));
        assert!(!state.busy);
    }

    #[test]
    fn credentials_require_a_manifest_and_a_separate_toggle() {
        let mut state = NativeDataImportState::default();
        let operation = JobId::new(1);
        state.reset_for_plan(PathBuf::from("source"));
        state.begin(operation);
        state.finish_plan(
            operation,
            Ok(plan(DesktopImportPlanStatus::Ready, &["agentkit.provider"])),
        );

        assert!(!state.credentials_confirmed);
        state.toggle_credentials();
        assert!(state.credentials_confirmed);
    }

    #[test]
    fn completed_report_prevents_accidental_repeat_until_reset() {
        let mut state = NativeDataImportState::default();
        let plan_operation = JobId::new(1);
        state.reset_for_plan(PathBuf::from("source"));
        state.begin(plan_operation);
        state.finish_plan(
            plan_operation,
            Ok(plan(DesktopImportPlanStatus::Ready, &[])),
        );
        assert!(state.can_execute());
        let execute_operation = JobId::new(2);
        state.begin(execute_operation);
        assert!(state.finish_execute(execute_operation, report()));
        assert!(!state.can_execute());

        state.reset();
        assert!(state.plan.is_none());
        assert!(state.report.is_none());
    }

    #[test]
    fn blocked_plan_never_exposes_execute() {
        let mut state = NativeDataImportState::default();
        let operation = JobId::new(1);
        state.reset_for_plan(PathBuf::from("source"));
        state.begin(operation);
        state.finish_plan(operation, Ok(plan(DesktopImportPlanStatus::Blocked, &[])));
        assert!(!state.can_execute());
    }
}
