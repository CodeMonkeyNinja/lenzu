use std::process::Command;

fn main() {
    // MinGW / MSVC toolchain detection (existing behavior)
    if std::env::var("NotMSVC").is_ok() {
        println!("cargo:warning=Using MinGW64 (GNU) toolchain.");
    }

    // Check for the LENZU_SKIP_SERVER_BUILD env var to allow fast cargo builds.
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

    let status = Command::new("pnpm")
        .args(["run", "build"])
        .current_dir(&server_dir)
        .status()
        .expect("failed to run pnpm — is Node.js / pnpm installed?");

    if !status.success() {
        panic!("lenzu_server build failed");
    }
}
