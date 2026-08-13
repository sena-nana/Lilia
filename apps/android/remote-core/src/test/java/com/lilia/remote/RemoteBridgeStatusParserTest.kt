package com.lilia.remote

import org.json.JSONObject
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test

class RemoteBridgeStatusParserTest {
    @Test
    fun parsesBridgeStatusResponse() {
        val status = RemotePayloadParser.parseBridgeStatus(
            JSONObject(
                """
                {
                  "ok": true,
                  "status": {
                    "hostEnabled": true,
                    "state": "listening",
                    "pcName": "Workstation"
                  }
                }
                """.trimIndent(),
            ),
        )

        assertTrue(status.hostEnabled)
        assertEquals("listening", status.state)
        assertEquals("Workstation", status.pcName)
        assertTrue(status.capabilities.supportsChatSend)
        assertFalse(status.capabilities.supportsSessionFork)
        assertFalse(status.capabilities.supportsProcessSession)
    }

    @Test
    fun parsesBridgeCapabilities() {
        val status = RemotePayloadParser.parseBridgeStatus(
            JSONObject(
                """
                {
                  "ok": true,
                  "status": {
                    "hostEnabled": true,
                    "state": "listening",
                    "pcName": "Workstation",
                    "capabilities": {
                      "supportsTaskInbox": true,
                      "supportsTimelineSubscription": false,
                      "supportsChatSend": false,
                      "supportsInteractionResponse": false,
                      "supportsInterrupt": false,
                      "supportsSessionFork": true,
                      "supportsProcessSession": true
                    }
                  }
                }
                """.trimIndent(),
            ),
        )

        assertTrue(status.capabilities.supportsTaskInbox)
        assertFalse(status.capabilities.supportsTimelineSubscription)
        assertFalse(status.capabilities.supportsChatSend)
        assertFalse(status.capabilities.supportsInteractionResponse)
        assertFalse(status.capabilities.supportsInterrupt)
        assertTrue(status.capabilities.supportsSessionFork)
        assertTrue(status.capabilities.supportsProcessSession)
    }

    @Test
    fun parsesDesktopBridgeStatusShapeWithExtraCapabilityMetadata() {
        val status = RemotePayloadParser.parseBridgeStatus(
            JSONObject(
                """
                {
                  "ok": true,
                  "status": {
                    "hostEnabled": true,
                    "state": "pairing",
                    "pcName": "Desk",
                    "endpoint": {
                      "endpointId": "pc-endpoint",
                      "relayUrl": null,
                      "directAddresses": []
                    },
                    "activeTicket": {
                      "id": "ticket-1",
                      "pcName": "Desk",
                      "protocolVersion": 1,
                      "challenge": "challenge-1",
                      "expiresAt": 1710000600000,
                      "pairingUri": "lilia-remote://pair?v=1&ticket=ticket-1",
                      "bridgeUrl": "http://192.168.1.12:41478",
                      "pcEndpoint": {
                        "endpointId": "pc-endpoint",
                        "relayUrl": null,
                        "directAddresses": []
                      }
                    },
                    "trustedDevices": [],
                    "capabilities": {
                      "protocolVersion": 1,
                      "minProtocolVersion": 1,
                      "alpn": "lilia.remote-control.v1",
	                      "supportsPairing": true,
	                      "supportsTaskInbox": true,
	                      "supportsTimelineSubscription": true,
	                      "supportsTimelinePagination": true,
	                      "supportsChatSend": true,
                      "supportsInteractionResponse": true,
                      "supportsInterrupt": true,
                      "supportsSessionFork": true,
                      "supportsProcessSession": true
                    }
                  }
                }
                """.trimIndent(),
            ),
        )

        assertTrue(status.hostEnabled)
        assertEquals("pairing", status.state)
        assertEquals("Desk", status.pcName)
	        assertTrue(status.capabilities.supportsTaskInbox)
	        assertTrue(status.capabilities.supportsTimelineSubscription)
	        assertTrue(status.capabilities.supportsTimelinePagination)
	        assertTrue(status.capabilities.supportsChatSend)
        assertTrue(status.capabilities.supportsInteractionResponse)
        assertTrue(status.capabilities.supportsInterrupt)
        assertTrue(status.capabilities.supportsSessionFork)
        assertTrue(status.capabilities.supportsProcessSession)
    }

    @Test
    fun parsesDisabledBridgeStatus() {
        val status = RemotePayloadParser.parseBridgeStatus(
            JSONObject(
                """
                {
                  "ok": true,
                  "status": {
                    "hostEnabled": false,
                    "state": "disabled",
                    "pcName": "Lilia PC"
                  }
                }
                """.trimIndent(),
            ),
        )

        assertFalse(status.hostEnabled)
        assertEquals("disabled", status.state)
    }

    @Test
    fun rejectsBridgeStatusError() {
        val err = runCatching {
            RemotePayloadParser.parseBridgeStatus(
                JSONObject()
                    .put("ok", false)
                    .put("error", "store unavailable"),
            )
        }.exceptionOrNull()

        assertEquals("store unavailable", err?.message)
    }
}
