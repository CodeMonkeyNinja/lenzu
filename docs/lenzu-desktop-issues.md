# GTK-rs Dependency Analysis — Upgrade Blockers

**Scope:** Main `lenzu` crate. Prototype-specific findings in
[`prototypes-desktop-issues.md`](prototypes-desktop-issues.md) (same `lenzu/docs/` directory).

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

**Consolidated into the [GTK-Migrations wiki page](https://github.com/CodeMonkeyNinja/lenzu/wiki/GTK-Migrations)** — all workaround tables,
guide cross-references, and migration decisions for the prototype workspace now live
there. See the following sections:

| Topic | `GTK-Migrations.md` section |
|-------|---------------------------|
| Full workaround table (22 rows) | § Full Workaround Table |
| Official guide cross-reference | § Guide Cross-Reference |
| Key combo architecture | § Key Combo Architecture |
| Summary: GTK3 vs GTK4 per invariant | § Summary Table |
| Architectural decisions timeline | § Architectural Decisions |
| Workspace dependency graph | § Workspace Dependency Graph |

A test crate is on the `feat/gtk4-upgrade-test` branch at
[`prototypes/x11-gtk-lens-test`](https://github.com/HidekiAI/lenzu-prototypes/tree/trunk/prototypes/x11-gtk-lens-test) for prototyping the main lenzu crate's migration. *(was `gtk4-lens-test` — renamed)*

---

## 4. Migration Surface Area

**Consolidated into the [GTK-Migrations wiki page](https://github.com/CodeMonkeyNinja/lenzu/wiki/GTK-Migrations)** — see § Main Crate Migration
Surface Area for the full GTK3→GTK4 mapping table.

Only one file uses the GTK family: **`lenzu/src/main.rs`** (~180 of 1536 lines).
All other 14 Rust source files are unaffected.

### Alternative: Replace GTK with winit + cairo

Prototyped in [`prototypes/winit-test`](https://github.com/HidekiAI/lenzu-prototypes/tree/trunk/prototypes/winit-test). Would eliminate all GTK
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
