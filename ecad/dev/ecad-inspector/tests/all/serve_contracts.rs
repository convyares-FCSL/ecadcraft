//! `ecadinspect serve` (brief 029, design 029 §5): the token, `Host` and `Origin` refusals, one
//! client at a time, size limits, malformed HTTP and frames, `open` by binary frame, export,
//! 127.0.0.1 only, replies matched by `id`.

use std::io::{Read, Write};
use std::net::{Ipv4Addr, TcpStream};
use std::time::Duration;

use ecad_inspector::serve::Server;
use serde_json::{Value, json};
use tungstenite::client::IntoClientRequest;
use tungstenite::{Message, WebSocket};

use crate::common::*;

/// A running server on a free port, serving `file`: its port and token.
fn start(file: Option<&std::path::Path>) -> (u16, String) {
    let s = Server::bind(file, 0).unwrap();
    assert_eq!(s.local_addr().unwrap().ip(), Ipv4Addr::LOCALHOST, "it listens on 127.0.0.1 only");
    let out = (s.port(), s.token().to_string());
    std::thread::spawn(move || s.run());
    out
}

/// A raw HTTP exchange: the status code and the response.
fn http(port: u16, request: &str) -> (u16, String) {
    let mut c = TcpStream::connect(("127.0.0.1", port)).unwrap();
    c.set_read_timeout(Some(Duration::from_secs(20))).unwrap();
    c.write_all(request.as_bytes()).unwrap();
    let mut out = Vec::new();
    let _ = c.read_to_end(&mut out);
    let text = String::from_utf8_lossy(&out).into_owned();
    let code = text.split(' ').nth(1).and_then(|c| c.parse().ok()).unwrap_or(0);
    (code, text)
}

fn get(port: u16, target: &str, host: &str) -> (u16, String) {
    http(port, &format!("GET {target} HTTP/1.1\r\nHost: {host}\r\n\r\n"))
}

type Ws = WebSocket<TcpStream>;

/// Connect the WebSocket with `token`, `host` and `origin`.
fn connect(port: u16, token: &str, host: Option<&str>, origin: Option<&str>) -> Result<Ws, tungstenite::Error> {
    let mut req = format!("ws://127.0.0.1:{port}/ws?t={token}").into_client_request()?;
    if let Some(h) = host {
        req.headers_mut().insert("Host", h.parse().unwrap());
    }
    if let Some(o) = origin {
        req.headers_mut().insert("Origin", o.parse().unwrap());
    }
    let stream = TcpStream::connect(("127.0.0.1", port)).unwrap();
    stream.set_read_timeout(Some(Duration::from_secs(60))).unwrap();
    tungstenite::client(req, stream).map(|(ws, _)| ws).map_err(|e| match e {
        tungstenite::HandshakeError::Failure(e) => e,
        tungstenite::HandshakeError::Interrupted(_) => tungstenite::Error::ConnectionClosed,
    })
}

fn send(ws: &mut Ws, v: Value) -> Value {
    ws.send(Message::text(v.to_string())).unwrap();
    read(ws)
}

fn read(ws: &mut Ws) -> Value {
    loop {
        match ws.read().unwrap() {
            Message::Text(t) => return serde_json::from_str(t.as_str()).unwrap(),
            Message::Close(c) => panic!("closed: {c:?}"),
            _ => {}
        }
    }
}

#[test]
fn the_page_needs_the_token_and_a_local_host() {
    let dir = TempDir::new("serve-page");
    let (port, token) = start(Some(&rich(&dir, "Main")));
    let local = format!("127.0.0.1:{port}");
    let (code, page) = get(port, &format!("/?t={token}"), &local);
    assert_eq!(code, 200, "{page}");
    assert!(page.contains("ECAD Inspector") && page.contains("Content-Security-Policy: default-src 'none'; script-src 'nonce-"), "{page}");
    assert!(!page.to_ascii_lowercase().contains("access-control-allow"), "no CORS headers");
    assert_eq!(get(port, &format!("/?t={token}"), &format!("localhost:{port}")).0, 200);
    // (target, host, code)
    let rows: Vec<(String, String, u16)> = vec![
        ("/".into(), local.clone(), 403),
        ("/?t=wrong".into(), local.clone(), 403),
        (format!("/?t={token}"), format!("evil.example:{port}"), 403),
        (format!("/?t={token}"), "127.0.0.1:1".into(), 403),
        (format!("/other?t={token}"), local.clone(), 404),
        (format!("/ws?t={token}"), local.clone(), 400),
    ];
    for (target, host, code) in rows {
        assert_eq!(get(port, &target, &host).0, code, "{target} with Host {host}");
    }
    // Malformed HTTP, and a head over 16 KiB.
    assert_eq!(http(port, "NONSENSE\r\n\r\n").0, 400);
    assert_eq!(http(port, &format!("POST /?t={token} HTTP/1.1\r\nHost: {local}\r\n\r\n")).0, 404);
    let big = format!("GET /?t={token} HTTP/1.1\r\nHost: {local}\r\nX-Pad: {}\r\n\r\n", "a".repeat(20_000));
    assert_eq!(http(port, &big).0, 431);
}

#[test]
fn the_websocket_needs_the_token_a_local_host_and_a_local_origin() {
    let dir = TempDir::new("serve-ws");
    let (port, token) = start(Some(&rich(&dir, "Main")));
    assert!(connect(port, "wrong", None, None).is_err(), "without the token");
    assert!(connect(port, &token, Some(&format!("evil.example:{port}")), None).is_err(), "a foreign Host");
    assert!(connect(port, &token, None, Some("http://evil.example")).is_err(), "a foreign Origin");
    assert!(connect(port, &token, None, Some(&format!("http://127.0.0.1:{}", port + 1))).is_err(), "another local port");
    for origin in [format!("http://127.0.0.1:{port}"), format!("http://localhost:{port}")] {
        let mut ws = connect(port, &token, None, Some(&origin)).unwrap();
        let hello = send(&mut ws, json!({"v": 1, "id": "h", "op": "hello"}));
        assert_eq!(hello["ok"]["source"]["name"], "rich.v2.ecad");
        ws.close(None).unwrap();
        while ws.read().is_ok() {}
    }
}

