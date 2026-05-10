# Planning: Extract `lenzu/` + `lenzu_server/` to `CodeMonkeyNinja`

**Status:** planning — decisions locked, ready to execute on user signal
**Goal:** move the shippable Lenzu product (Rust HUD client + Electron server +
packaging) to `github.com/CodeMonkeyNinja/lenzu`, alongside `manga-ocr-rs` and
the user's other published Rust crates. The `prototypes/` tree stays at
`github.com/HidekiAI/lenzu-prototypes` (renamed) as a dev-scratch archive.

## Locked decisions (2026-05-09)

| # | Question | Answer |
|---|---|---|
| 1 | New repo name under CodeMonkeyNinja | `lenzu` |
| 2 | Rename `lenzu/` → `lenzu_client/` during the move | **No** — keep as `lenzu/` |
| 3 | Preserve history (`git filter-repo`) vs. fresh init | **Preserve** via filter-repo |
| 4 | Fate of `HidekiAI/lenzu` after the split | **Prune + rename** to `HidekiAI/lenzu-prototypes` |
| 5 | AGPL DBNet ONNX in git history | **Yes, present** — must be stripped from new repo (see §5/§7) |

### Correction on #5 (verified against current tree)

User recollection was that DBNet was tracked via LFS and pulled from HF at
install. **Actual state:**

- No LFS is configured (`.gitattributes` does not exist; `git lfs ls-files`
  is empty).
- `assets/stabrise-text_detection_dbnet_ml_v02_model.onnx` (4.7 MB, AGPL-3.0)
  is committed as a regular blob and consumed at build time by
  `scripts/build-dbnet-tarball.sh:24`.
- `scripts/lenzu-appimage-installer.sh:75` downloads the AGPL sidecar from
  **GitHub Releases**, not HF. Only `scripts/setup.sh` uses HF — and only
  for Sarashina, manga-ocr, and PaddleOCR-VL (not DBNet).

Implication: a naive `filter-repo` carries the AGPL blob into the new
MIT-licensed repo. The filter must explicitly **invert-strip** that path.

---

## 0. Naming clarification

The user-facing term *"lenzu_client"* maps to the directory **`lenzu/`** in this
repo (the Rust HUD desktop client). The historical `lenzu_client/src-tauri/`
crate was a deprecated Tauri prototype, replaced by the Electron-based
`lenzu_server/`. There is no longer a `lenzu_client/` directory on disk;
`Cargo.toml:5` only references it in a comment.

**Decision (locked):** keep as `lenzu/`. Rename is a separate, optional
follow-up.

---

## 1. Why this is *not* a GitHub "Transfer ownership"

GitHub's repo-transfer feature moves the **entire** repository tree and history
under a new owner. It cannot transfer a subset of paths. So "transferring just
`lenzu/` + `lenzu_server/`" requires:

1. Creating a **new empty repo** under `CodeMonkeyNinja` (e.g.
   `CodeMonkeyNinja/lenzu`).
2. Producing a **filtered copy** of the current repo containing only the paths
   that move.
3. Pushing that filtered copy as the new repo's initial history.
4. Pruning the moved paths from `HidekiAI/lenzu` (or leaving them as a frozen
   archive — see §6).

Side benefit: GitHub will still serve old release-asset URLs from
`HidekiAI/lenzu/releases/...` because tag history isn't being deleted on the
HidekiAI side. New releases publish under the new org.

---

## 2. Scope — what moves, what stays

### Moves to `CodeMonkeyNinja/lenzu`

