use std::process::Command;

fn main() {
    // MinGW / MSVC toolchain detection (existing behavior)
    if std::env::var("NotMSVC").is_ok() {
        println!("cargo:warning=Using MinGW64 (GNU) toolchain.");
    }

    // ── System library prerequisite checks ──────────────────────────────────
    let system_packages = [
        ("gtk4", "libgtk-4-dev"),
        ("cairo", "libcairo2-dev"),
        ("pango", "libpango1.0-dev"),
        ("gdk-pixbuf-2.0", "libgdk-pixbuf-2.0-dev"),
        ("graphene-1.0", "libgraphene-1.0-dev"),
        ("x11", "libx11-dev"),
        ("xcb", "libxcb1-dev"),
        ("openssl", "libssl-dev"),
    ];

    let all_system_ok = system_packages.iter().all(|(pkg, deb_pkg)| {
        let found = Command::new("pkg-config")
            .args(["--exists", pkg])
            .status()
            .map(|s| s.success())
            .unwrap_or(false);
        if !found {
            println!(
                "cargo:warning=Missing system library: {pkg}. \
                 Install it with: sudo apt install {deb_pkg}"
            );
        }
        found
    });
    if !all_system_ok {
        println!("cargo:warning=Missing system libraries — run `scripts/setup.sh` to install all dependencies.");
    }

    // ── gRPC protobuf code generation ──────────────────────────────────────
    println!("cargo:rerun-if-changed=../proto/");
    // Fall back to vendored protoc when the system one isn't available
    // (e.g. fresh checkout without protobuf-compiler installed).
    if std::env::var("PROTOC").is_err() {
        if let Ok(path) = protoc_bin_vendored::protoc_bin_path() {
            std::env::set_var("PROTOC", path);
        }
    }
    tonic_build::compile_protos("../proto/lenzu_hud.proto")
        .expect("failed to compile lenzu_hud.proto");

    // ── Server build ───────────────────────────────────────────────────────
    if std::env::var("LENZU_SKIP_SERVER_BUILD").is_ok() {
        println!("cargo:warning=Skipping lenzu_server build (LENZU_SKIP_SERVER_BUILD set).");
        return;
    }

    println!("cargo:rerun-if-changed=../lenzu_server/src/");
    println!("cargo:rerun-if-changed=../lenzu_server/package.json");
    println!("cargo:rerun-if-changed=../lenzu_server/pnpm-lock.yaml");
    println!("cargo:rerun-if-changed=../lenzu_server/build.mjs");

    let server_dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .join("lenzu_server");

    let node_ok = Command::new("node")
        .arg("--version")
        .status()
        .map(|s| s.success())
        .unwrap_or(false);
    if !node_ok {
        println!("cargo:warning=Node.js not found — skipping lenzu_server build.");
        println!("cargo:warning=Install Node.js: https://nodejs.org  (or: nvm install --lts)");
        return;
    }

    let pnpm_ok = Command::new("pnpm")
        .arg("--version")
        .status()
        .map(|s| s.success())
        .unwrap_or(false);
    if !pnpm_ok {
        println!("cargo:warning=pnpm not found — skipping lenzu_server build.");
        println!("cargo:warning=Install pnpm: npm install -g pnpm");
        println!(
            "cargo:warning=       or: corepack enable && corepack prepare pnpm@latest --activate"
        );
        return;
    }

    // --ignore-scripts avoids ERR_PNPM_IGNORED_BUILDS for protobufjs and
    // electron-winstaller (pnpm v11 requires explicit approval).  Build-only
    // deps (esbuild, tsc) are sufficient for `pnpm run build`; the full
    // Electron binary install is handled by scripts/setup.sh.
    let status = match Command::new("pnpm")
        .args(["install", "--ignore-scripts"])
        .current_dir(&server_dir)
        .status()
    {
        Ok(s) => s,
        Err(e) => {
            println!("cargo:warning=pnpm install failed ({e}) — skipping lenzu_server build");
            return;
        }
    };
    if !status.success() {
        panic!("pnpm install failed in {server_dir:?}");
    }

    let status = match Command::new("pnpm")
        .args(["run", "build"])
        .current_dir(&server_dir)
        .status()
    {
        Ok(s) => s,
        Err(e) => {
            println!("cargo:warning=pnpm not found — skipping lenzu_server build ({e})");
            return;
        }
    };
    if !status.success() {
        panic!("lenzu_server build failed in {server_dir:?}");
    }
}
