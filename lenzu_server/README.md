# lenzu-hud — Electron transparent overlay HUD

A transparent, click-through desktop overlay that displays OCR and translation
results as subtitles.  Receives text over **gRPC** (primary) with **UDP** fallback,
both on loopback.

Part of the [Lenzu](https://github.com/CodeMonkeyNinja/lenzu) project — the GTK4
lens client (`lenzu`) auto-spawns and manages this process.

![simplescreenrecorder-2026-03-23_18 57 28](https://github.com/user-attachments/assets/b65d6be5-2592-48b9-858d-998f8c873cd8)

---

## Features

| Feature | Detail |
|---|---|
| Transparent window | RGBA compositing via Electron / Chromium |
| Click-through | Mouse events pass to windows underneath |
| Position control | `top` or `bottom` screen edge; toggle with ArrowUp |
| gRPC API | `SendText`, `MoveWindow`, `Shutdown` RPCs (port UDP+1) |
| UDP fallback | JSON datagrams on `udp_port` when gRPC unavailable |
| Effect-TS pipeline | Schema-validated config, Queue-based dispatch, Layer DI |
| Render modes | original, english, furigana, romaji, all, debug |

---

## Prerequisites

- **Node.js** ≥ 22 (see `.nvmrc`)
- **pnpm** (package manager)
- A **compositing window manager** on X11 (GNOME, KDE, XFCE with xfwm4 compositing)
- **libx11-dev** (`apt install libx11-dev`) — for X11 override_redirect helper

---

## Setup

```bash
pnpm install
pnpm run build
```

The build step compiles:
- TypeScript → JavaScript via esbuild (3 entry points: `main.ts`, `preload.ts`, `renderer/app.ts`)
- C helper `scripts/hud-set-override-redirect.c` → binary (for X11 decoration removal)

---

## Running

### Spawned by Lenzu (normal usage)

The GTK4 client auto-spawns this process.  Just run the client:

```bash
cd /path/to/lenzu
./scripts/run.sh
```

### Standalone (development)

```bash
# Start the HUD (terminal 1)
cd lenzu_server
GTK_CSD=0 node_modules/.bin/electron dist/main.js

# Send test messages (terminal 2)
node test_sender.js
```

---

## Configuration (`hud_config.json`)

Optional file in `lenzu_server/` root.  Missing fields fall back to defaults.

| Key | Type | Default | Description |
|---|---|---|---|
| `height` | number | `200` | Window height in px |
| `background_opacity` | number | `0.72` | Background opacity (0.0–1.0) |
| `text_color` | string | `"#f5e642"` | Caption text colour (CSS hex) |
| `font_size_pt` | number | `24` | Caption font size in points |
| `min_font_size_pt` | number | `0` | Minimum font size (0 = no shrink) |
| `bottom_margin` | number | `50` | Distance from screen bottom edge (px) |
| `udp_port` | number | `7331` | UDP listen port |
| `default_text` | string | `"Hello world, Hello Shiroe!"` | Startup placeholder text |

The gRPC port is `udp_port + 1` (default 7332), overridable via
`LENZU_OVERLAY_GRPC_PORT` env var.

---

## Protocol

### gRPC (port 7332, primary)

Three RPCs defined in `proto/lenzu_hud.proto` (service `LenzuHud`):

| RPC | Request | Effect |
|---|---|---|
| `SendText` | `TextMessage { text }` | Display text in HUD |
| `MoveWindow` | `WindowPosition { pos }` | Reposition to `"top"` or `"bottom"` |
| `Shutdown` | `Empty` | Graceful quit |

### UDP (port 7331, fallback)

JSON datagrams accepted when gRPC is unavailable:

| type | Fields | Effect |
|---|---|---|
| `message` | `text: string` | Display text in HUD |
| `plaintext` | `text: string` | Display text in HUD |
| `position` | `pos: "top" \| "bottom"` | Reposition overlay |
| `shutdown` | *(none)* | Quit |

Unparseable UDP payloads are treated as `{ type: "plaintext", text: "<raw>" }`.

---

## Architecture

```
┌──────────────────────────────────────────────────────────────────┐
│  Electron Main Process (src/main.ts)                             │
│                                                                  │
│  Effect.runPromise :: provide(AppLayer)                          │
│  ┌──────────────────┐  ┌──────────────────┐  ┌───────────────┐   │
│  │ HudConfigLive     │  │ UdpSocketLive    │  │ gRPC server   │   │
│  │ hud_config.json   │  │ dgram udp4       │  │ lenzu_hud.proto│   │
│  │ Schema.decode     │  │ Queue.offer(cmd)  │  │ 127.0.0.1:7332│   │
│  └───────┬──────────┘  └────────┬─────────┘  └───────┬───────┘   │
│          └── Layer.provideMerge ─┘                    │           │
│                        │                              │           │
│                        ▼                              │           │
│          ┌──────────────────────────┐                │           │
│          │  Effect.forever(Queue    │◄───────────────┘           │
│          │  .take) → processMessage │                            │
│          │  "message" → sendText()  │                            │
│          │  "position"→ reposition()│                            │
│          │  "shutdown" → app.quit() │                            │
│          └────────┬─────────────────┘                            │
│                   │                                              │
│  ┌────────────────▼──────────────────────────────────────────┐   │
│  │  BrowserWindow (transparent, frameless, alwaysOnTop)       │   │
│  │  · webContents.send("hud-text-changed")                    │   │
│  │  · IPC: get-config, move-window                            │   │
│  │  · X11 override_redirect via C helper (decoration removal) │   │
│  └──────────────────────┬─────────────────────────────────────┘   │
│                         │ preload.ts (contextBridge)               │
│                         ▼                                          │
│  ┌──────────────────────────────────────────────────────────┐     │
│  │  Renderer (src/renderer/app.ts) — effect-TS               │     │
│  │  · SynchronizedRef<RendererState>                         │     │
│  │  · Queue → renderText() into #subtitle-text               │     │
│  │  · ArrowUp/Down → IPC move-window (top/center/bottom)     │     │
│  │  · CSS: rgba background, text-shadow                     │     │
│  └──────────────────────────────────────────────────────────┘     │
└──────────────────────────────────────────────────────────────────┘
```

---

## Testing

```bash
pnpm vitest
```

Test files in `src/__tests__/`:

| File | Tests |
|---|---|
| `config.test.ts` | Config loading, Schema decode, defaults merge |
| `dispatch.test.ts` | `processMessage` routing (message/position/shutdown) |
| `grpc-handlers.test.ts` | `handleSendText` with test layers |
| `composition.test.ts` | Layer wiring end-to-end |
| `window-position.test.ts` | Top/center/bottom position math |

---

## X11 notes

- **Client-side decorations**: `GTK_CSD=0` env var disables them for a cleaner frameless window.
- **override_redirect**: The C helper (`scripts/hud-set-override-redirect.c`) removes X11 window decorations that Electron cannot hide. Compiled via `build.mjs`; non-fatal if `libx11-dev` is missing.
- **Transparency**: Requires a compositing window manager. XFCE users enable with `xfconf-query -c xfwm4 -p /general/use_compositing -s true`.

---

## Packaging

```bash
pnpm run deb    # electron-builder → dist-deb/ (deb + AppImage)
```
