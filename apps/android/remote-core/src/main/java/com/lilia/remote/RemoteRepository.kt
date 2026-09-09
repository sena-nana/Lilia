package com.lilia.remote

import android.content.Context
import android.content.SharedPreferences
import java.util.UUID
import org.json.JSONArray
import org.json.JSONObject

interface RemoteDeviceStore {
    fun deviceEndpointId(): String
    fun savePairing(ticket: RemotePairingTicket, sessionToken: String): SavedPc
    fun markActivePcSeen(pc: SavedPc, now: Long = System.currentTimeMillis()): SavedPc
}

class RemoteRepository internal constructor(
    private val prefs: RemotePreferences,
) : RemoteDeviceStore {
    constructor(context: Context) : this(
        AndroidRemotePreferences(context.getSharedPreferences("lilia_remote", Context.MODE_PRIVATE)),
    )

    override fun deviceEndpointId(): String {
        val existing = prefs.getString("device.endpointId", null)
        if (!existing.isNullOrBlank()) return existing
        val created = "android-${UUID.randomUUID()}"
        prefs.edit {
            putString("device.endpointId", created)
        }
        return created
    }

    fun activePc(): SavedPc? {
        return readActivePcFromPrefs()
    }

    fun activeTaskId(): String? =
        prefs.getString("active.taskId", null)?.takeIf { it.isNotBlank() }

    fun setActiveTaskId(taskId: String?) {
        prefs.edit {
            if (taskId.isNullOrBlank()) {
                remove("active.taskId")
            } else {
                putString("active.taskId", taskId)
            }
        }
    }

    fun savedPcs(): List<SavedPc> {
        val saved = readSavedPcs()
        if (saved.isNotEmpty()) return saved
        return readActivePcFromPrefs()?.let(::listOf).orEmpty()
    }

    override fun savePairing(ticket: RemotePairingTicket, sessionToken: String): SavedPc {
        require(sessionToken.isNotBlank()) { "sessionToken is required after pairing" }
        val pc = SavedPc(
            endpointId = ticket.endpointId,
            displayName = ticket.pcName,
            protocolVersion = ticket.protocolVersion,
            pairingUri = ticket.rawUri,
            bridgeUrl = ticket.bridgeUrl,
            lastActiveAt = System.currentTimeMillis(),
            sessionToken = sessionToken,
        )
        val updatedSavedPcs = upsertSavedPc(savedPcs(), pc)
        prefs.edit {
            putSavedPcList(updatedSavedPcs)
            putActivePc(pc)
            remove("active.taskId")
        }
        return pc
    }

    fun switchActivePc(pc: SavedPc, now: Long = System.currentTimeMillis()): SavedPc? {
        val saved = savedPcs()
        val selected = saved.firstOrNull { activePcConnectionMatches(it, pc) } ?: return null
        val updated = selected.copy(lastActiveAt = now)
        val updatedSavedPcs = upsertSavedPc(saved, updated)
        prefs.edit {
            putSavedPcList(updatedSavedPcs)
            putActivePc(updated)
        }
        return updated
    }

    override fun markActivePcSeen(pc: SavedPc, now: Long): SavedPc {
        val current = activePc()
        if (current == null || !activePcConnectionMatches(current, pc)) return pc
        val updated = current.copy(lastActiveAt = now)
        val updatedSavedPcs = upsertSavedPc(savedPcs(), updated)
        prefs.edit {
            putSavedPcList(updatedSavedPcs)
            putActivePc(updated)
        }
        return updated
    }

    fun clearActivePc() {
        prefs.edit {
            clearActivePcFields()
            remove("active.taskId")
        }
    }

    fun forgetPc(pc: SavedPc) {
        val remaining = savedPcs().filterNot {
            it.endpointId == pc.endpointId && it.bridgeUrl == pc.bridgeUrl
        }
        val active = activePc()
        prefs.edit {
            putSavedPcList(remaining)
            if (active != null && activePcConnectionMatches(active, pc)) {
                clearActivePcFields()
                remove("active.taskId")
            }
        }
    }

    private fun readActivePcFromPrefs(): SavedPc? {
        val endpointId = prefs.getString("active.endpointId", null) ?: return null
        val displayName = prefs.getString("active.displayName", null) ?: DEFAULT_PC_DISPLAY_NAME
        val pairingUri = prefs.getString("active.pairingUri", null) ?: ""
        val bridgeUrl = prefs.getString("active.bridgeUrl", null)?.takeIf { it.isNotBlank() }
            ?: run {
                clearActivePc()
                return null
            }
        val protocolVersion = prefs.getInt("active.protocolVersion", 1)
        val lastActiveAt = prefs.getLong("active.lastActiveAt", 0L)
        val sessionToken = prefs.getString("active.sessionToken", null).orEmpty()
        return SavedPc(
            endpointId = endpointId,
            displayName = displayName,
            protocolVersion = protocolVersion,
            pairingUri = pairingUri,
            bridgeUrl = bridgeUrl,
            lastActiveAt = lastActiveAt,
            sessionToken = sessionToken,
        )
    }

    private fun readSavedPcs(): List<SavedPc> {
        val raw = prefs.getString("saved.pcs", null)?.takeIf { it.isNotBlank() } ?: return emptyList()
        return runCatching {
            val array = JSONArray(raw)
            buildList {
                for (index in 0 until array.length()) {
                    val item = array.optJSONObject(index) ?: continue
                    val endpointId = item.optString("endpointId").takeIf { it.isNotBlank() } ?: continue
                    val bridgeUrl = item.optString("bridgeUrl").takeIf { it.isNotBlank() } ?: continue
                    add(
                        SavedPc(
                            endpointId = endpointId,
                            displayName = item.optString("displayName", DEFAULT_PC_DISPLAY_NAME),
                            protocolVersion = item.optInt("protocolVersion", 1),
                            pairingUri = item.optString("pairingUri", ""),
                            bridgeUrl = bridgeUrl,
                            lastActiveAt = item.optLong("lastActiveAt", 0L),
                            sessionToken = item.optString("sessionToken", ""),
                        ),
                    )
                }
            }
        }.getOrDefault(emptyList())
    }
}

