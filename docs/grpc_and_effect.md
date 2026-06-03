# gRPC + Effect-TS Integration for Lenzu

## 1. Current Architecture (Baseline)

### 1.1 Communication Flow

```
lenzu_client (Rust)                 lenzu_server (Electron/TypeScript)
  ┌────────────────────┐   UDP      ┌────────────────────────────────┐
  │  send_to_overlay()  ├──────────►│  dgram socket → IPC → renderer │
  │  send_shutdown_cmd()├──────────►│  JSON dispatch (type field)    │
  │  send_hud_position()├──────────►│  window reposition             │
  └────────────────────┘            └────────────────────────────────┘
```

Transport: **UDP datagrams** (fire-and-forget, no ACK, no ordering). Three JSON envelopes:

| Type | Fields | Handler (`main.ts`) |
|------|--------|-------------------|
| `message` | `{text: string}` | `webContents.send('hud-text-changed')` |
| `position` | `{pos: "top"\|"bottom"}` | `positionWindow(pos)` |
| `shutdown` | `{}` | `closeSocket()` + `app.quit()` |

### 1.2 Key Source Files

**Rust client** (`lenzu/src/main.rs`):
- 3 UDP send functions: lines 81–96, 99–109, 277–283
- `spawn_server()` (line 190): launches Electron, passes UDP port via `LENZU_OVERLAY_UDP_PORT`
- `kill_server()` (line 225): 2-phase shutdown — UDP graceful → SIGKILL process group
- Dependencies: `tokio` (rt-multi-thread), `std::net::UdpSocket`, `serde_json`

**TypeScript server** (`lenzu_server/src/main.ts`):
- 146 lines, zero runtime npm dependencies
- `dgram` UDP socket bound to `127.0.0.1:7331`
- Electron `ipcMain` for main↔renderer (config, move, text)
- Config: `config.ts` (32 lines) — reads `hud_config.json`, merges with defaults

**Preload bridge** (`lenzu_server/src/preload.ts`): 19 lines, exposes 3 methods

**Renderer** (`lenzu_server/src/renderer/app.ts`): 62 lines, pure DOM updates

### 1.3 Design Constraints

- **Latest-only semantics**: "delivery order and acknowledgement are irrelevant — only the most recent text matters" (TechDesign.md)
- **Single consumer**: the renderer is the only subscriber to text events
- **Loopback-only**: communication is always `127.0.0.1`
- **No persistence**: the server has no database, no cache, no message log

---

## 2. Proposed gRPC Integration

### 2.1 Rationale

| Problem with UDP | gRPC Solution |
|-----------------|---------------|
| No type safety (ad-hoc JSON parsing) | Protobuf contract with code generation |
| No connection awareness (fire-and-forget) | Persistent HTTP/2 connection, keepalive detection |
| Fragile string dispatch (`cmd.type`) | Typed RPC methods |
| Port conflicts on restart | gRPC server name resolution |
| No server→client channel (unless UDP both ways) | Optional future streaming |

### 2.2 Service Definition

```protobuf
syntax = "proto3";
package lenzu_hud;

service LenzuHud {
  // Send translated text to be displayed on the HUD overlay.
  rpc SendText (TextMessage) returns (Ack);

  // Reposition the HUD window.
  rpc MoveWindow (WindowPosition) returns (Ack);

  // Gracefully shut down the HUD server.
  rpc Shutdown (Empty) returns (Ack);
}

message TextMessage {
  string text = 1;
}

message WindowPosition {
  // "top" or "bottom"
  string position = 1;
}

message Empty {}

message Ack {
  bool ok = 1;
}
```

**Design notes:**
- All RPCs are **unary** (request→response). No streaming — the "latest-only" semantics are preserved.
- `Shutdown` returns an `Ack` before the server exits (mirrors the current 100ms sleep pattern).
- No `StreamSession` / bidirectional streaming — that would imply server-side OCR/TTS processing, which does not exist.

### 2.3 Rust Client Changes

**File: `lenzu/Cargo.toml`** — add:

```toml
tonic = "0.12"
prost = "0.13"
```

**File: `lenzu/build.rs`** — new, runs protobuf code generation:

```rust
fn main() -> Result<(), Box<dyn std::error::Error>> {
    tonic_build::compile_protos("proto/lenzu_hud.proto")?;
    Ok(())
}
```

**File: `lenzu/src/main.rs`** — replace 3 UDP functions with a shared gRPC client:

```rust
pub mod lenzu_hud {
    tonic::include_proto!("lenzu_hud");
}

use lenzu_hud::lenzu_hud_client::LenzuHudClient;
use lenzu_hud::{TextMessage, WindowPosition, Empty};
use tonic::transport::Endpoint;

// Shared client handle, constructed once at startup.
struct HudClient {
    inner: LenzuHudClient<tonic::transport::Channel>,
}

impl HudClient {
    async fn connect(port: u16) -> Result<Self, tonic::transport::Error> {
        let addr = format!("http://127.0.0.1:{}", port + 1); // gRPC port = UDP port + 1
        let channel = Endpoint::new(addr)?.connect().await?;
        Ok(Self { inner: LenzuHudClient::new(channel) })
    }

    async fn send_text(&mut self, text: &str) {
        let _ = self.inner.send_text(TextMessage { text: text.into() }).await;
    }

    async fn move_window(&mut self, pos: &str) {
        let _ = self.inner.move_window(WindowPosition { position: pos.into() }).await;
    }

    async fn shutdown(&mut self) {
        let _ = self.inner.shutdown(Empty {}).await;
    }
}
```

**Changes from current code:**

| Current (`main.rs` lines) | Replacement | Benefit |
|---------------------------|-------------|---------|
| `send_to_overlay` (81–96) | `HudClient::send_text()` | Typed call, connection-aware |
| `send_shutdown_command` (99–109) | `HudClient::shutdown()` | No 100ms sleep needed (gRPC ACK) |
| `send_hud_position` (277–283) | `HudClient::move_window()` | Same interface, typed args |

**Lifecycle changes:**
- Create `HudClient` after `spawn_server()` returns (give server time to start gRPC)
- Pass gRPC port (UDP port + 1) via `LENZU_OVERLAY_GRPC_PORT` env var (same pattern as current UDP port)
- `kill_server()` no longer needs the UDP shutdown phase — gRPC `shutdown()` returns ACK, then SIGKILL the process group

### 2.4 TypeScript Server Changes

**File: `lenzu_server/package.json`** — add runtime dependency:

```json
{
  "dependencies": {
    "@grpc/grpc-js": "^1.12.0",
    "@grpc/proto-loader": "^0.7.0"
  }
}
```

**File: `lenzu_server/build.mjs`** — add proto compilation step. Since `@grpc/proto-loader` is dynamic at runtime, no build-time codegen is strictly needed. The server loads the `.proto` file at startup via `loadSync`.

**File: `lenzu_server/src/main.ts`** — replace `dgram` with gRPC server:

```
Changes (conceptual):
  Remove:  dgram import, socket creation, socket event handlers,
           socket.close(), socketClosed flag
  Add:     @grpc/grpc-js Server, proto loading, RPC handler implementations
  Keep:    ipcMain handlers, BrowserWindow, positionWindow(), config,
           app lifecycle
```

The gRPC server replaces lines 82–133 of the current `main.ts` (~50 lines of UDP code replaced by ~40 lines of gRPC code). The Electron IPC to renderer stays unchanged — the gRPC handlers call the same `webContents.send('hud-text-changed')` and `positionWindow()`.

**What stays exactly as-is:**
- `preload.ts` (19 lines) — unchanged
- `renderer/app.ts` (62 lines) — unchanged
- `config.ts` (32 lines) — unchanged
- `window-position.ts` (30 lines) — unchanged
- `hud_config.json` — unchanged, except `udp_port` is deprecated and `grpc_port` added

### 2.5 Migration Path

```
Phase 1: Dual-protocol (gRPC + UDP fallback)
  - Add gRPC server alongside existing UDP socket
  - Rust client tries gRPC first, falls back to UDP
  - Verify parity across all 3 operations

Phase 2: Remove UDP
  - Remove dgram socket, UDP env vars
  - Rust client uses gRPC exclusively
  - Clean up legacy JSON parsing in main.ts
```

---

## 3. Two Tracks: Effect Data Types vs. Effect Framework

Effect-TS ships two distinct layers. They have **very different cost/benefit profiles** for this codebase and must be evaluated separately.

### 3.1 The Split

