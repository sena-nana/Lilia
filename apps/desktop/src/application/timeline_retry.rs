use lilia_contracts::TaskId;

use crate::application::composer::DesktopComposerTurnRequest;
use crate::application::{DesktopApplication, DesktopApplicationError, DesktopTurnDispatch};

pub use lilia_feature_timeline::{
    timeline_retry_context, TimelineRetryContext as DesktopTimelineRetryContext,
};

impl DesktopApplication {
    pub fn retry_task_timeline_event(
        &self,
        task_id: &TaskId,
        event_id: &str,
    ) -> Result<DesktopTurnDispatch, DesktopApplicationError> {
        let context = self.inner.timeline.retry_context(task_id, event_id)?;
        let composer = self.composer_state(task_id)?;
        let mut request = composer.turn_request();
        request.content = context.content;
        request.attachments = context.attachments;
        request.conversation_references = context.conversation_references;
        request.workflow = None;
        self.start_task_turn(request)
    }
}
