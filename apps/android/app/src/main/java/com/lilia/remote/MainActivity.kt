package com.lilia.remote

import android.Manifest
import android.content.Intent
import android.content.pm.PackageManager
import android.os.Build
import android.os.Bundle
import androidx.activity.ComponentActivity
import androidx.activity.compose.rememberLauncherForActivityResult
import androidx.activity.compose.setContent
import androidx.activity.result.contract.ActivityResultContracts
import androidx.compose.foundation.background
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.ColumnScope
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.rememberLazyListState
import androidx.compose.foundation.lazy.items
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material3.Button
import androidx.compose.material3.ButtonDefaults
import androidx.compose.material3.DropdownMenu
import androidx.compose.material3.DropdownMenuItem
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.OutlinedButton
import androidx.compose.material3.OutlinedTextField
import androidx.compose.material3.Surface
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.runtime.snapshotFlow
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.tooling.preview.Preview
import androidx.compose.ui.unit.dp
import androidx.core.content.ContextCompat
import kotlinx.coroutines.delay
import kotlinx.coroutines.flow.collect
import kotlinx.coroutines.launch

private const val DETAIL_LIVE_UPDATE_ERROR_MESSAGE = "实时更新暂时不可用，正在显示上次加载的任务。"
private const val DETAIL_SUBSCRIBE_INTERVAL_MS = 1_500L
private const val DETAIL_SNAPSHOT_FALLBACK_BASE_MS = 5_000L
private const val DETAIL_SNAPSHOT_FALLBACK_MAX_MS = 15_000L

class MainActivity : ComponentActivity() {
    private var pairingIntentUri by mutableStateOf<String?>(null)
    private var foregroundResumeToken by mutableStateOf(0L)
    private var didLeaveForeground = false

    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        val repository = RemoteRepository(applicationContext)
        pairingIntentUri = intent?.dataString
        setContent {
            LiliaRemoteApp(repository, pairingIntentUri, foregroundResumeToken)
        }
    }

    override fun onStart() {
        super.onStart()
        if (didLeaveForeground) {
            didLeaveForeground = false
            foregroundResumeToken += 1
        }
    }

    override fun onStop() {
        didLeaveForeground = true
        super.onStop()
    }

    override fun onNewIntent(intent: Intent) {
        super.onNewIntent(intent)
        setIntent(intent)
        pairingIntentUri = intent.dataString
    }
}

