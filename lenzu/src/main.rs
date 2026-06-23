use arboard::Clipboard;
use async_channel;
use gdk_pixbuf::prelude::*;
use gtk4::glib;
use gtk4::prelude::*;
use pango;
use pangocairo;
use std::cell::RefCell;
use std::fs::OpenOptions;
use std::io::Write;
use std::os::unix::process::CommandExt;
use std::path::Path;
use std::rc::Rc;
use std::sync::OnceLock;
extern crate libc;
use std::time::{Duration, Instant};
use x11rb::connection::Connection;
use x11rb::protocol::xproto::*;
use x11rb::protocol::Event;
use x11rb::rust_connection::RustConnection;

use isolang::Language;

// ── X11 singletons (GTK4 has no GdkScreen; use x11rb for pointer/window mgmt) ──

static X11_CONN: OnceLock<RustConnection> = OnceLock::new();
static WINDOW_XID: OnceLock<u32> = OnceLock::new();
static ROOT_WINDOW: OnceLock<u32> = OnceLock::new();

fn init_x11() -> bool {
    if X11_CONN.get().is_some() {
        return true;
    }
    let (conn, screen_num) = match RustConnection::connect(None) {
        Ok(v) => v,
        Err(_) => return false,
    };
    let root = conn.setup().roots[screen_num].root;
    let _ = ROOT_WINDOW.set(root);
    let _ = X11_CONN.set(conn);
    setup_key_grabs();
    true
}

fn x11_conn() -> Option<&'static RustConnection> {
    init_x11();
    X11_CONN.get()
}

fn root_window() -> Option<u32> {
    init_x11();
    ROOT_WINDOW.get().copied()
}

fn pointer_position() -> Option<(i32, i32, u16)> {
    let conn = x11_conn()?;
    let root = root_window()?;
    let reply = conn.query_pointer(root).ok()?.reply().ok()?;
    Some((reply.root_x as i32, reply.root_y as i32, reply.mask.bits()))
}

fn move_window(x: i32, y: i32) {
    if let (Some(conn), Some(&xid)) = (x11_conn(), WINDOW_XID.get()) {
        let aux = ConfigureWindowAux::new().x(x).y(y);
        let _ = conn.configure_window(xid, &aux);
        let _ = conn.flush();
    }
}

fn screen_height() -> i32 {
    x11_conn()
        .and_then(|conn| conn.setup().roots.first().map(|r| r.height_in_pixels as i32))
        .unwrap_or(1080)
}

fn set_window_state(window: &gtk4::ApplicationWindow) {
    let conn = match x11_conn() { Some(c) => c, None => return };
    let surface = match window.surface() { Some(s) => s, None => return };
    let x11_surface = match surface.downcast::<gdk4_x11::X11Surface>() {
        Ok(s) => s,
        Err(_) => return,
    };
    let xid = x11_surface.xid() as u32;
    eprintln!("[DBG] WINDOW_XID set to 0x{xid:x}");
    let _ = WINDOW_XID.set(xid);
    // Bypass the WM so configure_window(-10000,-10000) is honored.
    // Without override_redirect, WMs silently reject off-screen ConfigureRequests.
    let cwa = ChangeWindowAttributesAux::new().override_redirect(1u32);
    let _ = conn.change_window_attributes(xid, &cwa);
    // Start off-screen, pin above all other windows.
    let aux = ConfigureWindowAux::new()
        .x(-10000)
        .y(-10000)
        .stack_mode(StackMode::ABOVE);
    let _ = conn.configure_window(xid, &aux);
    let _ = conn.flush();
}

const KEYCODE_ESC: u8 = 9;
const KEYCODE_H: u8 = 43;
const KEYCODE_TAB: u8 = 23;

/// Register passive X11 key grabs on the root window so KeyPress events
/// for our combos arrive on our x11rb connection even when the lens window
/// has no WM focus (override_redirect windows never get focus from the WM).
/// This replaces query_keymap() polling — event-driven, no polling overhead.
fn setup_key_grabs() -> bool {
    let conn = match x11_conn() { Some(c) => c, None => return false };
    let root = match root_window() { Some(r) => r, None => return false };

    // Lock modifiers: none, CapsLock (LOCK), NumLock (M2), and both combined.
    // Must register each combo with every combination so the grab fires
    // regardless of lock state.
    let lock_bits: [u16; 4] = [0, u16::from(ModMask::LOCK), u16::from(ModMask::M2),
        u16::from(ModMask::LOCK) | u16::from(ModMask::M2)];
    let shift_bits = u16::from(ModMask::SHIFT);

    let ctrl_bits = u16::from(ModMask::CONTROL);
    for &lock in &lock_bits {
        // ESC alone (no modifiers besides lock keys)
        let _ = conn.grab_key(false, root, ModMask::from(lock), KEYCODE_ESC, GrabMode::ASYNC, GrabMode::ASYNC);
        // Shift+ESC
        let _ = conn.grab_key(false, root, ModMask::from(shift_bits | lock), KEYCODE_ESC, GrabMode::ASYNC, GrabMode::ASYNC);
        // Shift+H
        let _ = conn.grab_key(false, root, ModMask::from(shift_bits | lock), KEYCODE_H, GrabMode::ASYNC, GrabMode::ASYNC);
        // Shift+Tab
        let _ = conn.grab_key(false, root, ModMask::from(shift_bits | lock), KEYCODE_TAB, GrabMode::ASYNC, GrabMode::ASYNC);
        // Shift+Button1 — consumed by Lenzu, not forwarded to browser
        let _ = conn.grab_button(false, root,
            EventMask::BUTTON_PRESS | EventMask::BUTTON_RELEASE,
            GrabMode::ASYNC, GrabMode::ASYNC,
            0u32, 0u32,
            ButtonIndex::M1,
            ModMask::from(shift_bits | lock));
        // Ctrl+Shift+Button1 — force-remote OCR path
        let _ = conn.grab_button(false, root,
            EventMask::BUTTON_PRESS | EventMask::BUTTON_RELEASE,
            GrabMode::ASYNC, GrabMode::ASYNC,
            0u32, 0u32,
            ButtonIndex::M1,
            ModMask::from(ctrl_bits | shift_bits | lock));
    }
    let _ = conn.flush();
    true
}

/// Poll our x11rb connection for pending events from passive grabs.
/// Returns (key_events, btn_events) where each entry is (detail, state).
/// For keys: detail = keycode. For buttons: detail = button number (1 = left).
/// Non-blocking — returns empty vecs if no events pending.
fn poll_x11_events() -> (Vec<(u8, u16)>, Vec<(u8, u16)>) {
    let conn = match x11_conn() { Some(c) => c, None => return (vec![], vec![]) };
    let mut key_events = vec![];
    let mut btn_events = vec![];
    loop {
        let ev = match conn.poll_for_event() {
            Ok(Some(e)) => e,
            Ok(None) => break,
            Err(_) => break,
        };
        match ev {
            Event::KeyPress(ke) => key_events.push((ke.detail, ke.state.bits())),
            Event::ButtonPress(be) => btn_events.push((be.detail, be.state.bits())),
            _ => {},
        }
    }
    (key_events, btn_events)
}

fn input_shape_clickthrough(window: &gtk4::ApplicationWindow) {
    if let Some(surface) = window.surface() {
        let region = cairo::Region::create();
        surface.set_input_region(Some(&region));
    }
}
use lenzu::capture;
use lenzu::client;
use lenzu::config;
use lenzu::furigana;
use lenzu::ocr;
use lenzu::utils;

const HISTORY_PATH: &str = "/dev/shm/lenzu/ocr_history.txt";

fn format_for_overlay(
    results: &[client::TranslationResult],
    mode: &config::OverlayRenderMode,
) -> String {
    use config::OverlayRenderMode::*;
    results
        .iter()
        .map(|r| match mode {
            Original => r.original.clone(),
            English => r.english.clone().unwrap_or_else(|| r.original.clone()),
            Furigana => r.furigana.clone()
                .or_else(|| r.romaji.clone())
                .unwrap_or_else(|| r.original.clone()),
            Romaji => r.romaji.clone().unwrap_or_else(|| r.original.clone()),
            All => {
                let mut parts = vec![r.original.clone()];
                if let Some(v) = &r.english {
                    parts.push(v.clone());
                }
                if let Some(v) = &r.furigana {
                    parts.push(v.clone());
                }
                if let Some(v) = &r.romaji {
                    parts.push(v.clone());
                }
                parts.join("  |  ")
            }
            Debug => {
                let mut parts = vec![r.original.clone()];
                if let Some(v) = &r.english {
                    parts.push(format!("en: {}", v));
                }
                if let Some(v) = &r.furigana {
                    parts.push(format!("furigana: {}", v));
                }
                if let Some(v) = &r.romaji {
                    parts.push(format!("romaji: {}", v));
                }
                if let Some(v) = &r.top_xy {
                    parts.push(format!("top: {}", v));
                }
                if let Some(v) = &r.bot_xy {
                    parts.push(format!("bot: {}", v));
                }
                if let Some(v) = &r.debug_info {
                    parts.push(format!("debug: {}", v));
                }
                parts.join("\n")
            }
        })
        .collect::<Vec<_>>()
        .join("\n---\n")
}

