package com.lilia.remote

import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.withContext
import org.json.JSONArray
import org.json.JSONObject
import java.io.BufferedReader
import java.io.OutputStreamWriter
import java.net.HttpURLConnection
import java.net.URL
import java.util.UUID
import java.util.concurrent.atomic.AtomicLong

internal const val REMOTE_TIMELINE_PAGE_SIZE = 80

class RemoteHttpClient(
    private val repository: RemoteDeviceStore,
) {
    private val agentRequestIds = AtomicLong(1)
    private val negotiatedAgentEndpoints = mutableSetOf<String>()
    private val agentNegotiationLock = Any()

    suspend fun bridgeStatus(pc: SavedPc): Result<RemoteBridgeStatus> = withContext(Dispatchers.IO) {
        runCatching {
            RemotePayloadParser.parseBridgeStatus(getJson("${bridgeUrl(pc)}/status"))
        }
    }

    suspend fun pair(ticket: RemotePairingTicket): Result<SavedPc> = withContext(Dispatchers.IO) {
        runCatching {
            val body = JSONObject()
                .put("ticketId", ticket.ticketId)
                .put("challenge", ticket.challenge)
                .put("deviceName", android.os.Build.MODEL ?: "Android 手机")
                .put("protocolVersion", ticket.protocolVersion)
                .put(
                    "androidEndpoint",
                    JSONObject()
                        .put("endpointId", repository.deviceEndpointId())
                        .put("relayUrl", JSONObject.NULL)
                        .put("directAddresses", JSONArray()),
                )
            val response = postJson("${bridgeUrl(ticket)}/pair", body)
            RemoteEnvelopeAdapter.throwIfError(response, "配对失败")
            repository.savePairing(ticket)
        }
    }

    suspend fun listTasks(pc: SavedPc): Result<List<RemoteTaskSummary>> = withContext(Dispatchers.IO) {
        runCatching {
            val payload = dispatch(pc, JSONObject().put("type", "tasks.list").put("limit", 80))
            RemotePayloadParser.parseTaskList(payload)
        }
    }

    suspend fun providerStatus(pc: SavedPc): Result<RemoteProviderStatus> = withContext(Dispatchers.IO) {
        runCatching {
            val payload = dispatch(pc, JSONObject().put("type", "provider.status.read"))
            RemotePayloadParser.parseProviderStatus(payload)
        }
    }

    suspend fun resume(pc: SavedPc): Result<Boolean> = withContext(Dispatchers.IO) {
        runCatching {
            val payload = dispatch(
                pc,
                JSONObject()
                    .put("type", "connection.resume")
                    .put("androidEndpointId", repository.deviceEndpointId()),
            )
            payload.optBoolean("accepted")
        }
    }

    suspend fun taskDetail(
        pc: SavedPc,
        taskId: String,
        timelineLimit: Int = REMOTE_TIMELINE_PAGE_SIZE,
    ): Result<RemoteTaskDetail> = withContext(Dispatchers.IO) {
        runCatching {
            val taskPayload = dispatch(pc, JSONObject().put("type", "tasks.get").put("taskId", taskId))
            val timelinePayload = dispatch(
                pc,
                JSONObject()
                    .put("type", "timeline.snapshot")
                    .put("taskId", taskId)
                    .put("limit", timelineLimit)
                    .put("direction", "latest"),
            )
            val pendingPayload = dispatch(pc, JSONObject().put("type", "interaction.pending.read").put("taskId", taskId))
            RemotePayloadParser.parseTaskDetail(taskId, taskPayload, timelinePayload, pendingPayload)
        }
    }

    suspend fun timelineBefore(
        pc: SavedPc,
        taskId: String,
        beforeCursor: String,
        timelineLimit: Int = REMOTE_TIMELINE_PAGE_SIZE,
    ): Result<RemoteTimelinePage> = withContext(Dispatchers.IO) {
        runCatching {
            val payload = dispatch(
                pc,
                JSONObject()
                    .put("type", "timeline.snapshot")
                    .put("taskId", taskId)
                    .put("limit", timelineLimit)
                    .put("direction", "before")
                    .put("cursor", beforeCursor),
            )
            RemotePayloadParser.parseTimelinePagePayload(payload)
        }
    }

    suspend fun taskState(pc: SavedPc, taskId: String): Result<RemoteTaskState> = withContext(Dispatchers.IO) {
        runCatching {
            val taskPayload = dispatch(pc, JSONObject().put("type", "tasks.get").put("taskId", taskId))
            val pendingPayload = dispatch(pc, JSONObject().put("type", "interaction.pending.read").put("taskId", taskId))
            RemotePayloadParser.parseTaskState(taskId, taskPayload, pendingPayload)
        }
    }

    suspend fun subscribeTimeline(
        pc: SavedPc,
        taskId: String,
        afterEventId: String?,
    ): Result<List<RemoteTimelineItem>> = withContext(Dispatchers.IO) {
        runCatching {
            val request = JSONObject()
                .put("type", "timeline.subscribe")
                .put("taskId", taskId)
            if (afterEventId != null) {
                request.put("afterEventId", afterEventId)
            }
            val payload = dispatch(pc, request)
            RemotePayloadParser.parseTimelinePayload(payload)
        }
    }

    suspend fun sendMessage(
        pc: SavedPc,
        input: RemoteSendMessageInput,
    ): Result<Unit> = withContext(Dispatchers.IO) {
        runCatching {
            if (input.runtimeCommand == null) {
                submitAgentMessage(pc, input)
            } else {
                dispatch(pc, input.toRequestJson())
            }
            Unit
        }
    }

    suspend fun interrupt(pc: SavedPc, taskId: String): Result<Unit> = withContext(Dispatchers.IO) {
        runCatching {
            dispatch(pc, JSONObject().put("type", "chat.interrupt").put("taskId", taskId))
            Unit
        }
    }

    suspend fun retry(
        pc: SavedPc,
        taskId: String,
        eventId: String? = null,
    ): Result<Unit> = withContext(Dispatchers.IO) {
        runCatching {
            val request = JSONObject().put("type", "chat.retry").put("taskId", taskId)
            if (!eventId.isNullOrBlank()) {
                request.put("eventId", eventId)
            }
            dispatch(pc, request)
            Unit
        }
    }

    suspend fun resolveInteraction(
        pc: SavedPc,
        interaction: PendingInteraction,
        approve: Boolean,
        responseText: String? = null,
        selectedOptionIdsByQuestion: Map<String, List<String>> = emptyMap(),
    ): Result<Unit> = withContext(Dispatchers.IO) {
        runCatching {
            val response = JSONObject()
                .put("taskId", interaction.taskId)
                .put("requestId", interaction.requestId)
                .put("kind", interaction.kind)
                .put(
                    "result",
                    RemoteInteractionAdapter.resultForInteraction(
                        interaction,
                        approve,
                        responseText,
                        selectedOptionIdsByQuestion,
                    ),
                )
            dispatch(pc, JSONObject().put("type", "interaction.respond").put("response", response))
            Unit
        }
    }

    private fun dispatch(pc: SavedPc, request: JSONObject): JSONObject {
        val envelope = RemoteEnvelopeAdapter.requestEnvelope(pc, repository.deviceEndpointId(), request)
        val response = postJson("${bridgeUrl(pc)}/dispatch", envelope)
        val payload = RemoteEnvelopeAdapter.payloadOrThrow(response)
        if (shouldMarkActivePcSeen(request, payload)) {
            repository.markActivePcSeen(pc)
        }
        return payload
    }

    private fun submitAgentMessage(pc: SavedPc, input: RemoteSendMessageInput) {
        ensureAgentWireNegotiated(pc)
        val opened = dispatch(
            pc,
            JSONObject()
                .put("type", "agent.session.open")
                .put("taskId", input.taskId),
        )
        val session = opened.getJSONObject("session")
        val sessionId = session.getString("session_id")
        val expectedVersion = session.getJSONObject("cell").getLong("generation")
        val turnId = "android-turn-${UUID.randomUUID()}"
        val metadata = JSONObject(opened.optJSONObject("context")?.toString() ?: "{}")
            .putIfPresent("composer", input.composer)
            .putIfPresent("attachments", input.attachments)
            .putIfPresent("conversationReferences", input.conversationReferences)
            .putIfPresent("workflow", input.workflow)
            .putIfPresent("runtimeOptions", input.runtimeOptions)
        val response = agentWire(
            pc,
            "submit_turn",
            JSONObject()
                .put("session_id", sessionId)
                .put("expected_version", expectedVersion)
                .put("turn_id", turnId)
                .put(
                    "messages",
                    JSONArray().put(
                        JSONObject()
                            .put("role", "user")
                            .put("content", input.content)
                            .put("metadata", metadata)
                            .put("parts", JSONArray()),
                    ),
                )
                .put("idempotency_key", "android:${input.taskId}:$turnId"),
        )
        val accepted = agentWireValue(response, "accepted")
        check(accepted.getString("session_id") == sessionId) {
            "Agent Wire accepted a different session"
        }
    }

    private fun ensureAgentWireNegotiated(pc: SavedPc) {
        synchronized(agentNegotiationLock) {
            if (negotiatedAgentEndpoints.contains(pc.endpointId)) return
            val response = agentWire(pc, "negotiate", null)
            val negotiated = agentWireValue(response, "negotiated")
            check(negotiated.getInt("version") == 1) {
                "Desktop Agent Wire version is incompatible"
            }
            negotiatedAgentEndpoints.add(pc.endpointId)
        }
    }

    private fun agentWire(pc: SavedPc, method: String, params: JSONObject?): JSONObject {
        val request = JSONObject().put("method", method)
        if (params != null) request.put("params", params)
        val wireEnvelope = JSONObject()
            .put("request_id", agentRequestIds.getAndIncrement())
            .put(
                "hello",
                JSONObject()
                    .put("version", 1)
                    .put("required_features", JSONArray().put("monotonic-events"))
                    .put(
                        "optional_features",
                        JSONArray()
                            .put("approval-binding")
                            .put("event-resume")
                            .put("resource-ref"),
                    ),
            )
            .put("request", request)
        return dispatch(
            pc,
            JSONObject()
                .put("type", "agent.wire")
                .put("envelope", wireEnvelope),
        ).getJSONObject("envelope")
    }

    private fun agentWireValue(envelope: JSONObject, expectedType: String): JSONObject {
        val response = envelope.getJSONObject("response")
        val error = response.optJSONObject("Err")
        if (error != null) {
            throw RemoteBridgeException(
                code = error.optString("code", "agent.wire"),
                message = error.optString("message", "Agent Wire request failed"),
                retryable = error.optBoolean("retryable", false),
            )
        }
        val result = response.getJSONObject("Ok")
        check(result.getString("type") == expectedType) {
            "Unexpected Agent Wire response type"
        }
        return result.optJSONObject("value") ?: JSONObject()
    }

    private fun bridgeUrl(pc: SavedPc): String = pc.bridgeUrl.trim().trimEnd('/')

    private fun bridgeUrl(ticket: RemotePairingTicket): String = ticket.bridgeUrl.trim().trimEnd('/')

    private fun shouldMarkActivePcSeen(request: JSONObject, payload: JSONObject): Boolean {
        if (request.optString("type") != "connection.resume") return true
        return payload.optBoolean("accepted")
    }

    private fun postJson(url: String, body: JSONObject): JSONObject {
        val connection = (URL(url).openConnection() as HttpURLConnection).apply {
            requestMethod = "POST"
            connectTimeout = 5_000
            readTimeout = 30_000
            doOutput = true
            setRequestProperty("Content-Type", "application/json; charset=utf-8")
        }
        OutputStreamWriter(connection.outputStream, Charsets.UTF_8).use { writer ->
            writer.write(body.toString())
        }
        return try {
            readJsonResponse(connection)
        } finally {
            connection.disconnect()
        }
    }

    private fun getJson(url: String): JSONObject {
        val connection = (URL(url).openConnection() as HttpURLConnection).apply {
            requestMethod = "GET"
            connectTimeout = 5_000
            readTimeout = 10_000
        }
        return try {
            readJsonResponse(connection)
        } finally {
            connection.disconnect()
        }
    }

    private fun readJsonResponse(connection: HttpURLConnection): JSONObject {
        val statusCode = connection.responseCode
        val stream = if (statusCode in 200..299) {
            connection.inputStream
        } else {
            connection.errorStream
        }
        val text = stream?.let {
            BufferedReader(it.reader(Charsets.UTF_8)).use { reader -> reader.readText() }
        }.orEmpty()
        return RemoteHttpResponseAdapter.parseJson(statusCode, text)
    }
}

private fun RemoteSendMessageInput.toRequestJson(): JSONObject =
    JSONObject()
        .put("type", "chat.send")
        .put("taskId", taskId)
        .put("content", content)
        .putIfPresent("composer", composer)
        .putIfPresent("attachments", attachments)
        .putIfPresent("conversationReferences", conversationReferences)
        .putIfPresent("workflow", workflow)
        .putIfPresent("runtimeCommand", runtimeCommand)
        .putIfPresent("runtimeOptions", runtimeOptions)

private fun JSONObject.putIfPresent(key: String, value: Any?): JSONObject =
    if (value == null) this else put(key, value)