@Composable
fun LiliaRemoteApp(
    repository: RemoteRepository? = null,
    initialPairingUri: String? = null,
    foregroundResumeToken: Long = 0L,
) {
    val fallbackRepository = remember { repository }
    val gateway = remember(fallbackRepository) { fallbackRepository?.let { RemoteGateway(it) } }
    val scope = rememberCoroutineScope()
    val context = LocalContext.current
    val initialSavedPcs = remember { fallbackRepository?.savedPcs().orEmpty() }
    val initialPc = remember { fallbackRepository?.activePc() }
    val initialTaskId = remember { fallbackRepository?.activeTaskId() }
    val notificationPermissionLauncher = rememberLauncherForActivityResult(
        ActivityResultContracts.RequestPermission(),
    ) {}
    var savedPcs by remember { mutableStateOf(initialSavedPcs) }
    var activePc by remember { mutableStateOf(initialPc) }
    var selectedTask by remember { mutableStateOf<RemoteTaskDetail?>(null) }
    var inboxTasks by remember { mutableStateOf<List<RemoteTaskSummary>>(emptyList()) }
    var inboxLoading by remember { mutableStateOf(false) }
    var detailLoading by remember { mutableStateOf(false) }
    var remoteError by remember { mutableStateOf("") }
    var bridgeTone by remember { mutableStateOf("监听中") }
    var bridgeStatus by remember { mutableStateOf<RemoteBridgeStatus?>(null) }
    var providerStatus by remember { mutableStateOf<RemoteProviderStatus?>(null) }
    var pairingBusy by remember { mutableStateOf(false) }
    var pairingError by remember { mutableStateOf("") }
    var timelineLoadingOlder by remember { mutableStateOf(false) }
    var pendingPcSwitchTaskId by remember { mutableStateOf<String?>(null) }
    var pendingColdRestoreTaskId by remember { mutableStateOf(initialTaskId) }
    var pendingBranchAnchor by remember(selectedTask?.task?.taskId) { mutableStateOf<RemoteBranchAnchor?>(null) }

    fun resetActivePcDerivedState() {
        selectedTask = null
        inboxTasks = emptyList()
        inboxLoading = false
        detailLoading = false
        bridgeStatus = null
        providerStatus = null
        timelineLoadingOlder = false
        pendingBranchAnchor = null
        fallbackRepository?.setActiveTaskId(null)
        pendingColdRestoreTaskId = null
    }

    fun acceptPairedPc(pc: SavedPc) {
        savedPcs = fallbackRepository?.savedPcs() ?: listOf(pc)
        activePc = pc
        resetActivePcDerivedState()
        bridgeTone = "监听中"
        pairingError = ""
    }

    fun selectActivePc(pc: SavedPc) {
        val taskIdToRefresh = selectedTask?.task?.taskId
        val selected = fallbackRepository?.switchActivePc(pc) ?: pc
        savedPcs = fallbackRepository?.savedPcs() ?: savedPcs
        activePc = selected
        resetActivePcDerivedState()
        fallbackRepository?.setActiveTaskId(taskIdToRefresh)
        pendingPcSwitchTaskId = taskIdToRefresh
        pendingColdRestoreTaskId = taskIdToRefresh
        bridgeTone = "监听中"
        remoteError = ""
        pairingError = ""
    }

    fun pairWithTicket(ticket: RemotePairingTicket) {
        val remoteClient = gateway
        pairingBusy = true
        pairingError = ""
        remoteError = ""
        if (remoteClient == null) {
            acceptPairedPc(
                SavedPc(
                    endpointId = ticket.endpointId,
                    displayName = ticket.pcName,
                    protocolVersion = ticket.protocolVersion,
                    pairingUri = ticket.rawUri,
                    bridgeUrl = ticket.bridgeUrl,
                    lastActiveAt = System.currentTimeMillis(),
                ),
            )
            pairingBusy = false
            return
        }
        scope.launch {
            remoteClient.pair(ticket)
                .onSuccess { acceptPairedPc(it) }
                .onFailure {
                    val message = it.message ?: "配对失败"
                    pairingError = message
                    remoteError = message
                }
            pairingBusy = false
        }
    }

    fun clearUntrustedPc(pc: SavedPc?, message: String) {
        if (pc != null && shouldIgnorePcResult(activePc, pc)) {
            return
        }
        fallbackRepository?.clearActivePc()
        savedPcs = fallbackRepository?.savedPcs() ?: savedPcs
        activePc = null
        resetActivePcDerivedState()
        bridgeTone = "重新配对"
        remoteError = message
        pairingError = message
    }

    fun handleRemoteFailure(err: Throwable, fallback: String, pc: SavedPc? = null) {
        if (pc != null && shouldIgnorePcResult(activePc, pc)) {
            return
        }
        if (shouldClearActivePcForFailure(err)) {
            clearUntrustedPc(pc, UNTRUSTED_PC_MESSAGE)
            return
        }
        remoteError = err.message ?: fallback
    }

    fun clearLiveUpdateError() {
        if (remoteError == DETAIL_LIVE_UPDATE_ERROR_MESSAGE) {
            remoteError = ""
        }
    }

    fun handleLiveUpdateFailure(err: Throwable, pc: SavedPc) {
        if (shouldIgnorePcResult(activePc, pc)) return
        if (shouldClearActivePcForFailure(err)) {
            clearUntrustedPc(pc, UNTRUSTED_PC_MESSAGE)
            return
        }
        remoteError = DETAIL_LIVE_UPDATE_ERROR_MESSAGE
    }

    fun applyTaskDetail(detail: RemoteTaskDetail) {
        selectedTask = detail
        fallbackRepository?.setActiveTaskId(detail.task.taskId)
        inboxTasks = syncInboxTaskStatus(inboxTasks, detail)
    }

    fun applyOlderTimelinePage(taskId: String, page: RemoteTimelinePage) {
        val current = selectedTask ?: return
        if (current.task.taskId != taskId) return
        selectedTask = current.withOlderTimelinePage(page)
    }

    fun closeTaskDetail() {
        selectedTask = null
        timelineLoadingOlder = false
        fallbackRepository?.setActiveTaskId(null)
        pendingColdRestoreTaskId = null
    }

    suspend fun refreshTaskSnapshot(
        pc: SavedPc,
        remoteClient: RemoteGateway,
        taskId: String,
    ): Boolean {
        val result = remoteClient.taskDetail(pc, taskId)
        val detail = result.getOrElse {
            handleLiveUpdateFailure(it, pc)
            return false
        }
        if (shouldIgnorePcResult(activePc, pc) || selectedTask?.task?.taskId != taskId) {
            return false
        }
        applyTaskDetail(detail)
        clearLiveUpdateError()
        return true
    }

    fun loadOlderTimeline(pc: SavedPc, taskId: String) {
        val remoteClient = gateway ?: return
        val detail = selectedTask ?: return
        val beforeCursor = detail.timelinePage.beforeCursor ?: return
        if (!detail.timelinePage.hasMoreBefore || timelineLoadingOlder) return
        timelineLoadingOlder = true
        scope.launch {
            remoteClient.timelineBefore(pc, taskId, beforeCursor)
                .onSuccess {
                    if (shouldIgnorePcResult(activePc, pc)) return@onSuccess
                    applyOlderTimelinePage(taskId, it)
                }
                .onFailure { handleRemoteFailure(it, "加载更早时间线失败", pc) }
            if (!shouldIgnorePcResult(activePc, pc)) {
                timelineLoadingOlder = false
            }
        }
    }

    suspend fun recoverLiveTaskUpdate(
        err: Throwable,
        pc: SavedPc,
        remoteClient: RemoteGateway,
        taskId: String,
    ): Boolean {
        if (shouldClearActivePcForFailure(err)) {
            handleLiveUpdateFailure(err, pc)
            return false
        }
        return refreshTaskSnapshot(pc, remoteClient, taskId)
    }

    suspend fun refreshTaskSubscription(
        pc: SavedPc,
        remoteClient: RemoteGateway,
        taskId: String,
    ): Boolean {
        val current = selectedTask
        if (current == null || current.task.taskId != taskId) return false
        val timelineResult = remoteClient.subscribeTimeline(
            pc,
            taskId,
            current.timeline.lastOrNull()?.id,
        )
        if (timelineResult.isFailure) {
            return recoverLiveTaskUpdate(
                timelineResult.exceptionOrNull() ?: RuntimeException("Timeline subscription failed"),
                pc,
                remoteClient,
                taskId,
            )
        }
        val timeline = timelineResult.getOrThrow()
        val stateResult = remoteClient.taskState(pc, taskId)
        if (stateResult.isFailure) {
            return recoverLiveTaskUpdate(
                stateResult.exceptionOrNull() ?: RuntimeException("Task state refresh failed"),
                pc,
                remoteClient,
                taskId,
            )
        }
        val state = stateResult.getOrThrow()
        if (shouldIgnorePcResult(activePc, pc) || selectedTask?.task?.taskId != taskId) {
            return false
        }
        val latest = selectedTask ?: return false
        val detail = latest.copy(
            task = state.task,
            runtimePhase = state.runtimePhase,
            timeline = RemotePayloadParser.mergeTimeline(latest.timeline, timeline),
            pendingInteraction = state.pendingInteraction,
        )
        applyTaskDetail(detail)
        clearLiveUpdateError()
        return true
    }

    suspend fun refreshProviderStatus(pc: SavedPc, remoteClient: RemoteGateway) {
        remoteClient.providerStatus(pc)
            .onSuccess {
                if (shouldIgnorePcResult(activePc, pc)) return@onSuccess
                providerStatus = it
            }
            .onFailure {
                if (shouldIgnorePcResult(activePc, pc)) return@onFailure
                providerStatus = null
            }
    }

    suspend fun loadInbox(pc: SavedPc, remoteClient: RemoteGateway): Boolean {
        val accepted = remoteClient.resume(pc).getOrElse {
            if (shouldIgnorePcResult(activePc, pc)) return false
            bridgeTone = "离线"
            handleRemoteFailure(it, "恢复远程会话失败", pc)
            return false
        }
        if (shouldIgnorePcResult(activePc, pc)) return false
        if (!accepted) {
            clearUntrustedPc(pc, UNTRUSTED_PC_MESSAGE)
            return false
        }
        val status = remoteClient.bridgeStatus(pc)
            .onSuccess {
                if (shouldIgnorePcResult(activePc, pc)) return@onSuccess
                bridgeStatus = it
                bridgeTone = bridgeToneLabel(it)
            }
            .onFailure {
                if (shouldIgnorePcResult(activePc, pc)) return@onFailure
                bridgeStatus = null
                bridgeTone = "离线"
            }
            .getOrNull()
        if (shouldIgnorePcResult(activePc, pc)) return false
        refreshProviderStatus(pc, remoteClient)
        if (shouldIgnorePcResult(activePc, pc)) return false
        if (status?.capabilities?.supportsTaskInbox == false) {
            inboxTasks = emptyList()
            remoteError = taskInboxUnsupportedMessage()
            return true
        }
        remoteClient.listTasks(pc)
            .onSuccess {
                if (shouldIgnorePcResult(activePc, pc)) return@onSuccess
                val detail = selectedTask
                inboxTasks = if (detail == null) it else syncInboxTaskStatus(it, detail)
            }
            .onFailure { handleRemoteFailure(it, "加载任务失败", pc) }
        return true
    }

    fun openTask(pc: SavedPc, taskId: String) {
        val remoteClient = gateway ?: return
        detailLoading = true
        remoteError = ""
        fallbackRepository?.setActiveTaskId(taskId)
        scope.launch {
            refreshProviderStatus(pc, remoteClient)
            remoteClient.taskDetail(pc, taskId)
                .onSuccess {
                    if (shouldIgnorePcResult(activePc, pc)) return@onSuccess
                    applyTaskDetail(it.withRelatedTasks(inboxTasks))
                }
                .onFailure { handleRemoteFailure(it, "加载任务失败", pc) }
            if (!shouldIgnorePcResult(activePc, pc)) {
                detailLoading = false
            }
        }
    }

    fun runTaskAction(
        pc: SavedPc,
        taskId: String,
        failureMessage: String,
        clearError: Boolean = true,
        onSuccess: () -> Unit = {},
        action: suspend (RemoteGateway) -> Result<Unit>,
    ) {
        val remoteClient = gateway ?: return
        detailLoading = true
        if (clearError) remoteError = ""
        scope.launch {
            val result = action(remoteClient)
            result.onFailure { handleRemoteFailure(it, failureMessage, pc) }
            if (result.isSuccess) {
                if (!shouldIgnorePcResult(activePc, pc)) {
                    onSuccess()
                }
                remoteClient.taskDetail(pc, taskId)
                    .onSuccess {
                        if (shouldIgnorePcResult(activePc, pc)) return@onSuccess
                        applyTaskDetail(it.withRelatedTasks(inboxTasks))
                    }
                    .onFailure { handleRemoteFailure(it, "加载任务失败", pc) }
            }
            if (!shouldIgnorePcResult(activePc, pc)) {
                detailLoading = false
            }
        }
    }

    fun sendProcessCommand(command: org.json.JSONObject, failureMessage: String, clearError: Boolean = true) {
        val pc = activePc ?: return
        val detail = selectedTask ?: return
        val taskId = detail.task.taskId
        if (command.optString("type") == "process_session" && command.optString("action") == "spawn") {
            taskRunBlockInfo(detail.task, detail.relatedTasks)?.let {
                remoteError = it.reason
                return
            }
        }
        runTaskAction(pc, taskId, failureMessage, clearError) { remoteClient ->
            remoteClient.sendMessage(
                pc,
                RemoteSendMessageInput(
                    taskId = taskId,
                    content = "",
                    runtimeCommand = command,
                ),
            )
        }
    }

    fun refreshInbox(pc: SavedPc) {
        val remoteClient = gateway ?: return
        inboxLoading = true
        remoteError = ""
        scope.launch {
            loadInbox(pc, remoteClient)
            if (!shouldIgnorePcResult(activePc, pc)) {
                inboxLoading = false
            }
        }
    }

    fun resumeActivePcFromForeground(pc: SavedPc) {
        val remoteClient = gateway ?: return
        val taskId = selectedTask?.task?.taskId ?: fallbackRepository?.activeTaskId()
        inboxLoading = true
        detailLoading = taskId != null
        remoteError = ""
        scope.launch {
            val resumed = loadInbox(pc, remoteClient)
            if (
                resumed &&
                taskId != null &&
                !shouldIgnorePcResult(activePc, pc) &&
                (selectedTask == null || selectedTask?.task?.taskId == taskId)
            ) {
                remoteClient.taskDetail(pc, taskId)
                    .onSuccess {
                        if (shouldIgnorePcResult(activePc, pc)) return@onSuccess
                        applyTaskDetail(it.withRelatedTasks(inboxTasks))
                        clearLiveUpdateError()
                    }
                    .onFailure { handleRemoteFailure(it, "刷新任务失败", pc) }
            }
            if (!shouldIgnorePcResult(activePc, pc)) {
                inboxLoading = false
                if (taskId != null) {
                    detailLoading = false
                }
            }
        }
    }

    LaunchedEffect(initialPairingUri) {
        val uri = initialPairingUri?.takeIf { it.isNotBlank() } ?: return@LaunchedEffect
        PairingUriParser.parse(uri)
            .onSuccess { pairWithTicket(it) }
            .onFailure {
                val message = it.message ?: "配对链接无效"
                pairingError = message
                remoteError = message
            }
    }

    LaunchedEffect(activePc?.endpointId, activePc?.bridgeUrl) {
        val pc = activePc ?: return@LaunchedEffect
        val remoteClient = gateway ?: return@LaunchedEffect
        inboxLoading = true
        remoteError = ""
        loadInbox(pc, remoteClient)
        if (!shouldIgnorePcResult(activePc, pc)) {
            inboxLoading = false
        }
        val taskIdToRefresh = pendingPcSwitchTaskId ?: pendingColdRestoreTaskId ?: return@LaunchedEffect
        pendingPcSwitchTaskId = null
        pendingColdRestoreTaskId = null
        if (shouldIgnorePcResult(activePc, pc)) return@LaunchedEffect
        openTask(pc, taskIdToRefresh)
    }

    LaunchedEffect(activePc?.endpointId, activePc?.bridgeUrl) {
        if (activePc == null) {
            RemoteKeepAliveService.stop(context)
            return@LaunchedEffect
        }
        if (
            Build.VERSION.SDK_INT >= Build.VERSION_CODES.TIRAMISU &&
            ContextCompat.checkSelfPermission(context, Manifest.permission.POST_NOTIFICATIONS) !=
            PackageManager.PERMISSION_GRANTED
        ) {
            notificationPermissionLauncher.launch(Manifest.permission.POST_NOTIFICATIONS)
        }
        RemoteKeepAliveService.start(context)
    }

    LaunchedEffect(foregroundResumeToken) {
        if (foregroundResumeToken == 0L) return@LaunchedEffect
        val pc = activePc ?: return@LaunchedEffect
        resumeActivePcFromForeground(pc)
    }

    LaunchedEffect(
        activePc?.endpointId,
        activePc?.bridgeUrl,
        selectedTask?.task?.taskId,
        bridgeStatus?.capabilities?.supportsTimelineSubscription,
    ) {
        val pc = activePc ?: return@LaunchedEffect
        val remoteClient = gateway ?: return@LaunchedEffect
        val taskId = selectedTask?.task?.taskId ?: return@LaunchedEffect
        var fallbackDelayMs: Long? = null
        while (true) {
            val supportsTimelineSubscription =
                bridgeStatus?.capabilities?.supportsTimelineSubscription != false
            delay(
                if (supportsTimelineSubscription && fallbackDelayMs == null) {
                    DETAIL_SUBSCRIBE_INTERVAL_MS
                } else {
                    fallbackDelayMs ?: DETAIL_SNAPSHOT_FALLBACK_BASE_MS
                },
            )
            if (shouldIgnorePcResult(activePc, pc) || selectedTask?.task?.taskId != taskId) {
                return@LaunchedEffect
            }
            val updated = if (supportsTimelineSubscription) {
                refreshTaskSubscription(pc, remoteClient, taskId)
            } else {
                refreshTaskSnapshot(pc, remoteClient, taskId)
            }
            fallbackDelayMs = if (updated) null else nextLiveUpdateDelay(fallbackDelayMs)
        }
    }

    MaterialTheme(
        colorScheme = MaterialTheme.colorScheme.copy(
            background = Color(0xFF151716),
            surface = Color(0xFF202321),
            primary = Color(0xFF74C7A4),
            secondary = Color(0xFFE3B35B),
            onBackground = Color(0xFFE7ECE8),
            onSurface = Color(0xFFE7ECE8),
            onPrimary = Color(0xFF10221A),
        ),
    ) {
        Surface(
            modifier = Modifier.fillMaxSize(),
            color = MaterialTheme.colorScheme.background,
        ) {
            when {
                activePc == null -> PairingScreen(
                    initialPairingUri = initialPairingUri,
                    busy = pairingBusy,
                    externalError = pairingError,
                    onPair = { ticket ->
                        pairWithTicket(ticket)
                    },
                )
                selectedTask != null -> TaskDetailScreen(
                    pc = activePc!!,
                    detail = selectedTask!!,
                    taskRunBlockInfo = taskRunBlockInfo(selectedTask!!.task, selectedTask!!.relatedTasks),
                    savedPcs = savedPcs,
                    capabilities = bridgeStatus?.capabilities ?: RemoteCapabilities(),
                    providerStatus = providerStatus,
                    loading = detailLoading,
                    error = remoteError,
                    pendingBranchAnchor = pendingBranchAnchor,
                    timelineLoadingOlder = timelineLoadingOlder,
                    onBack = { closeTaskDetail() },
                    onSelectPc = { selectActivePc(it) },
                    onRefresh = { openTask(activePc!!, selectedTask!!.task.taskId) },
                    onInterrupt = {
                        val pc = activePc!!
                        val taskId = selectedTask!!.task.taskId
                        runTaskAction(pc, taskId, "中断失败", clearError = false) { remoteClient ->
                            remoteClient.interrupt(pc, taskId)
                        }
                    },
                    onRetry = { eventId ->
                        val pc = activePc!!
                        val taskId = selectedTask!!.task.taskId
                        runTaskAction(pc, taskId, "重试失败") { remoteClient ->
                            remoteClient.retry(pc, taskId, eventId)
                        }
                    },
                    onResolveInteraction = { interaction, approve, responseText, selectedOptions ->
                        val pc = activePc!!
                        val taskId = selectedTask!!.task.taskId
                        runTaskAction(pc, taskId, "回复交互失败") { remoteClient ->
                            remoteClient.resolveInteraction(
                                pc,
                                interaction,
                                approve,
                                responseText,
                                selectedOptions,
                            )
                        }
                    },
                    onSelectBranchAnchor = { pendingBranchAnchor = it },
                    onClearBranchAnchor = { pendingBranchAnchor = null },
                    onLoadOlderTimeline = {
                        loadOlderTimeline(activePc!!, selectedTask!!.task.taskId)
                    },
                    onSend = { message, branchAnchor ->
                        val pc = activePc!!
                        val detail = selectedTask!!
                        val taskId = detail.task.taskId
                        val blockInfo = taskRunBlockInfo(detail.task, detail.relatedTasks)
                        if (blockInfo != null) {
                            remoteError = blockInfo.reason
                        } else {
                            runTaskAction(
                                pc,
                                taskId,
                                "发送失败",
                                onSuccess = {
                                    if (pendingBranchAnchor == branchAnchor) {
                                        pendingBranchAnchor = null
                                    }
                                },
                            ) { remoteClient ->
                                remoteClient.sendMessage(
                                    pc,
                                    RemoteSendMessageInput(
                                        taskId = taskId,
                                        content = message,
                                        runtimeCommand = branchAnchor?.let(RemoteRuntimeCommandAdapter::sessionFork),
                                    ),
                                )
                            }
                        }
                    },
                    onStartProcess = { command ->
                        sendProcessCommand(
                            RemoteRuntimeCommandAdapter.processSpawn(command),
                            "启动进程失败",
                        )
                    },
                    onSendProcessStdin = { stdin ->
                        sendProcessCommand(
                            RemoteRuntimeCommandAdapter.processWriteStdin(stdin),
                            "发送标准输入失败",
                            clearError = false,
                        )
                    },
                    onStopProcess = {
                        sendProcessCommand(
                            RemoteRuntimeCommandAdapter.processKill(),
                            "停止进程失败",
                            clearError = false,
                        )
                    },
                )
                else -> InboxScreen(
                    pc = activePc!!,
                    savedPcs = savedPcs,
                    tasks = inboxTasks,
                    loading = inboxLoading,
                    error = remoteError,
                    bridgeTone = bridgeTone,
                    onSelectPc = { selectActivePc(it) },
                    onRefresh = { refreshInbox(activePc!!) },
                    onOpenTask = { openTask(activePc!!, it.taskId) },
                    onForgetPc = {
                        val pc = activePc!!
                        fallbackRepository?.forgetPc(pc)
                        savedPcs = fallbackRepository?.savedPcs() ?: savedPcs.filterNot {
                            activePcConnectionMatches(it, pc)
                        }
                        activePc = null
                        resetActivePcDerivedState()
                    },
                )
            }
        }
    }
}