fn send_to_overlay(text: &str, port: u16) {
    if let Ok(socket) = std::net::UdpSocket::bind("127.0.0.1:0") {
        let addr = format!("127.0.0.1:{}", port);
        let message = serde_json::json!({
            "type": "message",
            "text": text
        });
        eprintln!(
            "[UDP] About to send message to port {}: {:?}",
            port, message
        );
        eprintln!("[HUD] Sending overlay text: {}", text);
        let _ = socket.send_to(message.to_string().as_bytes(), addr);
        eprintln!("[UDP] Sent message to port {}: {}", port, text);
    }
}

/// Send a shutdown command to the server via UDP
fn send_shutdown_command(port: u16) {
    if let Ok(socket) = std::net::UdpSocket::bind("127.0.0.1:0") {
        let addr = format!("127.0.0.1:{}", port);
        let message = serde_json::json!({
            "type": "shutdown"
        });
        let _ = socket.send_to(message.to_string().as_bytes(), addr);
        // Give the server a moment to process the shutdown command
        let _ = std::thread::sleep(std::time::Duration::from_millis(100));
    }
}

struct AppState {
    config: config::AppConfig,
    pixels: Option<gdk_pixbuf::Pixbuf>,
    ocr_result: String,
    status: String,
    last_capture: Instant,
    clipboard: Clipboard,
    api_key: String,
    is_loading: bool,
    spinner_angle: f64,
    flash_alpha: f64,
    server_process: Option<std::process::Child>,
    /// DBNet text detector, shared across capture threads via `Arc`.
    /// `None` when `text_detection_model` is not configured or the `onnx` feature is absent.
    text_detector: Option<std::sync::Arc<dyn ocr::text_detection::TextDetector + Send + Sync>>,
    /// manga-ocr-rs local OCR handle, shared across threads.
    /// When both detection and OCR confidence >= 71%, results are returned
    /// immediately without hitting any LLM service.
    local_ocr: Option<std::sync::Arc<manga_ocr_rs::MangaOcr>>,
    /// Current HUD vertical position: `true` = top, `false` = bottom.
    /// Auto-toggled when the cursor moves within 30 % of the opposite screen edge.
    hud_at_top: bool,
    /// Cloned handle to the tokio runtime owned by `main`. Used by the OCR
    /// worker to spawn async tasks. Clone freely — handles are cheap and
    /// the runtime is dropped when `main` returns.
    tokio_handle: tokio::runtime::Handle,
    /// JoinHandle of the in-flight OCR task, if any. Taking and calling
    /// abort() on this closes the underlying reqwest TCP socket, so the
    /// remote backend stops billing/computing.
    in_flight: Option<tokio::task::JoinHandle<()>>,
    /// Monotonically increasing generation id. Bumped every time a new
    /// capture starts (or the in-flight task is cancelled). The worker
    /// stamps every preview/result with the gen it was spawned under;
    /// the receiver drops mismatches so no stale frame paints the HUD.
    /// Step 6 wires the stamp + filter; step 5 only maintains the counter.
    current_generation: u64,
}

impl AppState {
    /// Cancel any in-flight OCR task and advance the generation counter.
    /// Called from every shift-modified interaction: shift+click,
    /// ctrl+shift+click, shift+tab, shift+h, and shift+esc.
    fn cancel_inflight(&mut self, reason: &'static str) {
        if let Some(h) = self.in_flight.take() {
            eprintln!("[OCR] cancel: {reason}");
            h.abort();
        }
        self.current_generation = self.current_generation.wrapping_add(1);
    }
}

/// Path to `lenzu_server` directory (dev tree only).
/// Resolved at runtime from the binary location (target/debug/lenzu →
/// ../../lenzu_server). Returns None when the binary is installed system-wide
/// (e.g. /usr/bin/lenzu) — in that case the packaged `lenzu-hud` binary on
/// PATH is used instead.
fn lenzu_server_dir() -> Option<std::path::PathBuf> {
    std::env::current_exe()
        .ok()
        .and_then(|p| {
            // binary: <repo>/target/debug/lenzu  →  parent×2 = <repo>/target  →  parent×3 = <repo>
            p.parent()?.parent()?.parent().map(|r| r.join("lenzu_server"))
        })
        .filter(|p| p.is_dir())
        .or_else(|| {
            let manifest = Path::new(env!("CARGO_MANIFEST_DIR")).join("../lenzu_server");
            manifest.is_dir().then_some(manifest)
        })
}

/// Spawn the Electron HUD. UDP port is passed via `LENZU_OVERLAY_UDP_PORT` so it
/// stays in sync with `overlay_udp_port` in `lenzu_config.json`.
///
/// Lookup order:
///   1. `lenzu-hud` on PATH — packaged install (electron-builder .deb).
///   2. Dev-tree fallback: `<repo>/lenzu_server/node_modules/.bin/electron dist/main.js`.
///
/// The HUD child is placed in its own process group so we can kill the entire
/// Electron tree on exit (Electron forks a renderer + zygote).
fn spawn_server(port: u16) -> Option<std::process::Child> {
    let mut cmd = if let Some(dir) = lenzu_server_dir() {
        // Dev tree: launch Electron directly so the child PID is the Electron process.
        let mut c = std::process::Command::new("node_modules/.bin/electron");
        c.args(["dist/main.js"]).current_dir(&dir);
        c
    } else if let Ok(appdir) = std::env::var("APPDIR") {
        // AppImage: APPDIR is set by the AppRun hook but usr/bin is NOT added to
        // PATH, so Command::new("lenzu-hud") gets ENOENT.  Use the absolute path.
        let hud = std::path::PathBuf::from(appdir).join("usr/bin/lenzu-hud");
        std::process::Command::new(hud)
    } else {
        // System install: lenzu-hud is on PATH (e.g. /usr/bin/lenzu-hud from .deb).
        std::process::Command::new("lenzu-hud")
    };

    // When the parent (lenzu) dies for any reason — clean exit, crash, or
    // SIGKILL — the kernel delivers SIGTERM to the HUD child so Electron
    // doesn't linger as an orphan.
    unsafe {
        cmd.pre_exec(|| {
            libc::prctl(libc::PR_SET_PDEATHSIG, libc::SIGTERM as libc::c_int);
            Ok(())
        });
    }

    cmd.env("LENZU_OVERLAY_UDP_PORT", port.to_string())
        .env("GTK_CSD", "0")
        .process_group(0)
        .spawn()
        .map_err(|e| eprintln!("lenzu: could not spawn HUD (lenzu-hud or dev tree): {e}"))
        .ok()
}

/// Kill the server child process and reap it.
fn kill_server(server: &mut Option<std::process::Child>, config: &config::AppConfig) {
    if let Some(mut child) = server.take() {
        // First try graceful shutdown via UDP
        send_shutdown_command(config.overlay_udp_port);
        // Give the server time to process the shutdown command
        std::thread::sleep(std::time::Duration::from_millis(500));
        // Force-kill the entire process group — Electron spawns child processes
        // that child.kill() (SIGKILL on the PID) won't reach.
        let id = child.id() as i32;
        unsafe {
            libc::kill(-id, libc::SIGKILL);
        }
        // Reap without blocking in case the process is already gone
        let _ = child.try_wait();
        eprintln!("lenzu: Electron overlay terminated.");
    }
}

fn hex_to_rgb(hex: &str) -> (f64, f64, f64) {
    let hex = hex.trim_start_matches('#');
    if hex.len() != 6 {
        return (0.0, 1.0, 0.8);
    } // Fallback
    let r = u8::from_str_radix(&hex[0..2], 16).unwrap_or(0) as f64 / 255.0;
    let g = u8::from_str_radix(&hex[2..4], 16).unwrap_or(255) as f64 / 255.0;
    let b = u8::from_str_radix(&hex[4..6], 16).unwrap_or(204) as f64 / 255.0;
    (r, g, b)
}

/// Country-flag emoji for common languages; falls back to 🌐.
fn lang_flag(lang: &Language) -> &'static str {
    match lang.to_639_1() {
        Some("ja") => "🇯🇵",
        Some("en") => "🇺🇸",
        Some("zh") => "🇨🇳",
        Some("ko") => "🇰🇷",
        Some("fr") => "🇫🇷",
        Some("de") => "🇩🇪",
        Some("es") => "🇪🇸",
        _ => "🌐",
    }
}

fn ready_status(src: &Language, dest: &Language) -> String {
    format!(
        "{}→{} | Shift+Click | Ctrl+Shift+Click | Shift+H:ヘルプ | ESC",
        lang_flag(src),
        lang_flag(dest),
    )
}

