package com.lilia.remote

import org.json.JSONArray
import org.json.JSONObject

const val DEFAULT_PC_DISPLAY_NAME = "Lilia 电脑"

data class RemotePairingTicket(
    val protocolVersion: Int,
    val ticketId: String,
    val challenge: String,
    val endpointId: String,
    val pcName: String,
    val bridgeUrl: String,
    val rawUri: String,
)

data class SavedPc(
    val endpointId: String,
    val displayName: String,
    val protocolVersion: Int,
    val pairingUri: String,
    val bridgeUrl: String,
    val lastActiveAt: Long,
)

data class RemoteTaskSummary(
    val taskId: String,
    val title: String,
    val projectName: String?,
    val status: String,
    val dependsOn: List<String> = emptyList(),
    val lastActivity: String,
    val pendingAction: String?,
)

data class RemoteTimelineItem(
    val id: String,
    val title: String,
    val summary: String,
    val status: String,
    val kind: String = "event",
    val role: String? = null,
    val details: List<RemoteTimelineDetail> = emptyList(),
    val branchSourceTurnId: String? = null,
    val retryable: Boolean = false,
)

data class RemoteTimelineDetail(
    val label: String,
    val value: String,
)

data class RemoteTimelinePage(
    val events: List<RemoteTimelineItem>,
    val beforeCursor: String? = null,
    val afterCursor: String? = null,
    val hasMoreBefore: Boolean = false,
    val hasMoreAfter: Boolean = false,
)

enum class RemoteSessionForkMode(val wireValue: String, val label: String) {
    CONTINUE("continue", "从选中轮次继续"),
    FORK("fork", "从选中轮次分叉"),
}

data class RemoteBranchAnchor(
    val sourceTurnId: String,
    val mode: RemoteSessionForkMode,
)

data class RemoteSendMessageInput(
    val taskId: String,
    val content: String,
    val composer: JSONObject? = null,
    val attachments: JSONArray? = null,
    val conversationReferences: JSONArray? = null,
    val workflow: JSONObject? = null,
    val runtimeCommand: JSONObject? = null,
    val runtimeOptions: JSONObject? = null,
)

data class PendingInteraction(
    val taskId: String,
    val requestId: String,
    val kind: String,
    val backend: String,
    val title: String,
    val body: String,
    val approveLabel: String,
    val declineLabel: String,
    val raw: JSONObject,
)

data class RemoteTaskDetail(
    val task: RemoteTaskSummary,
    val relatedTasks: List<RemoteTaskSummary>,
    val runtimePhase: String,
    val processSessionId: String?,
    val timeline: List<RemoteTimelineItem>,
    val pendingInteraction: PendingInteraction?,
    val timelinePage: RemoteTimelinePage = RemoteTimelinePage(timeline),
)

data class RemoteTaskState(
    val task: RemoteTaskSummary,
    val runtimePhase: String,
    val pendingInteraction: PendingInteraction?,
)

data class RemoteBridgeStatus(
    val hostEnabled: Boolean,
    val state: String,
    val pcName: String,
    val capabilities: RemoteCapabilities = RemoteCapabilities(),
)

data class RemoteCapabilities(
    val supportsTaskInbox: Boolean = true,
    val supportsTimelineSubscription: Boolean = true,
    val supportsTimelinePagination: Boolean = false,
    val supportsChatSend: Boolean = true,
    val supportsInteractionResponse: Boolean = true,
    val supportsInterrupt: Boolean = true,
    val supportsAgentWire: Boolean = false,
)

data class RemoteProviderStatus(
    val backend: String,
    val ready: Boolean,
)
