package com.lilia.remote

import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.json.JSONObject
import org.junit.Test

class MainActivityStateTest {
    @Test
    fun savedPcConnectionMatchesRequiresSameEndpointAndBridge() {
        val pc = savedPc(endpointId = "pc-1", bridgeUrl = "http://192.168.1.12:41478")

        assertTrue(savedPcConnectionMatches(pc, pc.copy(displayName = "Renamed")))
        assertFalse(savedPcConnectionMatches(null, pc))
        assertFalse(savedPcConnectionMatches(pc.copy(endpointId = "pc-2"), pc))
        assertFalse(savedPcConnectionMatches(pc.copy(bridgeUrl = "http://192.168.1.13:41478"), pc))
    }

    @Test
    fun shouldIgnorePcResultIgnoresClearedOrStaleActivePcResults() {
        val pc = savedPc(endpointId = "pc-1", bridgeUrl = "http://192.168.1.12:41478")

        assertTrue(shouldIgnorePcResult(null, pc))
        assertFalse(shouldIgnorePcResult(pc, pc.copy(displayName = "Renamed")))
        assertTrue(shouldIgnorePcResult(pc.copy(endpointId = "pc-2"), pc))
        assertTrue(shouldIgnorePcResult(pc.copy(bridgeUrl = "http://192.168.1.13:41478"), pc))
    }

    @Test
    fun switchingActivePcResetsDerivedStateAndMakesOldResultsStale() {
        val oldPc = savedPc(endpointId = "pc-1", bridgeUrl = "http://192.168.1.12:41478")
        val newPc = savedPc(endpointId = "pc-2", bridgeUrl = "http://192.168.1.13:41478")

        assertTrue(shouldIgnorePcResult(newPc, oldPc))
        assertFalse(shouldIgnorePcResult(newPc, newPc))
    }

    @Test
    fun activePcSwitcherItemsKeepActivePcAndSameEndpointAlternates() {
        val active = savedPc(
            endpointId = "pc-shared",
            displayName = "Desk",
            bridgeUrl = "http://192.168.1.12:41478",
        )
        val studio = savedPc(
            endpointId = "pc-shared",
            displayName = "Studio",
            bridgeUrl = "http://192.168.1.44:41478",
        )

        val items = activePcSwitcherItems(active, listOf(studio, active))

        assertEquals(listOf("Desk", "Studio"), items.map { it.pc.displayName })
        assertEquals(listOf(true, false), items.map { it.active })
    }

    @Test
    fun unauthorizedRemoteFailureRequiresPairAgain() {
        val revoked = RemoteBridgeException(
            code = "unauthorized",
            message = "Android device is no longer trusted",
        )
        val offline = RemoteBridgeException(
            code = "unavailable",
            message = "Bridge offline",
            retryable = true,
        )

        assertTrue(shouldClearActivePcForFailure(revoked))
        assertFalse(shouldClearActivePcForFailure(offline))
        assertEquals("这台电脑已取消信任本手机，请重新配对。", UNTRUSTED_PC_MESSAGE)
    }

    @Test
    fun sessionForkRuntimeCommandUsesDesktopWireShape() {
        val command = RemoteRuntimeCommandAdapter.sessionFork(
            RemoteBranchAnchor("turn-source", RemoteSessionForkMode.CONTINUE),
        )

        assertSessionForkCommand(command, "turn-source", "continue")
    }

    @Test
    fun processSessionRuntimeCommandsUseDesktopWireShape() {
        val spawn = RemoteRuntimeCommandAdapter.processSpawn("cargo test --workspace")
        val stdin = RemoteRuntimeCommandAdapter.processWriteStdin("q")
        val kill = RemoteRuntimeCommandAdapter.processKill()

        assertEquals("process_session", spawn.getString("type"))
        assertEquals("spawn", spawn.getString("action"))
        assertEquals("cargo test --workspace", spawn.getString("command"))
        assertEquals("process_session", stdin.getString("type"))
        assertEquals("write_stdin", stdin.getString("action"))
        assertEquals("q", stdin.getString("stdin"))
        assertEquals("process_session", kill.getString("type"))
        assertEquals("kill", kill.getString("action"))
    }

    @Test
    fun taskRunBlockInfoRejectsBlockedCurrentTask() {
        val task = task("task-1", "Blocked task", "blocked")

        val info = taskRunBlockInfo(task, listOf(task))

        assertEquals("此任务已标记为阻塞，无法启动会话。", info?.reason)
        assertEquals(listOf("task-1"), info?.chain?.map { it.taskId })
    }

    @Test
    fun taskRunBlockInfoReportsDirectUnfinishedDependency() {
        val current = task("task-1", "Current", "waiting", dependsOn = listOf("dep-1"))
        val dependency = task("dep-1", "Design pass", "running")

        val info = taskRunBlockInfo(current, listOf(current, dependency))

        assertEquals("依赖任务完成前无法发送：Design pass（运行中）。", info?.reason)
        assertEquals(listOf("task-1", "dep-1"), info?.chain?.map { it.taskId })
    }

    @Test
    fun taskRunBlockInfoReportsTransitiveUnfinishedDependency() {
        val current = task("task-1", "Current", "waiting", dependsOn = listOf("dep-1"))
        val doneDependency = task("dep-1", "Implementation", "done", dependsOn = listOf("dep-2"))
        val runningDependency = task("dep-2", "Design pass", "running")

        val info = taskRunBlockInfo(current, listOf(current, doneDependency, runningDependency))

        assertEquals("依赖任务完成前无法发送：Design pass（运行中）。", info?.reason)
        assertEquals(listOf("task-1", "dep-1", "dep-2"), info?.chain?.map { it.taskId })
    }

