package com.lilia.remote

import org.json.JSONArray
import org.json.JSONObject

object RemotePayloadParser {
    private val branchableTimelineStatuses = setOf("success", "completed", "done")
    private val timelineDetailWhitespace = Regex("\\s+")
    private const val TIMELINE_DETAIL_VALUE_MAX_LENGTH = 600

    fun parseBridgeStatus(response: JSONObject): RemoteBridgeStatus {
        RemoteEnvelopeAdapter.throwIfError(response, "读取桥接状态失败")
        val status = response.optJSONObject("status") ?: error("桥接状态缺少 status")
        return RemoteBridgeStatus(
            hostEnabled = status.optBoolean("hostEnabled"),
            state = status.optString("state", "disconnected"),
            pcName = status.optString("pcName", DEFAULT_PC_DISPLAY_NAME),
            capabilities = parseCapabilities(status.optJSONObject("capabilities")),
        )
    }

    private fun parseCapabilities(capabilities: JSONObject?): RemoteCapabilities {
        if (capabilities == null) return RemoteCapabilities()
        return RemoteCapabilities(
            supportsTaskInbox = capabilities.optBoolean("supportsTaskInbox", true),
            supportsTimelineSubscription = capabilities.optBoolean("supportsTimelineSubscription", true),
            supportsTimelinePagination = capabilities.optBoolean("supportsTimelinePagination", false),
            supportsChatSend = capabilities.optBoolean("supportsChatSend", true),
            supportsInteractionResponse = capabilities.optBoolean("supportsInteractionResponse", true),
            supportsInterrupt = capabilities.optBoolean("supportsInterrupt", true),
            supportsAgentWire = capabilities.optBoolean("supportsAgentWire", false),
            supportsSessionFork = capabilities.optBoolean("supportsSessionFork", false),
            supportsProcessSession = capabilities.optBoolean("supportsProcessSession", false),
        )
    }

    fun parseProviderStatus(payload: JSONObject): RemoteProviderStatus =
        RemoteProviderStatus(
            backend = payload.optString("backend").ifBlank { "unknown" },
            ready = payload.optBoolean("ready"),
        )

    fun parseTaskList(payload: JSONObject): List<RemoteTaskSummary> {
        val tasks = payload.optJSONArray("tasks") ?: JSONArray()
        return buildList {
            for (index in 0 until tasks.length()) {
                val task = tasks.getJSONObject(index)
                add(parseTaskSummary(task) ?: continue)
            }
        }
    }

    fun parseTaskState(
        taskId: String,
        taskPayload: JSONObject,
        pendingPayload: JSONObject,
    ): RemoteTaskState {
        val task = taskPayload.optJSONObject("task")
        val runtime = taskPayload.optJSONObject("runtime")
        val summary = parseTaskSummary(taskId, task)
        return RemoteTaskState(
            task = summary,
            runtimePhase = runtime?.nonBlankString("phase", summary.status) ?: summary.status,
            pendingInteraction = parsePending(summary.taskId, pendingPayload.optJSONArray("interactions") ?: JSONArray()),
        )
    }

    fun parseTaskDetail(
        taskId: String,
        taskPayload: JSONObject,
        timelinePayload: JSONObject,
        pendingPayload: JSONObject,
    ): RemoteTaskDetail {
        val state = parseTaskState(taskId, taskPayload, pendingPayload)
        val runtime = taskPayload.optJSONObject("runtime")
        val timelinePage = parseTimelinePagePayload(timelinePayload)
        return RemoteTaskDetail(
            task = state.task,
            relatedTasks = listOf(state.task),
            runtimePhase = state.runtimePhase,
            processSessionId = runtime?.optNullableString("processSessionId"),
            timeline = timelinePage.events,
            pendingInteraction = state.pendingInteraction,
            timelinePage = timelinePage,
        )
    }

    fun parseTimelinePayload(payload: JSONObject): List<RemoteTimelineItem> =
        parseTimelinePagePayload(payload).events

    fun parseTimelinePagePayload(payload: JSONObject): RemoteTimelinePage {
        val events = parseTimeline(payload.optJSONArray("events") ?: JSONArray())
        val page = payload.optJSONObject("page")
        return RemoteTimelinePage(
            events = events,
            beforeCursor = page?.optNullableString("beforeCursor"),
            afterCursor = page?.optNullableString("afterCursor"),
            hasMoreBefore = page?.optBoolean("hasMoreBefore") ?: false,
            hasMoreAfter = page?.optBoolean("hasMoreAfter") ?: false,
        )
    }