#[test]
fn one_page_at_a_time_and_replies_match_their_ids() {
    let dir = TempDir::new("serve-one");
    let (port, token) = start(Some(&rich(&dir, "Main")));
    let mut first = connect(port, &token, None, None).unwrap();
    // Requests answered in order, each echoing its id.
    for (id, op) in [("a", "summary"), ("b", "devices"), ("7", "verify")] {
        let r = send(&mut first, json!({"v": 1, "id": id, "op": op}));
        assert_eq!(r["id"], id, "{r}");
    }
    let mut second = connect(port, &token, None, None).unwrap();
    match second.read() {
        Ok(Message::Close(Some(c))) => {
            assert_eq!(u16::from(c.code), 1013);
            assert!(c.reason.as_str().contains("another inspector page is connected"), "{c:?}");
        }
        other => panic!("the second page is refused: {other:?}"),
    }
    // The first still works; once it closes, a new page may connect.
    assert!(send(&mut first, json!({"op": "hello"}))["ok"].is_object());
    first.close(None).unwrap();
    while first.read().is_ok() {}
    std::thread::sleep(Duration::from_millis(100));
    let mut third = connect(port, &token, None, None).unwrap();
    assert!(send(&mut third, json!({"op": "hello"}))["ok"].is_object());
}

#[test]
fn a_dropped_file_replaces_the_source_and_limits_hold() {
    let dir = TempDir::new("serve-open");
    let (port, token) = start(None);
    let mut ws = connect(port, &token, None, None).unwrap();
    assert_eq!(send(&mut ws, json!({"op": "summary"}))["error"]["what"], "not-open");
    let bytes = std::fs::read(rich(&dir, "Dropped")).unwrap();
    ws.send(Message::text(json!({"v": 1, "id": "o", "op": "open", "name": "C:\\elsewhere\\drop.ecad", "size": bytes.len()}).to_string())).unwrap();
    ws.send(Message::binary(bytes.clone())).unwrap();
    let opened = read(&mut ws);
    assert_eq!(opened["id"], "o");
    assert_eq!(opened["ok"]["source"]["kind"], "bytes");
    assert_eq!(opened["ok"]["source"]["name"], "drop.ecad", "a file name only, never a path");
    let pages = send(&mut ws, json!({"op": "pages"}));
    assert_eq!(pages["ok"]["rows"][0]["title"], "Dropped");
    // Too large to drop (row 12), a size that does not match, bytes with no open, a request
    // over 1 MiB, reload of a dropped file.
    let big = send(&mut ws, json!({"op": "open", "name": "x.ecad", "size": (1u64 << 30) + 1}));
    assert_eq!(big["error"]["what"], "too-large");
    assert!(big["error"]["way_out"].as_str().unwrap().contains("ecadinspect serve <file>"));
    ws.send(Message::text(json!({"op": "open", "name": "x.ecad", "size": 10}).to_string())).unwrap();
    ws.send(Message::binary(vec![1u8, 2, 3])).unwrap();
    assert_eq!(read(&mut ws)["error"]["what"], "bad-request");
    ws.send(Message::binary(vec![1u8, 2, 3])).unwrap();
    assert_eq!(read(&mut ws)["error"]["what"], "bad-request");
    let huge = json!({"op": "hello", "pad": "p".repeat(1_100_000)});
    assert_eq!(send(&mut ws, huge)["error"]["what"], "too-large");
    assert_eq!(send(&mut ws, json!({"op": "reload"}))["error"]["what"], "bad-request");
    // Malformed frames: not JSON.
    assert_eq!(send(&mut ws, json!("just a string"))["error"]["what"], "bad-request");
    ws.send(Message::text("{not json")).unwrap();
    assert_eq!(read(&mut ws)["error"]["what"], "bad-request");
    // The dropped source still answers; export comes back as text.
    let export = send(&mut ws, json!({"op": "export_jsonl"}));
    assert!(export["ok"]["text"].as_str().unwrap().lines().count() > 20);
}

#[test]
fn reload_reopens_the_file_named_on_the_command_line() {
    let dir = TempDir::new("serve-reload");
    let path = rich(&dir, "Main");
    let (port, token) = start(Some(&path));
    let mut ws = connect(port, &token, None, None).unwrap();
    let mut bytes = std::fs::read(&path).unwrap();
    bytes.extend_from_slice(&[0u8; 4096]);
    std::fs::write(&path, &bytes).unwrap();
    assert_eq!(send(&mut ws, json!({"op": "devices"}))["error"]["what"], "changed");
    let r = send(&mut ws, json!({"id": "r", "op": "reload"}));
    assert_eq!(r["id"], "r");
    assert!(r["ok"]["source"].is_object(), "{r}");
    assert!(send(&mut ws, json!({"op": "hello"}))["ok"].is_object());
}

#[test]
fn the_banner_prints_the_address_with_the_token() {
    let dir = TempDir::new("serve-banner");
    let s = Server::bind(Some(&rich(&dir, "Main")), 0).unwrap();
    let b = s.banner();
    assert!(b.contains("serving rich.v2.ecad (") && b.contains("read-only"), "{b}");
    assert!(b.contains(&format!("open http://127.0.0.1:{}/?t={}", s.port(), s.token())), "{b}");
    assert_eq!(s.token().len(), 24, "96 bits as hex");
}