private fun nextLiveUpdateDelay(currentDelayMs: Long?): Long =
    minOf(
        (currentDelayMs ?: 0L) + DETAIL_SNAPSHOT_FALLBACK_BASE_MS,
        DETAIL_SNAPSHOT_FALLBACK_MAX_MS,
    )

@Composable
private fun PairingScreen(
    initialPairingUri: String?,
    busy: Boolean,
    externalError: String,
    onPair: (RemotePairingTicket) -> Unit,
) {
    var pairingUri by remember { mutableStateOf(initialPairingUri.orEmpty()) }
    var error by remember { mutableStateOf("") }
    var scannerVisible by remember { mutableStateOf(false) }
    val context = LocalContext.current
    val cameraPermissionLauncher = rememberLauncherForActivityResult(
        ActivityResultContracts.RequestPermission(),
    ) { granted ->
        if (granted) {
            scannerVisible = true
            error = ""
        } else {
            error = "需要相机权限才能扫描配对二维码。"
        }
    }

    LaunchedEffect(initialPairingUri) {
        val uri = initialPairingUri?.takeIf { it.isNotBlank() } ?: return@LaunchedEffect
        pairingUri = uri
        error = ""
    }

    Box(modifier = Modifier.fillMaxSize()) {
        Column(
            modifier = Modifier
                .fillMaxSize()
                .padding(horizontal = 20.dp, vertical = 18.dp),
            verticalArrangement = Arrangement.SpaceBetween,
        ) {
            Column(verticalArrangement = Arrangement.spacedBy(18.dp)) {
                RemoteHeader(title = "当前电脑", subtitle = "未连接", tone = "就绪")
                Column(verticalArrangement = Arrangement.spacedBy(10.dp)) {
                    Text(
                        text = "Lilia 远控",
                        style = MaterialTheme.typography.headlineMedium,
                        fontWeight = FontWeight.SemiBold,
                        color = MaterialTheme.colorScheme.onBackground,
                    )
                    Text(
                        text = "将这台手机与受信任电脑配对，用手机查看任务、继续对话并处理审批。",
                        style = MaterialTheme.typography.bodyLarge,
                        color = Muted,
                    )
                }
                StatusPanel(
                    rows = listOf(
                        "Android 壳" to "Compose 远控客户端",
                        "传输方式" to "局域网 HTTP 桥接",
                        "远控协议" to "等待配对",
                    ),
                )
            }

            Column(verticalArrangement = Arrangement.spacedBy(12.dp)) {
                OutlinedTextField(
                    value = pairingUri,
                    onValueChange = {
                        pairingUri = it
                        error = ""
                    },
                    modifier = Modifier.fillMaxWidth(),
                    minLines = 3,
                    label = { Text("配对链接") },
                    placeholder = { Text("lilia-remote://pair?...") },
                )
                val displayError = error.ifBlank { externalError }
                if (displayError.isNotBlank()) {
                    Text(displayError, color = Danger, style = MaterialTheme.typography.bodySmall)
                }
                OutlinedButton(
                    onClick = {
                        val granted = ContextCompat.checkSelfPermission(
                            context,
                            Manifest.permission.CAMERA,
                        ) == PackageManager.PERMISSION_GRANTED
                        if (granted) {
                            scannerVisible = true
                        } else {
                            cameraPermissionLauncher.launch(Manifest.permission.CAMERA)
                        }
                    },
                    modifier = Modifier.fillMaxWidth(),
                    enabled = !busy,
                    shape = RoundedCornerShape(8.dp),
                ) {
                    Text("扫描二维码")
                }
                Button(
                    onClick = {
                        PairingUriParser.parse(pairingUri)
                            .onSuccess { onPair(it) }
                            .onFailure { error = it.message ?: "配对链接无效" }
                    },
                    modifier = Modifier.fillMaxWidth(),
                    enabled = !busy,
                    shape = RoundedCornerShape(8.dp),
                    colors = ButtonDefaults.buttonColors(
                        containerColor = MaterialTheme.colorScheme.primary,
                        contentColor = MaterialTheme.colorScheme.onPrimary,
                    ),
                ) {
                    Text(if (busy) "配对中…" else "配对电脑")
                }
            }
        }

        if (scannerVisible) {
            PairingQrScanner(
                onPairingUri = { uri ->
                    scannerVisible = false
                    pairingUri = uri
                    PairingUriParser.parse(uri)
                        .onSuccess { onPair(it) }
                        .onFailure { error = it.message ?: "配对二维码无效" }
                },
                onClose = { scannerVisible = false },
                onError = { error = it },
            )
        }
    }
}