    fun mergeTimeline(
        existing: List<RemoteTimelineItem>,
        incoming: List<RemoteTimelineItem>,
    ): List<RemoteTimelineItem> {
        if (existing.isEmpty()) return incoming
        if (incoming.isEmpty()) return existing
        val merged = LinkedHashMap<String, RemoteTimelineItem>()
        existing.forEach { item -> merged[item.id] = item }
        incoming.forEach { item -> merged[item.id] = item }
        return merged.values.toList()
    }

    fun prependTimeline(
        existing: List<RemoteTimelineItem>,
        incoming: List<RemoteTimelineItem>,
    ): List<RemoteTimelineItem> {
        if (incoming.isEmpty()) return existing
        if (existing.isEmpty()) return incoming
        val merged = LinkedHashMap<String, RemoteTimelineItem>()
        incoming.forEach { item -> merged[item.id] = item }
        existing.forEach { item -> merged[item.id] = item }
        return merged.values.toList()
    }

    private fun parseTaskSummary(task: JSONObject, fallbackTaskId: String? = null): RemoteTaskSummary? {
        val taskId = task.firstNonBlankString("taskId", "id") ?: fallbackTaskId ?: return null
        return parseTaskSummary(taskId, task)
    }

    private fun parseTaskSummary(taskId: String, task: JSONObject?): RemoteTaskSummary =
        RemoteTaskSummary(
            taskId = task?.firstNonBlankString("taskId", "id") ?: taskId,
            title = task?.nonBlankString("title", "未命名任务") ?: "未命名任务",
            projectName = task?.firstNonBlankString("projectName", "projectId"),
            status = task?.nonBlankString("status", "waiting") ?: "waiting",
            dependsOn = parseStringArray(task?.optJSONArray("dependsOn")),
            lastActivity = formatRemoteTime(task?.optLong("createdAt", 0L) ?: 0L),
            pendingAction = null,
        )

    private fun parseStringArray(array: JSONArray?): List<String> {
        if (array == null) return emptyList()
        return buildList {
            for (index in 0 until array.length()) {
                val value = array.optString(index).takeIf { it.isNotBlank() } ?: continue
                add(value)
            }
        }
    }

    private fun parseTimeline(events: JSONArray): List<RemoteTimelineItem> {
        val rows = buildList {
            for (index in 0 until events.length()) {
                add(events.getJSONObject(index))
            }
        }
        return rows.mapIndexed { index, event ->
            RemoteTimelineItem(
                id = event.optString("id", "event-$index"),
                kind = event.optString("kind").ifBlank { "event" },
                title = timelineTitle(event),
                summary = timelineSummary(event),
                status = event.optString("status").ifBlank { "info" },
                role = timelineRole(event),
                details = timelineDetails(event),
                branchSourceTurnId = timelineBranchSourceTurnId(event),
                retryable = timelineRetryableError(event, rows),
            )
        }
    }

    private fun timelineRetryableError(event: JSONObject, events: List<JSONObject>): Boolean {
        if (event.optString("kind") != "error") return false
        if (retryContextHasPayload(event.optJSONObject("payload")?.optJSONObject("retryContext"))) {
            return true
        }
        val turnId = event.optString("turnId").takeIf { it.isNotBlank() } ?: return false
        return events.any { candidate ->
            candidate.optString("kind") == "message" &&
                candidate.optString("turnId") == turnId &&
                candidate.optJSONObject("payload")?.optString("role") == "user" &&
                retryContextHasPayload(candidate.optJSONObject("payload"))
        }
    }

    private fun retryContextHasPayload(payload: JSONObject?): Boolean {
        if (payload == null) return false
        if (payload.optString("content").isNotBlank()) return true
        val attachments = payload.optJSONArray("attachments")
        if (attachments != null && attachments.length() > 0) return true
        val conversationReferences = payload.optJSONArray("conversationReferences")
        return conversationReferences != null && conversationReferences.length() > 0
    }

    private fun timelineBranchSourceTurnId(event: JSONObject): String? {
        if (event.optString("kind") != "message") return null
        val payload = event.optJSONObject("payload") ?: return null
        if (payload.optString("role") != "assistant") return null
        if (event.optString("status") !in branchableTimelineStatuses) return null
        return event.optString("turnId").takeIf { it.isNotBlank() }
    }

