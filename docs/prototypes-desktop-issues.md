# Prototypes Desktop Issues — GTK3/4, Windowing, Overlays

**Scope:** `lenzu-prototypes` workspace investigations into GTK3→4 migration,
transparent overlay windows, X11 pointer tracking, and related desktop issues.

**Counterpart:** `docs/lenzu-desktop-issues.md` (main repo perspective)

---

## 1. GTK3→GTK4 Migration — Complete

**Status as of 2026-06-21:** All GTK3 workspace members have been ported to GTK4
(gtk4-rs 0.11.x / glib 0.22.x). The workspace is now fully GTK4.

**Members migrated:**
- [`jp_ocr_app`](https://github.com/HidekiAI/lenzu-prototypes/tree/trunk/prototypes/jp_ocr_app) (was GTK3 0.18.2) → GTK4 0.11.3, with x11rb pointer tracking
- [`x11-gtk-lens-test`](https://github.com/HidekiAI/lenzu-prototypes/tree/trunk/prototypes/x11-gtk-lens-test) (was [`x11-gtk3-lens-test`](https://github.com/HidekiAI/lenzu-prototypes/tree/trunk/prototypes/x11-gtk3-lens-test), GTK3 0.18.2) → GTK4 0.11.3
- [`gtk4_dialogbox_test`](https://github.com/HidekiAI/lenzu-prototypes/tree/trunk/prototypes/gtk4_dialogbox_test) (was GTK4 0.8.1) → 0.11.3
- [`gtk_gdk_test`](https://github.com/HidekiAI/lenzu-prototypes/tree/trunk/prototypes/gtk_gdk_test) (was GTK4 0.8.1) → 0.11.3

### Key Workarounds Used

| Problem | Workaround |
|---------|------------|
| `gdk::Screen::default()` removed — no root window for pointer tracking | Use `x11rb::query_pointer()` on root window directly |
| `surface.move_to()` not exposed in gtk4-rs 0.11 | Use x11rb `configure_window()` with `ConfigureWindowAux::new().x(y).y(y)` |
| `surface.input_shape_combine_region()` not exposed | Use `surface.set_input_region(Some(&region))` with empty `Region` |
| `window.set_keep_above(true)` removed | Use x11rb `_NET_WM_STATE` ClientMessage protocol |
| `gdk::keys::constants` module is private | Use `gdk::Key::Escape` directly |
| `pangocairo::show_layout()` not at crate root | Use `pangocairo::functions::show_layout()` |
| `gdk_pixbuf::CairoContextExt` not at crate root | Use `gdk_pixbuf::prelude::*` and call `cr.set_source_pixbuf()` as trait method |
| `style_context().add_provider()` deprecated since GTK4 4.10 | Use `gtk4::style_context_add_provider_for_display()` (raw FFI) |
| `window.display()` ambiguous (`RootExt` vs `WidgetExt`) | Use `gtk4::prelude::RootExt::display(&window)` explicitly |
| `OnceLock::get_or_try_init()` unstable | Use manual `set()` + `get()` pattern |
| glib `clone!(@strong/@weak)` syntax removed | Use `clone!(#[strong]/#[weak])` attribute syntax in glib 0.22 |
| `X11Surface::xid()` returns `u64`, x11rb expects `u32` | Cast: `x11_surface.xid() as u32` |
| `cairo::Region::create()` returns `Region` not `Option` | Assign directly, no `if let Some` needed |
| `window.show()` deprecated | Use `window.present()` |
| `configure_window` API: x11rb 0.13 `ClientMessageEvent::new()` | Takes 4 args (format, window, atom, ClientMessageData), not 8 |
| `KeyButMask::BUTTON_1` renamed | Use `KeyButMask::BUTTON1` (no underscore) |

### X11 Window Management Architecture

Both lens prototypes use this pattern for X11-specific features:

```
x11rb connection (singleton via OnceLock)
  ├── query_pointer() → cursor tracking (replaces GdkScreen)
  ├── configure_window() → positioning (replaces surface.move_to())
  ├── _NET_WM_STATE → keep-above (replaces set_keep_above())
  └── send_event() → stacking (replaces GdkToplevel)
```

The window XID is obtained from `gdk4_x11::X11Surface::xid()` after
`window.present()`. An earlier approach using `_NET_WM_PID` scanning of root
window children proved unreliable (see §5).

---

## 2. Pitfalls & Lessons Learned (Post-Migration Runtime Issues)

Even after the code compiles cleanly, several runtime issues were discovered
that required additional fixes:

### 2a. Window XID Discovery: `X11Surface::xid()` vs PID Scanning

**Wrong approach:** Scan root window children matching `_NET_WM_PID`:

```rust
// DON'T — fragile, fails in practice
let tree = conn.query_tree(root).ok()?.reply().ok()?;
for &child in &tree.children {
    let pid = get_property(child, "_NET_WM_PID");
    if pid == std::process::id() { /* found it */ }
}
```

**Why it fails:**
- The GTK4 window may not be mapped yet when the first poll fires.
- Window managers (especially xfwm4, mutter) reparent the client window under
  a frame window that is NOT a direct child of root — the scan never finds it.
- `_NET_WM_PID` is not always set immediately after `present()`.

**Correct approach:** Use GDK4's own X11 surface, always available after
`present()`:

```rust
// DO — works every time on X11
let surface = window.surface()?;
let x11_surface = surface.downcast::<gdk4_x11::X11Surface>().ok()?;
let xid = x11_surface.xid() as u32;   // u64 → u32 for x11rb
```

**Must call AFTER `window.present()`** — `window.surface()` returns `None` if
the window is not yet realized.

### 2b. x11rb Connection Init Must Not Panic

**Wrong approach:**
```rust
let conn = ONCE.get_or_init(|| RustConnection::connect(None).ok().unwrap());
//                                                       ^^^^^^^^ PANICS
```

If `$DISPLAY` is unset or the X server is unreachable, the `.unwrap()` kills
the entire app. **Always** handle X11 connection failure gracefully:

```rust
fn init_x11() -> bool {
    if ONCE.get().is_some() { return true; }
    if let Ok((conn, _)) = RustConnection::connect(None) {
        let _ = ONCE.set(conn);
        true
    } else {
        false
    }
}
```

All cursor-tracking and window-management functions should be no-ops when X11
is unavailable (the app remains functional as a static overlay).

### 2c. `set_window_state` (keep-above) Must Follow `present()`

```rust
window.present();                    // must be first
set_window_state(&window);           // window.surface() is now Some
input_shape_clickthrough(&window);   // same
```

Calling `set_window_state()` before `present()` silently fails because
`window.surface()` returns `None`.

### 2d. `surface.set_input_region()` Takes `Option<&Region>`

```rust
// Correct:
let region = cairo::Region::create();
surface.set_input_region(Some(&region));   // takes Option<&Region>
```

Not `&Region`. The `Option` wrapper was silently wrong in earlier versions.

### 2e. `RefCell` Borrow Must Be Dropped Before Blocking Calls

**Wrong approach — holds mutable borrow across blocking sequence:**

```rust
// DON'T — RefCell panic when draw/animation callbacks fire during iteration
let mut s = state.borrow_mut();
s.is_loading = true;
window.set_visible(false);
while glib::MainContext::default().iteration(false) {}  // ← timer/draw callbacks
std::thread::sleep(Duration::from_millis(400));          //   try to borrow state
s.pixels = Some(...);
```

Holding a `RefCell` mutable guard across `glib::MainContext::iteration(false)`
or `std::thread::sleep()` causes a **panic** when other callbacks (draw function,
animation timer) try to `borrow()`/`borrow_mut()` the same `RefCell`.

**Correct approach — scope-guard the mutable borrow:**

```rust
// DO — drop the mutable guard before the blocking sequence
{
    let mut s = state.borrow_mut();
    s.is_loading = true;
    s.status = "CAPTURING...";
}
window.set_visible(false);
while glib::MainContext::default().iteration(false) {}
std::thread::sleep(Duration::from_millis(400));
// re-borrow after blocking
let mut s = state.borrow_mut();
s.pixels = Some(...);
```

Check also with `try_borrow()` / `try_borrow_mut()` for the initial condition
check rather than calling `borrow_mut()` directly, to avoid panicking if another
borrow is active.

### 2f. `Session`/Model Loading Must Be Optional

The ONNX detection prototype ([`x11-gtk-lens-test`](https://github.com/HidekiAI/lenzu-prototypes/tree/trunk/prototypes/x11-gtk-lens-test)) panicked on startup with
`.expect("No valid .onnx file found (>1MB)!")` when no model file was in cwd.
Always make model loading graceful:

```rust
// DON'T — panics when no .onnx file exists
let model = Session::builder().unwrap()
    .commit_from_file(&model_path).unwrap();

// DO — model is Option<Session>
let model = fs::read_dir(".")
    .ok()?
    .find(|p| p.extension() == Some("onnx"))
    .and_then(|p| Session::builder().ok()?.commit_from_file(&p).ok());
```

All inference and detection code should check `if let Some(ref mut m) = model`
before attempting to run the model.

### 2g. Transparent Lens: Draw Must Clear with RGBA(0,0,0,0) + Operator::Source

**Wrong approach — paints opaque black over the entire lens area:**

```rust
// DON'T — opaque black fills the window, hides transparent CSS background
cr.set_source_rgb(0.0, 0.0, 0.0);
cr.paint().ok();
```

The CSS `window { background: transparent; }` alone is insufficient — the
DrawingArea's `draw_func` paints over it with opaque black, blocking any
transparency.

**Correct approach — clear with fully transparent colour using Source operator:**

```rust
// DO — transparent clear, then switch back to Over for overlays
cr.set_source_rgba(0.0, 0.0, 0.0, 0.0);
cr.set_operator(cairo::Operator::Source);
cr.paint().ok();
cr.set_operator(cairo::Operator::Over);
```

The `Operator::Source` replaces the existing pixels entirely (including the
window's transparent background) with the source colour. Switching back to
`Operator::Over` after the clear ensures subsequent drawing operations
composite correctly.

This pattern is used in [`jp_ocr_app`](https://github.com/HidekiAI/lenzu-prototypes/tree/trunk/prototypes/jp_ocr_app) and was applied to [`x11-gtk-lens-test`](https://github.com/HidekiAI/lenzu-prototypes/tree/trunk/prototypes/x11-gtk-lens-test)
in the same fix.

---

### 2h. SCIM Stderr Noise — GTK4 GDK Auto-Launches Broken SCIM

Both [`jp_ocr_app`](https://github.com/HidekiAI/lenzu-prototypes/tree/trunk/prototypes/jp_ocr_app) and [`x11-gtk-lens-test`](https://github.com/HidekiAI/lenzu-prototypes/tree/trunk/prototypes/x11-gtk-lens-test) print to stderr on every launch:

```
Loading socket Config module ...
Creating backend ...
Loading x11 FrontEnd module ...
Failed to load x11 FrontEnd module.
```

**Source:** These messages come from **SCIM** (Smart Common Input Method), a CJK
input method framework. GTK4's GDK X11 backend auto-detects SCIM and forks a
`scim-launcher -d -c socket -e socket -f x11 --no-stay -d` child process.

The launch sequence (observed via strace):
1. GTK4 GDK X11 calls `scim -h` to check availability.
2. `scim-im-agent` is spawned.
3. It launches `scim-launcher -f x11`, which fails.

**Why it fails:** The X11 frontend module at
`/usr/lib/x86_64-linux-gnu/scim-1.0/1.4.0/FrontEnd/x11.so` loads (all shared
library deps resolve) but internal initialization fails. SCIM also looks for
`/etc/scim/global` and `~/.scim/global` config files, which don't exist in
stock installs.

**Why `GTK_IM_MODULE=` doesn't help:** The SCIM launch is built into GDK's X11
backend source code, independent of GTK's IM module system. Neither
`GTK_IM_MODULE=ibus` nor `GTK_IM_MODULE=` empty suppresses it. An already-
running SCIM daemon (launched at session login via `scim-launcher -c simple
-e all -f socket`) coexists but doesn't affect the GTK4 child process.

**Impact:** Cosmetic only — the messages go to stderr from the forked child
process. Our GTK app runs unaffected. No functionality is degraded.

**To suppress:**
- Uninstall SCIM (`sudo apt remove scim`), or
- Redirect stderr (`cargo run -p jp_ocr_app 2>/dev/null`), or
- Create `/etc/scim/global` or `~/.scim/global` config (SCIM repeatedly looks
  for these; missing config may trigger the init failure).

---

## 3. Workspace Dependency Graph (Post-Migration)

```
lenzu-prototypes workspace (all GTK4)
├── GTK4 0.11 members: jp_ocr_app, x11-gtk-lens-test, gtk_gdk_test, gtk4_dialogbox_test
│   └── gtk4 0.11.3 → glib 0.22.7, gdk-pixbuf 0.22.0, pango 0.22.6, pangocairo 0.22.0
│   └── gdk4-x11 0.11.0 (for XID queries)
│
├── No-GTK members: winit-test, dbnet-test, manga-ocr-test, sarashina-onnx-test, etc.
└── libraronity (workspace member in other repo)
```

All members now use the same glib/gdk-pixbuf/pango generation (0.22.x),
eliminating the unsound dual-glib state.

### Remaining workspace issues
- `gtk3 0.18.2` (the final GTK3 release, marked UNMAINTAINED) is no longer a
  dependency in any workspace member, but Cargo.lock may retain it if any
  transitive dep references it.
- `gdk4-wayland` and `gdk4-win32` are deps of `gtk_gdk_test` only (for platform
  testing), pinned to 0.11.x matching the gtk4 generation.

---

## 4. Dependabot PR Situation (as of 2026-06-20)

**5 open PRs**, all dependabot bumps, all failing `review / review` check:

| # | Crate | Bump | Files | Status |
|---|-------|------|-------|--------|
| #35 | `tokenizers` | 0.20.4 → 0.23.1 | `Cargo.toml`+`Cargo.lock` | Failing review check |
| #34 | `gdk4-win32` | 0.8.2 → 0.11.0 | `Cargo.lock` only | Failing review check |
| #33 | `gdk-pixbuf` | 0.19.8 → 0.22.0 | `Cargo.lock` only | Failing review check |
| #32 | `gtk4` | 0.8.2 → 0.11.3 | `Cargo.lock` only | Failing review check |
| #31 | `pangocairo` | 0.18.0 → 0.22.0 | `Cargo.lock` only | Failing review check |

All fail on a Gemini AI review step (infrastructure timeout), not code issues.
PRs #31–#34 bump GTK4-rs crates and are tightly coupled — they must land together
or not at all. PR #35 (tokenizers) is independent.

**Note:** PRs #31–#34 are now effectively superseded — the workspace already
uses gtk4 0.11.x / glib 0.22.x ecosystem. These PRs only affect `Cargo.lock`.

---

## 5. Decision Log

| Date | Decision | Rationale |
|------|----------|-----------|
| Pre-2026-04 | GTK4 evaluated and rejected | graphene/gobject dep complexity, API churn, build failures |
| 2026-04 | Electron for HUD (not GTK) | X11 transparency works, Web UI flexibility |
| 2026-06-20 | GTK3 lens prototypes kept on 0.18.x | GdkScreen removal blocks GTK4 migration; GTK3 works for current needs |
| 2026-06-20 | Shared docs convention established | `lenzu/docs/prototypes-desktop-issues.md` (from here) + `lenzu/docs/lenzu-desktop-issues.md` (from main repo) |
| 2026-06-21 | **GTK3→GTK4 migration completed** | All 4 GTK members ported to gtk4-rs 0.11.x / glib 0.22.x; x11rb replaces removed GDK APIs for X11-specific window management; both lens prototypes compile and link cleanly |
| 2026-06-21 | GdkScreen workaround finalized | x11rb `query_pointer()` + `_NET_WM_STATE` + `configure_window()` — stable, X11-only, no GDK dependency for window management |
| 2026-06-21 | **PID scanning for XID abandoned** | `_NET_WM_PID` on root window children is unreliable (WM reparenting, unmapped windows, timing). Use `gdk4_x11::X11Surface::xid()` after `present()` instead. |
| 2026-06-21 | **x11rb connection must init gracefully** | `get_or_init(|| connect(None).unwrap())` panics on missing `$DISPLAY`. Use manual `set()` with `bool` return instead. |
| 2026-06-21 | **`set_window_state`/`surface()` requires realization** | `window.surface()` returns `None` before `window.present()`. All surface/XID access must happen after. |
| 2026-06-21 | **Unit tests added for known runtime panics** | 9 tests across `jp_ocr_app` (4) and `x11-gtk-lens-test` (5) catch RefCell borrow-across-blocking panics, Pixbuf creation, model-loading graceful failure, and try_borrow pattern — now verifiable in `cargo test` without X11 display. |
| 2026-06-21 | **`x11-gtk-lens-test` transparent draw fix** | Draw function used `cr.set_source_rgb(0,0,0)` → `paint()`, filling the lens with opaque black and hiding the transparent CSS window background. Changed to `rgba(0,0,0,0)` + `Operator::Source` + switch back to `Over` (matching `jp_ocr_app` pattern). |
| 2026-06-21 | **SCIM stderr noise identified** | Both lens apps print SCIM init errors on every launch. Traced to GTK4 GDK X11 backend auto-forking `scim-launcher -f x11`. Cosmetic only — app unaffected. `GTK_IM_MODULE` has no effect; suppress via stderr redirect or `apt remove scim`. Documented in §2h. |
| 2026-06-21 | **Trunk GTK3 dual-glib fix + CI** | `gdk-pixbuf = \"0.19\"` in GTK3 members conflicted with `gdk 0.18`'s internal `gdk-pixbuf 0.18.5`. Pinned to `0.18`. Added GitHub Actions CI (`cargo test --workspace`). Removed all Gemini AI workflows. |
| 2026-06-21 | **jp_ocr_app spinner lollipop tail fixed** | Cairo `arc()` draws an implicit line from the current path point to the arc start. `show_layout()` leaves the current point at the last glyph, so the arc was connected to it — producing a straight "lollipop tail". Fixed by inserting `cr.new_sub_path()` before `cr.arc()`. See §2i. |
| 2026-06-21 | **jp_ocr_app white window + frozen spinner during capture fixed** | `flash_alpha=1.0` painted the entire lens area with an opaque white rectangle on every capture, causing the "white window". Additionally, `std::thread::sleep(400ms)` blocked the GTK main loop, preventing the animation timer from firing and keeping the spinner frozen (static). Fixed by removing `flash_alpha` entirely and replacing the blocking sleep with `glib::timeout_add_local(400ms)`. See §2j. |

---

## 2i. Spinner Lollipop Tail — jp_ocr_app (FIXED 2026-06-21)

**Symptom:** The loading spinner in the UI panel appeared distorted — a
straight line extended from the spinner arc to an off-center point, like
a lollipop stick.

**Root cause:** Cairo's `cr.arc()` does NOT start a fresh path. If a current
point exists in the path, `arc()` implicitly draws a straight `line_to()` from
that point to the arc's start position before drawing the arc itself.

The draw function calls `pangocairo::functions::show_layout()` to render the
status text (e.g. "CAPTURING...") just before drawing the spinner. PangoCairo's
`show_layout()` leaves the cairo current point at the last glyph position in the
layout. The subsequent `cr.arc()` then drew a line from that glyph position back
to the arc's start, creating the visible tail artifact.

**Fix:**

```rust
cr.new_sub_path();   // ← break the implicit line-to
cr.arc(0.0, 0.0, 8.0, 0.0, 1.5 * std::f64::consts::PI);
```

`cr.new_sub_path()` starts a new sub-path without moving the current point,
so Cairo has no start point to draw a line from. The same fix applies to any
`arc()` call that follows text rendering or other drawing that leaves an open
path.

**Commits:** `be26043` (jp_ocr_app); same pattern fixed in `lenzu` at `75f7dbf`.

---

## 2j. White Window and Frozen Spinner During Capture — jp_ocr_app (FIXED 2026-06-21)

**Symptoms:**
- The lens window turned solid white for ~160ms whenever a screen capture
  was triggered.
- The loading spinner appeared static/frozen during the capturing phase
  (no rotation), then suddenly jumped to a new angle when the OCR result
  arrived.

### White window

**Root cause:** `flash_alpha = 1.0` was set immediately after capture
success, painting a fully opaque white `cr.rectangle()` over the entire
400×400 lens area (via `cairo::Operator::Over`). The animation timer
decremented `flash_alpha -= 0.1` per 16ms frame, fading it back to zero
over ~160ms. During that fade the lens appeared white.

**Fix:** Removed `flash_alpha` entirely — the field was deleted from
`AppState`, dropped from the draw function, and the animation step removed
from the 16ms timer. The lens now shows the captured image directly with
no overlay.

### Frozen spinner

**Root cause:** The capture flow called `std::thread::sleep(400ms)` on the
main GTK thread to give the compositor time to hide the window before
`GetImage`. Sleeping on the main thread blocks the glib main loop entirely,
preventing every registered `glib::timeout_add_local` callback — including
the 16ms animation timer — from firing. As a result `spinner_angle` was
never incremented during the entire 400ms wait, so the spinner appeared as
a static arc.

**Fix:** Replaced the blocking sleep with `glib::timeout_add_local(400ms, ...)`:

```rust
// Before (blocks main loop — animation timer cannot fire):
window_poll.set_visible(false);
while glib::MainContext::default().iteration(false) {}
std::thread::sleep(Duration::from_millis(400));
// ... capture, set_visible(true) ...

// After (main loop stays live during the 400ms compositor wait):
window_poll.set_visible(false);
while glib::MainContext::default().iteration(false) {}
glib::timeout_add_local(Duration::from_millis(400), move || {
    // ... capture, set_visible(true) ...
    glib::ControlFlow::Break
});
```

With the async timer, the main loop keeps running during the 400ms window-hide
delay: the animation timer fires every 16ms, `spinner_angle` advances, and the
spinner is already rotating when the window reappears after capture.

**Note:** The `borrow_mut()` scope inside the timeout callback is kept tight —
state is released before `set_visible(true)` and `queue_draw()` are called,
per the §2e RefCell scope-guard rule.

**Commits:** `abc70b4`.

---

## 2k. Spinner Does Not Rotate During OCR Wait — jp_ocr_app (TODO)

**Symptom:** The loading spinner shows as a static C/U-shaped arc during
the "CAPTURING…" / OCR-in-progress phase. `spinner_angle` is incremented
by the 16ms animation timer and `queue_draw()` is called, but the window
does not visually update.

**Suspected cause:** The capture flow calls `window.set_visible(false)`
then `window.set_visible(true)` to hide the window during screen grab.
The hide/show cycle unmaps and re-maps the GDK surface. After re-map,
GTK4's frame-clock scheduling may not resume delivering frames to the
window until some internal threshold is met, causing `queue_draw()` calls
to be silently dropped or deferred indefinitely.

**Why it works in Lenzu:** The production `lenzu` app avoids
`set_visible(false/true)` during capture — it moves the window
off-screen (via x11rb `configure_window`) so the GDK surface is never
unmapped. `queue_draw()` always hits a live, mapped surface and the frame
clock keeps ticking.

**Investigation starting points:**
- Replace `set_visible(false/true)` with `move_window(-2000, -2000)` /
  `move_window(win_x, win_y)` and add an `is_capturing` guard in the poll
  timer to prevent the continuous `move_window` loop from overriding the
  off-screen position.
- Alternatively, call `window.queue_draw()` AND `area.queue_draw()` (the
  DrawingArea child) after `set_visible(true)` — GTK4 may not propagate
  invalidation to children on re-map.

**Tracked in code:** `TODO(proto)` comment in [`prototypes/jp_ocr_app/src/main.rs`](https://github.com/HidekiAI/lenzu-prototypes/blob/trunk/prototypes/jp_ocr_app/src/main.rs)
next to the `window_anim.queue_draw()` call.

---

*This document lives in `lenzu/docs/` alongside
[`lenzu-desktop-issues.md`](lenzu-desktop-issues.md). Prototype source code
referenced here is at
[github.com/HidekiAI/lenzu-prototypes](https://github.com/HidekiAI/lenzu-prototypes).*
