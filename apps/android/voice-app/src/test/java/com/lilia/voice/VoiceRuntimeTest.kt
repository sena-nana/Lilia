package com.lilia.voice

import com.lilia.remote.RemoteTaskDetail
import com.lilia.remote.RemoteTaskSummary
import com.lilia.remote.RemoteTimelineItem
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test

class VoiceRuntimeTest {
    private val runtime = VoiceRuntime()

    @Test
    fun commandModeSearchEntersDictationAndDictationTextDoesNotExecuteCommand() {
        val initial = VoiceRuntimeState(tasks = listOf(task("task-1", "LiliaCode")))

        val dictating = runtime.reduce(initial, "搜索").state
        val result = runtime.reduce(dictating, "打开第一个")

        assertEquals(VoicePageId.SEARCH_RESULTS, result.state.pageId)
        assertEquals("打开第一个", result.state.searchQuery)
        assertEquals(VoiceAction.None, result.action)
    }

    @Test
    fun manifestAllowsActionsOnlyInMatchingPages() {
        val hub = VoiceRuntimeState()
        val selection = hub.copy(pageId = VoicePageId.PROJECT_SELECTION, tasks = listOf(task("task-1", "LiliaCode")))
        val detail = selection.copy(pageId = VoicePageId.PROJECT_DETAIL, selectedTask = detail(task("task-1", "LiliaCode")))

        assertFalse(runtime.isActionAllowed(hub, VoiceAction.OpenTask("task-1")))
        assertTrue(runtime.isActionAllowed(selection, VoiceAction.OpenTask("task-1")))
        assertTrue(runtime.isActionAllowed(detail, VoiceAction.StartProcess("task-1", "cargo test --workspace")))
    }

    @Test
    fun opensProjectByIndex() {
        val state = VoiceRuntimeState(
            pageId = VoicePageId.PROJECT_SELECTION,
            tasks = listOf(task("task-1", "First"), task("task-2", "Second")),
        )

        val result = runtime.reduce(state, "打开第二个")

        assertEquals(VoiceAction.OpenTask("task-2"), result.action)
    }

    @Test
    fun opensProjectByName() {
        val state = VoiceRuntimeState(
            pageId = VoicePageId.PROJECT_SELECTION,
            tasks = listOf(task("task-1", "LiliaCode"), task("task-2", "MutsukiCore")),
        )

        val result = runtime.reduce(state, "打开 MutsukiCore")

        assertEquals(VoiceAction.OpenTask("task-2"), result.action)
    }

    @Test
    fun stopRequiresConfirmationBeforeActionRuns() {
        val state = VoiceRuntimeState(
            pageId = VoicePageId.PROJECT_DETAIL,
            selectedTask = detail(task("task-1", "LiliaCode")),
        )

        val pending = runtime.reduce(state, "停止")
        val confirmed = runtime.reduce(pending.state, "确认")

        assertEquals(VoicePageId.CONFIRM, pending.state.pageId)
        assertEquals(VoiceAction.None, pending.action)
        assertEquals(VoiceAction.StopProcess("task-1"), confirmed.action)
        assertEquals(VoicePageId.PROJECT_DETAIL, confirmed.state.pageId)
    }

    @Test
    fun globalBackReturnsFromLogToProjectDetailAndHomeFromDetail() {
        val detail = detail(task("task-1", "LiliaCode"))
        val logState = VoiceRuntimeState(pageId = VoicePageId.LOG, selectedTask = detail)

        val backToDetail = runtime.reduce(logState, "返回").state
        val backHome = runtime.reduce(backToDetail, "返回").state

        assertEquals(VoicePageId.PROJECT_DETAIL, backToDetail.pageId)
        assertEquals(VoicePageId.HUB, backHome.pageId)
    }

    private fun task(taskId: String, title: String): RemoteTaskSummary =
        RemoteTaskSummary(
            taskId = taskId,
            title = title,
            projectName = title,
            status = "waiting",
            lastActivity = "now",
            pendingAction = null,
        )

    private fun detail(task: RemoteTaskSummary): RemoteTaskDetail =
        RemoteTaskDetail(
            task = task,
            relatedTasks = listOf(task),
            runtimePhase = "waiting",
            processSessionId = null,
            timeline = listOf(RemoteTimelineItem("event-1", "Started", "Ready", "completed")),
            pendingInteraction = null,
        )
}