    private fun timelineTitle(event: JSONObject): String {
        val title = event.optString("title").takeIf { it.isNotBlank() }
        if (title != null) return title
        if (event.optString("kind") == "message") {
            when (timelineRole(event)) {
                "assistant" -> return "助手"
                "user" -> return "用户"
            }
        }
        return timelineKindLabel(event.optString("kind"))
    }

    private fun timelineRole(event: JSONObject): String? =
        event.optJSONObject("payload")
            ?.optString("role")
            ?.takeIf { it.isNotBlank() }

    private fun timelineSummary(event: JSONObject): String {
        val payload = event.optJSONObject("payload")
        return event.optString("summary")
            .takeUnless { event.isNull("summary") }
            ?.takeIf { it.isNotBlank() }
            ?: payload?.firstNonBlankString(
                "content",
                "text",
                "summary",
                "message",
                "error",
                "reason",
                "details",
                "command",
                "cmd",
                "path",
                "query",
                "url",
                "result",
                "response",
                "output",
                "stdout",
                "stderr",
            )
            ?: ""
    }

    private fun timelineDetails(event: JSONObject): List<RemoteTimelineDetail> {
        val payload = event.optJSONObject("payload") ?: JSONObject()
        return listOfNotNull(
            timelineDetail(
                "工具",
                payload.firstNonBlankString("toolName", "name", "tool", "function", "hookName"),
            ),
            timelineDetail("命令", payload.firstNonBlankString("command", "cmd", "shellCommand")),
            timelineDetail("路径", payload.firstNonBlankString("file", "filePath", "path")),
            timelineDetail("查询", payload.firstNonBlankString("query", "url")),
            timelineDetail(
                "输入",
                payload.timelineValueString("input", "arguments", "args", "parameters", "params", "request"),
            ),
            timelineDetail(
                if (isFailureStatus(event.optString("status"))) "错误" else "输出",
                payload.timelineValueString("result", "response", "output", "stdout", "stderr"),
            ),
            timelineDetail("代码", payload.firstNonBlankString("code", "exitCode", "statusCode")),
            timelineDetail("轮次", event.optString("turnId").takeIf { it.isNotBlank() }),
        )
    }

    private fun timelineDetail(
        label: String,
        value: String?,
    ): RemoteTimelineDetail? {
        val normalized = value?.replace(timelineDetailWhitespace, " ")?.trim().orEmpty()
        if (normalized.isBlank()) return null
        return RemoteTimelineDetail(
            label = label,
            value = normalized.truncate(TIMELINE_DETAIL_VALUE_MAX_LENGTH),
        )
    }

    private fun JSONObject.timelineValueString(vararg names: String): String? {
        for (name in names) {
            if (!has(name) || isNull(name)) continue
            val value = opt(name) ?: continue
            val text = when (value) {
                is JSONObject -> value.toString()
                is JSONArray -> value.toString()
                else -> value.toString()
            }.takeIf { it.isNotBlank() }
            if (text != null) return text
        }
        return null
    }

    private fun isFailureStatus(status: String): Boolean =
        status == "failed" || status == "error" || status == "cancelled"

    private fun timelineKindLabel(kind: String): String =
        when (kind) {
            "message" -> "消息"
            "assistant_message" -> "助手"
            "user_message" -> "用户"
            "reasoning" -> "推理"
            "plan" -> "计划"
            "goal" -> "目标"
            "todo_list" -> "待办列表"
            "tool" -> "工具"
            "tool_call" -> "工具调用"
            "tool_result" -> "工具结果"
            "tool_consent" -> "工具请求"
            "ask_user" -> "问题"
            "architecture_change" -> "架构变更"
            "command" -> "命令"
            "subagent" -> "子代理"
            "file_change" -> "文件变更"
            "file_read" -> "文件读取"
            "search" -> "搜索"
            "web_fetch" -> "网页获取"
            "mcp" -> "MCP"
            "web_search" -> "网页搜索"
            "diagnostic" -> "诊断"
            "title_update" -> "标题更新"
            "error" -> "错误"
            "turn" -> "轮次"
            else -> kind.ifBlank { "事件" }
        }

    private fun parsePending(taskId: String, interactions: JSONArray): PendingInteraction? {
        val row = firstReplyableInteraction(interactions) ?: return null
        val payload = row.optJSONObject("payload") ?: JSONObject()
        val kind = row.optString("kind")
        return PendingInteraction(
            taskId = row.optString("taskId").ifBlank { taskId },
            requestId = row.optString("requestId"),
            kind = kind,
            backend = row.optString("backend"),
            title = interactionTitle(kind, payload),
            body = interactionBody(kind, payload),
            approveLabel = approveLabel(kind, payload),
            declineLabel = declineLabel(kind, payload),
            raw = row,
        )
    }

