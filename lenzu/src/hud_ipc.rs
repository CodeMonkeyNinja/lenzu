//! Dual-protocol IPC to lenzu_server:
//!   1. gRPC (primary) — typed, connection-aware, persistent HTTP/2
//!   2. UDP  (fallback) — fire-and-forget JSON datagrams
//!
//! Each public function tries gRPC first.  If the server isn't reachable
//! the call silently falls back to UDP so the HUD keeps working.

use std::time::Duration;
use tonic::transport::{Channel, Endpoint};
use tonic::IntoRequest;

tonic::include_proto!("lenzu_hud");

use std::sync::Mutex;

/// Shared gRPC channel — initialised once after the server spawns.
static GRPC_CHANNEL: Mutex<Option<Channel>> = Mutex::new(None);

/// Initialise the shared gRPC channel (idempotent).
pub async fn init_channel(grpc_port: u16) {
    let addr = format!("http://127.0.0.1:{}", grpc_port);
    if let Ok(endpoint) = Endpoint::from_shared(addr)
        .map(|e| e.connect_timeout(Duration::from_millis(500)))
    {
        if let Ok(ch) = endpoint.connect().await {
            *GRPC_CHANNEL.lock().unwrap() = Some(ch);
            eprintln!("[gRPC] channel established on port {grpc_port}");
        }
    }
}

/// Reset the gRPC channel (used in tests).
#[cfg(test)]
fn reset_channel() {
    *GRPC_CHANNEL.lock().unwrap() = None;
}

fn with_client<F, R>(f: F) -> Option<R>
where
    F: FnOnce(lenzu_hud_client::LenzuHudClient<Channel>) -> R,
{
    GRPC_CHANNEL
        .lock()
        .unwrap()
        .as_ref()
        .map(|ch| f(lenzu_hud_client::LenzuHudClient::new(ch.clone())))
}

/// Send HUD text via gRPC, falling back to UDP.
pub fn send_text(text: &str, udp_port: u16) {
    if let Some(result) = with_client(|mut client| {
        let msg = TextMessage { text: text.to_string() };
        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap();
        rt.block_on(async { client.send_text(msg.into_request()).await })
    }) {
        match result {
            Ok(_) => return,
            Err(status) => eprintln!("[gRPC] send_text failed, falling back to UDP: {status}"),
        }
    }
    send_text_udp(text, udp_port);
}

/// Send text via UDP (legacy fallback).
pub(crate) fn send_text_udp(text: &str, port: u16) {
    if let Ok(socket) = std::net::UdpSocket::bind("127.0.0.1:0") {
        let addr = format!("127.0.0.1:{}", port);
        let message = serde_json::json!({
            "type": "message",
            "text": text,
        });
        let _ = socket.send_to(message.to_string().as_bytes(), &addr);
    }
}

/// Move HUD window via gRPC, falling back to UDP.
pub fn move_window(pos: &str, udp_port: u16) {
    if let Some(result) = with_client(|mut client| {
        let msg = WindowPosition { pos: pos.to_string() };
        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap();
        rt.block_on(async { client.move_window(msg.into_request()).await })
    }) {
        match result {
            Ok(_) => return,
            Err(status) => eprintln!("[gRPC] move_window failed, falling back to UDP: {status}"),
        }
    }
    move_window_udp(pos, udp_port);
}

/// Move window via UDP (legacy fallback).
pub(crate) fn move_window_udp(pos: &str, port: u16) {
    if let Ok(socket) = std::net::UdpSocket::bind("127.0.0.1:0") {
        let addr = format!("127.0.0.1:{port}");
        let msg = serde_json::json!({"type": "position", "pos": pos});
        let _ = socket.send_to(msg.to_string().as_bytes(), &addr);
    }
}

/// Send shutdown command via gRPC, falling back to UDP.
pub fn send_shutdown(udp_port: u16) {
    if let Some(result) = with_client(|mut client| {
        let msg = Empty {};
        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap();
        rt.block_on(async { client.shutdown(msg.into_request()).await })
    }) {
        match result {
            Ok(_) => return,
            Err(status) => eprintln!("[gRPC] shutdown failed, falling back to UDP: {status}"),
        }
    }
    send_shutdown_udp(udp_port);
}

/// Send shutdown via UDP (legacy fallback).
pub(crate) fn send_shutdown_udp(port: u16) {
    if let Ok(socket) = std::net::UdpSocket::bind("127.0.0.1:0") {
        let addr = format!("127.0.0.1:{}", port);
        let message = serde_json::json!({"type": "shutdown"});
        let _ = socket.send_to(message.to_string().as_bytes(), &addr);
        let _ = std::thread::sleep(Duration::from_millis(100));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::Value;

    fn bind_test_server() -> (std::net::UdpSocket, u16) {
        let sock = std::net::UdpSocket::bind("127.0.0.1:0").unwrap();
        let port = sock.local_addr().unwrap().port();
        (sock, port)
    }

    fn recv_json(sock: &std::net::UdpSocket) -> Value {
        let mut buf = [0u8; 4096];
        let n = sock.recv(&mut buf).unwrap();
        serde_json::from_slice(&buf[..n]).unwrap()
    }

    #[test]
    fn with_client_returns_none_when_not_initialized() {
        reset_channel();
        let result = with_client(|_| 42);
        assert!(result.is_none());
    }

    #[test]
    fn send_text_via_udp_emits_json_message() {
        let (sock, port) = bind_test_server();
        send_text_udp("hello 世界", port);
        let json = recv_json(&sock);
        assert_eq!(json["type"], "message");
        assert_eq!(json["text"], "hello 世界");
    }

    #[test]
    fn move_window_via_udp_emits_json_position() {
        let (sock, port) = bind_test_server();
        move_window_udp("top", port);
        let json = recv_json(&sock);
        assert_eq!(json["type"], "position");
        assert_eq!(json["pos"], "top");
    }

    #[test]
    fn move_window_via_udp_bottom() {
        let (sock, port) = bind_test_server();
        move_window_udp("bottom", port);
        let json = recv_json(&sock);
        assert_eq!(json["type"], "position");
        assert_eq!(json["pos"], "bottom");
    }

    #[test]
    fn shutdown_via_udp_emits_json_shutdown() {
        let (sock, port) = bind_test_server();
        send_shutdown_udp(port);
        let json = recv_json(&sock);
        assert_eq!(json["type"], "shutdown");
    }

    #[test]
    fn public_send_text_falls_back_to_udp_when_no_grpc_channel() {
        reset_channel();
        let (sock, port) = bind_test_server();
        send_text("udp fallback test", port);
        let json = recv_json(&sock);
        assert_eq!(json["type"], "message");
        assert_eq!(json["text"], "udp fallback test");
    }

    #[test]
    fn public_move_window_falls_back_to_udp_when_no_grpc_channel() {
        reset_channel();
        let (sock, port) = bind_test_server();
        move_window("top", port);
        let json = recv_json(&sock);
        assert_eq!(json["type"], "position");
        assert_eq!(json["pos"], "top");
    }

    #[test]
    fn init_channel_unreachable_port_does_not_panic() {
        reset_channel();
        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap();
        rt.block_on(init_channel(1)); // port 1 is privileged — connection refused
        assert!(GRPC_CHANNEL.lock().unwrap().is_none());
    }
}