@Composable
private fun InboxScreen(
    pc: SavedPc,
    savedPcs: List<SavedPc>,
    tasks: List<RemoteTaskSummary>,
    loading: Boolean,
    error: String,
    bridgeTone: String,
    onSelectPc: (SavedPc) -> Unit,
    onRefresh: () -> Unit,
    onOpenTask: (RemoteTaskSummary) -> Unit,
    onForgetPc: () -> Unit,
) {
    Column(
        modifier = Modifier
            .fillMaxSize()
            .padding(horizontal = 16.dp, vertical = 18.dp),
        verticalArrangement = Arrangement.spacedBy(14.dp),
    ) {
        RemoteHeader(
            title = "当前电脑",
            subtitle = pc.displayName,
            tone = bridgeTone,
            activePc = pc,
            savedPcs = savedPcs,
            onSelectPc = onSelectPc,
        )
        Row(horizontalArrangement = Arrangement.spacedBy(8.dp)) {
            OutlinedButton(onClick = onForgetPc, shape = RoundedCornerShape(8.dp)) {
                Text("移除电脑")
            }
            OutlinedButton(onClick = onRefresh, shape = RoundedCornerShape(8.dp)) {
                Text(if (loading) "刷新中" else "刷新")
            }
        }
        LazyColumn(verticalArrangement = Arrangement.spacedBy(10.dp)) {
            item {
                Text(
                    text = "任务收件箱",
                    style = MaterialTheme.typography.titleLarge,
                    fontWeight = FontWeight.SemiBold,
                    color = MaterialTheme.colorScheme.onBackground,
                )
            }
            if (error.isNotBlank()) {
                item { Text(error, color = Danger, style = MaterialTheme.typography.bodySmall) }
            }
            if (loading && tasks.isEmpty()) {
                item { Text("正在加载任务…", color = Muted, style = MaterialTheme.typography.bodyMedium) }
            }
            if (!loading && tasks.isEmpty() && error.isBlank()) {
                item { Text("当前电脑还没有可显示的任务。", color = Muted, style = MaterialTheme.typography.bodyMedium) }
            }
            items(tasks, key = { it.taskId }) { task ->
                TaskCard(
                    task = task,
                    blockInfo = taskRunBlockInfo(task, tasks),
                    onClick = { onOpenTask(task) },
                )
            }
        }
    }
}

