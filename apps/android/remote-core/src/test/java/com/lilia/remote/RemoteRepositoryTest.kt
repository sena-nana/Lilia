package com.lilia.remote

import org.junit.Assert.assertEquals
import org.junit.Assert.assertNotNull
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test

class RemoteRepositoryTest {
    @Test
    fun deviceEndpointIdIsStableAcrossRepositoryInstances() {
        val preferences = InMemoryRemotePreferences()
        val first = RemoteRepository(preferences).deviceEndpointId()
        val second = RemoteRepository(preferences).deviceEndpointId()

        assertTrue(first.startsWith("android-"))
        assertEquals(first, second)
    }

    @Test
    fun savePairingPersistsActivePcWithBridgeUrl() {
        val preferences = InMemoryRemotePreferences()
        val saved = RemoteRepository(preferences).savePairing(ticket())

        val restored = RemoteRepository(preferences).activePc()
        val savedPcs = RemoteRepository(preferences).savedPcs()

        assertNotNull(restored)
        assertEquals("pc-endpoint", saved.endpointId)
        assertEquals("pc-endpoint", restored?.endpointId)
        assertEquals("Desk", restored?.displayName)
        assertEquals(1, restored?.protocolVersion)
        assertEquals("lilia-remote://pair?v=1", restored?.pairingUri) // already challenge-free fixture
        assertEquals("http://192.168.1.12:41478", restored?.bridgeUrl)
        assertTrue((restored?.lastActiveAt ?: 0L) > 0L)
        assertEquals(listOf("pc-endpoint"), savedPcs.map { it.endpointId })
    }

    @Test
    fun savePairingPersistsMultipleSavedPcsAndActivatesLatestPairing() {
        val preferences = InMemoryRemotePreferences()
        val repository = RemoteRepository(preferences)

        repository.savePairing(ticket(endpointId = "pc-desk", pcName = "Desk"))
        repository.savePairing(
            ticket(
                endpointId = "pc-studio",
                pcName = "Studio",
                bridgeUrl = "http://192.168.1.44:41478",
                rawUri = "lilia-remote://pair?v=1&pc=studio",
            ),
        )

        val restored = RemoteRepository(preferences)

        assertEquals("pc-studio", restored.activePc()?.endpointId)
        assertEquals(setOf("pc-desk", "pc-studio"), restored.savedPcs().map { it.endpointId }.toSet())
    }

    @Test
    fun savePairingKeepsSameEndpointPcsWhenBridgeUrlDiffers() {
        val preferences = InMemoryRemotePreferences()
        val repository = RemoteRepository(preferences)

        repository.savePairing(ticket(endpointId = "pc-shared", pcName = "Desk"))
        repository.savePairing(
            ticket(
                endpointId = "pc-shared",
                pcName = "Studio",
                bridgeUrl = "http://192.168.1.44:41478",
                rawUri = "lilia-remote://pair?v=1&pc=studio",
            ),
        )

        val restored = RemoteRepository(preferences)

        assertEquals("Studio", restored.activePc()?.displayName)
        assertEquals(
            setOf("http://192.168.1.12:41478", "http://192.168.1.44:41478"),
            restored.savedPcs().map { it.bridgeUrl }.toSet(),
        )
    }

    @Test
    fun switchActivePcPersistsSelectedSavedPcByConnectionAndUpdatesSeenAt() {
        val preferences = InMemoryRemotePreferences()
        val repository = RemoteRepository(preferences)
        val desk = repository.savePairing(ticket(endpointId = "pc-shared", pcName = "Desk"))
        repository.savePairing(
            ticket(
                endpointId = "pc-shared",
                pcName = "Studio",
                bridgeUrl = "http://192.168.1.44:41478",
            ),
        )

        val selected = repository.switchActivePc(desk, now = 1710000004321)
        val restored = RemoteRepository(preferences)

        assertEquals("Desk", selected?.displayName)
        assertEquals("Desk", restored.activePc()?.displayName)
        assertEquals("http://192.168.1.12:41478", restored.activePc()?.bridgeUrl)
        assertEquals(1710000004321, restored.activePc()?.lastActiveAt)
        assertEquals(
            1710000004321,
            restored.savedPcs().first { it.bridgeUrl == "http://192.168.1.12:41478" }.lastActiveAt,
        )
    }

    @Test
    fun activeTaskIdPersistsForColdResume() {
        val preferences = InMemoryRemotePreferences()
        val repository = RemoteRepository(preferences)
        repository.savePairing(ticket())

        repository.setActiveTaskId("task-42")

        assertEquals("task-42", RemoteRepository(preferences).activeTaskId())
    }

    @Test
    fun savePairingAndClearActivePcDropPersistedActiveTaskId() {
        val preferences = InMemoryRemotePreferences()
        val repository = RemoteRepository(preferences)
        repository.savePairing(ticket())
        repository.setActiveTaskId("task-42")

        repository.savePairing(ticket(endpointId = "pc-studio"))
        assertNull(RemoteRepository(preferences).activeTaskId())

        repository.setActiveTaskId("task-99")
        repository.clearActivePc()

        assertNull(RemoteRepository(preferences).activeTaskId())
    }

