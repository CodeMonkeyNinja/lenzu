# Migration Plan: `lenzu/docs/` → `lenzu.wiki/`

**Status:** Not started
**Last updated:** 2026-06-22

## Rationale

Centralize all Lenzu documentation into the GitHub Wiki for discoverability,
connected linking, and a single source of truth spanning the main product
(`CodeMonkeyNinja/lenzu`) and prototype research (`HidekiAI/lenzu-prototypes`).

## Important: GitHub Wiki flattens subdirectories

GitHub Wiki ignores directory structure — all pages are served at the root level.
- `lenzu/technical-design.md` → accessible at `/wiki/technical-design` (NOT `/wiki/lenzu/technical-design`)
- All sidebar links and internal wiki links must use flat page names (no `lenzu/` prefix)
- Image raw URLs use the `wiki` path: `https://raw.githubusercontent.com/wiki/{owner}/{repo}/{path-in-wiki-git}`

## Directory structure in wiki (git repo layout — for organization only)

```
Home.md                     # landing page (exists)
Configuration-Reference.md  # user-facing (exists)
Troubleshooting.md          # user-facing (exists)
_Sidebar.md                 # navigation (new)
_Footer.md                  # optional

lenzu/                      # subdir for git organization only — wiki flattens
  technical-design.md       #  → wiki page: "technical-design"
  ...
  HELP.png                  #  → raw: /wiki/CodeMonkeyNinja/lenzu/lenzu/HELP.png
  lenzu-beta-demo.gif
  Lenzu-demo-furigana-only.gif

prototypes/                 # subdir for git organization only — wiki flattens
  prototypes-desktop-issues.md  # → wiki page: "prototypes-desktop-issues"
  prototypes-reference.md
```

## Phases

### Phase 0 — Set up wiki structure
- Create `_Sidebar.md` with categorized navigation
- Create category directories (`lenzu/`, `prototypes/`)

### Phase 1 — Migrate files from `lenzu/docs/` to wiki
- Copy all 19 `.md` + 1 `.puml` + 3 images to wiki
- **Fix 6 internal markdown links** (strip `.md` suffix for wiki-style links):
  - `lenzu-desktop-issues.md` line 4: `[prototypes-desktop-issues.md](prototypes-desktop-issues.md)` → `[prototypes-desktop-issues.md](prototypes/prototypes-desktop-issues)`
  - `lenzu-desktop-issues.md` line 62: same fix
  - `prototypes-desktop-issues.md` line 479: `[lenzu-desktop-issues.md](lenzu-desktop-issues.md)` → `[lenzu-desktop-issues.md](lenzu/lenzu-desktop-issues)`
- **Fix image paths in scores.md**: `../assets/` → full GitHub raw URLs

### Phase 2 — Incorporate lenzu-prototypes content
- Fetch `lenzu-prototypes` repo README cross-reference table
- Create `prototypes/prototypes-reference.md` with that content, rewriting `lenzu/docs/` paths to wiki links

### Phase 3 — Fix references in `README.md` (main repo)
| Location | Original | → | Replacement |
|---|---|---|---|
| Line 20 | `docs/lenzu-beta-demo.gif` | → | `https://raw.githubusercontent.com/CodeMonkeyNinja/lenzu.wiki/main/lenzu/lenzu-beta-demo.gif` |
| Line 24 | `docs/Lenzu-demo-furigana-only.gif` | → | same pattern |
| Line 32 | `[Technical Design](./docs/technical-design.md)` | → | `[Technical Design](https://github.com/CodeMonkeyNinja/lenzu/wiki/lenzu/technical-design)` |
| Line 118 | `docs/HELP.png` | → | wiki raw URL |
| Line 120 | `[lenzu/README.md](lenzu/README.md)` | → | `[lenzu/README.md](https://github.com/CodeMonkeyNinja/lenzu/blob/trunk/lenzu/README.md)` |
| Line 214 | `[lenzu/NOTICES.md](lenzu/NOTICES.md)` | → | `[...](https://github.com/CodeMonkeyNinja/lenzu/blob/trunk/lenzu/NOTICES.md)` |

### Phase 4 — Clean up
- Add `docs/ARCHIVE.md` (or `docs/README.md`) in main repo redirecting to wiki
- Or delete `docs/` entirely (user decision)

## Asset strategy

| Asset | Source | Wiki treatment |
|---|---|---|
| `docs/HELP.png` | main repo `docs/` | Copy to `lenzu/HELP.png` in wiki |
| `docs/lenzu-beta-demo.gif` | main repo `docs/` | Copy to `lenzu/` in wiki |
| `docs/Lenzu-demo-furigana-only.gif` | main repo `docs/` | Copy to `lenzu/` in wiki |
| `assets/architecture.png` | main repo `assets/` | Reference via raw GitHub URL |
| `assets/Unit-test-*.png` (3) | main repo `assets/` | Reference via raw GitHub URL |

## Link audit summary

- **6** actual markdown `[]()` links need wiki-style rewriting (3 files)
- **~25** plain-text code-span cross-references — leave as-is (informational, not links)
- **11** image/video assets referenced (4 in `docs/`, 7 in `assets/`)
- **0** anchor links
