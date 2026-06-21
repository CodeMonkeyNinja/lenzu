# GTK-rs Dependency Analysis — Upgrade Blockers

**Scope:** Main `lenzu` crate. Prototype-specific findings in
`lenzu-prototypes/docs/prototypes-desktop-issues.md`.

---

## 1. Current State

### Main `lenzu` crate (`lenzu/Cargo.toml`)

| Crate | Version | Upgradable to |
|---|---|---|
| `gtk` | `0.18.2` | **0.18.2 is final — UNMAINTAINED** |
| `gdk` | `0.18.2` | **0.18.2 is final — UNMAINTAINED** |
| `gdk-pixbuf` | `0.18.5` | 0.22.0 |
| `cairo-rs` | `0.18.5` | 0.22.0 |
| `pango` | `0.18.3` | 0.22.6 |
| `pangocairo` | `0.18.0` | 0.22.0 |
| `glib` | `0.22.7` | Already latest |

**Key skew:** `glib` is at 0.22 while all six tightly-coupled crates remain at 0.18.
`Cargo.lock` carries **two versions of glib**: `0.18.5` (transitive from gtk/gdk/cairo/pango)
and `0.22.7` (direct dependency).

---

## 2. Critical Finding: There Is No gtk-rs 0.22 for GTK3

The gtk-rs project **abandoned GTK3 bindings at 0.18.x** and directed users to
`gtk4` for GTK4. Verified:

- `cargo info gtk` → `gtk 0.18.2` marked **"UNMAINTAINED Rust bindings for the
  GTK+ 3 library (use gtk4 instead)"**
- `cargo info gdk` → same: **"UNMAINTAINED Rust bindings for the GDK 3 library
  (use gdk4 instead)"**
- No `gtk3` crate exists on crates.io (404)
- Supporting crates (`cairo-rs`, `pango`, `pangocairo`, `gdk-pixbuf`) DO have
  0.22.x versions, but they cannot be bumped independently of `gtk`/`gdk`
  without type mismatches (as documented in `.github/dependabot.yml`)

---

## 3. The Six Tightly-Coupled Crates

From `.github/dependabot.yml`:

> *"These gtk-rs-core crates are tightly coupled to gtk/gdk 0.18. Bumping them
> independently causes type mismatches"*

**The six:** `gdk-pixbuf`, `cairo-rs`, `pango`, `pangocairo`, `gdk`, `gtk`

Since `gtk` and `gdk` have no 0.22 release, upgrading the supporting crates alone
is impossible without breaking the build.

---

## 3b. GTK4 Migration — Prototypes Succeeded

The lenzu-prototypes workspace has completed a GTK3→GTK4 migration (gtk4-rs 0.11.x)
for all its GTK members. Key workarounds documented in
[prototypes-desktop-issues.md](prototypes-desktop-issues.md) §1:

| Problem | Workaround |
|---------|------------|
| `gdk::Screen::default()` removed | x11rb `query_pointer()` on root window |
| `window.move_()` removed | x11rb `configure_window()` |
| `input_shape_combine_region()` removed | `surface.set_input_region(Some(&region))` |
| `set_keep_above(true)` removed | x11rb `_NET_WM_STATE` ClientMessage |
| `connect_draw` → raw Cairo | `WidgetExt::snapshot()` with `gtk::Snapshot` |
| `pangocairo::show_layout()` moved | `pangocairo::functions::show_layout()` |
| `gdk::keys::constants` private | `gdk::Key::Escape` directly |

A test crate is on the `feat/gtk4-upgrade-test` branch at
`prototypes/gtk4-lens-test/` for prototyping the main lenzu crate's migration.

---

## 4. Migration Surface Area

Only one file uses the GTK family: **`lenzu/src/main.rs`** (~180 of 1536 lines).
All other 14 Rust source files are unaffected.

### If migrating to GTK4 (`gtk4` crate)

Would require rewriting the GTK3-specific patterns in main.rs:

| Current (gtk-rs 0.18 GTK3) | Notes for GTK4 |
|---|---|
| `Window::new(Toplevel)` | `ApplicationWindow` or `Window` + `Application` pattern |
| `Screen::rgba_visual()`, `set_visual()` | `gtk4::WidgetExt` handles transparency differently |
| `connect_key_press_event` → `Propagation` | `EventControllerKey` instead of signal per-widget |
| `connect_draw` → `cairo::Context` | `WidgetExt::snapshot()` with `gtk::Snapshot` (no raw Cairo) |
| `input_shape_combine_region()` | GTK4 `WidgetExt::set_size_request()` + compositor-only |
| `gdk::Display::default_seat()` | Different seat/pointer API |
| `cairo::Region::create()` | Changed to `cairo::Region::new()` (minor) |
| `pangocairo::show_layout()` | Still works in 0.22 (unchanged) |
| `gtk::events_pending()` / `main_iteration()` | Replaced by async/event-loop patterns |

### Alternative: Replace GTK with winit + cairo

Prototyped in `lenzu-prototypes/prototypes/winit-test/`. Would eliminate all GTK
dependencies but requires implementing window management, transparency, cursor
tracking, and rendering from scratch.

---

## 5. Architectural Decisions (from lenzu-prototypes docs)

### GTK4 was evaluated and rejected

> *"GTK4 was evaluated and abandoned — graphene/gobject dep complexity, API churn,
> prototype build failures. GTK3 provides everything needed and is simpler to build
> against."*
>
> **Decision: GTK3 (`gtk-rs` 0.18) — permanent choice, not a stepping stone to GTK4**
>
> — `docs/planning.md`

### Transparent full-screen GTK3 overlay deferred indefinitely

> *"A transparent full-screen GTK3 window with drawn rectangles would be the most
> interactive option but carries the same alpha-compositing / ghosting risk that
> caused the Tauri->Electron migration."*
>
> — `docs/technical-design.phase4-predetect.md`

### Compositor ghosting (xfwm4)

Semi-transparent areas fill with dark ghost pixels under xfwm4 compositing.
Mitigated by a 3-frame erase cycle in the Electron HUD. Full fix: replace xfwm4
compositing with `picom --backend glx --no-use-damage`.

### Compositor sync timing

Between `hide()` and X11 `GetImage`, ~33-50ms pause needed for compositor to
repaint desktop. Implemented as 200-400ms sleep in main.rs.

### Tauri/WebKit2GTK abandoned

Dropped due to alpha compositing artifacts on X11. Migrated to Electron, which
produces clean transparency. Thin white titlebar strip remains on some compositors.

---

## 6. Verdict: Upgrade Not Possible

**gtk-rs 0.18 → 0.22 for GTK3 is impossible** — there is no 0.22 GTK3 binding.

The options are:

| Option | Effort | Risk | Notes |
|---|---|---|---|
| **Stay on 0.18** | None | Low | Works today; dual glib in lockfile is cosmetic |
| **Migrate to GTK4** | High | Medium | Prototypes succeeded; test crate on `feat/gtk4-upgrade-test` |
| **Replace GTK with winit** | Very high | High | Prototyped but bare-bones; full window stack needed |

### Answer to "is it possible to upgrade?"

**No** — not along the gtk-rs version line. `gtk` 0.18.2 is the terminal release.
GTK4 migration is now proven possible (prototypes workspace), with a test crate
on the `feat/gtk4-upgrade-test` branch to port the main lenzu crate.

---

*See also: `docs/planning.md`, `docs/technical-design.phase4-predetect.md`*
