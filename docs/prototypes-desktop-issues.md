# Prototypes Desktop Issues — GTK3/4, Windowing, Overlays

**Scope:** `lenzu-prototypes` workspace investigations into GTK3→4 migration,
transparent overlay windows, X11 pointer tracking, and related desktop issues.

**Counterpart:** `docs/lenzu-desktop-issues.md` (main repo perspective)

---

## 1. GTK3→GTK4 Migration — Complete

**Status as of 2026-06-21:** All GTK3 workspace members have been ported to GTK4
(gtk4-rs 0.11.x / glib 0.22.x). The workspace is now fully GTK4.

**Members migrated:**
- `jp_ocr_app` (was GTK3 0.18.2) → GTK4 0.11.3, with x11rb pointer tracking
- `x11-gtk-lens-test` (was `x11-gtk3-lens-test`, GTK3 0.18.2) → GTK4 0.11.3
- `gtk4_dialogbox_test` (was GTK4 0.8.1) → 0.11.3
- `gtk_gdk_test` (was GTK4 0.8.1) → 0.11.3

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

For `jp_ocr_app`, the X11 connection is initialized once lazily and also
discovers the window's own XID by scanning `_NET_WM_PID` on the root window's
children. For `x11-gtk-lens-test`, the XID is obtained from
`gdk4_x11::X11Surface::xid()` after `window.present()`.

---

## 2. Workspace Dependency Graph (Post-Migration)

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

## 3. Dependabot PR Situation (as of 2026-06-20)

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

## 4. Decision Log

| Date | Decision | Rationale |
|------|----------|-----------|
| Pre-2026-04 | GTK4 evaluated and rejected | graphene/gobject dep complexity, API churn, build failures |
| 2026-04 | Electron for HUD (not GTK) | X11 transparency works, Web UI flexibility |
| 2026-06-20 | GTK3 lens prototypes kept on 0.18.x | GdkScreen removal blocks GTK4 migration; GTK3 works for current needs |
| 2026-06-20 | Shared docs convention established | `lenzu/docs/prototypes-desktop-issues.md` (from here) + `lenzu/docs/lenzu-desktop-issues.md` (from main repo) |
| 2026-06-21 | **GTK3→GTK4 migration completed** | All 4 GTK members ported to gtk4-rs 0.11.x / glib 0.22.x; x11rb replaces removed GDK APIs for X11-specific window management; both lens prototypes compile and link cleanly |
| 2026-06-21 | GdkScreen workaround finalized | x11rb `query_pointer()` + `_NET_WM_STATE` + `configure_window()` — stable, X11-only, no GDK dependency for window management |

---

*This document is written from the `lenzu-prototypes` workspace. Updates flow
one-way: findings here are appended to this file, then cross-referenced in the
main repo's `lenzu-desktop-issues.md` as needed.*