| | Effect **data types only** | Effect **runtime framework** |
|---|---|---|
| What | `Option<T>`, `Either<E, T>`, `Chunk<T>` | `Effect.gen`, `Layer`, `Scope`, `ManagedRuntime`, fibers, `PubSub`, `Stream` |
| Dep size port | 1 module from `effect` (~5 KB tree-shaken) | Full `effect` + `@effect/platform` + `@effect/platform-node` (~50 KB) |
| Paradigm shift | None — drop-in for `T \| null`, try/catch | Full functional effect system |
| Payoff | Exhaustive null/error handling at type level | Structured concurrency, DI, resource scopes |
| Cost | One `import { Option } from "effect"` | Build integration, new patterns across main.ts |

### 3.2 Track A: Data Types Only (Option, Either)

These are lightweight functional primitives that can be adopted **incrementally** — one file at a time — without committing to the full Effect framework.

**Where `Option<T>` would help** (current code — `main.ts`):

```typescript
// Line 17 — nullable reference, null-checked before every use site
let mainWindow: BrowserWindow | null = null;

// With Option:
let mainWindow: Option<BrowserWindow> = Option.none();
// Use: Option.match(mainWindow, { onNone: ..., onSome: win => ... })
// Eliminates: if (mainWindow && !mainWindow.isDestroyed()) at 5 call sites
```

**Where `Either<E, T>` would help** (current code — `config.ts` lines 25–31):

```typescript
// Silent try/catch swallows parse errors
try {
  return { ...DEFAULT_CONFIG, ...JSON.parse(raw) };
} catch {
  return { ...DEFAULT_CONFIG };
}

// With Either:
return pipe(
  try(() => JSON.parse(raw) as HudConfig),
  Either.match({ onLeft: () => DEFAULT_CONFIG, onRight: v => ({ ...DEFAULT_CONFIG, ...v }) })
);
// Failure is explicit in the return type
```

**Other candidates:**

| Current pattern | File, lines | `Option`/`Either` benefit |
|----------------|-------------|--------------------------|
| `JSON.parse` in `try`/`catch` | `main.ts:97` | `Either` makes parse-failure an explicit path |
| `HudConfig` field access (could be missing) | `config.ts:26-31` | `Option` field access instead of silent defaults |
| `window.electronHUD` on global | `preload.ts:11` | `Option` — bridge may not be available in tests |

**However** — TypeScript 5.x strict null checks already catch most null-dereference bugs at compile time. The main practical benefit is *convenience* (`Option.map` / `Option.flatMap` chains vs. `if (x !== null) { ... }` guards).

**If you only want `Option<T>`** without committing to the Effect project at all, consider:

| Option | Size | Tradeoff |
|--------|------|----------|
| `effect/Option` (tree-shaken) | ~5 KB | Stable, maintained by Effect team, but pulls in module internals |
| [oxide.ts](https://github.com/tycho01/oxide.ts) | ~1 KB | Minimal, standalone, but unmaintained since 2022 |
| `T \| null` + TypeScript strict | 0 KB | Already works — just use `if (x)` guards |

**Recommendation: `T | null` is sufficient for now.** If null-checks proliferate, reach for `effect/Option` in isolation — no need for the full framework.

### 3.3 Track B: Framework (Deferred)

The full Effect framework (`Layer`, `Scope`, `Effect.gen`, `ManagedRuntime`) adds these capabilities:

| Capability | What it would replace in `main.ts` | Viable now? |
|-----------|-----------------------------------|-------------|
| DI Layer | Ad-hoc `loadConfig()`, global config variable | **Not needed** — config is loaded once with no dependency graph |
| Resource Scope | `socket.close()` in `before-quit` + error handler | **Not needed** — gRPC server lifecycle is a single socket |
| Structured concurrency | Callback nesting (gRPC handlers, IPC handlers, app lifecycle) | **Not yet** — 3 handler groups don't justify the abstraction |
| Error channel | `try/catch` around JSON parse + gRPC handler errors | **Overkill** — errors are trivial and local |

**What the framework does NOT add** (rejected from the original proposal):

| Feature | Why Not |
|---------|---------|
| `PubSub.bounded<LensEvent>(16)` | 1 subscriber, latest-only — `webContents.send()` is correct |
| `Stream.fromPubSub` → `Stream.bufferChunks({ sliding })` | No streaming pipeline exists on the server |
| `Effect.acquireRelease` for OCR/TTS | Processing runs on the Rust client, not the server |
| `@dr_nikson/effect-grpc` | Niche community package — use `@grpc/grpc-js` directly |

**Decision: Defer the framework.** Revisit if:
- The server grows beyond ~250 lines
- Multiple services need lifecycle management
- The config system becomes a dependency graph with multiple sources (env, file, CLI, remote)

### 3.4 What the Framework Would Look Like If Adopted Later

If and when the server crosses the complexity threshold, the migration would be:

```
config.ts (32 lines)
  → config-effect.ts (~40 lines)     — Config as a Layer

gRPC server in main.ts (~40 lines)
  → grpc-server.ts (~50 lines)        — gRPC lifecycle in Effect Scope

Electron app setup in main.ts (~90 lines)
  → electron-app.ts (~80 lines)       — BrowserWindow, IPC, positioning

main.ts (146 lines)
  → main.ts (~30 lines)               — Composes the 3 Layers, runs the program
```

This restructure can happen **without touching** preload.ts, app.ts, or window-position.ts. No pub/sub, no streams, no fibers-per-message — just `Layer` + `Scope` for resource-safe startup/shutdown.

---

## 4. Migration Plan

### Phase 0: Protobuf Definition + Code Generation (1-2 days)

- [ ] Create `proto/lenzu_hud.proto` in repo root (shared between Rust and TS)
- [ ] Add `tonic-build` + `prost` to Rust build.rs
- [ ] Add `@grpc/grpc-js` + `@grpc/proto-loader` to lenzu_server
- [ ] Verify code generation in both languages

### Phase 1: gRPC Adoption (3-5 days)

- [ ] Add gRPC server to `main.ts` alongside existing UDP socket (dual-protocol)
- [ ] Add `HudClient` to Rust `main.rs`, replace 3 UDP functions
- [ ] Pass gRPC port via `LENZU_OVERLAY_GRPC_PORT` env var
- [ ] Test parity: SendText, MoveWindow, Shutdown
- [ ] Remove UDP code from both client and server

### Phase 2a: Effect Data Types (ad-hoc, low-effort)

**Do this when a specific nullable/error pattern becomes annoying in a file.**

- [ ] `npm install effect` (tree-shaken by esbuild — only used imports ship)
- [ ] Replace `BrowserWindow | null` → `Option<BrowserWindow>` in main.ts
- [ ] Replace `try/catch` JSON.parse → `Either` in main.ts (lines 97–117)
- [ ] Replace `try/catch` config loader → `Option` or `Either` in config.ts

Each replacement is file-local, ~5-10 lines changed, no architectural impact. Can be done incrementally or not at all.

### Phase 2b: Effect Framework (deferred)

**Only if the server grows beyond ~250 lines or gains additional services.**
*The data types from Phase 2a are a prerequisite — once `Option`/`Either` are in the type vocabulary, the framework layers on naturally.*

- [ ] Refactor config.ts → config-effect.ts (Config as an Effect Layer)
- [ ] Extract gRPC server lifecycle into Effect Scope
- [ ] Compose Electron + Config + gRPC layers in main.ts

### What Never Changes

- `preload.ts` — stays minimal, no Effect
- `renderer/app.ts` — stays minimal, no Effect
- `window-position.ts` — stays a pure function
- Electron IPC (`ipcMain.handle`, `webContents.send`) — remains the main↔renderer bridge

---

## 5. Tradeoffs Summary

| | gRPC | Effect data types | Effect framework |
|---|------|-------------------|-----------------|
| **Lines changed (Rust)** | ~30 added, ~20 removed | 0 | 0 |
| **Lines changed (TS)** | ~40 added, ~50 removed | ~15 added, ~15 removed | ~80 added, ~30 removed |
| **New deps (Rust)** | `tonic`, `prost`, `tonic-build` | None | None |
| **New deps (TS)** | `@grpc/grpc-js`, `@grpc/proto-loader` | `effect` (tree-shaken to ~5 KB) | `effect` + `@effect/platform` + `@effect/platform-node` |
| **Bundle size impact** | ~200 KB (gRPC core) | ~5 KB (Option/Either only) | ~50 KB |
| **Architectural value** | Type-safe transport, connection management | Explicit null/error in type signatures | Structured lifecycle, DI, resource scopes |
| **Paradigm shift** | None (protobuf is additive) | Minimal (drop-in replacement for `?`/`!`) | Significant (`Effect.gen`, `Layer`, `Scope`) |
| **Risk** | Low | Very low | Low-Medium |
| **Recommendation** | **Adopt** — clear win over fragile UDP | **Adopt ad-hoc** — pull in `effect/Option` when a specific nullable pattern becomes painful | **Defer** — revisit at 250+ lines or when multiple lifecycle services emerge |