/// Send a position command to lenzu_server so the HUD moves to `"top"` or `"bottom"`.
fn send_hud_position(pos: &str, port: u16) {
    if let Ok(socket) = std::net::UdpSocket::bind("127.0.0.1:0") {
        let addr = format!("127.0.0.1:{port}");
        let msg = serde_json::json!({"type": "position", "pos": pos});
        let _ = socket.send_to(msg.to_string().as_bytes(), addr);
    }
}

/// Resolve the path to one of the NOTICES files.  Tries dev tree first
/// (`<repo>/lenzu/<filename>`), then the packaged location
/// (`/usr/share/doc/lenzu/<filename>`).
fn notices_path(filename: &str) -> Option<std::path::PathBuf> {
    let dev = Path::new(env!("CARGO_MANIFEST_DIR")).join(filename);
    if dev.is_file() {
        return Some(dev);
    }
    let packaged = Path::new("/usr/share/doc/lenzu").join(filename);
    packaged.is_file().then_some(packaged)
}

/// Scrollable dialog showing third-party license attributions.
fn show_about_dialog(parent: &gtk4::ApplicationWindow) {
    let read = |name: &str| -> Option<String> {
        notices_path(name).and_then(|p| std::fs::read_to_string(p).ok())
    };
    let body = match (read("NOTICES.md"), read("NOTICES.crates.md")) {
        (Some(curated), Some(crates)) => format!("{curated}\n\n---\n\n{crates}"),
        (Some(curated), None) => curated,
        (None, Some(crates)) => crates,
        (None, None) => "Third-party notices not found.  See:\n\
             /usr/share/doc/lenzu/NOTICES.md\n\
             https://github.com/CodeMonkeyNinja/lenzu"
            .to_string(),
    };

    let dialog = gtk4::Window::new();
    dialog.set_title(Some("Lenzu — About / Third-Party Notices"));
    dialog.set_default_size(640, 480);
    dialog.set_modal(true);
    dialog.set_transient_for(Some(parent));
    dialog.set_destroy_with_parent(true);

    let scrolled = gtk4::ScrolledWindow::new();
    scrolled.set_policy(gtk4::PolicyType::Automatic, gtk4::PolicyType::Automatic);
    scrolled.set_vexpand(true);
    scrolled.set_hexpand(true);

    let text_view = gtk4::TextView::new();
    text_view.set_editable(false);
    text_view.set_cursor_visible(false);
    text_view.set_wrap_mode(gtk4::WrapMode::Word);
    text_view.set_left_margin(12);
    text_view.set_right_margin(12);
    text_view.set_top_margin(8);
    text_view.set_bottom_margin(8);
    text_view.buffer().set_text(&body);

    scrolled.set_child(Some(&text_view));

    let close_button = gtk4::Button::with_label("Close");
    let dialog_clone = dialog.clone();
    close_button.connect_clicked(move |_| { dialog_clone.close(); });

    let vbox = gtk4::Box::new(gtk4::Orientation::Vertical, 0);
    vbox.append(&scrolled);
    vbox.append(&close_button);
    dialog.set_child(Some(&vbox));
    dialog.present();
}