@Composable
private fun TaskDetailScreen(
    pc: SavedPc,
    detail: RemoteTaskDetail,
    taskRunBlockInfo: TaskRunBlockInfo?,
    savedPcs: List<SavedPc>,
    capabilities: RemoteCapabilities,
    providerStatus: RemoteProviderStatus?,
    loading: Boolean,
    error: String,
    pendingBranchAnchor: RemoteBranchAnchor?,
    timelineLoadingOlder: Boolean,
    onBack: () -> Unit,
    onSelectPc: (SavedPc) -> Unit,
    onRefresh: () -> Unit,
    onInterrupt: () -> Unit,
    onRetry: (String?) -> Unit,
    onResolveInteraction: (PendingInteraction, Boolean, String, Map<String, List<String>>) -> Unit,
    onSelectBranchAnchor: (RemoteBranchAnchor) -> Unit,
    onClearBranchAnchor: () -> Unit,
    onLoadOlderTimeline: () -> Unit,
    onSend: (String, RemoteBranchAnchor?) -> Unit,
    onStartProcess: (String) -> Unit,
    onSendProcessStdin: (String) -> Unit,
    onStopProcess: () -> Unit,
) {
    var draft by remember(detail.task.taskId) { mutableStateOf("") }
    var processCommand by remember(detail.task.taskId) { mutableStateOf("") }
    var processStdin by remember(detail.processSessionId) { mutableStateOf("") }
    val pendingInteraction = detail.pendingInteraction
    val providerReady = providerStatus?.ready != false
    val processRunning = detail.processSessionId != null
    val hasRetryableError = detail.timeline.any { it.retryable }
    val retryEnabled = !loading && providerReady && capabilities.supportsChatSend
    var interactionResponse by remember(pendingInteraction?.requestId) { mutableStateOf("") }
    var selectedInteractionOptions by remember(pendingInteraction?.requestId) { mutableStateOf<Set<String>>(emptySet()) }
    val timelineListState = rememberLazyListState()
    var didScrollInitialTimeline by remember(detail.task.taskId) { mutableStateOf(false) }
    LaunchedEffect(capabilities.supportsSessionFork) {
        if (!capabilities.supportsSessionFork && pendingBranchAnchor != null) {
            onClearBranchAnchor()
        }
    }
    LaunchedEffect(
        detail.task.taskId,
        detail.timelinePage.hasMoreBefore,
        timelineLoadingOlder,
    ) {
        snapshotFlow { timelineListState.firstVisibleItemIndex }
            .collect { firstVisibleIndex ->
                if (
                    firstVisibleIndex <= 1 &&
                    detail.timelinePage.hasMoreBefore &&
                    !timelineLoadingOlder
                ) {
                    onLoadOlderTimeline()
                }
            }
    }
    LaunchedEffect(detail.task.taskId, detail.timeline.lastOrNull()?.id) {
        if (detail.timeline.isEmpty()) return@LaunchedEffect
        val lastIndex = detail.timeline.lastIndex
        val lastVisibleIndex = timelineListState.layoutInfo.visibleItemsInfo.lastOrNull()?.index
        val shouldFollowLatest = !didScrollInitialTimeline ||
            lastVisibleIndex == null ||
            lastVisibleIndex >= lastIndex - 1
        if (shouldFollowLatest) {
            timelineListState.scrollToItem(lastIndex)
            didScrollInitialTimeline = true
        }
    }
    Column(
        modifier = Modifier
            .fillMaxSize()
            .padding(horizontal = 16.dp, vertical = 18.dp),
        verticalArrangement = Arrangement.spacedBy(12.dp),
    ) {
        RemoteHeader(
            title = pc.displayName,
            subtitle = detail.task.title,
            tone = detail.runtimePhase,
            activePc = pc,
            savedPcs = savedPcs,
            onSelectPc = onSelectPc,
        )
        OutlinedButton(onClick = onBack, shape = RoundedCornerShape(8.dp)) {
            Text("返回")
        }
        Row(horizontalArrangement = Arrangement.spacedBy(8.dp)) {
            OutlinedButton(onClick = onRefresh, shape = RoundedCornerShape(8.dp)) {
                Text(if (loading) "刷新中" else "刷新")
            }
            OutlinedButton(
                onClick = onInterrupt,
                enabled = !loading && capabilities.supportsInterrupt,
                shape = RoundedCornerShape(8.dp),
            ) {
                Text("中断")
            }
            OutlinedButton(
                onClick = { onRetry(null) },
                enabled = retryEnabled && hasRetryableError,
                shape = RoundedCornerShape(8.dp),
            ) {
                Text("重试")
            }
        }
        providerStatus?.let {
            Text(
                text = "模型后端 ${it.backend}：${if (it.ready) "就绪" else "未就绪"}",
                color = if (it.ready) MaterialTheme.colorScheme.primary else Danger,
                style = MaterialTheme.typography.bodySmall,
            )
        }
        if (error.isNotBlank()) {
            Text(error, color = Danger, style = MaterialTheme.typography.bodySmall)
        }
        if (taskRunBlockInfo != null) {
            BlockedTaskPanel(taskRunBlockInfo)
        }
        if (capabilities.supportsProcessSession) {
            ProcessSessionPanel(
                running = processRunning,
                processSessionId = detail.processSessionId,
                command = processCommand,
                stdin = processStdin,
                loading = loading,
                startBlockReason = taskRunBlockInfo?.reason,
                onCommandChange = { processCommand = it },
                onStdinChange = { processStdin = it },
                onStart = {
                    val command = processCommand.trim()
                    if (command.isNotEmpty()) {
                        processCommand = ""
                        onStartProcess(command)
                    }
                },
                onSendStdin = {
                    if (processStdin.isNotEmpty()) {
                        val value = processStdin
                        processStdin = ""
                        onSendProcessStdin(value)
                    }
                },
                onStop = onStopProcess,
            )
        }
        pendingInteraction?.let { interaction ->
            val optionQuestion = interactionOptionQuestion(interaction)
            val canResolveInteraction = interactionCanSubmit(
                optionQuestion,
                selectedInteractionOptions,
                interactionResponse,
            )
            Panel {
                Text(interaction.title, color = MaterialTheme.colorScheme.onSurface, fontWeight = FontWeight.SemiBold)
                Text(interaction.body, color = Muted, style = MaterialTheme.typography.bodyMedium)
                if (optionQuestion != null) {
                    Column(verticalArrangement = Arrangement.spacedBy(8.dp)) {
                        optionQuestion.options.forEach { option ->
                            val selected = selectedInteractionOptions.contains(option.id)
                            val onClick = {
                                val maxSelections = optionQuestion.maxSelections
                                selectedInteractionOptions = if (optionQuestion.mode == "multi") {
                                    if (selected) {
                                        selectedInteractionOptions - option.id
                                    } else if (
                                        maxSelections != null &&
                                        selectedInteractionOptions.size >= maxSelections
                                    ) {
                                        selectedInteractionOptions
                                    } else {
                                        selectedInteractionOptions + option.id
                                    }
                                } else {
                                    setOf(option.id)
                                }
                            }
                            if (selected) {
                                Button(onClick = onClick, enabled = !loading, shape = RoundedCornerShape(8.dp)) {
                                    Text(option.label)
                                }
                            } else {
                                OutlinedButton(onClick = onClick, enabled = !loading, shape = RoundedCornerShape(8.dp)) {
                                    Text(option.label)
                                }
                            }
                            option.description?.let {
                                Text(it, color = Muted, style = MaterialTheme.typography.bodySmall)
                            }
                        }
                    }
                }
                if (interactionAcceptsTextResponse(interaction)) {
                    OutlinedTextField(
                        value = interactionResponse,
                        onValueChange = { interactionResponse = it },
                        modifier = Modifier.fillMaxWidth(),
                        minLines = 2,
                        label = { Text("回复") },
                    )
                }
                Row(horizontalArrangement = Arrangement.spacedBy(8.dp)) {
                    Button(
                        onClick = {
                            val response = interactionResponse.trim()
                            val selectedOptions = selectedOptionsByQuestion(optionQuestion, selectedInteractionOptions)
                            interactionResponse = ""
                            selectedInteractionOptions = emptySet()
                            onResolveInteraction(interaction, true, response, selectedOptions)
                        },
                        enabled = !loading && capabilities.supportsInteractionResponse && canResolveInteraction,
                        shape = RoundedCornerShape(8.dp),
                    ) {
                        Text(interactionPrimaryActionLabel(interaction, interactionResponse))
                    }
                    OutlinedButton(
                        onClick = {
                            interactionResponse = ""
                            selectedInteractionOptions = emptySet()
                            onResolveInteraction(interaction, false, "", emptyMap())
                        },
                        enabled = !loading && capabilities.supportsInteractionResponse,
                        shape = RoundedCornerShape(8.dp),
                    ) {
                        Text(interaction.declineLabel)
                    }
                }
            }
        }
        LazyColumn(
            state = timelineListState,
            modifier = Modifier.weight(1f),
            verticalArrangement = Arrangement.spacedBy(8.dp),
        ) {
            if (timelineLoadingOlder) {
                item(key = "timeline-loading-older") {
                    Text(
                        "正在加载更早事件…",
                        color = Muted,
                        style = MaterialTheme.typography.bodySmall,
                    )
                }
            }
            if (detail.timeline.isEmpty()) {
                item {
                    Text(
                        "还没有时间线事件。",
                        color = Muted,
                        style = MaterialTheme.typography.bodyMedium,
                    )
                }
            }
            items(detail.timeline, key = { it.id }) { item ->
                TimelineRow(
                    item = item,
                    loading = loading,
                    retryEnabled = retryEnabled,
                    branchEnabled = capabilities.supportsSessionFork,
                    onRetry = { onRetry(item.id) },
                    onStartBranch = { mode ->
                        item.branchSourceTurnId?.let { turnId ->
                            onSelectBranchAnchor(RemoteBranchAnchor(turnId, mode))
                        }
                    },
                    blockReason = taskRunBlockInfo?.reason,
                )
            }
        }
        pendingBranchAnchor?.takeIf { capabilities.supportsSessionFork }?.let { anchor ->
            Panel {
                Row(
                    modifier = Modifier.fillMaxWidth(),
                    horizontalArrangement = Arrangement.SpaceBetween,
                    verticalAlignment = Alignment.CenterVertically,
                ) {
                    Column(verticalArrangement = Arrangement.spacedBy(2.dp)) {
                        Text(
                            anchor.mode.label,
                            color = MaterialTheme.colorScheme.onSurface,
                            fontWeight = FontWeight.Medium,
                        )
                        Text(anchor.sourceTurnId, color = Muted, style = MaterialTheme.typography.bodySmall)
                    }
                    OutlinedButton(
                        onClick = onClearBranchAnchor,
                        enabled = !loading,
                        shape = RoundedCornerShape(8.dp),
                    ) {
                        Text("清除")
                    }
                }
            }
        }
        OutlinedTextField(
            value = draft,
            onValueChange = { draft = it },
            modifier = Modifier.fillMaxWidth(),
            enabled = taskRunBlockInfo == null,
            label = { Text("消息") },
            placeholder = { Text("通过电脑端运行器发送") },
        )
        Button(
            onClick = {
                val message = draft.trim()
                if (message.isNotEmpty() && taskRunBlockInfo == null) {
                    val branchAnchor = pendingBranchAnchor
                    draft = ""
                    onSend(message, branchAnchor)
                }
            },
            modifier = Modifier.fillMaxWidth(),
            enabled = !loading && providerReady && capabilities.supportsChatSend && taskRunBlockInfo == null &&
                draft.trim().isNotEmpty(),
            shape = RoundedCornerShape(8.dp),
            colors = ButtonDefaults.buttonColors(
                containerColor = MaterialTheme.colorScheme.primary,
                contentColor = MaterialTheme.colorScheme.onPrimary,
            ),
        ) {
            Text(
                when {
                    loading -> "处理中…"
                    !providerReady -> "模型后端未就绪"
                    !capabilities.supportsChatSend -> "不支持发送"
                    taskRunBlockInfo != null -> "已阻塞"
                    else -> "发送"
                },
            )
        }
    }
}

