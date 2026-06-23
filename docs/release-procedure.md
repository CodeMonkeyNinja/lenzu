# Lenzu Release Procedure

Step-by-step guide for cutting a new versioned release and publishing the AppImage to GitHub. Follow every step in order.

---

## 0. Prerequisites

- Working directory: `/home/hidekiai/projects/remote/github/mine/codemonkeyninja/lenzu`
  (this directory has `.git/config` pre-wired to use the correct SSH key for CodeMonkeyNinja)
- All feature work must already be committed on `trunk`
- `gh` CLI must be authenticated for CodeMonkeyNinja (`gh auth status`)

---

## 1. Verify the feature works at runtime

Build and launch the Electron HUD against the live X11 display, then screenshot it.

```bash
cd lenzu_server
pnpm run build
DISPLAY=:0 GTK_CSD=0 node_modules/.bin/electron dist/main.js &
sleep 3
DISPLAY=:0 gnome-screenshot -f /tmp/lenzu-verify.png
# view /tmp/lenzu-verify.png and confirm the expected behavior is visible
kill %1
```

Do not skip this step and substitute a test run — the test suite does not exercise the HUD renderer.

---

## 2. Bump versions

Two files must be updated together. Increment the patch number (e.g. `0.1.15` → `0.1.16`):

**`lenzu/Cargo.toml`** — the product version (used by the Rust binary and the AppImage filename):
```toml
version = "0.1.16"
```

**`lenzu_server/package.json`** — the Electron HUD package version:
```json
"version": "1.0.1"
```

Both must be bumped on every release; they are displayed in different places but users see both.

---

## 3. Update RELEASE_NOTES.md

Add a new section at the top (just below the `## Lenzu —` heading):

```markdown
### What's new in v0.1.16

- **Short title** — one-sentence description of the change.
```

The entire contents of `RELEASE_NOTES.md` become the GitHub Release body, so keep entries concise and user-facing.

---

## 4. Build the HUD and Rust client

```bash
# From repo root
cd lenzu_server && pnpm run build && cd ..
cargo build --release --all-features -p lenzu
```

Confirm the binary reports the new version:
```bash
./target/release/lenzu --version
# expected: lenzu 0.1.16
```

---

## 5. Build the AppImage

```bash
bash scripts/build-lenzu-appimage.sh
```

Output lands in `target/appimage/`. Confirm:
```bash
ls target/appimage/lenzu-0.1.16-x86_64.appimage
ls target/appimage/lenzu-bundle-0.1.16.tar
```

Both files must exist before continuing.

---

## 6. Install locally

The local install uses a symlink — no sudo required:

```bash
cp target/appimage/lenzu-0.1.16-x86_64.appimage ~/.local/share/lenzu/lenzu-0.1.16.AppImage
chmod +x ~/.local/share/lenzu/lenzu-0.1.16.AppImage
ln -sf ~/.local/share/lenzu/lenzu-0.1.16.AppImage ~/.local/bin/lenzu
```

Verify:
```bash
~/.local/bin/lenzu --version
# expected: lenzu 0.1.16
```

---

## 7. Commit and push

Stage only the version/release files — do not stage `README.md` if its only diff is whitespace/table reformatting (the Markdown formatter touches it spuriously):

```bash
git add Cargo.lock RELEASE_NOTES.md lenzu/Cargo.toml lenzu_server/package.json lenzu_server/src/renderer/app.ts
git commit -m "chore: bump to v0.1.16, <short summary of change>"
git push
```

---

## 8. Tag and push the release tag

This is what triggers the `release.yml` CI workflow:

```bash
git tag v0.1.16
git push origin v0.1.16
```

---

## 9. Wait for CI to pass

Poll until the release workflow finishes (~10–15 min):

```bash
cd /home/hidekiai/projects/remote/github/mine/codemonkeyninja/lenzu
until gh run list --workflow=release.yml --limit=1 2>&1 | grep -v "in_progress"; do sleep 15; done
```

If it fails, check the logs:
```bash
gh run view --log-failed $(gh run list --workflow=release.yml --limit=1 --json databaseId --jq '.[0].databaseId')
```

---

## 10. Verify the GitHub Release page

```bash
gh release view v0.1.16
```

Expected assets attached to the release:
- `lenzu-0.1.16-x86_64.appimage`
- `lenzu-bundle-0.1.16.tar`
- `lenzu-bundle-<previous version>.tar` (one prior release kept per storage policy)
- `lenzu-models-dbnet-0.2.0.tar.xz`
- `lenzu-models-manga-ocr-0.1.0.tar.xz`

The release body should contain the `v0.1.16` section from `RELEASE_NOTES.md`.

---

## 11. Clean up stale bundle assets (if any)

If the release shows `lenzu-bundle-*.tar` files from versions older than the previous release, delete them:

```bash
gh release delete-asset v0.1.16 lenzu-bundle-0.1.OLD.tar -y
```

The root cause is `target/appimage/` being restored from the cargo cache with old tars still in it. This is fixed in `scripts/make-installers.sh` (the pre-build cleanup now removes stale `lenzu-bundle-*.tar` files), so this step should not be needed after that fix is in the cache.

---

## Key file locations

| File | Purpose |
|------|---------|
| `lenzu/Cargo.toml` | Product version (source of truth for AppImage filename) |
| `lenzu_server/package.json` | Electron HUD package version |
| `RELEASE_NOTES.md` | Release body — entire file is used as GitHub Release notes |
| `scripts/make-installers.sh` | CI build entry point; `--ci` flag used in `release.yml` |
| `scripts/build-lenzu-appimage.sh` | Builds the single-bundle AppImage |
| `scripts/lenzu-appimage-installer.sh` | End-user installer for model sidecars |
| `.github/workflows/release.yml` | CI: triggered by `v*` tag push; builds and publishes the release |

## Known pitfalls

- **`sudo dpkg -i` fails in Claude Code** — the shell has no TTY, so sudo cannot prompt for a password. Use the AppImage install path (step 6) instead of `.deb` packages.
- **Screenshot tools**: `gnome-screenshot -f <path>` is the most reliable; `scrot` and `import` can exit 144 in some environments.
- **`git push` and `git tag` must use the per-account working dir** (`~/projects/remote/github/mine/codemonkeyninja/lenzu`), which has the correct SSH key wired in `.git/config`. The `/usr/src/...` checkout also works for file edits but use the `~/projects/remote/...` dir for all git operations.
- **Old bundle tars in CI cache** — fixed in `make-installers.sh`; if you see them again, check whether the `rm -fv ... lenzu-bundle-*.tar` line is still present in `build_appimage()`.
- **`pnpm-workspace.yaml` without a `packages` field breaks pnpm 9** — pnpm 9 treats any directory containing `pnpm-workspace.yaml` as a workspace root and requires a `packages:` list. If CI fails with `packages field missing or empty`, the file either has no `packages` key or it's empty. Fix: delete the file if the directory is a single package (not a monorepo), or add `packages: ['.']`. The `allowBuilds` / `onlyBuiltDependencies` settings belong in `pnpm.json`, not in `pnpm-workspace.yaml`. First hit: v0.2.0 release, `lenzu_server/pnpm-workspace.yaml`.
- **`release.yml` system deps must track `test.yml`** — when GTK or other system deps are updated, both workflow files need to change. `test.yml` is usually fixed first (it runs on every push); `release.yml` is easy to miss because it only runs on tag push. After any toolkit version bump, grep both workflows for the old package name. First hit: v0.2.0, `libgtk-3-dev` left in `release.yml` after GTK4 migration.
