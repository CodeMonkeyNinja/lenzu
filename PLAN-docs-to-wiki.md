# Migration Plan: `lenzu/docs/` → `lenzu.wiki/`

**Status:** Not started
**Last updated:** 2026-06-22

## Rationale

Centralize all Lenzu documentation into the GitHub Wiki for discoverability,
connected linking, and a single source of truth spanning the main product
(`CodeMonkeyNinja/lenzu`) and prototype research (`HidekiAI/lenzu-prototypes`).

## Directory structure in wiki

```
Home.md                     # landing page (exists)
Configuration-Reference.md  # user-facing (exists)
Troubleshooting.md          # user-facing (exists)
_Sidebar.md                 # navigation (new)
_Footer.md                  # optional

lenzu/
  technical-design.md
  technical-design.lens-window.md
  technical-design.OCR.md
  technical-design.manga-ocr.md
  technical-design.sarashina.md
  technical-design.phase4-predetect.md
  technical-design.cancel-inflight.md
  planning.md
  planning-ollama-to-llamacpp.md
  planning-codemonkeyninja-extraction.md
  scores.md
  model-evaluation.md
  session-log-2026-04-14.md
  todo-usability.md
  grpc_and_effect.md
  release-procedure.md
  PR-4-WorkOrder.md
  lenzu-desktop-issues.md
  sequence-flow.puml
  HELP.png
  lenzu-beta-demo.gif
  Lenzu-demo-furigana-only.gif

prototypes/
  prototypes-desktop-issues.md
  prototypes-reference.md    # new: cross-reference table from lenzu-prototypes README
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
