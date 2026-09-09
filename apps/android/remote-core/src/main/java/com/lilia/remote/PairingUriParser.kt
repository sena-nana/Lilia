package com.lilia.remote

import java.net.InetAddress
import java.net.URI
import java.net.URLDecoder
import java.net.UnknownHostException

object PairingUriParser {
    private const val SUPPORTED_PROTOCOL_VERSION = 1
    private val SUPPORTED_SCHEMES = setOf("lilia-remote", "lilia-voice")

    const val PUBLIC_BRIDGE_CONFIRM_MESSAGE =
        "桥接地址指向公网主机，确认后才能继续配对（默认仅允许本机/私网）"

    fun parse(
        value: String,
        allowPublicBridge: Boolean = false,
    ): Result<RemotePairingTicket> = runCatching {
        val trimmed = value.trim()
        val uri = URI(trimmed)
        require(uri.scheme in SUPPORTED_SCHEMES) { "请输入 Lilia 配对链接" }
        require(uri.host == "pair") { "请输入 Lilia 配对链接" }
        val query = parseQuery(uri.rawQuery)
        val version = query["v"]?.toIntOrNull()
            ?: error("配对链接缺少协议版本")
        require(version == SUPPORTED_PROTOCOL_VERSION) {
            "不支持的远控协议版本：$version"
        }
        val ticket = query["ticket"]?.takeIf { it.isNotBlank() }
            ?: error("配对链接缺少票据")
        val challenge = query["challenge"]?.takeIf { it.isNotBlank() }
            ?: error("配对链接缺少验证信息")
        val endpoint = query["endpoint"]?.takeIf { it.isNotBlank() }
            ?: error("配对链接缺少电脑端点")
        val name = query["name"]?.takeIf { it.isNotBlank() } ?: DEFAULT_PC_DISPLAY_NAME
        val bridge = query["bridge"]?.takeIf { it.isNotBlank() }
            ?: error("配对链接缺少桥接地址")
        validateBridgeUrl(bridge, allowPublicBridge)
        RemotePairingTicket(
            protocolVersion = version,
            ticketId = ticket,
            challenge = challenge,
            endpointId = endpoint,
            pcName = name,
            bridgeUrl = bridge,
            rawUri = trimmed,
        )
    }

    /** Drop one-time `challenge` from a pairing URI before persisting. */
    fun sanitizePersistedPairingUri(rawUri: String): String {
        if (rawUri.isBlank()) return rawUri
        return runCatching {
            val uri = URI(rawUri.trim())
            val rawQuery = uri.rawQuery ?: return rawUri.trim()
            val kept = rawQuery
                .split('&')
                .filter { part ->
                    val key = part.substringBefore('=', missingDelimiterValue = part)
                    decode(key) != "challenge"
                }
            val scheme = uri.scheme ?: return rawUri.trim()
            val host = uri.host ?: return rawUri.trim()
            val path = uri.rawPath ?: ""
            if (kept.isEmpty()) {
                "$scheme://$host$path"
            } else {
                "$scheme://$host$path?${kept.joinToString("&")}"
            }
        }.getOrDefault(rawUri.trim())
    }

    private fun parseQuery(rawQuery: String?): Map<String, String> {
        if (rawQuery.isNullOrBlank()) return emptyMap()
        return buildMap {
            rawQuery.split('&')
                .filter { it.isNotBlank() }
                .forEach { pair ->
                    val parts = pair.split('=', limit = 2)
                    val key = decode(parts[0])
                    if (key.isNotBlank()) {
                        put(key, decode(parts.getOrElse(1) { "" }))
                    }
                }
        }
    }

    private fun decode(value: String): String =
        URLDecoder.decode(value, Charsets.UTF_8.name())

    private fun validateBridgeUrl(value: String, allowPublicBridge: Boolean) {
        val bridgeUri = URI(value)
        val scheme = bridgeUri.scheme?.lowercase()
        require(scheme == "http" || scheme == "https") {
            "配对桥接地址必须使用 HTTP(S)"
        }
        require(!bridgeUri.host.isNullOrBlank()) {
            "配对桥接地址缺少主机"
        }
        require(bridgeUri.port > 0) {
            "配对桥接地址缺少端口"
        }
        require(bridgeUri.rawPath.isNullOrBlank() || bridgeUri.rawPath == "/") {
            "配对桥接地址不能包含路径"
        }
        require(bridgeUri.rawQuery.isNullOrBlank() && bridgeUri.rawFragment.isNullOrBlank()) {
            "配对桥接地址不能包含查询参数或片段"
        }
        val host = bridgeUri.host!!.trim()
        if (!isLoopbackOrPrivateHost(host) && !allowPublicBridge) {
            error(PUBLIC_BRIDGE_CONFIRM_MESSAGE)
        }
    }

    internal fun isLoopbackOrPrivateHost(host: String): Boolean {
        val normalized = host.trim().lowercase().trim('[', ']')
        if (normalized == "localhost" || normalized.endsWith(".localhost")) {
            return true
        }
        // Emulator special alias for host loopback via adb reverse / 10.0.2.2.
        if (normalized == "10.0.2.2") {
            return true
        }
        // Never treat cloud metadata endpoints as an allowed "private" bridge.
        if (normalized == "metadata" ||
            normalized == "metadata.google.internal" ||
            normalized.endsWith(".internal") ||
            normalized == "169.254.169.254"
        ) {
            return false
        }
        return try {
            val address = InetAddress.getByName(normalized)
            val bytes = address.address
            if (bytes.size == 4 &&
                bytes[0] == 169.toByte() &&
                bytes[1] == 254.toByte()
            ) {
                // Link-local IPv4 (incl. metadata) — not a LAN bridge target.
                return false
            }
            address.isLoopbackAddress || address.isSiteLocalAddress || isUniqueLocalIpv6(address)
        } catch (_: UnknownHostException) {
            // Fail closed for unresolvable / weird public-looking hosts.
            false
        }
    }

    private fun isUniqueLocalIpv6(address: InetAddress): Boolean {
        val bytes = address.address
        if (bytes.size != 16) return false
        // fc00::/7
        return (bytes[0].toInt() and 0xfe) == 0xfc
    }
}
