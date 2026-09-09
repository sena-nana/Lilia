# LiliaCode security threat model (short)

> Status: living summary of trust boundaries and residual risks after the
> Critical / High / Medium hardening passes (#83–#85) and Low follow-ups
> (`fix/low-ipc-lease-threat-model`). Not a full STRIDE workbook.

## Assets

- User workspace files and secrets (keyring / credential registry)
- Agent tool execution (process, HTTP, browser, shell)
- GitHub OAuth access tokens (device flow)
- Product SQLite storage writer authority
- Remote Android ↔ PC control channel
- Service observe / agent wire surfaces

## Trust boundaries

| Boundary | Trust assumption |
|----------|------------------|
| Same OS user | **Not** a security boundary for desktop IPC, lock files, or local sockets |
| Other OS users | Isolated via home-dir / Unix modes / Windows profile ACLs |
| Local network | Hostile unless TLS + auth; cleartext only behind explicit opt-in |
| Install directory | Host `.dll` / `.so` / `.dylib` beside the launcher is trusted |
| Hook configuration | Treated as trusted operator input once configured |

## Component notes

### Agent tools (#84)

- Process exec: `allow_network=false` is fail-closed via a network-program /
  URL denylist (best-effort; not an OS network sandbox).
- HTTP snapshot: SSRF controls (scheme, private/metadata hosts, redirect
  re-validation). DNS rebinding between check and connect remains.
- Permission modes: `free` maps to Ask; `full` remains broad — host backends
  are the last line of defense.

### Remote bridge (#83)

- Default bind is loopback (`127.0.0.1`). Non-loopback requires an explicit
  dangerous env opt-in.
- Unauthenticated `/status` is public-only (no tickets / trusted device ids).
- `/dispatch` requires a pairing-minted Bearer session token.
- Residual: cleartext HTTP if LAN bind is opted in; token theft on LAN without
  TLS; full iroh/mTLS is follow-up.

### Hooks (#84)

- Prefer argv / JSON argv over shell when safe; shell fallback still exists for
  free-form hooks. Workspace `cwd` is canonicalized when provided.
- Residual: a configured shell hook can run arbitrary commands as the user.

### Service observe (#85)

- Non-loopback `LILIA_SERVICE_BIND` refuses to start without
  `LILIA_SERVICE_OBSERVE_TOKEN`.
- Observe + `/agent/wire` require Bearer when a token is configured; `/health`
  stays open for probes.
- Secrets prefer OS keyring (`LILIA_SERVICE_IN_MEMORY_SECRETS=1` for headless CI).

### MCP / markdown (#85)

- MCP URLs are HTTPS-only unless insecure HTTP is explicitly opted in.
- Markdown local images must canonicalize under a workspace root; remote images
  reuse HostHttp-style SSRF checks.

### Single-instance IPC (L1)

- Descriptor at `~/.lilia/run/instance.json` (Unix `0600`, `run/` `0700`).
- Loopback-only listener + per-instance token.
- **Residual:** same-user processes can read the token and forge CLI forwards.

### GitHub OAuth (L2)

- Bundled Client ID is **public** (native OAuth client). Not a secret.
- Scopes `repo read:user` match current binding + repository features; further
  narrowing would break private-repo login without a product redesign.

### Launcher `libloading` (L3)

- Host library loads only from the directory adjacent to the launcher
  executable (no `PATH` / `LD_LIBRARY_PATH` override).
- **Residual:** a planted library beside a writable install dir can be loaded;
  rely on install/updater integrity.

### Writer lease (L4)

- Process-local epoch registry + optional single-machine file lock
  (Unix flock / Windows exclusive open), lock file Unix `0600` with owner/pid
  payload for operators.
- Crash recovery = OS releasing the lock (no TTL required).
- **Not provided:** cross-process epoch fencing of late commands after
  takeover, or distributed/cluster leases.

### Updater positives

- Desktop depends on `minisign-verify` for update artifact signature checks.
- Treat updater signature verification + immutable install layout as the
  primary control against hostile host-library planting (L3).

## Explicit non-goals (current)

- Cross-user desktop IPC secrecy
- Full OS network / syscall sandbox for agent tools
- iroh / mTLS migration for Remote + Service
- Distributed writer fencing / multi-node storage

## Related PRs

- #83 — Critical remote HTTP bridge
- #84 — High agent host / hooks / permission modes
- #85 — Medium MCP / markdown / service observe / secrets