    private fun firstReplyableInteraction(interactions: JSONArray): JSONObject? {
        for (index in 0 until interactions.length()) {
            val row = interactions.optJSONObject(index) ?: continue
            if (row.optString("requestId").isBlank()) continue
            if (row.optString("kind").isBlank()) continue
            return row
        }
        return null
    }

    private fun interactionTitle(kind: String, payload: JSONObject): String =
        payload.optString("title")
            .ifBlank { payload.optString("displayName") }
            .ifBlank { payload.optString("serverName") }
            .ifBlank { payload.optString("toolName") }
            .ifBlank {
                when (kind) {
                    "permission_approval" -> "权限请求"
                    "architecture_change" -> "架构变更"
                    "mcp_elicitation" -> "MCP 请求"
                    "plan_approval" -> "计划审批"
                    "ask_user" -> "问题"
                    "tool_consent" -> "工具请求"
                    else -> "待处理操作"
                }
            }

    private fun interactionBody(kind: String, payload: JSONObject): String {
        val firstQuestion = payload.optJSONArray("questions")?.optJSONObject(0)
        val question = firstQuestion?.optString("question")?.takeIf { it.isNotBlank() }
        if (question != null) return question
        return payload.optString("reason")
            .ifBlank { payload.optString("description") }
            .ifBlank { payload.optString("message") }
            .ifBlank { architectureChangeSummary(payload) }
            .ifBlank {
                when (kind) {
                    "tool_consent" -> toolConsentSummary(payload)
                    "mcp_elicitation" -> "电脑端运行器正在请求来自 ${payload.optString("serverName", "MCP 服务")} 的输入。"
                    "permission_approval" -> "电脑端运行器正在请求额外访问权限。"
                    else -> "正在等待电脑端运行器的决定。"
                }
            }
    }

    private fun approveLabel(kind: String, payload: JSONObject): String {
        val firstQuestion = payload.optJSONArray("questions")?.optJSONObject(0)
        return firstQuestion?.optString("confirmLabel")?.takeIf { it.isNotBlank() }
            ?: when (kind) {
                "permission_approval", "tool_consent" -> "允许"
                "architecture_change" -> "应用"
                "mcp_elicitation" -> "接受"
                "plan_approval" -> "批准计划"
                "ask_user" -> "回答"
                else -> "批准"
            }
    }

    private fun declineLabel(kind: String, payload: JSONObject): String {
        val firstQuestion = payload.optJSONArray("questions")?.optJSONObject(0)
        return firstQuestion?.optString("cancelLabel")?.takeIf { it.isNotBlank() }
            ?: when (kind) {
                "permission_approval", "tool_consent" -> "拒绝"
                "mcp_elicitation" -> "拒绝"
                "plan_approval" -> "拒绝计划"
                else -> "拒绝"
            }
    }

    private fun toolConsentSummary(payload: JSONObject): String {
        val command = payload.optJSONObject("input")?.optString("cmd")?.takeIf { it.isNotBlank() }
            ?: payload.optString("blockedPath").takeIf { it.isNotBlank() }
        return command?.let { "工具输入：$it" } ?: "电脑端运行器想要使用工具。"
    }

    private fun architectureChangeSummary(payload: JSONObject): String {
        val changes = payload.optJSONArray("changes") ?: return ""
        if (changes.length() == 0) return ""
        return "请求 ${changes.length()} 项架构变更"
    }

    private fun JSONObject.optNullableString(name: String): String? {
        if (!has(name) || isNull(name)) return null
        return optString(name).takeIf { it.isNotBlank() }
    }

    private fun JSONObject.nonBlankString(name: String, fallback: String): String =
        optString(name).takeIf { it.isNotBlank() } ?: fallback

    private fun JSONObject.firstNonBlankString(vararg names: String): String? {
        for (name in names) {
            if (!has(name) || isNull(name)) continue
            val value = optString(name).takeIf { it.isNotBlank() }
            if (value != null) return value
        }
        return null
    }

    private fun String.truncate(maxLength: Int): String {
        if (length <= maxLength) return this
        return take(maxLength).trimEnd() + "…"
    }

    private fun formatRemoteTime(value: Long): String {
        if (value <= 0L) return "暂无活动时间"
        return java.text.DateFormat.getDateTimeInstance(
            java.text.DateFormat.SHORT,
            java.text.DateFormat.SHORT,
        ).format(java.util.Date(value))
    }
}