/// Modal dialog listing all keyboard shortcuts, displayed in Japanese.
fn show_help_dialog(parent: &gtk4::ApplicationWindow) {
    let help = format!("\
Lenzu v{}
━━━━━━━━━━━━━━━━━━━━━━━━━━
ショートカット一覧

Shift＋クリック
  → レンズ内のテキストをOCR・翻訳

Ctrl＋Shift＋クリック
  → 全画面スキャン
    （カーソル最近傍のテキストを翻訳）

Shift＋Tab
  → 翻訳方向を切り替え
    （🇯🇵→🇺🇸  ⟷  🇺🇸→🇯🇵）

Shift＋H
  → このヘルプを表示

ESC
  → 実行中のOCRをキャンセル

Shift＋ESC
  → 終了", env!("CARGO_PKG_VERSION"));

    let dialog = gtk4::Window::new();
    dialog.set_title(Some("Lenzu ヘルプ"));
    dialog.set_default_size(420, 400);
    dialog.set_modal(true);
    dialog.set_transient_for(Some(parent));
    dialog.set_destroy_with_parent(true);

    let text_view = gtk4::TextView::new();
    text_view.set_editable(false);
    text_view.set_cursor_visible(false);
    text_view.set_wrap_mode(gtk4::WrapMode::Word);
    text_view.set_left_margin(12);
    text_view.set_right_margin(12);
    text_view.set_top_margin(8);
    text_view.set_bottom_margin(8);
    text_view.buffer().set_text(&help);

    let about_button = gtk4::Button::with_label("About");
    let close_button = gtk4::Button::with_label("Close");

    let hbox = gtk4::Box::new(gtk4::Orientation::Horizontal, 6);
    hbox.set_margin_top(6);
    hbox.set_margin_bottom(6);
    hbox.set_halign(gtk4::Align::End);
    hbox.append(&about_button);
    hbox.append(&close_button);

    let vbox = gtk4::Box::new(gtk4::Orientation::Vertical, 0);
    vbox.append(&text_view);
    vbox.append(&hbox);
    dialog.set_child(Some(&vbox));

    let dialog_clone = dialog.clone();
    let parent_clone = parent.clone();
    about_button.connect_clicked(move |_| {
        dialog_clone.close();
        show_about_dialog(&parent_clone);
    });

    let dialog_clone = dialog.clone();
    close_button.connect_clicked(move |_| { dialog_clone.close(); });

    dialog.present();
}

fn main() -> glib::ExitCode {
    if std::env::args().any(|a| a == "--version" || a == "-V") {
        println!("lenzu {}", env!("CARGO_PKG_VERSION"));
        return glib::ExitCode::SUCCESS;
    }
    eprintln!("[Lenzu] v{}", env!("CARGO_PKG_VERSION"));
    // Ensure /dev/shm/lenzu/ exists for all runtime output files.
    let _ = std::fs::create_dir_all("/dev/shm/lenzu");

    // Single-instance guard — bail out if another lenzu is already running.
    const PID_FILE: &str = "/dev/shm/lenzu/lenzu.pid";
    if let Ok(existing) = std::fs::read_to_string(PID_FILE) {
        if let Ok(pid) = existing.trim().parse::<libc::pid_t>() {
            let alive = unsafe { libc::kill(pid, 0) } == 0;
            if alive {
                eprintln!("lenzu: already running (pid {pid}) — exiting");
                std::process::exit(1);
            }
        }
    }
    let _ = std::fs::write(PID_FILE, format!("{}\n", std::process::id()));

    // Multi-threaded tokio runtime owned for the lifetime of the process.
    // The OCR worker spawns async tasks onto it so a new shift+* input can
    // cancel an in-flight HTTP request. GTK/glib still runs on the main
    // thread with its own executor — the two cooperate via `async-channel`.
    let tokio_runtime = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .expect("failed to build tokio runtime");
    let tokio_handle = tokio_runtime.handle().clone();

    // OPENROUTER_API_KEY is optional — ollama (local) is the primary backend.
    // When the key is absent, the fallback path is disabled; Ctrl+Shift+Click remote
    // override will show an error in the HUD instead of making a remote call.
    let api_key = std::env::var("OPENROUTER_API_KEY").unwrap_or_else(|_| {
        eprintln!("INFO: OPENROUTER_API_KEY not set — OpenRouter fallback disabled.");
        String::new()
    });

    let mut cfg = config::AppConfig::load();

    // CLI override: --furigana_only skips LLM enrichment and romaji
    if std::env::args().any(|a| a == "--furigana_only") {
        cfg.furigana_only = true;
        eprintln!("[Config] --furigana_only: MeCab furigana only, LLM enrichment disabled");
    }

    // CLI override: --nomecab_overwrite disables MeCab furigana overwrite on LLM fallback results
    // (comparison + warning logging still runs regardless)
    if std::env::args().any(|a| a == "--nomecab_overwrite") {
        cfg.mecab_overwrite = false;
        eprintln!("[Config] --nomecab_overwrite: MeCab will compare but NOT overwrite LLM furigana");
    }

    eprintln!(
        "[Config] llm={} | model={} | text_detection_model={} | threshold={} dilation={} pad={}x{}",
        cfg.llm_api_endpoint,
        cfg.llm_default_model,
        cfg.text_detection_model.as_deref().unwrap_or("(none)"),
        cfg.text_detection_threshold,
        cfg.text_detection_dilation,
        cfg.text_detection_pad_x,
        cfg.text_detection_pad_y,
    );
    eprintln!(
        "[Config] flags: furigana_only={} mecab_overwrite={} enrichment_enabled={} overlay_enabled={}",
        cfg.furigana_only, cfg.mecab_overwrite, cfg.enrichment_enabled, cfg.overlay_enabled,
    );

    // The configured path is relative by default ("assets/...") which only works
    // when launched from the repo root.  Inside an AppImage / .deb install /
    // sidecar tarball, search standard data dirs by basename so the binary
    // finds the model wherever the user installed it.  Updates cfg in place
    // so downstream uses (BUG-4 retry, state clones) see the resolved path.
    fn resolve_model_path(configured: &str) -> Option<String> {
        let p = std::path::Path::new(configured);
        if p.exists() {
            return Some(configured.to_string());
        }
        let basename = p.file_name()?.to_str()?;
        let xdg_data = std::env::var("XDG_DATA_HOME").ok().filter(|s| !s.is_empty())
            .or_else(|| std::env::var("HOME").ok().map(|h| format!("{h}/.local/share")));
        let candidates: Vec<String> = [
            xdg_data.map(|d| format!("{d}/lenzu/models/{basename}")),
            Some(format!("/usr/share/lenzu/models/{basename}")),
            Some(format!("/usr/local/share/lenzu/models/{basename}")),
        ].into_iter().flatten().collect();
        candidates.into_iter().find(|c| std::path::Path::new(c).exists())
    }
    if let Some(orig) = cfg.text_detection_model.clone() {
        match resolve_model_path(&orig) {
            Some(resolved) if resolved != orig => {
                eprintln!("[OCR] text detection model resolved: {orig} → {resolved}");
                cfg.text_detection_model = Some(resolved);
            }
            Some(_) => {} // CWD-relative path exists as-is (dev workflow)
            None => {
                eprintln!(
                    "[OCR] text detection model '{orig}' not found in CWD or standard data \
                     dirs (~/.local/share/lenzu/models/, /usr/share/lenzu/models/) — \
                     install lenzu-models-dbnet .deb or untar the sidecar tarball"
                );
            }
        }
    }

    // Build the text detector once at startup; shared across capture threads via Arc.
    let text_detector: Option<std::sync::Arc<dyn ocr::text_detection::TextDetector + Send + Sync>> =
        match ocr::text_detection::build_text_detector(
            cfg.text_detection_model.as_deref(),
            cfg.text_detection_threshold,
            cfg.text_detection_dilation,
            cfg.text_detection_pad_x,
            cfg.text_detection_pad_y,
        ) {
            Ok(Some(arc)) => {
                eprintln!("[OCR] text detection enabled ({})", cfg.text_detection_model.as_deref().unwrap_or(""));
                Some(arc)
            }
            Ok(None) => None,
            Err(e) => {
                eprintln!("[OCR] failed to load text detector: {e} — falling back to full-image OCR");
                None
            }
        };

    // Load manga-ocr-rs models once at startup for local-first OCR.
    // Requires the text detector to be enabled — no point doing local OCR
    // without detection to produce bounding boxes.
    let local_ocr: Option<std::sync::Arc<manga_ocr_rs::MangaOcr>> = if text_detector.is_some() {
        match ocr::local_ocr::LocalOcrEngine::new() {
            Ok(engine) => {
                eprintln!("[OCR] local manga-ocr loaded — confidence-gated pipeline active");
                Some(engine.handle())
            }
            Err(e) => {
                eprintln!("[OCR] manga-ocr unavailable ({e}) — local-first pipeline disabled");
                None
            }
        }
    } else {
        None
    };

    let app = gtk4::Application::builder()
        .application_id("io.github.codemonkeyninja.lenzu")
        .build();

    app.connect_activate(move |app_ref| {
        init_x11();

        let server_process = if cfg.overlay_enabled {
            spawn_server(cfg.overlay_udp_port)
        } else {
            None
        };

        let state = Rc::new(RefCell::new(AppState {
            config: cfg.clone(),
            pixels: None,
            ocr_result: String::new(),
            status: ready_status(&cfg.translate_src, &cfg.translate_dest),
            last_capture: Instant::now() - Duration::from_secs(2),
            clipboard: Clipboard::new().expect("Failed to init clipboard"),
            api_key: api_key.clone(),
            is_loading: false,
            spinner_angle: 0.0,
            flash_alpha: 0.0,
            server_process,
            text_detector: text_detector.clone(),
            local_ocr: local_ocr.clone(),
            hud_at_top: false,
            tokio_handle: tokio_handle.clone(),
            in_flight: None,
            current_generation: 0,
        }));

        let window = gtk4::ApplicationWindow::new(app_ref);
        window.set_default_size(cfg.lens_size, cfg.lens_size + cfg.ui_panel_height);
        window.set_decorated(false);

        let css = gtk4::CssProvider::new();
        css.load_from_string("window { background: transparent; }");
        let display = gtk4::prelude::RootExt::display(&window);
        gtk4::style_context_add_provider_for_display(&display, &css, gtk4::STYLE_PROVIDER_PRIORITY_APPLICATION);

        // Key combos (ESC, Shift+ESC, Shift+H, Shift+Tab) are handled via X11
        // passive key grabs (setup_key_grabs) and polled in the 16ms timer so
        // they work even when the window has no WM focus (override_redirect).

        let cfg_del = cfg.clone();
        let state_del = state.clone();
        window.connect_close_request(move |_| {
            kill_server(&mut state_del.borrow_mut().server_process, &cfg_del);
            glib::Propagation::Proceed
        });

        // (gen_id, result) — gen_id lets the receiver drop stale frames from
        // a cancelled generation that were already in the buffer before abort()
        // fired. abort() stops future sends, but the channel may hold sends that
        // ran just before the cancel; the gen stamp is how we filter those out.
        let (tx, rx) = async_channel::bounded::<(u64, Result<(Vec<client::TranslationResult>, client::OcrMeta), String>)>(3);

        let area = gtk4::DrawingArea::new();
        area.set_hexpand(true);
        area.set_vexpand(true);

        let state_draw = state.clone();
        area.set_draw_func(move |da, cr, _w, _h| {
            let s = match state_draw.try_borrow() {
                Ok(s) => s,
                Err(_) => return,
            };
            // HUD color: override based on session paid token spend
            let (r, g, b) = {
                let (sp, sc) = client::session_paid_tokens();
                let total = sp + sc;
                if s.config.token_critical_threshold > 0 && total >= s.config.token_critical_threshold {
                    (1.0, 0.27, 0.27) // red (#FF4444)
                } else if s.config.token_warning_threshold > 0 && total >= s.config.token_warning_threshold {
                    (1.0, 0.65, 0.0) // orange (#FFA600)
                } else {
                    hex_to_rgb(&s.config.hud_color_hex)
                }
            };

            cr.set_source_rgba(0.0, 0.0, 0.0, 0.0);
            cr.set_operator(cairo::Operator::Source);
            cr.paint().ok();
            cr.set_operator(cairo::Operator::Over);

            if let Some(ref pb) = s.pixels {
                cr.set_source_pixbuf(pb, 0.0, 0.0);
                cr.paint().ok();
            }

            if s.flash_alpha > 0.0 {
                cr.set_source_rgba(1.0, 1.0, 1.0, s.flash_alpha);
                cr.rectangle(
                    0.0,
                    0.0,
                    s.config.lens_size as f64,
                    s.config.lens_size as f64,
                );
                cr.fill().ok();
            }

            cr.set_source_rgb(r, g, b);
            cr.set_line_width(2.0);
            cr.rectangle(
                1.0,
                1.0,
                (s.config.lens_size - 2) as f64,
                (s.config.lens_size - 2) as f64,
            );
            cr.stroke().ok();

            cr.set_source_rgba(0.01, 0.01, 0.05, 0.85);
            cr.rectangle(
                0.0,
                s.config.lens_size as f64,
                s.config.lens_size as f64,
                s.config.ui_panel_height as f64,
            );
            cr.fill().ok();

            let context = da.pango_context();
            let layout = pango::Layout::new(&context);

            cr.set_source_rgb(r, g, b);
            layout.set_text(&s.status);
            cr.move_to(12.0, (s.config.lens_size + 10) as f64);
            pangocairo::functions::show_layout(cr, &layout);

            if s.is_loading {
                cr.save().ok();
                cr.translate(
                    (s.config.lens_size - 30) as f64,
                    (s.config.lens_size + 20) as f64,
                );
                cr.rotate(s.spinner_angle);
                cr.set_line_width(3.0);
                cr.set_source_rgb(r, g, b);
                cr.new_sub_path();
                cr.arc(0.0, 0.0, 8.0, 0.0, 1.5 * std::f64::consts::PI);
                cr.stroke().ok();
                cr.restore().ok();
            }

            cr.set_source_rgb(1.0, 1.0, 1.0);
            let font_str = format!("Sans Bold {}", s.config.font_size);
            let font_desc = pango::FontDescription::from_string(&font_str);
            layout.set_font_description(Some(&font_desc));
            layout.set_text(&s.ocr_result);
            layout.set_width(pango::units_from_double((s.config.lens_size - 24) as f64));
            layout.set_ellipsize(pango::EllipsizeMode::End);
            cr.move_to(12.0, (s.config.lens_size + 40) as f64);
            pangocairo::functions::show_layout(cr, &layout);
        });

        window.set_child(Some(&area));

        let state_rx = state.clone();
        let window_rx = window.clone();
        glib::spawn_future_local(async move {
        while let Ok((msg_gen, api_result)) = rx.recv().await {
        let mut s = state_rx.borrow_mut();
        // Drop results from a cancelled / superseded generation so stale
        // frames never paint the HUD. abort() kills future sends; this
        // handles the window where sends already sat in the channel buffer
        // before abort() fired.
        if msg_gen != s.current_generation {
            continue;
        }
        match api_result {
            Ok((results, meta)) => {
                // Preview results show text immediately but keep the lens modal
                // (is_loading stays true) so the user can't stack new requests.
                if !meta.preview {
                    s.is_loading = false;
                    // Display window starts from when the result is ready, not from
                    // click time. Without this reset, fast OCR (< result_display_secs)
                    // keeps show_lens=true for several seconds after the result is
                    // already visible, making the lens appear "stuck".
                    s.last_capture = Instant::now();
                }

                let combined_english = results
                    .iter()
                    .map(|r| r.english.clone().unwrap_or_else(|| r.original.clone()))
                    .collect::<Vec<_>>()
                    .join("\n");

                s.ocr_result = combined_english.clone();
                s.status = if meta.preview {
                    format!("OCR done ({} items) — enriching…", results.len())
                } else {
                    format!("SUCCESS ({} items)", results.len())
                };

                if s.config.overlay_enabled {
                    let text = format_for_overlay(&results, &s.config.overlay_render_mode);
                    eprintln!("[HUD] Overlay enabled – prepared text: {}", text);
                    send_to_overlay(&text, s.config.overlay_udp_port);
                }

                // Only update clipboard and history on final results (not preview)
                if !meta.preview {
                    let combined_original = results
                        .iter()
                        .map(|r| r.original.clone())
                        .collect::<Vec<_>>()
                        .join("\n");
                    let _ = s.clipboard.set_text(combined_original);

                    let trimmed_english = combined_english.trim();
                    if !trimmed_english.is_empty() {
                        if let Ok(mut f) = OpenOptions::new()
                            .create(true)
                            .append(true)
                            .open(HISTORY_PATH)
                        {
                            let combined_original = results
                                .iter()
                                .map(|r| r.original.clone())
                                .collect::<Vec<_>>()
                                .join(" / ");
                            let _ = writeln!(
                                f,
                                "[{}] ({}, {:.1}s) {} → {}",
                                chrono::Local::now().format("%H:%M:%S"),
                                meta.backend,
                                meta.elapsed_ms as f64 / 1000.0,
                                combined_original.trim(),
                                trimmed_english
                            );
                        } else {
                            eprintln!("[history] failed to open {}", HISTORY_PATH);
                        }
                    }
                }
            }
            Err(e) => {
                s.is_loading = false;
                s.last_capture = Instant::now();
                eprintln!("[OCR] API/parse error: {}", e);
                s.status = format!("API Error: {}", e);
            }
        }
            window_rx.queue_draw();
        }
    });

    let window_anim = window.clone();
    let state_anim = state.clone();
    glib::timeout_add_local(Duration::from_millis(16), move || {
        let mut s = state_anim.borrow_mut();
        if s.is_loading {
            s.spinner_angle += 0.2;
            window_anim.queue_draw();
        }
        if s.flash_alpha > 0.0 {
            s.flash_alpha -= 0.1;
            window_anim.queue_draw();
        }
        glib::ControlFlow::Continue
    });

        let window_main = window.clone();
        let state_main = state.clone();
        let app_main = app_ref.clone();
        let mut esc_was_down = false;
        let mut h_was_down = false;
        let mut tab_was_down = false;
        let mut esc_last_action = Instant::now();
        let mut h_last_action = Instant::now();
        let mut tab_last_action = Instant::now();
        glib::timeout_add_local(Duration::from_millis(16), move || {
            let (x, y, mask) = match pointer_position() {
                Some(p) => p,
                None => {
                    eprintln!("[DBG] pointer_position() returned None — X11 conn or root window missing");
                    return glib::ControlFlow::Continue;
                }
            };

            let s_conf = state_main.borrow().config.clone();
            let win_x = x - (s_conf.lens_size / 2);
            let win_y = y - (s_conf.lens_size / 2);

            let shift_bits = KeyButMask::SHIFT.bits();
            let ctrl_bits = KeyButMask::CONTROL.bits();

            // Show the lens only while Shift is held (preview), while OCR is running,
            // or for a configured number of seconds after the last capture.
            // Use off-screen positioning to keep the GDK surface mapped (no frame-clock gaps).
            let shift_held = (mask & shift_bits) != 0;
            let show_lens = {
                let s = state_main.borrow();
                shift_held || s.is_loading || s.last_capture.elapsed() < Duration::from_secs(s.config.result_display_secs)
            };
            if show_lens {
                move_window(win_x, win_y);
                window_main.queue_draw();
            } else {
                move_window(-10000, -10000);
            }

            // ── Key combo + button detection via X11 passive grabs ───────────────
            // Passive grabs deliver KeyPress/ButtonPress events on our x11rb connection
            // even when the lens window has no WM focus (override_redirect bypasses WM).
            // Rising-edge detection: act on first event per press-release cycle.
            // 200ms debounce prevents auto-repeat double-trigger.
            let (key_events, btn_events) = poll_x11_events();
            {
                let mut esc_in_events = false;
                let mut h_in_events = false;
                let mut tab_in_events = false;
                let mut esc_state = 0u16;
                for &(kc, state) in &key_events {
                    if kc == KEYCODE_ESC { esc_in_events = true; esc_state = state; }
                    if kc == KEYCODE_H { h_in_events = true; }
                    if kc == KEYCODE_TAB { tab_in_events = true; }
                }

                if esc_in_events && !esc_was_down && esc_last_action.elapsed() > Duration::from_millis(200) {
                    esc_last_action = Instant::now();
                    if (esc_state & KeyButMask::SHIFT.bits()) != 0 {
                        let mut s = state_main.borrow_mut();
                        let cfg = s.config.clone();
                        kill_server(&mut s.server_process, &cfg);
                        drop(s);
                        app_main.quit();
                        return glib::ControlFlow::Break;
                    }
                    let mut s = state_main.borrow_mut();
                    s.cancel_inflight("user-cancel");
                    s.is_loading = false;
                    s.status = ready_status(&s.config.translate_src, &s.config.translate_dest);
                    drop(s);
                    window_main.queue_draw();
                }
                if h_in_events && !h_was_down && h_last_action.elapsed() > Duration::from_millis(200) {
                    h_last_action = Instant::now();
                    {
                        let mut s = state_main.borrow_mut();
                        s.cancel_inflight("help-dialog");
                        s.is_loading = false;
                    }
                    show_help_dialog(&window_main);
                }
                if tab_in_events && !tab_was_down && tab_last_action.elapsed() > Duration::from_millis(200) {
                    tab_last_action = Instant::now();
                    let mut s = state_main.borrow_mut();
                    s.cancel_inflight("direction-toggle");
                    s.is_loading = false;
                    let (new_src, new_dest) = (s.config.translate_dest, s.config.translate_src);
                    s.config.translate_src = new_src;
                    s.config.translate_dest = new_dest;
                    s.status = ready_status(&s.config.translate_src, &s.config.translate_dest);
                    drop(s);
                    window_main.queue_draw();
                }
                esc_was_down = esc_in_events;
                h_was_down   = h_in_events;
                tab_was_down = tab_in_events;
            }

            // ── Shift+Click detection via XGrabButton passive grab ────────────────
            // Shift+Button1 is consumed by our passive grab (browser never sees it).
            // Detect from ButtonPress events in the x11rb connection buffer, not from
            // query_pointer mask (which would fire continuously while held down).
            let is_shift_click = btn_events.iter().any(|&(btn, state)| {
                btn == 1 && (state & shift_bits) != 0
            });
            let force_remote = btn_events.iter().any(|&(btn, state)| {
                btn == 1 && (state & shift_bits) != 0 && (state & ctrl_bits) != 0
            });

            // ── HUD auto-reposition ───────────────────────────────────────────────
            // Cursor in bottom 30 % → HUD to top; cursor in top 30 % → HUD to bottom.
            // 30–70 % is a dead zone to prevent oscillation.
            {
                let sh = screen_height();
                let frac = if sh > 0 { y as f32 / sh as f32 } else { 0.5 };
                let (cur_top, port) = {
                    let s = state_main.borrow();
                    (s.hud_at_top, s.config.overlay_udp_port)
                };
                if frac > 0.70 && !cur_top {
                    send_hud_position("top", port);
                    state_main.borrow_mut().hud_at_top = true;
                } else if frac < 0.30 && cur_top {
                    send_hud_position("bottom", port);
                    state_main.borrow_mut().hud_at_top = false;
                }
            }

            if is_shift_click {
            // A new shift-modified click supersedes any in-flight OCR — abort
            // the old request's TCP socket so the remote backend stops billing,
            // and clear is_loading so the new capture isn't blocked by the old
            // task's un-run completion handler.
            {
                let mut s = state_main.borrow_mut();
                if s.in_flight.is_some() {
                    s.cancel_inflight("new-capture");
                    s.is_loading = false;
                }
            }

            // 1s debounce only — the is_loading gate was dropped now that
            // cancel_inflight above supersedes any in-progress OCR. The
            // debounce still prevents accidental double-clicks from firing
            // two captures in rapid succession (which is a different concern
            // from "user deliberately clicked again because they're tired
            // of waiting" — that's handled by cancel_inflight).
            // IMPORTANT: do NOT hold borrow_mut() across gtk::main_iteration() —
            // the animation timer also calls borrow_mut() and will panic.
            let should_capture = {
                let s = state_main.borrow();
                s.last_capture.elapsed() > Duration::from_secs(1)
            };

            if should_capture {
                // Arm state, extract config values, then DROP borrow before event loop.
                let (fallback_api_key, primary_endpoint, primary_model,
                     free_remote_endpoint, free_remote_model,
                     fallback_endpoint, fallback_model, prompt,
                     per_region_prompt, text_detector, local_ocr,
                     enrichment_enabled, enrichment_model, enrichment_prompt,
                     enrichment_timeout_secs) = {
                    let mut s = state_main.borrow_mut();
                    s.last_capture = Instant::now();
                    s.status = if force_remote {
                        "CAPTURING (remote)...".to_string()
                    } else {
                        "CAPTURING...".to_string()
                    };
                    s.is_loading = true;
                    s.flash_alpha = 1.0;
                    let vals = (
                        s.api_key.clone(),
                        s.config.llm_api_endpoint.clone(),
                        s.config.llm_default_model.clone(),
                        s.config.free_remote_endpoint.clone(),
                        s.config.free_remote_model.clone(),
                        s.config.fallback_llm_api_endpoint.clone(),
                        s.config.fallback_llm_model.clone(),
                        s.config.resolved_prompt(),
                        s.config.resolved_per_region_prompt(),
                        s.text_detector.clone(),
                        s.local_ocr.clone(),
                        s.config.enrichment_enabled,
                        s.config.enrichment_model.clone(),
                        s.config.resolved_enrichment_prompt(),
                        s.config.enrichment_timeout_secs,
                    );
                    vals
                    // borrow_mut dropped here — safe for other callbacks to borrow
                };

                window_main.queue_draw();
                move_window(-10000, -10000);
                while glib::MainContext::default().iteration(false) {}
                std::thread::sleep(Duration::from_millis(400));

                // Feature 2: Ctrl+Shift+Click with DBNet → capture full desktop so DBNet
                // can scan the entire screen for the closest text region to the cursor.
                // Without a detector (no `onnx` feature or no model configured) the normal
                // lens-sized capture is used and sent to the remote backend unchanged.
                let is_fullscreen_scan = force_remote && text_detector.is_some();
                let (cap_x, cap_y, cap_w, cap_h) = if is_fullscreen_scan {
                    let (sw, sh) = capture::screen_size().unwrap_or((1920, 1080));
                    (0i32, 0i32, sw, sh)
                } else {
                    (win_x.max(0), win_y.max(0),
                     s_conf.lens_size as u32, s_conf.lens_size as u32)
                };
                // Cursor position inside the captured image (same as screen coords when
                // cap origin is (0,0); used by closest_box_to_point in Feature 2).
                let cursor_cap_x = (x - cap_x) as u32;
                let cursor_cap_y = (y - cap_y) as u32;

                match capture::capture_x11(cap_x, cap_y, cap_w, cap_h) {
                    Ok(raw) => {
                        let dyn_image = utils::raw_to_dynamic_image(&raw, cap_w, cap_h);

                        // Only update the lens pixbuf for the lens-sized capture; the
                        // full-desktop image is too large to display in the small window.
                        if !is_fullscreen_scan {
                            let mut pb_data = raw.clone();
                            utils::swap_bytes_for_pixbuf(&mut pb_data);
                            let mut s = state_main.borrow_mut();
                            s.pixels = Some(gdk_pixbuf::Pixbuf::from_mut_slice(
                                pb_data,
                                gdk_pixbuf::Colorspace::Rgb,
                                true,
                                8,
                                s_conf.lens_size,
                                s_conf.lens_size,
                                s_conf.lens_size * 4,
                            ));
                        }

                        move_window(win_x, win_y);
                        let tx_clone = tx.clone();
                        let (tokio_handle_thread, gen_id) = {
                            let s = state_main.borrow();
                            (s.tokio_handle.clone(), s.current_generation)
                        };
                        // Spawn on the shared tokio runtime and stash the JoinHandle in
                        // AppState so a later shift-modified interaction can call abort()
                        // on it — which closes the in-flight reqwest TCP socket so the
                        // remote backend stops billing/computing.
                        let handle = tokio_handle_thread.spawn(async move {
                            let mut result: Result<(Vec<client::TranslationResult>, client::OcrMeta), String> = async {
                                // Greyscale once; DBNet only needs luminance and this avoids
                                // the crate doing its own (possibly inconsistent) conversion.
                                // CPU work inside an async task — block_in_place yields the
                                // tokio worker slot so other tasks can progress. Safe here
                                // because the outer runtime is rt-multi-thread.
                                let gray_image = tokio::task::block_in_place(|| dyn_image.grayscale());

                                // ── Feature 2: Ctrl+Shift+Click ──────────────────────────────
                                // Full desktop was captured; run DBNet and pick the text region
                                // nearest the cursor, then send that tight crop to remote OCR.
                                if force_remote {
                                    if let Some(ref det) = text_detector {
                                        let mut boxes = tokio::task::block_in_place(|| det.detect(&gray_image));
                                        utils::save_fullscreen_debug(&dyn_image, &boxes);

                                        if !boxes.is_empty() {
                                            let mut idx = ocr::text_detection::closest_box_to_point(
                                                &boxes, cursor_cap_x, cursor_cap_y,
                                            );

                                            // BUG-4: if the chosen box is an over-merge (spans > 60%
                                            // of screen width OR height), crop to that box and re-run
                                            // DBNet on just that sub-image using scale-table params
                                            // appropriate for the sub-image size.  Sub-box coordinates
                                            // are remapped back to original image space (+ox, +oy) so
                                            // the cursor position stays valid for closest_box_to_point.
                                            let chosen_w = boxes[idx].width();
                                            let chosen_h = boxes[idx].height();
                                            if chosen_w as u64 > cap_w as u64 * 6 / 10
                                                || chosen_h as u64 > cap_h as u64 * 6 / 10
                                            {
                                                let entry = s_conf.detection_params_for(chosen_w, chosen_h);
                                                if let Ok(Some(det_nd)) = ocr::text_detection::build_text_detector(
                                                    s_conf.text_detection_model.as_deref(),
                                                    entry.threshold,
                                                    entry.dilation,
                                                    entry.pad_x,
                                                    entry.pad_y,
                                                ) {
                                                    let ox = boxes[idx].x1;
                                                    let oy = boxes[idx].y1;
                                                    let sub_boxes = tokio::task::block_in_place(|| {
                                                        let sub = dyn_image.crop_imm(ox, oy, chosen_w, chosen_h);
                                                        let sub_gray = sub.grayscale();
                                                        det_nd.detect(&sub_gray)
                                                    });
                                                    // Remap from crop-space → original image space so
                                                    // cursor coords stay valid for closest_box_to_point.
                                                    let remapped: Vec<ocr::text_detection::TextBoundingBox> =
                                                        sub_boxes.into_iter().map(|b| {
                                                            ocr::text_detection::TextBoundingBox {
                                                                x1: b.x1 + ox,
                                                                y1: b.y1 + oy,
                                                                x2: b.x2 + ox,
                                                                y2: b.y2 + oy,
                                                                confidence: b.confidence,
                                                                contours: b.contours,
                                                            }
                                                        }).collect();
                                                    if remapped.len() > boxes.len() {
                                                        eprintln!(
                                                            "[DBNet] BUG-4 retry (sub-crop {}×{}, \
                                                             dilation={} thr={:.2}): {} boxes \
                                                             (was {}, chosen {:.0}%w {:.0}%h of screen)",
                                                            chosen_w, chosen_h,
                                                            entry.dilation, entry.threshold,
                                                            remapped.len(),
                                                            boxes.len(),
                                                            chosen_w as f64 / cap_w as f64 * 100.0,
                                                            chosen_h as f64 / cap_h as f64 * 100.0,
                                                        );
                                                        boxes = remapped;
                                                        idx = ocr::text_detection::closest_box_to_point(
                                                            &boxes, cursor_cap_x, cursor_cap_y,
                                                        );
                                                        utils::save_fullscreen_debug(&dyn_image, &boxes);
                                                    }
                                                }
                                            }
                                            // ── Local-first OCR on chosen box ──────────
                                            // Try manga-ocr-rs before hitting the LLM chain.
                                            if let Some(ref mocr) = local_ocr {
                                                let chosen = &boxes[idx];
                                                if chosen.confidence >= 0.71 {
                                                    let (local_result, _) = tokio::task::block_in_place(|| {
                                                        let engine = ocr::local_ocr::LocalOcrEngine::from_arc(
                                                            std::sync::Arc::clone(mocr),
                                                        );
                                                        engine.try_local_pipeline(
                                                            &dyn_image, &[chosen.clone()],
                                                            s_conf.text_detection_crop_padding, 256,
                                                            Some(s_conf.low_conf_max_chars),
                                                        )
                                                    });
                                                    if let Some(results) = local_result {
                                                        let mut t_results: Vec<client::TranslationResult> = results.iter().map(|r| {
                                                            client::TranslationResult {
                                                                original: r.text.clone(),
                                                                top_xy: Some(format!("{},{}", r.source_box.x1, r.source_box.y1)),
                                                                bot_xy: Some(format!("{},{}", r.source_box.x2, r.source_box.y2)),
                                                                debug_info: Some(format!(
                                                                    "local-ocr det:{:.0}% ocr:{:.1}% {}ms",
                                                                    r.source_box.confidence * 100.0,
                                                                    r.confidence * 100.0, r.ocr_ms,
                                                                )),
                                                                ..Default::default()
                                                            }
                                                        }).collect();
                                                        let total_ocr_ms: u128 = results.iter().map(|r| r.ocr_ms).sum();
                                                        eprintln!("[OCR] fullscreen local-first succeeded — skipping LLM chain");
                                                        // Phase 1: raw text preview
                                                        let _ = tx_clone.send((gen_id, Ok((t_results.clone(), client::OcrMeta {
                                                            backend: "local:manga-ocr".to_string(),
                                                            elapsed_ms: total_ocr_ms,
                                                            preview: true,
                                                        })))).await;

                                                        // Phase 2: furigana (+ romaji unless furigana_only) — MeCab, ~5ms
                                                        let furigana_ok = furigana::annotate(&mut t_results, s_conf.furigana_only);
                                                        let do_enrich = enrichment_enabled && !s_conf.furigana_only;
                                                        if furigana_ok && do_enrich {
                                                            let _ = tx_clone.send((gen_id, Ok((t_results.clone(), client::OcrMeta {
                                                                backend: "local:manga-ocr+furigana".to_string(),
                                                                elapsed_ms: total_ocr_ms,
                                                                preview: true,
                                                            })))).await;
                                                        }

                                                        // Phase 3: LLM enrichment (translation) — skipped in furigana_only mode
                                                        let mut enriched = false;
                                                        if do_enrich {
                                                            let enrich_model = enrichment_model.as_deref().unwrap_or(&primary_model);
                                                            enriched = client::enrich_local_results(
                                                                &mut t_results, &primary_endpoint, enrich_model,
                                                                &enrichment_prompt, enrichment_timeout_secs,
                                                                s_conf.primary_num_ctx,
                                                            ).await;
                                                        }

                                                        // Final send (via closure return → line 1140)
                                                        let backend = match (furigana_ok, enriched) {
                                                            (true, true)  => "local:manga-ocr+furigana+enriched",
                                                            (true, false) => "local:manga-ocr+furigana",
                                                            (false, true) => "local:manga-ocr+enriched",
                                                            (false, false) => "local:manga-ocr",
                                                        };
                                                        return Ok((t_results, client::OcrMeta {
                                                            backend: backend.to_string(),
                                                            elapsed_ms: total_ocr_ms,
                                                            preview: false,
                                                        }));
                                                    }
                                                    eprintln!("[OCR] fullscreen local-first: confidence below gate — falling through to LLM");
                                                }
                                            }

                                            let cropper = ocr::text_cropper::TextCropper::new(
                                                s_conf.text_detection_crop_padding, 256,
                                            ).with_pad_percent(0.10);
                                            if let Some(crop) = cropper
                                                .crop(&dyn_image, &[boxes[idx].clone()])
                                                .into_iter()
                                                .next()
                                            {
                                                eprintln!(
                                                    "[DBNet] fullscreen: {} boxes, closest idx={} \
                                                     box=({},{})→({},{}) cursor=({},{})",
                                                    boxes.len(), idx,
                                                    boxes[idx].x1, boxes[idx].y1,
                                                    boxes[idx].x2, boxes[idx].y2,
                                                    cursor_cap_x, cursor_cap_y,
                                                );
                                                let dual = client::DualOcrClient::new(
                                                    primary_endpoint,
                                                    primary_model,
                                                    s_conf.local_fallback_models.clone(),
                                                    free_remote_endpoint,
                                                    free_remote_model,
                                                    fallback_endpoint,
                                                    fallback_model,
                                                    s_conf.extra_fallback_models.clone(),
                                                    fallback_api_key,
                                                    s_conf.fallback_max_dimension,
                                                    s_conf.primary_max_dimension,
                                                    s_conf.primary_num_ctx,
                                                    s_conf.local_timeout_secs,
                                                    s_conf.remote_timeout_secs,
                                                    s_conf.paid_remote_timeout_secs,
                                                    prompt,
                                                );
                                                return dual
                                                    .call_api_force_fallback(&crop.image)
                                                    .await
                                                    .map_err(|e| e.to_string());
                                            }
                                        }
                                        return Err(
                                            "No text detected near cursor".to_string()
                                        );
                                    }
                                    // No detector — fall through to Phase 1 with lens image
                                }

                                // ── Feature 1: Shift+Click (per-region DBNet) ────────────────
                                // DBNet detects all text regions in the lens capture; each crop
                                // is sent as a separate OCR request with the per-region prompt.
                                // Falls back to full-image OCR if detection finds nothing or all
                                // per-region calls fail.
                                //
                                // OPT-1: keep the detected boxes so the Phase 1 fallback can
                                // crop to the union bbox instead of sending the full lens image.
                                let mut detected_boxes: Vec<ocr::text_detection::TextBoundingBox> = Vec::new();
                                if !force_remote {
                                    if let Some(ref det) = text_detector {
                                        let boxes = tokio::task::block_in_place(|| {
                                            let raw_boxes = det.detect(&gray_image);
                                            // Refine: merge overlapping clusters and
                                            // re-detect within each union region to
                                            // split stacked bubbles / remove bubble-wrap dupes.
                                            ocr::local_ocr::refine_boxes(
                                                &raw_boxes, &dyn_image, det.as_ref(),
                                            )
                                        });
                                        // Save lens debug image with refined bounding boxes.
                                        utils::save_lens_debug(&dyn_image, &boxes);

                                        if !boxes.is_empty() {
                                            detected_boxes = boxes.clone();

                                            // ── Local-first OCR (manga-ocr-rs) ─────────────────
                                            // If all boxes have detection confidence >= 71% AND
                                            // manga-ocr-rs returns OCR confidence >= 71% for each,
                                            // return immediately — no LLM needed.
                                            if let Some(ref mocr) = local_ocr {
                                                // Incremental: send each box's result to
                                                // HUD as it completes so text accumulates
                                                // on screen instead of flashing.
                                                let tx_progress = tx_clone.clone();
                                                let furigana_only_flag = s_conf.furigana_only;
                                                let (local_result, partials) = tokio::task::block_in_place(|| {
                                                    let engine = ocr::local_ocr::LocalOcrEngine::from_arc(
                                                        std::sync::Arc::clone(mocr),
                                                    );
                                                    engine.try_local_pipeline_incremental(
                                                    &dyn_image,
                                                    &boxes,
                                                    s_conf.text_detection_crop_padding,
                                                    256,
                                                    Some(s_conf.low_conf_max_chars),
                                                    |accumulated| {
                                                        // Show all results as preview — even low-
                                                        // confidence garbage — so the HUD indicates
                                                        // the system is working while the LLM fallback
                                                        // chain produces the final accurate result.
                                                        let mut t_results: Vec<client::TranslationResult> = accumulated.iter().map(|r| {
                                                            client::TranslationResult {
                                                                original: r.text.clone(),
                                                                top_xy: Some(format!("{},{}", r.source_box.x1, r.source_box.y1)),
                                                                bot_xy: Some(format!("{},{}", r.source_box.x2, r.source_box.y2)),
                                                                debug_info: Some(format!(
                                                                    "local-ocr det:{:.0}% ocr:{:.1}% {}ms",
                                                                    r.source_box.confidence * 100.0,
                                                                    r.confidence * 100.0,
                                                                    r.ocr_ms,
                                                                )),
                                                                ..Default::default()
                                                            }
                                                        }).collect();
                                                        furigana::annotate(&mut t_results, furigana_only_flag);
                                                        let total_ms: u128 = accumulated.iter().map(|r| r.ocr_ms).sum();
                                                        let _ = tx_progress.send_blocking((gen_id, Ok((t_results, client::OcrMeta {
                                                            backend: format!("local:manga-ocr+furigana ({}/{})", accumulated.len(), accumulated.len()),
                                                            elapsed_ms: total_ms,
                                                            preview: true,
                                                        }))));
                                                    },
                                                )
                                                });
                                                // --furigana_only ⇒ never call the LLM, even when local OCR confidence
                                                // is below gate.  Accept the partials and let MeCab annotate whatever
                                                // text we got; the user explicitly opted out of LLM enrichment.
                                                let accepted = local_result.or_else(|| {
                                                    if s_conf.furigana_only && !partials.is_empty() {
                                                        eprintln!(
                                                            "[OCR] --furigana_only: accepting {} low-confidence local result(s) (skipping LLM chain)",
                                                            partials.len(),
                                                        );
                                                        Some(partials)
                                                    } else {
                                                        None
                                                    }
                                                });
                                                if let Some(results) = accepted {
                                                    let mut t_results: Vec<client::TranslationResult> = results.iter().map(|r| {
                                                        client::TranslationResult {
                                                            original: r.text.clone(),
                                                            top_xy: Some(format!("{},{}", r.source_box.x1, r.source_box.y1)),
                                                            bot_xy: Some(format!("{},{}", r.source_box.x2, r.source_box.y2)),
                                                            debug_info: Some(format!(
                                                                "local-ocr det:{:.0}% ocr:{:.1}% {}ms",
                                                                r.source_box.confidence * 100.0,
                                                                r.confidence * 100.0,
                                                                r.ocr_ms,
                                                            )),
                                                            ..Default::default()
                                                        }
                                                    }).collect();
                                                    let total_ocr_ms: u128 = results.iter().map(|r| r.ocr_ms).sum();
                                                    eprintln!("[OCR] local-first pipeline succeeded — {} results, skipping LLM chain", t_results.len());

                                                    // Phase 2: furigana (+ romaji unless furigana_only) — MeCab, ~5ms
                                                    let furigana_ok = furigana::annotate(&mut t_results, s_conf.furigana_only);
                                                    let do_enrich = enrichment_enabled && !s_conf.furigana_only;
                                                    if furigana_ok && do_enrich {
                                                        let _ = tx_clone.send((gen_id, Ok((t_results.clone(), client::OcrMeta {
                                                            backend: "local:manga-ocr+furigana".to_string(),
                                                            elapsed_ms: total_ocr_ms,
                                                            preview: true,
                                                        })))).await;
                                                    }

                                                    // Phase 3: LLM enrichment (translation) — skipped in furigana_only mode
                                                    let mut enriched = false;
                                                    if do_enrich {
                                                        let enrich_model = enrichment_model.as_deref().unwrap_or(&primary_model);
                                                        enriched = client::enrich_local_results(
                                                            &mut t_results, &primary_endpoint, enrich_model,
                                                            &enrichment_prompt, enrichment_timeout_secs,
                                                            s_conf.primary_num_ctx,
                                                        ).await;
                                                    }

                                                    // Final send (via closure return)
                                                    let backend = match (furigana_ok, enriched) {
                                                        (true, true)  => "local:manga-ocr+furigana+enriched",
                                                        (true, false) => "local:manga-ocr+furigana",
                                                        (false, true) => "local:manga-ocr+enriched",
                                                        (false, false) => "local:manga-ocr",
                                                    };
                                                    return Ok((t_results, client::OcrMeta {
                                                        backend: backend.to_string(),
                                                        elapsed_ms: total_ocr_ms,
                                                        preview: false,
                                                    }));
                                                }
                                                eprintln!("[OCR] local-first pipeline: confidence below gate — falling through to LLM chain");
                                            }

                                            let cropper = ocr::text_cropper::TextCropper::new(
                                                s_conf.text_detection_crop_padding,
                                                256,
                                            ).with_pad_percent(0.10);
                                            let crops = cropper.crop(&dyn_image, &boxes);
                                            if !crops.is_empty() {
                                                let dual_region = client::DualOcrClient::new(
                                                    primary_endpoint.clone(),
                                                    primary_model.clone(),
                                                    s_conf.local_fallback_models.clone(),
                                                    free_remote_endpoint.clone(),
                                                    free_remote_model.clone(),
                                                    fallback_endpoint.clone(),
                                                    fallback_model.clone(),
                                                    s_conf.extra_fallback_models.clone(),
                                                    fallback_api_key.clone(),
                                                    s_conf.fallback_max_dimension,
                                                    s_conf.primary_max_dimension,
                                                    s_conf.primary_num_ctx,
                                                    s_conf.local_timeout_secs,
                                                    s_conf.remote_timeout_secs,
                                                    s_conf.paid_remote_timeout_secs,
                                                    per_region_prompt,
                                                );
                                                let mut all_results = Vec::new();
                                                let mut last_meta = None;
                                                for crop in crops {
                                                    match dual_region.call_api(&crop.image).await {
                                                        Ok((mut results, meta)) => {
                                                            // top_xy/bot_xy come from the DBNet
                                                            // box (lens coords), not from the LLM.
                                                            for r in &mut results {
                                                                r.top_xy = Some(format!(
                                                                    "{},{}",
                                                                    crop.source_box.x1,
                                                                    crop.source_box.y1
                                                                ));
                                                                r.bot_xy = Some(format!(
                                                                    "{},{}",
                                                                    crop.source_box.x2,
                                                                    crop.source_box.y2
                                                                ));
                                                            }
                                                            all_results.extend(results);
                                                            last_meta = Some(meta);
                                                        }
                                                        Err(e) => {
                                                            eprintln!(
                                                                "[OCR] per-region call failed: {e}"
                                                            );
                                                        }
                                                    }
                                                }
                                                if !all_results.is_empty() {
                                                    return Ok((all_results, last_meta.unwrap()));
                                                }
                                                // All per-region calls failed — fall through to
                                                // full-image OCR below.
                                            }
                                        }
                                    }
                                }

                                // ── Phase 1 fallback ─────────────────────────────────────────
                                // No detector, no boxes detected, all per-region calls failed,
                                // or force_remote without a detector (lens image → remote).
                                let dual = client::DualOcrClient::new(
                                    primary_endpoint,
                                    primary_model,
                                    s_conf.local_fallback_models.clone(),
                                    free_remote_endpoint,
                                    free_remote_model,
                                    fallback_endpoint,
                                    fallback_model,
                                    s_conf.extra_fallback_models.clone(),
                                    fallback_api_key,
                                    s_conf.fallback_max_dimension,
                                    s_conf.primary_max_dimension,
                                    s_conf.primary_num_ctx,
                                    s_conf.local_timeout_secs,
                                    s_conf.remote_timeout_secs,
                                    s_conf.paid_remote_timeout_secs,
                                    prompt,
                                );
                                // OPT-1: if DBNet found boxes but all per-region calls failed,
                                // crop to the union bbox rather than sending the full lens image.
                                // When no boxes were detected (or force_remote), falls back to
                                // the full dyn_image unchanged.
                                let union_crop = if !force_remote {
                                    ocr::text_detection::compute_union_bbox(&detected_boxes)
                                        .and_then(|union| {
                                            let cropper = ocr::text_cropper::TextCropper::new(
                                                s_conf.text_detection_crop_padding, 0,
                                            ).with_pad_percent(0.10);
                                            cropper.crop(&dyn_image, &[union])
                                                .into_iter().next().map(|c| c.image)
                                        })
                                } else {
                                    None
                                };
                                let fallback_img = union_crop.as_ref().unwrap_or(&dyn_image);
                                if force_remote {
                                    dual.call_api_force_fallback(fallback_img).await
                                } else {
                                    dual.call_api(fallback_img).await
                                }.map_err(|e| e.to_string())
                            }.await;

                            // MeCab check: always compare LLM furigana against MeCab's
                            // dictionary-based readings (logs timing + MATCH/MISMATCH).
                            // Overwrites LLM furigana with MeCab's unless --nomecab_overwrite.
                            if let Ok((ref mut results, _)) = result {
                                furigana::compare_and_maybe_overwrite(results, s_conf.mecab_overwrite);
                            }

                            let _ = tx_clone.send((gen_id, result)).await;
                        });
                        state_main.borrow_mut().in_flight = Some(handle);
                    }
                    Err(_) => {
                        let mut s = state_main.borrow_mut();
                        s.is_loading = false;
                        s.status = "Capture Failed".to_string();
                        move_window(win_x, win_y);
                    }
                }
            }
        }
        glib::ControlFlow::Continue
    });

        // realize() creates the GDK surface (and thus the X11 XID) without
        // mapping the window, so we can set override_redirect before present().
        gtk4::prelude::WidgetExt::realize(&window);
        set_window_state(&window);
        input_shape_clickthrough(&window);
        window.present();
    }); // close connect_activate
    let exit_code = app.run();
    let _ = std::fs::remove_file(PID_FILE);
    exit_code
}