    @Test
    fun taskRunBlockInfoAllowsCompletedDependencyChain() {
        val current = task("task-1", "Current", "waiting", dependsOn = listOf("dep-1"))
        val dependency = task("dep-1", "Design pass", "done")

        assertEquals(null, taskRunBlockInfo(current, listOf(current, dependency)))
    }

    @Test
    fun taskRunBlockInfoReportsDependencyCycle() {
        val current = task("task-1", "Current", "waiting", dependsOn = listOf("dep-1"))
        val dependency = task("dep-1", "Dependency", "done", dependsOn = listOf("task-1"))

        val info = taskRunBlockInfo(current, listOf(current, dependency))

        assertEquals("检测到任务依赖循环，无法启动会话。", info?.reason)
        assertEquals(listOf("task-1", "dep-1", "task-1"), info?.chain?.map { it.taskId })
    }

    @Test
    fun syncInboxTaskStatusUpdatesMatchingCardStatusAndPendingAction() {
        val tasks = listOf(
            taskSummary(status = "running"),
            taskSummary(taskId = "task-2", status = "waiting"),
        )
        val detail = taskDetail(status = "blocked", pendingActionTitle = "Permission request")

        val updated = syncInboxTaskStatus(tasks, detail)

        assertEquals("blocked", updated[0].status)
        assertEquals("Permission request", updated[0].pendingAction)
        assertEquals("waiting", updated[1].status)
        assertEquals(null, updated[1].pendingAction)
    }

    @Test
    fun syncInboxTaskStatusClearsResolvedPendingAction() {
        val tasks = listOf(taskSummary(status = "blocked", pendingAction = "Permission request"))
        val detail = taskDetail(status = "running")

        val updated = syncInboxTaskStatus(tasks, detail)

        assertEquals("running", updated[0].status)
        assertEquals(null, updated[0].pendingAction)
    }

    @Test
    fun olderTimelinePagePrependsEventsAndKeepsPaginationState() {
        val detail = taskDetail(
            timeline = listOf(
                timelineItem("event-2", "Second"),
                timelineItem("event-3", "Third"),
            ),
            page = RemoteTimelinePage(
                events = listOf(timelineItem("event-2", "Second"), timelineItem("event-3", "Third")),
                beforeCursor = "before-2",
                afterCursor = "after-3",
                hasMoreBefore = true,
                hasMoreAfter = false,
            ),
        )
        val olderPage = RemoteTimelinePage(
            events = listOf(timelineItem("event-1", "First"), timelineItem("event-2", "Duplicate")),
            beforeCursor = "before-1",
            hasMoreBefore = false,
            hasMoreAfter = true,
        )

        val updated = detail.withOlderTimelinePage(olderPage)

        assertEquals(listOf("event-1", "event-2", "event-3"), updated.timeline.map { it.id })
        assertEquals("before-1", updated.timelinePage.beforeCursor)
        assertEquals("after-3", updated.timelinePage.afterCursor)
        assertFalse(updated.timelinePage.hasMoreBefore)
        assertFalse(updated.timelinePage.hasMoreAfter)
    }

    private fun savedPc(
        endpointId: String,
        bridgeUrl: String,
        displayName: String = "Desk",
    ): SavedPc =
        SavedPc(
            endpointId = endpointId,
            displayName = displayName,
            protocolVersion = 1,
            pairingUri = "lilia-remote://pair?v=1",
            bridgeUrl = bridgeUrl,
            lastActiveAt = 1710000000000,
        )

    private fun task(
        taskId: String,
        title: String,
        status: String,
        dependsOn: List<String> = emptyList(),
    ): RemoteTaskSummary =
        RemoteTaskSummary(
            taskId = taskId,
            title = title,
            projectName = "Lilia",
            status = status,
            dependsOn = dependsOn,
            lastActivity = "now",
            pendingAction = null,
        )

    private fun taskSummary(
        taskId: String = "task-1",
        status: String = "running",
        pendingAction: String? = null,
    ): RemoteTaskSummary =
        RemoteTaskSummary(
            taskId = taskId,
            title = "Task $taskId",
            projectName = "Lilia",
            status = status,
            lastActivity = "Today",
            pendingAction = pendingAction,
        )

    private fun taskDetail(
        taskId: String = "task-1",
        status: String = "running",
        pendingActionTitle: String? = null,
        timeline: List<RemoteTimelineItem> = emptyList(),
        page: RemoteTimelinePage = RemoteTimelinePage(timeline),
    ): RemoteTaskDetail =
        RemoteTaskDetail(
            task = taskSummary(taskId = taskId, status = status),
            relatedTasks = listOf(taskSummary(taskId = taskId, status = status)),
            runtimePhase = status,
            processSessionId = null,
            timeline = timeline,
            pendingInteraction = pendingActionTitle?.let(::pendingInteraction),
            timelinePage = page,
        )

    private fun timelineItem(id: String, summary: String): RemoteTimelineItem =
        RemoteTimelineItem(
            id = id,
            title = id,
            summary = summary,
            status = "completed",
        )

    private fun pendingInteraction(title: String): PendingInteraction =
        PendingInteraction(
            taskId = "task-1",
            requestId = "request-1",
            kind = "tool_consent",
            backend = "codex",
            title = title,
            body = "Allow command?",
            approveLabel = "Allow",
            declineLabel = "Deny",
            raw = JSONObject(),
        )

    private fun assertSessionForkCommand(command: org.json.JSONObject, sourceTurnId: String, mode: String) {
        assertEquals("session_fork", command.getString("type"))
        assertEquals(true, command.getBoolean("excludeTurns"))
        assertEquals(sourceTurnId, command.getString("sourceTurnId"))
        assertEquals(mode, command.getString("mode"))
    }
}
