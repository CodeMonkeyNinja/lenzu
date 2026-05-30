## Lenzu — transparent OCR lens overlay for Linux

### What's new in v0.1.7

- **HUD color/opacity regression fix** — yellow text (`#f5e642`) and semi-transparent background (`0.45`) restored. A prior schema-alignment commit renamed `hud_config.json` keys from camelCase to snake_case, accidentally activating a dormant cyan `#00FFCC` and opaque `0.85` that had always been in the file but were silently ignored because the camelCase keys didn't match the TypeScript interface.

### What's new in v0.1.6

- **AppImage HUD fix** — `lenzu-hud` now resolves via `$APPDIR/usr/bin/lenzu-hud` when running inside the AppImage. v0.1.5 shipped with the Electron HUD silently failing to spawn (ENOENT) because AppRun sets `APPDIR` but never adds `usr/bin` to `PATH`.

### What's new in v0.1.5

- **Single-instance guard** — a second `lenzu` launch detects the running instance via PID file and exits cleanly instead of spawning a duplicate lens + HUD.
- **HUD orphan fix** — the Electron HUD now exits automatically whenever lenzu dies (clean exit, crash, or external kill) via `prctl(PR_SET_PDEATHSIG)`.
- **`onnx` feature on by default** — plain `cargo build` no longer silently produces a crippled binary with text detection disabled. Use `--no-default-features` to opt out.
- **Dependency security bumps** — `manga-ocr-rs` 0.1.3 → 0.1.4, `jp_detect` 0.2.3 → 0.2.4.
- **Docs** — expanded architecture rationale: why Electron for the HUD (WebKit2GTK ghost-text bug, Lottie/animation plans), Wayland TODO with performance analysis, deleted orphaned capture stubs.

A draggable magnifier that captures whatever is under it, OCR's any Japanese text,
and overlays the result (with optional furigana / LLM enrichment) as a click-through HUD.

### Download

Grab a single file from the **Assets** section below:

- **`lenzu-bundle-X.Y.Z.tar`** — recommended. Contains the AppImage + both model
  sidecars + an installer script + `run.sh` (~560 MB). One file, one untar, you're done.
- Or download the four pieces separately if you only want the AppImage and not the
  manga-ocr fallback path.

### Quick start

```bash
# 1. Extract the bundle
tar -xf lenzu-bundle-*.tar
cd lenzu-bundle-*/

# 2. Install the model sidecars to ~/.local/share/lenzu/
LENZU_RELEASE_BASE=file://$(pwd) ./lenzu-appimage-installer.sh

# 3. Run it
./run.sh                       # full path: ollama / OpenRouter enrichment
./run.sh --furigana_only       # offline path: DBNet + manga-ocr, no LLM
```

`run.sh` checks if `ollama` is reachable on `localhost:11434` (native install or
Docker container), starts a `lenzu-ollama` Docker container if Docker is available
but ollama isn't, and falls through to OpenRouter (set `OPENROUTER_API_KEY=sk-...`)
if neither is available. To run without any LLM enrichment, use `--furigana_only`.

### One system dependency

The AppImage cannot bundle MeCab (libc-linked dictionary lookups), so install it
via apt:

```bash
sudo apt install mecab mecab-ipadic-utf8
```

Without MeCab, furigana annotation will be skipped.

### What's in the bundle

| File | Purpose |
|------|---------|
| `lenzu-X.Y.Z-x86_64.appimage` | Rust client + Electron HUD, statically packaged |
| `lenzu-models-dbnet-0.2.0.tar.xz` | AGPL-3.0 DBNet text-detection ONNX |
| `lenzu-models-manga-ocr-0.1.0.tar.xz` | Apache-2.0 manga-ocr ONNX (~340 MB xz) |
| `lenzu-appimage-installer.sh` | Untars the two model sidecars to `~/.local/share/lenzu/` |
| `run.sh` | ollama / Docker preflight + launches the AppImage |
| `README.txt` | Same quick-start, on-disk |

### Licensing

- **lenzu** itself is MIT.
- **DBNet** model (`stabrise-text_detection_dbnet_ml_v02_model.onnx`) is **AGPL-3.0**
  and shipped as a separate sidecar tarball — it is *not* bundled into the
  MIT-licensed AppImage.
- **manga-ocr** models (`mayocream/manga-ocr-onnx`) are Apache-2.0.
- See `usr/share/doc/lenzu/NOTICES.md` inside the AppImage for the full
  per-crate attribution.

### Issues / source

https://github.com/CodeMonkeyNinja/lenzu
