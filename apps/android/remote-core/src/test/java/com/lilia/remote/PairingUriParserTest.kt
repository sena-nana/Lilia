package com.lilia.remote

import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Test

class PairingUriParserTest {
    @Test
    fun parsesPairingUriWithBridgeUrl() {
        val ticket = PairingUriParser.parse(
            " lilia-remote://pair?v=1&ticket=ticket-1&challenge=challenge-1" +
                "&endpoint=pc-1&name=Lilia%20Desktop&bridge=http%3A%2F%2F192.168.1.5%3A41478 ",
        ).getOrThrow()

        assertEquals(1, ticket.protocolVersion)
        assertEquals("ticket-1", ticket.ticketId)
        assertEquals("challenge-1", ticket.challenge)
        assertEquals("pc-1", ticket.endpointId)
        assertEquals("Lilia Desktop", ticket.pcName)
        assertEquals("http://192.168.1.5:41478", ticket.bridgeUrl)
    }

    @Test
    fun parsesVoicePairingUriAliasWithSameProtocol() {
        val ticket = PairingUriParser.parse(
            "lilia-voice://pair?v=1&ticket=ticket-1&challenge=challenge-1" +
                "&endpoint=pc-1&bridge=http%3A%2F%2F192.168.1.5%3A41478",
        ).getOrThrow()

        assertEquals("ticket-1", ticket.ticketId)
        assertEquals("pc-1", ticket.endpointId)
        assertEquals("http://192.168.1.5:41478", ticket.bridgeUrl)
    }

    @Test
    fun defaultsPcNameWhenNameIsMissing() {
        val ticket = PairingUriParser.parse(
            "lilia-remote://pair?v=1&ticket=ticket-1&challenge=challenge-1" +
                "&endpoint=pc-1&bridge=http%3A%2F%2F192.168.1.5%3A41478",
        ).getOrThrow()

        assertEquals("Lilia 电脑", ticket.pcName)
    }

    @Test
    fun rejectsPairingUrisWithoutBridgeUrl() {
        val result = PairingUriParser.parse(
            "lilia-remote://pair?v=1&ticket=ticket-1&challenge=challenge-1&endpoint=pc-1",
        )

        assertTrue(result.isFailure)
        assertEquals("配对链接缺少桥接地址", result.exceptionOrNull()?.message)
    }

    @Test
    fun rejectsNonPairingUris() {
        val result = PairingUriParser.parse(
            "https://example.test/pair?v=1&ticket=ticket-1&challenge=challenge-1&endpoint=pc-1",
        )

        assertTrue(result.isFailure)
    }

    @Test
    fun rejectsUnsupportedProtocolVersions() {
        val result = PairingUriParser.parse(
            "lilia-remote://pair?v=2&ticket=ticket-1&challenge=challenge-1&endpoint=pc-1",
        )

        assertTrue(result.isFailure)
        assertEquals("不支持的远控协议版本：2", result.exceptionOrNull()?.message)
    }

    @Test
    fun rejectsNonHttpBridgeUrls() {
        val result = PairingUriParser.parse(
            "lilia-remote://pair?v=1&ticket=ticket-1&challenge=challenge-1" +
                "&endpoint=pc-1&bridge=file%3A%2F%2F%2Ftmp%2Flilia",
        )

        assertTrue(result.isFailure)
        assertEquals("配对桥接地址必须使用 HTTP(S)", result.exceptionOrNull()?.message)
    }

    @Test
    fun rejectsBridgeUrlsWithoutExplicitPort() {
        val result = PairingUriParser.parse(
            "lilia-remote://pair?v=1&ticket=ticket-1&challenge=challenge-1" +
                "&endpoint=pc-1&bridge=http%3A%2F%2F192.168.1.5",
        )

        assertTrue(result.isFailure)
        assertEquals("配对桥接地址缺少端口", result.exceptionOrNull()?.message)
    }

    @Test
    fun rejectsBridgeUrlsWithPath() {
        val result = PairingUriParser.parse(
            "lilia-remote://pair?v=1&ticket=ticket-1&challenge=challenge-1" +
                "&endpoint=pc-1&bridge=http%3A%2F%2F192.168.1.5%3A41478%2Fbridge",
        )

        assertTrue(result.isFailure)
        assertEquals("配对桥接地址不能包含路径", result.exceptionOrNull()?.message)
    }

    @Test
    fun rejectsBridgeUrlsWithQueryOrFragment() {
        val result = PairingUriParser.parse(
            "lilia-remote://pair?v=1&ticket=ticket-1&challenge=challenge-1" +
                "&endpoint=pc-1&bridge=http%3A%2F%2F192.168.1.5%3A41478%3Fdebug%3D1",
        )

        assertTrue(result.isFailure)
        assertEquals("配对桥接地址不能包含查询参数或片段", result.exceptionOrNull()?.message)
    }

    @Test
    fun rejectsPublicHttpBridgeHostsByDefault() {
        val result = PairingUriParser.parse(
            "lilia-remote://pair?v=1&ticket=ticket-1&challenge=challenge-1" +
                "&endpoint=pc-1&bridge=http%3A%2F%2F203.0.113.10%3A41478",
        )

        assertTrue(result.isFailure)
        assertEquals(PairingUriParser.PUBLIC_BRIDGE_CONFIRM_MESSAGE, result.exceptionOrNull()?.message)
    }

    @Test
    fun acceptsPublicBridgeHostsWhenExplicitlyConfirmed() {
        val ticket = PairingUriParser.parse(
            "lilia-remote://pair?v=1&ticket=ticket-1&challenge=challenge-1" +
                "&endpoint=pc-1&bridge=http%3A%2F%2F203.0.113.10%3A41478",
            allowPublicBridge = true,
        ).getOrThrow()

        assertEquals("http://203.0.113.10:41478", ticket.bridgeUrl)
    }

    @Test
    fun acceptsLoopbackAndEmulatorBridgeHosts() {
        val loopback = PairingUriParser.parse(
            "lilia-remote://pair?v=1&ticket=ticket-1&challenge=challenge-1" +
                "&endpoint=pc-1&bridge=http%3A%2F%2F127.0.0.1%3A41478",
        ).getOrThrow()
        assertEquals("http://127.0.0.1:41478", loopback.bridgeUrl)

        val emulator = PairingUriParser.parse(
            "lilia-remote://pair?v=1&ticket=ticket-1&challenge=challenge-1" +
                "&endpoint=pc-1&bridge=http%3A%2F%2F10.0.2.2%3A41478",
        ).getOrThrow()
        assertEquals("http://10.0.2.2:41478", emulator.bridgeUrl)
    }

    @Test
    fun sanitizePersistedPairingUriRemovesChallenge() {
        val sanitized = PairingUriParser.sanitizePersistedPairingUri(
            "lilia-remote://pair?v=1&ticket=ticket-1&challenge=challenge-1" +
                "&endpoint=pc-1&bridge=http%3A%2F%2F192.168.1.5%3A41478",
        )
        assertTrue(!sanitized.contains("challenge"))
        assertTrue(sanitized.contains("ticket=ticket-1"))
        assertTrue(sanitized.contains("endpoint=pc-1"))
    }
}