@Composable
private fun ProcessSessionPanel(
    running: Boolean,
    processSessionId: String?,
    command: String,
    stdin: String,
    loading: Boolean,
    startBlockReason: String?,
    onCommandChange: (String) -> Unit,
    onStdinChange: (String) -> Unit,
    onStart: () -> Unit,
    onSendStdin: () -> Unit,
    onStop: () -> Unit,
) {
    Panel {
        Text("进程会话", color = MaterialTheme.colorScheme.onSurface, fontWeight = FontWeight.SemiBold)
        Text(
            text = if (running) "运行中 ${processSessionId.orEmpty()}" else "空闲",
            color = if (running) MaterialTheme.colorScheme.primary else Muted,
            style = MaterialTheme.typography.bodySmall,
        )
        if (running) {
            OutlinedTextField(
                value = stdin,
                onValueChange = onStdinChange,
                modifier = Modifier.fillMaxWidth(),
                minLines = 1,
                label = { Text("stdin") },
                placeholder = { Text("q") },
            )
            Row(horizontalArrangement = Arrangement.spacedBy(8.dp)) {
                Button(
                    onClick = onSendStdin,
                    enabled = !loading && stdin.isNotEmpty(),
                    shape = RoundedCornerShape(8.dp),
                ) {
                    Text("发送 stdin")
                }
                OutlinedButton(
                    onClick = onStop,
                    enabled = !loading,
                    shape = RoundedCornerShape(8.dp),
                ) {
                    Text("停止")
                }
            }
        } else {
            if (startBlockReason != null) {
                Text(startBlockReason, color = Danger, style = MaterialTheme.typography.bodySmall)
            }
            OutlinedTextField(
                value = command,
                onValueChange = onCommandChange,
                modifier = Modifier.fillMaxWidth(),
                enabled = startBlockReason == null,
                minLines = 1,
                label = { Text("命令") },
                placeholder = { Text("npm test -- --watch") },
            )
            Button(
                onClick = onStart,
                modifier = Modifier.fillMaxWidth(),
                enabled = !loading && startBlockReason == null && command.trim().isNotEmpty(),
                shape = RoundedCornerShape(8.dp),
            ) {
                Text(
                    when {
                        loading -> "处理中…"
                        startBlockReason != null -> "已阻塞"
                        else -> "启动进程"
                    },
                )
            }
        }
    }
}