fun activePcConnectionMatches(current: SavedPc, candidate: SavedPc): Boolean =
    current.endpointId == candidate.endpointId && current.bridgeUrl == candidate.bridgeUrl

private fun upsertSavedPc(savedPcs: List<SavedPc>, pc: SavedPc): List<SavedPc> =
    (savedPcs.filterNot { activePcConnectionMatches(it, pc) } + pc)
        .sortedByDescending { it.lastActiveAt }

private fun RemotePreferencesEditor.putActivePc(pc: SavedPc) {
    putString("active.endpointId", pc.endpointId)
    putString("active.displayName", pc.displayName)
    putString("active.pairingUri", pc.pairingUri)
    putString("active.bridgeUrl", pc.bridgeUrl)
    putInt("active.protocolVersion", pc.protocolVersion)
    putLong("active.lastActiveAt", pc.lastActiveAt)
    if (pc.sessionToken.isBlank()) {
        remove("active.sessionToken")
    } else {
        putString("active.sessionToken", pc.sessionToken)
    }
}

private fun RemotePreferencesEditor.clearActivePcFields() {
    remove("active.endpointId")
    remove("active.displayName")
    remove("active.pairingUri")
    remove("active.bridgeUrl")
    remove("active.protocolVersion")
    remove("active.lastActiveAt")
    remove("active.sessionToken")
}

private fun RemotePreferencesEditor.putSavedPcList(savedPcs: List<SavedPc>) {
    val array = JSONArray()
    savedPcs.forEach { pc ->
        array.put(
            JSONObject()
                .put("endpointId", pc.endpointId)
                .put("displayName", pc.displayName)
                .put("protocolVersion", pc.protocolVersion)
                .put("pairingUri", pc.pairingUri)
                .put("bridgeUrl", pc.bridgeUrl)
                .put("lastActiveAt", pc.lastActiveAt)
                .put("sessionToken", pc.sessionToken),
        )
    }
    putString("saved.pcs", array.toString())
}

internal interface RemotePreferences {
    fun getString(key: String, defaultValue: String?): String?
    fun getInt(key: String, defaultValue: Int): Int
    fun getLong(key: String, defaultValue: Long): Long
    fun edit(block: RemotePreferencesEditor.() -> Unit)
}

internal interface RemotePreferencesEditor {
    fun putString(key: String, value: String?)
    fun putInt(key: String, value: Int)
    fun putLong(key: String, value: Long)
    fun remove(key: String)
}

private class AndroidRemotePreferences(
    private val prefs: SharedPreferences,
) : RemotePreferences {
    override fun getString(key: String, defaultValue: String?): String? =
        prefs.getString(key, defaultValue)

    override fun getInt(key: String, defaultValue: Int): Int =
        prefs.getInt(key, defaultValue)

    override fun getLong(key: String, defaultValue: Long): Long =
        prefs.getLong(key, defaultValue)

    override fun edit(block: RemotePreferencesEditor.() -> Unit) {
        val editor = prefs.edit()
        AndroidRemotePreferencesEditor(editor).block()
        editor.apply()
    }
}

private class AndroidRemotePreferencesEditor(
    private val editor: SharedPreferences.Editor,
) : RemotePreferencesEditor {
    override fun putString(key: String, value: String?) {
        if (value == null) {
            editor.remove(key)
        } else {
            editor.putString(key, value)
        }
    }

    override fun putInt(key: String, value: Int) {
        editor.putInt(key, value)
    }

    override fun putLong(key: String, value: Long) {
        editor.putLong(key, value)
    }

    override fun remove(key: String) {
        editor.remove(key)
    }
}