    @Test
    fun forgetPcRemovesOnlySelectedSavedPcAndClearsActiveWhenItMatches() {
        val preferences = InMemoryRemotePreferences()
        val repository = RemoteRepository(preferences)
        repository.savePairing(ticket(endpointId = "pc-desk", pcName = "Desk"))
        val studio = repository.savePairing(
            ticket(
                endpointId = "pc-studio",
                pcName = "Studio",
                bridgeUrl = "http://192.168.1.44:41478",
            ),
        )

        repository.forgetPc(studio)
        val restored = RemoteRepository(preferences)

        assertNull(restored.activePc())
        assertEquals(listOf("pc-desk"), restored.savedPcs().map { it.endpointId })
    }

    @Test
    fun clearActivePcRemovesPairingStateButKeepsDeviceEndpoint() {
        val preferences = InMemoryRemotePreferences()
        val repository = RemoteRepository(preferences)
        val endpointId = repository.deviceEndpointId()
        repository.savePairing(ticket())

        repository.clearActivePc()

        assertNull(RemoteRepository(preferences).activePc())
        assertEquals(endpointId, RemoteRepository(preferences).deviceEndpointId())
    }

    @Test
    fun activePcDropsLegacyRecordWithoutBridgeUrl() {
        val preferences = InMemoryRemotePreferences()
        preferences.edit {
            putString("active.endpointId", "pc-endpoint")
            putString("active.displayName", "Desk")
            putString("active.pairingUri", "lilia-remote://pair?v=1")
            putInt("active.protocolVersion", 1)
            putLong("active.lastActiveAt", 1710000000000)
        }

        assertNull(RemoteRepository(preferences).activePc())
    }

    @Test
    fun markActivePcSeenUpdatesCurrentPcLastActiveAt() {
        val preferences = InMemoryRemotePreferences()
        val repository = RemoteRepository(preferences)
        val saved = repository.savePairing(ticket())

        val updated = repository.markActivePcSeen(saved, now = 1710000001234)

        assertEquals(1710000001234, updated.lastActiveAt)
        assertEquals(1710000001234, RemoteRepository(preferences).activePc()?.lastActiveAt)
    }

    @Test
    fun markActivePcSeenIgnoresStalePcEndpoint() {
        val preferences = InMemoryRemotePreferences()
        val repository = RemoteRepository(preferences)
        repository.savePairing(ticket())
        val stalePc = RemoteRepository(preferences).activePc()!!.copy(endpointId = "pc-other")

        val result = repository.markActivePcSeen(stalePc, now = 1710000001234)

        assertEquals(stalePc, result)
        assertEquals("pc-endpoint", RemoteRepository(preferences).activePc()?.endpointId)
        assertTrue((RemoteRepository(preferences).activePc()?.lastActiveAt ?: 0L) != 1710000001234L)
    }

    @Test
    fun markActivePcSeenIgnoresStaleBridgeUrl() {
        val preferences = InMemoryRemotePreferences()
        val repository = RemoteRepository(preferences)
        repository.savePairing(ticket())
        val stalePc = RemoteRepository(preferences).activePc()!!
            .copy(bridgeUrl = "http://192.168.1.99:41478")

        val result = repository.markActivePcSeen(stalePc, now = 1710000001234)

        assertEquals(stalePc, result)
        assertEquals("http://192.168.1.12:41478", RemoteRepository(preferences).activePc()?.bridgeUrl)
        assertTrue((RemoteRepository(preferences).activePc()?.lastActiveAt ?: 0L) != 1710000001234L)
    }

    @Test
    fun savePairingStripsChallengeFromPersistedPairingUri() {
        val preferences = InMemoryRemotePreferences()
        val saved = RemoteRepository(preferences).savePairing(
            ticket(
                rawUri = "lilia-remote://pair?v=1&ticket=ticket-1&challenge=challenge-1&endpoint=pc-endpoint",
            ),
        )
        assertTrue(!saved.pairingUri.contains("challenge"))
        assertEquals(
            "lilia-remote://pair?v=1&ticket=ticket-1&endpoint=pc-endpoint",
            RemoteRepository(preferences).activePc()?.pairingUri,
        )
    }

    private fun ticket(
        endpointId: String = "pc-endpoint",
        pcName: String = "Desk",
        bridgeUrl: String = "http://192.168.1.12:41478",
        rawUri: String = "lilia-remote://pair?v=1",
    ): RemotePairingTicket =
        RemotePairingTicket(
            protocolVersion = 1,
            ticketId = "ticket-1",
            challenge = "challenge-1",
            endpointId = endpointId,
            pcName = pcName,
            bridgeUrl = bridgeUrl,
            rawUri = rawUri,
        )
}

private class InMemoryRemotePreferences : RemotePreferences {
    private val values = mutableMapOf<String, Any?>()

    override fun getString(key: String, defaultValue: String?): String? =
        values[key] as? String ?: defaultValue

    override fun getInt(key: String, defaultValue: Int): Int =
        values[key] as? Int ?: defaultValue

    override fun getLong(key: String, defaultValue: Long): Long =
        values[key] as? Long ?: defaultValue

    override fun edit(block: RemotePreferencesEditor.() -> Unit) {
        InMemoryRemotePreferencesEditor(values).block()
    }
}

private class InMemoryRemotePreferencesEditor(
    private val values: MutableMap<String, Any?>,
) : RemotePreferencesEditor {
    override fun putString(key: String, value: String?) {
        if (value == null) {
            values.remove(key)
        } else {
            values[key] = value
        }
    }

    override fun putInt(key: String, value: Int) {
        values[key] = value
    }

    override fun putLong(key: String, value: Long) {
        values[key] = value
    }

    override fun remove(key: String) {
        values.remove(key)
    }
}