@Composable
private fun BlockedTaskPanel(info: TaskRunBlockInfo) {
    Panel {
        Text("阻塞任务", color = Danger, fontWeight = FontWeight.SemiBold)
        Text(info.reason, color = Muted, style = MaterialTheme.typography.bodyMedium)
        if (info.chain.isNotEmpty()) {
            Text("阻塞链路", color = MaterialTheme.colorScheme.onSurface, fontWeight = FontWeight.Medium)
            info.chain.forEachIndexed { index, task ->
                Text(
                    text = "${index + 1}. ${task.title} (${taskStatusLabel(task.status)})",
                    color = if (task.status == "done") Muted else Danger,
                    style = MaterialTheme.typography.bodySmall,
                )
            }
        }
    }
}

@Composable
private fun RemoteHeader(
    title: String,
    subtitle: String,
    tone: String,
    activePc: SavedPc? = null,
    savedPcs: List<SavedPc> = emptyList(),
    onSelectPc: (SavedPc) -> Unit = {},
) {
    var menuExpanded by remember { mutableStateOf(false) }
    val pcItems = activePcSwitcherItems(activePc, savedPcs)
    Row(
        modifier = Modifier.fillMaxWidth(),
        horizontalArrangement = Arrangement.SpaceBetween,
        verticalAlignment = Alignment.CenterVertically,
    ) {
        Column(modifier = Modifier.weight(1f)) {
            Text(title, style = MaterialTheme.typography.labelMedium, color = Muted)
            Text(
                subtitle,
                style = MaterialTheme.typography.titleMedium,
                fontWeight = FontWeight.Medium,
                color = MaterialTheme.colorScheme.onBackground,
                maxLines = 1,
                overflow = TextOverflow.Ellipsis,
            )
        }
        Row(verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(8.dp)) {
            if (pcItems.size > 1) {
                Box {
                    OutlinedButton(
                        onClick = { menuExpanded = true },
                        modifier = Modifier.width(112.dp),
                        shape = RoundedCornerShape(8.dp),
                    ) {
                        Text(
                            activePc?.displayName ?: "电脑",
                            maxLines = 1,
                            overflow = TextOverflow.Ellipsis,
                        )
                    }
                    DropdownMenu(
                        expanded = menuExpanded,
                        onDismissRequest = { menuExpanded = false },
                    ) {
                        pcItems.forEach { item ->
                            DropdownMenuItem(
                                text = {
                                    Column {
                                        Text(
                                            if (item.active) "${item.pc.displayName}（当前）" else item.pc.displayName,
                                            maxLines = 1,
                                            overflow = TextOverflow.Ellipsis,
                                        )
                                        Text(
                                            item.pc.bridgeUrl,
                                            color = Muted,
                                            style = MaterialTheme.typography.bodySmall,
                                            maxLines = 1,
                                            overflow = TextOverflow.Ellipsis,
                                        )
                                    }
                                },
                                onClick = {
                                    menuExpanded = false
                                    if (!item.active) {
                                        onSelectPc(item.pc)
                                    }
                                },
                            )
                        }
                    }
                }
            }
            Box(
                modifier = Modifier
                    .size(8.dp)
                    .clip(CircleShape)
                    .background(MaterialTheme.colorScheme.secondary),
            )
            Text(
                text = tone,
                modifier = Modifier.padding(start = 8.dp),
                style = MaterialTheme.typography.labelLarge,
                color = MaterialTheme.colorScheme.secondary,
            )
        }
    }
}

@Composable
private fun StatusPanel(rows: List<Pair<String, String>>) {
    Panel {
        rows.forEach { row ->
            StatusRow(label = row.first, value = row.second)
        }
    }
}

@Composable
private fun Panel(content: @Composable ColumnScope.() -> Unit) {
    Column(
        modifier = Modifier
            .fillMaxWidth()
            .clip(RoundedCornerShape(8.dp))
            .background(MaterialTheme.colorScheme.surface)
            .padding(14.dp),
        verticalArrangement = Arrangement.spacedBy(12.dp),
        content = content,
    )
}

@Composable
private fun StatusRow(label: String, value: String) {
    Row(
        modifier = Modifier.fillMaxWidth(),
        horizontalArrangement = Arrangement.SpaceBetween,
        verticalAlignment = Alignment.CenterVertically,
    ) {
        Text(label, color = Color(0xFF9BA49E), style = MaterialTheme.typography.bodyMedium)
        Text(value, color = MaterialTheme.colorScheme.onSurface, style = MaterialTheme.typography.bodyMedium)
    }
}