| Path | Notes |
|---|---|
| `lenzu/` | Rust HUD client crate (the "lenzu_client") |
| `lenzu_server/` | Electron server (Node/TypeScript) |
| `packaging/` | `lenzu` + `lenzu-models-dbnet` package recipes |
| `scripts/` | build/install/run scripts (most of them) |
| `.github/workflows/` | tag-driven AppImage release pipeline |
| `Makefile` | top-level build entry points |
| `README.md` | product README |
| `RELEASE_NOTES.md` | release-page body |
| `LICENSE` | MIT (Rust code) |
| `SECURITY.md` | |
| `assets/` | demo gifs, icons referenced by README — **except** `assets/stabrise-text_detection_dbnet_ml_v02_model.onnx` (AGPL, stripped per §7) |
| `models/` | committed Sarashina ONNX (Apache 2.0) + LICENSE/NOTICE files. DBNet is **not** under `models/` — it lives in `assets/` and is stripped |
| `docs/` (subset) | technical-design.*, planning.*, scores.md, model-evaluation.md, todo-usability.md, sequence-flow.puml — anything that documents the shipped product |
| `build.rs` | top-level build script (verify it's product-only, not prototype-glue) |

### Stays at `HidekiAI/lenzu` (or moves to a renamed archive — see §6)

| Path | Notes |
|---|---|
| `prototypes/` | 17 prototype crates (winit, gtk4, sarashina-onnx, dbnet-test, mecab, manga-ocr-test, etc.) |
| `mirror/` | personal mirror dir |
| `notebooks/` | Jupyter scratch |
| `Cargo.toml` (modified) | drops `lenzu` from `members`, keeps prototypes |
| `Cargo.lock` (regenerated) | |
| `docs/` (subset) | anything purely about prototypes |

### Per-file decisions still needed

- **`docs/`** — audit each `docs/*.md` and tag M / S / B before filter-repo.
  `docs/scores.md` is referenced by both product and prototype READMEs;
  canonical copy goes to CodeMonkeyNinja, a copy stays at the prototypes repo.
- **`scripts/colab/`** — likely prototype-related; default S (stay).
- **`build.rs`, `.gitignore`, `.vscode/`, `project.code-workspace`** —
  duplicate or split as needed.
- **`Cargo.lock`** — regenerated post-extraction; do not try to filter.

---

## 3. History strategy — `git filter-repo` (locked)

Preserves blame, commit history, and tag context for moved files. Rewrites
history, so commit SHAs differ from `HidekiAI/lenzu`'s.

```bash
# 1. clone a fresh working copy (filter-repo rewrites history destructively)
git clone --no-local git@github.com:HidekiAI/lenzu.git lenzu-extract
cd lenzu-extract

# 2a. KEEP only the paths that move
git filter-repo \
  --path lenzu/ \
  --path lenzu_server/ \
  --path packaging/ \
  --path scripts/ \
  --path .github/workflows/ \
  --path Makefile \
  --path README.md \
  --path RELEASE_NOTES.md \
  --path LICENSE \
  --path SECURITY.md \
  --path assets/ \
  --path models/ \
  --path docs/    # prune prototype-only docs in a follow-up commit

# 2b. STRIP the AGPL DBNet blob from the new repo's history
#     (file is committed at assets/stabrise-text_detection_dbnet_ml_v02_model.onnx)
git filter-repo --invert-paths \
  --path assets/stabrise-text_detection_dbnet_ml_v02_model.onnx \
  --force

# 3. point at the new remote and push
git remote add origin git@github.com:CodeMonkeyNinja/lenzu.git
git push -u origin trunk

# 4. cherry-pick or re-create release tags as needed
```

Notes:
- Run on a **separate clone**, never on the working repo.
- `filter-repo` also rewrites tags; review which tags carry over. v0.1.0
  release-asset URLs already exist on `HidekiAI/lenzu` and stay there for
  back-compat redirect; new releases tag from the new repo.
- After filter, the new repo's root `Cargo.toml` must be rewritten to drop
  prototype `members` entries (see §4).
- After stripping the AGPL blob, `scripts/build-dbnet-tarball.sh` will fail
  until either (a) the model is fetched into `assets/` at build time, or
  (b) the script is updated to take the path via env var. See §5.

---

## 4. Cargo workspace fixup

Current root `Cargo.toml` (`/usr/src/github/mine/hidekiai/lenzu/Cargo.toml`)
lists `lenzu` plus 12 active prototype members.

**New `CodeMonkeyNinja/lenzu/Cargo.toml`:**
```toml
[workspace]
resolver = "2"
members = ["lenzu"]
```
(plus future siblings as they're added)

**Updated `HidekiAI/lenzu/Cargo.toml`:**
- Remove `"lenzu",` from `members`.
- Keep all `prototypes/*` entries.
- Regenerate `Cargo.lock` (`cargo check --workspace`).

---

## 5. Hardcoded `HidekiAI/lenzu` URLs

Verified by `grep -rn "HidekiAI/lenzu"` against the repo (excluding
node_modules/target/squashfs-root/lenzu-bundle). References that need updating
in the **new** repo only (HidekiAI side keeps its own copies):

| File | Line | What |
|---|---|---|
| `RELEASE_NOTES.md` | 69 | "Issues / source" link |
| `README.md` | 91 | link to `docs/scores.md` |
| `.github/workflows/release.yml` | 16 | comment only — `gh` calls follow `$GITHUB_*` env automatically |
| `scripts/lenzu-appimage-installer.sh` | 37 | `RELEASE_BASE` URL pattern |
| `lenzu/src/client.rs` | 232, 282 | `HTTP-Referer` header for OpenRouter API attribution |
| `lenzu/README.md` | 438 | benchmark link |

Files that reference `HidekiAI/lenzu` but **stay at HidekiAI** (prototype
READMEs) need no changes:
- `prototypes/dbnet-ocr-pipeline/README.md:169`
- `prototypes/manga-ocr-test/README.md:45`
- `prototypes/umi-ocr-eval/README.md:21`
- `prototypes/paddleocr-vl-manga/README.md:71`

---

## 6. HidekiAI/lenzu after the split — prune + rename (locked)

Plan: prune the moved paths from history *and* rename the repo to
`HidekiAI/lenzu-prototypes`.

```bash
# fresh clone — never run on the working tree
git clone --no-local git@github.com:HidekiAI/lenzu.git lenzu-prune
cd lenzu-prune

# strip everything that moved (mirror of §3 step 2a)
git filter-repo --invert-paths \
  --path lenzu/ \
  --path lenzu_server/ \
  --path packaging/ \
  --path scripts/ \
  --path .github/workflows/ \
  --path Makefile \
  --path RELEASE_NOTES.md \
  --path SECURITY.md \
  --path assets/ \
  --path models/
# Note: README.md and LICENSE are kept (rewritten on the prototype side)
# and docs/ is kept whole — prototype docs reference docs/scores.md.

# regenerate Cargo.toml: drop "lenzu", keep prototypes/* members
# regenerate Cargo.lock
cargo check --workspace

git push --force-with-lease origin trunk    # destructive — confirm with user
```

Then via GitHub UI:
- Rename `HidekiAI/lenzu` → `HidekiAI/lenzu-prototypes`. GitHub installs
  an automatic redirect from the old slug, so existing release-asset URLs
  (`HidekiAI/lenzu/releases/download/v0.1.0/...`) keep working.
- Update the renamed repo's `README.md` to point at `CodeMonkeyNinja/lenzu`
  for the active product.

Old release-asset URLs continue to resolve because (a) release artifacts
live on tags, and (b) the rename redirect handles the path change.

---

## 7. License audit — verified state and required actions

**Verified state (2026-05-09):**

| Path | License | In git? | Notes |
|---|---|---|---|
| `LICENSE` (root) | MIT | yes | covers Rust code |
| `models/sarashina2.2-mini-fp16/`, `…-fp32/` | Apache 2.0 | yes (committed blobs) | upstream Sarashina; OK to keep in MIT repo (Apache 2.0 ⊂ MIT-compatible) |
| `models/LICENSE`, `models/NOTICE`, `models/README.md` | — | yes | upstream attribution files |
| `assets/stabrise-text_detection_dbnet_ml_v02_model.onnx` | **AGPL-3.0** | **yes** (4.7 MB blob) | **must be stripped** during filter-repo into new repo |
| `lenzu_server/LICENSE` | (separate) | yes | verify it doesn't conflict |

**Required actions before the new repo goes public:**

1. Filter-repo step §3 strips `assets/stabrise-text_detection_dbnet_ml_v02_model.onnx`
   from the new repo's history. Verify post-filter with:
   ```bash
   git log --all --name-only --pretty=format: | grep stabrise || echo "OK: no AGPL blob in history"
   ```
2. `scripts/build-dbnet-tarball.sh` currently reads the model from
   `assets/<that file>` (line 24). Update it to:
   - read `LENZU_DBNET_ONNX` env var first; OR
   - fetch the model from the existing HidekiAI release on demand.
   This way the build script works in CodeMonkeyNinja without re-introducing
   the AGPL blob.
3. The first CodeMonkeyNinja release that needs `lenzu-models-dbnet-*.tar.xz`
   either re-publishes from a CI job that downloads the model into a runner
   workspace (not committed), or simply delegates to the existing HidekiAI
   release URL (back-compat redirect after rename still works).
4. Confirm `lenzu_server/LICENSE` and `models/LICENSE` don't introduce
   restrictions incompatible with the root MIT license.

---

## 8. Release pipeline migration

`.github/workflows/release.yml` is tag-driven and uses `$GITHUB_REPOSITORY`
env vars — it follows the move automatically. The only manual edits:
1. Comment on line 16 (cosmetic).
2. Verify the workflow has the secrets/permissions it needs in the new org
   (`GITHUB_TOKEN` is automatic; any custom secrets must be recreated under
   `CodeMonkeyNinja` org settings).
3. Cut the first new tag from `CodeMonkeyNinja/lenzu` (e.g. `v0.1.1`) to
   confirm the AppImage build + publish path works end-to-end.

---

## 9. Step-by-step execution checklist

When the user signals "go":

- [ ] **Pre-flight**
  - [ ] Audit each file under `docs/` and tag M (move) / S (stay) / B (both)
  - [ ] Verify `lenzu_server/LICENSE` and `models/LICENSE` are
        MIT-compatible (Apache 2.0 is fine; anything else needs review)
  - [ ] Decide whether `scripts/colab/`, `notebooks/`, `mirror/` should
        also stay at HidekiAI side (probably yes)
  - [ ] Update `scripts/build-dbnet-tarball.sh` to read the DBNet path
        from `LENZU_DBNET_ONNX` env var (so it works post-strip)
- [ ] **New repo: `CodeMonkeyNinja/lenzu`**
  - [ ] Create empty repo via GitHub UI
  - [ ] On a fresh clone of `HidekiAI/lenzu`, run the two filter-repo
        commands from §3 (keep + invert-strip AGPL blob)
  - [ ] Verify the AGPL blob is gone:
        `git log --all --name-only --pretty=format: | grep stabrise`
        should return empty
  - [ ] Rewrite root `Cargo.toml` → `members = ["lenzu"]`, regenerate
        `Cargo.lock` with `cargo check --workspace`
  - [ ] Update the 6 hardcoded URLs from §5
  - [ ] Push to `CodeMonkeyNinja/lenzu`
  - [ ] On a clean clone, verify `cargo build --release` and
        `pnpm --dir lenzu_server install && pnpm --dir lenzu_server build`
        both succeed
  - [ ] Recreate any custom Actions secrets at the org level
        (`GITHUB_TOKEN` is automatic)
  - [ ] Cut a sanity-check release tag (e.g. `v0.1.1-rc.0`) and verify
        the AppImage publishes under the new org
- [ ] **HidekiAI side: prune + rename to `lenzu-prototypes`**
  - [ ] On a fresh clone, run the invert-paths filter-repo from §6
  - [ ] Update `Cargo.toml` to drop `"lenzu"` from members; regenerate
        `Cargo.lock`
  - [ ] **Confirm with user before** `git push --force-with-lease`
        (history rewrite is destructive)
  - [ ] Rename `HidekiAI/lenzu` → `HidekiAI/lenzu-prototypes` via
        GitHub UI (verify redirect works for old release URLs)
  - [ ] Update the renamed repo's `README.md` to point at
        `CodeMonkeyNinja/lenzu` for the product
- [ ] **Comms / external**
  - [ ] Crates.io (if `lenzu` is published): update `repository` field on
        next publish
  - [ ] Any external docs/links the user controls (HF model card, blog
        posts, gists, etc.)
  - [ ] OpenRouter API attribution still uses `HTTP-Referer:
        https://github.com/CodeMonkeyNinja/lenzu` post-edit — sanity-check
        that the model providers see the new URL.