@Composable
private fun TaskCard(task: RemoteTaskSummary, blockInfo: TaskRunBlockInfo?, onClick: () -> Unit) {
    Panel {
        Column(
            modifier = Modifier.clickable(onClick = onClick),
            verticalArrangement = Arrangement.spacedBy(6.dp),
        ) {
            Row(
                modifier = Modifier.fillMaxWidth(),
                horizontalArrangement = Arrangement.SpaceBetween,
                verticalAlignment = Alignment.Top,
            ) {
                Text(task.title, color = MaterialTheme.colorScheme.onSurface, fontWeight = FontWeight.SemiBold)
                Text(
                    taskStatusLabel(task.status),
                    color = if (blockInfo != null) Danger else MaterialTheme.colorScheme.secondary,
                    style = MaterialTheme.typography.labelMedium,
                )
            }
            Text(task.projectName ?: "独立对话", color = Muted, style = MaterialTheme.typography.bodySmall)
            if (blockInfo != null) {
                Text(blockInfo.reason, color = Danger, style = MaterialTheme.typography.bodySmall)
                val chainText = blockInfo.chain.joinToString(" -> ") { it.title }
                if (chainText.isNotBlank()) {
                    Text(chainText, color = Muted, style = MaterialTheme.typography.bodySmall)
                }
            }
            task.pendingAction?.let {
                Text(it, color = MaterialTheme.colorScheme.primary, style = MaterialTheme.typography.bodySmall)
            }
            Text(task.lastActivity, color = Color(0xFF87918B), style = MaterialTheme.typography.bodySmall)
        }
    }
}

@Composable
private fun TimelineRow(
    item: RemoteTimelineItem,
    loading: Boolean,
    retryEnabled: Boolean,
    branchEnabled: Boolean,
    onRetry: () -> Unit,
    onStartBranch: (RemoteSessionForkMode) -> Unit,
    blockReason: String?,
) {
    val statusColor = timelineStatusColor(item.status)
    Panel {
        Row(
            modifier = Modifier.fillMaxWidth(),
            horizontalArrangement = Arrangement.SpaceBetween,
            verticalAlignment = Alignment.Top,
        ) {
            Column(
                modifier = Modifier.weight(1f),
                verticalArrangement = Arrangement.spacedBy(4.dp),
            ) {
                Text(
                    item.title,
                    color = MaterialTheme.colorScheme.onSurface,
                    fontWeight = FontWeight.Medium,
                    maxLines = 2,
                    overflow = TextOverflow.Ellipsis,
                )
                Row(horizontalArrangement = Arrangement.spacedBy(6.dp)) {
                    TimelineChip(text = item.kind, color = Muted)
                    item.role?.let { role ->
                        TimelineChip(text = timelineRoleLabel(role), color = MaterialTheme.colorScheme.primary)
                    }
                }
            }
            TimelineChip(text = timelineStatusLabel(item.status), color = statusColor)
        }
        if (item.summary.isNotBlank()) {
            Text(
                item.summary,
                color = if (item.role == "assistant" || item.role == "user") {
                    MaterialTheme.colorScheme.onSurface
                } else {
                    Muted
                },
                style = MaterialTheme.typography.bodyMedium,
            )
        }
        item.details.forEach { detail ->
            TimelineDetailRow(detail)
        }
        if (item.retryable) {
            OutlinedButton(
                onClick = onRetry,
                enabled = retryEnabled,
                shape = RoundedCornerShape(8.dp),
            ) {
                Text("重试")
            }
        }
        if (branchEnabled && item.branchSourceTurnId != null) {
            if (blockReason != null) {
                Text(blockReason, color = Danger, style = MaterialTheme.typography.bodySmall)
            }
            Row(horizontalArrangement = Arrangement.spacedBy(8.dp)) {
                OutlinedButton(
                    onClick = { onStartBranch(RemoteSessionForkMode.CONTINUE) },
                    enabled = !loading && blockReason == null,
                    shape = RoundedCornerShape(8.dp),
                ) {
                    Text("继续")
                }
                OutlinedButton(
                    onClick = { onStartBranch(RemoteSessionForkMode.FORK) },
                    enabled = !loading && blockReason == null,
                    shape = RoundedCornerShape(8.dp),
                ) {
                    Text("分叉")
                }
            }
        }
    }
}

@Composable
private fun TimelineChip(text: String, color: Color) {
    Text(
        text = text,
        modifier = Modifier
            .clip(RoundedCornerShape(6.dp))
            .background(color.copy(alpha = 0.14f))
            .padding(horizontal = 7.dp, vertical = 3.dp),
        color = color,
        style = MaterialTheme.typography.labelSmall,
        maxLines = 1,
        overflow = TextOverflow.Ellipsis,
    )
}

@Composable
private fun TimelineDetailRow(detail: RemoteTimelineDetail) {
    Row(
        modifier = Modifier.fillMaxWidth(),
        horizontalArrangement = Arrangement.spacedBy(8.dp),
        verticalAlignment = Alignment.Top,
    ) {
        Text(
            detail.label.replaceFirstChar { it.uppercase() },
            modifier = Modifier.width(72.dp),
            color = Color(0xFF87918B),
            style = MaterialTheme.typography.labelSmall,
        )
        Text(
            detail.value,
            modifier = Modifier.weight(1f),
            color = Muted,
            style = MaterialTheme.typography.bodySmall,
        )
    }
}

@Composable
private fun timelineStatusColor(status: String): Color =
    when (status) {
        "success", "completed", "done" -> MaterialTheme.colorScheme.primary
        "failed", "error", "cancelled" -> Danger
        "requires_action" -> MaterialTheme.colorScheme.secondary
        "running", "started", "in_progress", "pending" -> Color(0xFF8DBBE8)
        else -> Muted
    }

private val Muted = Color(0xFFB8C2BC)
private val Danger = Color(0xFFE07A6F)

private fun bridgeToneLabel(status: RemoteBridgeStatus): String {
    if (!status.hostEnabled) return "已禁用"
    return when (status.state) {
        "pairing" -> "配对中"
        "listening", "connected" -> "监听中"
        "disconnected" -> "离线"
        else -> status.state
    }
}

private fun timelineRoleLabel(role: String): String =
    when (role) {
        "assistant" -> "助手"
        "user" -> "用户"
        "system" -> "系统"
        else -> role
    }

private fun timelineStatusLabel(status: String): String =
    when (status) {
        "success", "completed", "done" -> "已完成"
        "failed", "error" -> "失败"
        "cancelled" -> "已取消"
        "requires_action" -> "需要操作"
        "running", "started", "in_progress" -> "运行中"
        "pending" -> "等待中"
        "info" -> "信息"
        else -> status
    }

private fun interactionAcceptsTextResponse(interaction: PendingInteraction): Boolean {
    if (interaction.kind != "ask_user" && interaction.kind != "plan_approval") return false
    val questions = interaction.raw.optJSONObject("payload")?.optJSONArray("questions") ?: return false
    for (index in 0 until questions.length()) {
        val question = questions.optJSONObject(index) ?: continue
        if (question.optString("mode") == "confirm") return true
        if (question.optBoolean("allowOther")) return true
        val options = question.optJSONArray("options")
        if (options == null || options.length() == 0) return true
    }
    return false
}

private fun interactionPrimaryActionLabel(interaction: PendingInteraction, responseText: String): String {
    if (responseText.trim().isEmpty()) return interaction.approveLabel
    val question = interaction.raw
        .optJSONObject("payload")
        ?.optJSONArray("questions")
        ?.optJSONObject(0)
        ?: return interaction.approveLabel
    if (question.optString("mode") != "confirm") return interaction.approveLabel
    return if (interaction.kind == "plan_approval") "发送修改意见" else "发送回复"
}

@Preview
@Composable
private fun LiliaRemoteAppPreview() {
    LiliaRemoteApp()
}
