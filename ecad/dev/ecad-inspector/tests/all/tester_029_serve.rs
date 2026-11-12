//! Tester contracts for `ecadinspect serve` (brief 029 criteria 7, 8, 10, 11; design §2.1, §5,
//! §6), written from the brief and the design: the address, loopback only, every refusal variant
//! of the token, `Host` and `Origin`, one client at a time, a dropped file replacing the source,
//! the envelope (`v`, `id`), hostile HTTP and frames that must never stop the server, and hostile
//! text returned as plain JSON.

use std::io::{Read, Write};
use std::net::{IpAddr, Ipv4Addr, SocketAddr, TcpStream};
use std::time::Duration;

use ecad_inspector::serve::Server;
use serde_json::{Value, json};
use tungstenite::client::IntoClientRequest;
use tungstenite::protocol::frame::coding::CloseCode;
use tungstenite::{Message, WebSocket};

use crate::common::*;

fn start(file: Option<&std::path::Path>) -> (u16, String, String) {
    let s = Server::bind(file, 0).unwrap();
    let out = (s.port(), s.token().to_string(), s.address());
    std::thread::spawn(move || s.run());
    out
}

fn http_raw(port: u16, bytes: &[u8]) -> (u16, String) {
    let mut c = TcpStream::connect(("127.0.0.1", port)).unwrap();
    c.set_read_timeout(Some(Duration::from_secs(20))).unwrap();
    let _ = c.write_all(bytes);
    let mut out = Vec::new();
    let _ = c.read_to_end(&mut out);
    let text = String::from_utf8_lossy(&out).into_owned();
    (text.split(' ').nth(1).and_then(|c| c.parse().ok()).unwrap_or(0), text)
}

fn get(port: u16, target: &str, host: &str) -> u16 {
    http_raw(port, format!("GET {target} HTTP/1.1\r\nHost: {host}\r\n\r\n").as_bytes()).0
}

type Ws = WebSocket<TcpStream>;

fn connect(port: u16, token: &str, host: Option<&str>, origin: Option<&str>) -> Result<Ws, String> {
    let mut req = format!("ws://127.0.0.1:{port}/ws?t={token}").into_client_request().map_err(|e| e.to_string())?;
    if let Some(h) = host {
        req.headers_mut().insert("Host", h.parse().unwrap());
    }
    if let Some(o) = origin {
        req.headers_mut().insert("Origin", o.parse().unwrap());
    }
    let stream = TcpStream::connect(("127.0.0.1", port)).unwrap();
    stream.set_read_timeout(Some(Duration::from_secs(60))).unwrap();
    tungstenite::client(req, stream).map(|(ws, _)| ws).map_err(|e| match e {
        tungstenite::HandshakeError::Failure(tungstenite::Error::Http(r)) => format!("http {}", r.status().as_u16()),
        tungstenite::HandshakeError::Failure(e) => e.to_string(),
        tungstenite::HandshakeError::Interrupted(_) => "interrupted".into(),
    })
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

fn ask_text(ws: &mut Ws, text: &str) -> Value {
    ws.send(Message::text(text.to_string())).unwrap();
    read(ws)
}

fn ask(ws: &mut Ws, v: Value) -> Value {
    ask_text(ws, &v.to_string())
}

fn what(r: &Value) -> &str {
    r["error"]["what"].as_str().unwrap_or("")
}

#[test]
fn the_address_is_loopback_with_a_token_and_nothing_answers_on_other_addresses() {
    let dir = TempDir::new("t029-addr");
    let f = rich(&dir, "Title");
    let s = Server::bind(Some(&f), 0).unwrap();
    let (port, token, addr) = (s.port(), s.token().to_string(), s.address());
    assert_eq!(addr, format!("http://127.0.0.1:{port}/?t={token}"));
    assert!(token.len() >= 24 && token.chars().all(|c| c.is_ascii_hexdigit()), "96 bits as hex: {token}");
    assert!(s.banner().contains(&addr) && s.banner().contains("read-only"), "{}", s.banner());
    let s2 = Server::bind(Some(&f), 0).unwrap();
    assert_ne!(s2.token(), token, "a token per run");
    std::thread::spawn(move || s.run());
    let short = Duration::from_millis(800);
    // Another loopback address, IPv6 loopback, and this machine's own outward address, if any.
    let mut others: Vec<IpAddr> = vec![IpAddr::V4(Ipv4Addr::new(127, 0, 0, 2)), "::1".parse().unwrap()];
    if let Ok(u) = std::net::UdpSocket::bind("0.0.0.0:0")
        && u.connect("192.0.2.1:9").is_ok()
        && let Ok(a) = u.local_addr()
        && !a.ip().is_loopback()
        && !a.ip().is_unspecified()
    {
        others.push(a.ip());
    }
    for ip in others {
        let r = TcpStream::connect_timeout(&SocketAddr::new(ip, port), short);
        assert!(r.is_err(), "nothing answers on {ip}:{port}");
    }
    assert_eq!(get(port, &format!("/?t={token}"), &format!("127.0.0.1:{port}")), 200);
}

#[test]
fn every_variant_of_a_wrong_token_host_or_origin_is_refused() {
    let dir = TempDir::new("t029-refuse");
    let f = rich(&dir, "Title");
    let (port, token, _) = start(Some(&f));
    let good = format!("127.0.0.1:{port}");
    let prefix = &token[..token.len() - 1];
    // GET /: token variants.
    for target in ["/".to_string(), "/?t=".into(), format!("/?t={prefix}"), format!("/?t={token}0"), format!("/?T={token}"), format!("/?x={token}")] {
        assert_eq!(get(port, &target, &good), 403, "{target}");
    }
    // GET /: Host variants.
    for host in [
        "evil.example".to_string(),
        format!("evil.example:{port}"),
        "127.0.0.1".into(),
        format!("127.0.0.1:{}", port.wrapping_add(1)),
        format!("localhost.evil.example:{port}"),
        format!("127.0.0.1.nip.io:{port}"),
        format!("[::1]:{port}"),
        format!("0.0.0.0:{port}"),
    ] {
        assert_eq!(get(port, &format!("/?t={token}"), &host), 403, "Host {host}");
    }
    assert_eq!(http_raw(port, format!("GET /?t={token} HTTP/1.1\r\n\r\n").as_bytes()).0, 403, "no Host");
    assert_eq!(get(port, &format!("/?t={token}"), &format!("LOCALHOST:{port}")), 200, "Host is case-insensitive");
    // Upgrade: token, Host and Origin variants.
    for (tok, host, origin) in [
        (prefix.to_string(), None, None),
        (String::new(), None, None),
        (token.clone(), Some(format!("evil.example:{port}")), None),
        (token.clone(), None, Some("null".to_string())),
        (token.clone(), None, Some("http://evil.example".to_string())),
        (token.clone(), None, Some(format!("http://127.0.0.1:{port}.evil.example"))),
        (token.clone(), None, Some(format!("https://127.0.0.1:{port}"))),
        (token.clone(), None, Some(format!("http://127.0.0.1:{}", port.wrapping_add(1)))),
        (token.clone(), None, Some(format!("http://evil.example/?http://127.0.0.1:{port}"))),
        (token.clone(), None, Some("file://".to_string())),
    ] {
        let r = connect(port, &tok, host.as_deref(), origin.as_deref());
        assert!(r.is_err(), "refused: token {tok:?}, host {host:?}, origin {origin:?}");
    }
    for origin in [format!("http://127.0.0.1:{port}"), format!("http://localhost:{port}")] {
        let mut ws = connect(port, &token, None, Some(&origin)).unwrap_or_else(|e| panic!("{origin}: {e}"));
        assert!(ask(&mut ws, json!({"v":1,"id":"h","op":"hello"}))["ok"].is_object());
        let _ = ws.close(None);
        while ws.read().is_ok() {}
        // The slot frees when the page goes.
        std::thread::sleep(Duration::from_millis(200));
    }
}

#[test]
fn a_second_page_gets_1013_the_first_keeps_working_and_a_new_page_can_connect_after_it_closes() {
    let dir = TempDir::new("t029-one");
    let f = rich(&dir, "Title");
    let (port, token, _) = start(Some(&f));
    let mut a = connect(port, &token, None, None).unwrap();
    assert!(ask(&mut a, json!({"v":1,"id":1,"op":"hello"}))["ok"].is_object());
    let mut b = connect(port, &token, None, None).unwrap();
    let closed = loop {
        match b.read() {
            Ok(Message::Close(c)) => break c,
            Ok(_) => continue,
            Err(_) => break None,
        }
    };
    let c = closed.expect("a close frame");
    assert_eq!(c.code, CloseCode::Again);
    assert!(c.reason.contains("another inspector page"), "{}", c.reason);
    assert_eq!(ask(&mut a, json!({"v":1,"id":"still","op":"summary"}))["id"], "still", "the first page still answers");
    a.close(None).unwrap();
    while a.read().is_ok() {}
    // The slot frees when the first page goes; allow the server a moment to notice.
    let mut ok = false;
    for _ in 0..50 {
        if let Ok(mut c) = connect(port, &token, None, None)
            && let Ok(Message::Text(t)) = c.send(Message::text(json!({"v":1,"id":"n","op":"hello"}).to_string())).and_then(|_| c.read())
        {
            ok = serde_json::from_str::<Value>(t.as_str()).unwrap()["ok"].is_object();
        }
        if ok {
            break;
        }
        std::thread::sleep(Duration::from_millis(100));
    }
    assert!(ok, "a new page connects after the first closed");
}

#[test]
fn a_dropped_file_replaces_the_open_one_and_its_verify_is_the_new_files() {
    let dir = TempDir::new("t029-drop");
    let a = rich(&dir, "First file");
    let b = upgraded(&dir, &history());
    let a_bytes = std::fs::read(&a).unwrap();
    let a_time = std::fs::metadata(&a).unwrap().modified().unwrap();
    let (port, token, _) = start(Some(&a));
    let mut ws = connect(port, &token, None, None).unwrap();
    let before = ask(&mut ws, json!({"v":1,"id":1,"op":"hello"}));
    assert_eq!(before["ok"]["source"]["name"], "rich.v2.ecad", "{before}");
    let pages_a = ask(&mut ws, json!({"v":1,"id":2,"op":"pages"}));
    assert!(pages_a.to_string().contains("First file"), "{pages_a}");
    let bytes = std::fs::read(&b).unwrap();
    ws.send(Message::text(json!({"v":1,"id":"drop","op":"open","name":"C:\\Users\\x\\secret\\dropped.ecad","size":bytes.len()}).to_string()))
        .unwrap();
    ws.send(Message::binary(bytes.clone())).unwrap();
    let sum = read(&mut ws);
    assert_eq!(sum["id"], "drop", "{sum}");
    assert!(sum["ok"].is_object(), "{sum}");
    let after = ask(&mut ws, json!({"v":1,"id":3,"op":"hello"}));
    assert_eq!(after["ok"]["source"]["name"], "dropped.ecad", "a name only, never the path: {after}");
    assert_eq!(after["ok"]["source"]["size"], bytes.len());
    let pages_b = ask(&mut ws, json!({"v":1,"id":4,"op":"pages"}));
    assert!(!pages_b.to_string().contains("First file"), "{pages_b}");
    // The Verify answered is the new file's: wait for it to finish.
    let mut v = Value::Null;
    for _ in 0..200 {
        v = ask(&mut ws, json!({"v":1,"id":5,"op":"verify"}));
        if v["ok"]["state"] != "running" {
            break;
        }
        std::thread::sleep(Duration::from_millis(50));
    }
    assert_eq!(v["ok"]["state"], "done", "{v}");
    let branches = ask(&mut ws, json!({"v":1,"id":6,"op":"branches"}));
    assert!(!branches.to_string().contains("Option B"), "the dropped file has no Option B branch: {branches}");
    // `reload` goes back to the file named on the command line.
    let r = ask(&mut ws, json!({"v":1,"id":"re","op":"reload"}));
    assert!(r["ok"].is_object(), "{r}");
    assert_eq!(ask(&mut ws, json!({"v":1,"id":7,"op":"hello"}))["ok"]["source"]["name"], "rich.v2.ecad");
    let _ = ws.close(None);
    assert_eq!(std::fs::read(&a).unwrap(), a_bytes, "never written");
    assert_eq!(std::fs::metadata(&a).unwrap().modified().unwrap(), a_time);
}

#[test]
fn the_envelope_echoes_ids_and_checks_the_version() {
    let dir = TempDir::new("t029-env");
    let f = rich(&dir, "T");
    let (port, token, _) = start(Some(&f));
    let mut ws = connect(port, &token, None, None).unwrap();
    for id in [json!("a-7"), json!(7), json!(-3), json!(1.5), json!("é∑😀"), json!("x".repeat(64))] {
        let r = ask(&mut ws, json!({"v":1,"id":id,"op":"summary"}));
        assert_eq!(r["id"], id, "{r}");
        assert_eq!(r["v"], 1);
        assert!(r["ok"].is_object(), "{r}");
    }
    let r = ask(&mut ws, json!({"v":1,"op":"summary"}));
    assert!(r["id"].is_null() && r["ok"].is_object(), "{r}");
    for (req, want) in [
        (json!({"v":1,"id":"x".repeat(65),"op":"hello"}), "bad-request"),
        (json!({"v":1,"id":{"a":1},"op":"hello"}), "bad-request"),
        (json!({"v":1,"id":[1],"op":"hello"}), "bad-request"),
        (json!({"v":2,"id":"p","op":"hello"}), "protocol"),
        (json!({"v":"1","id":"p","op":"hello"}), "bad-request"),
        (json!({"v":-1,"id":"p","op":"hello"}), "bad-request"),
        (json!({"v":1,"id":"p"}), "bad-request"),
        (json!({"v":1,"id":"p","op":7}), "bad-request"),
        (json!({"v":1,"id":"p","op":"subscribe","topic":"commits"}), "unknown-op"),
        (json!({"v":1,"id":"p","op":"unsubscribe"}), "unknown-op"),
        (json!({"v":1,"id":"p","op":"write"}), "unknown-op"),
        (json!({"v":1,"id":"p","op":"devices","q":"q".repeat(257)}), "bad-request"),
    ] {
        let r = ask(&mut ws, req.clone());
        assert_eq!(what(&r), want, "{req} -> {r}");
        assert!(r["error"]["why"].as_str().is_some_and(|s| !s.is_empty()) && r["error"]["way_out"].as_str().is_some_and(|s| !s.is_empty()), "{r}");
    }
    let r = ask(&mut ws, json!({"v":2,"id":"p","op":"hello"}));
    assert_eq!(r["id"], "p", "the protocol error keeps the id: {r}");
    // Unknown fields are ignored.
    let r = ask(&mut ws, json!({"v":1,"id":"u","op":"pages","future":{"x":1}}));
    assert!(r["ok"].is_object(), "{r}");
    // Paging at the edges.
    for (from, count) in [(u64::MAX, 10), (0, u64::MAX), (u64::MAX, u64::MAX), (0, 0)] {
        let r = ask(&mut ws, json!({"v":1,"id":"pg","op":"devices","from":from,"count":count}));
        assert!(r["ok"].is_object(), "from {from} count {count}: {r}");
        assert!(r["ok"]["rows"].as_array().map_or(0, Vec::len) <= 1000);
    }
    for (from, count) in [(json!(-1), json!(5)), (json!("a"), json!(5)), (json!(0), json!(1e300)), (json!(0), json!(-5))] {
        let r = ask(&mut ws, json!({"v":1,"id":"pg","op":"devices","from":from,"count":count}));
        assert!(r["ok"].is_object() || r["error"].is_object(), "{r}");
    }
}

#[test]
fn hostile_frames_never_stop_the_server() {
    let dir = TempDir::new("t029-hostile");
    let f = rich(&dir, "T");
    let (port, token, _) = start(Some(&f));
    let mut ws = connect(port, &token, None, None).unwrap();
    let nested = "[".repeat(100_000);
    for text in [
        "".to_string(),
        "{".into(),
        "null".into(),
        "[]".into(),
        "\"op\"".into(),
        "{\"op\":\"hello\"".into(),
        nested.clone(),
        format!("{{\"v\":1,\"id\":\"n\",\"op\":\"pages\",\"x\":{nested}}}"),
        "{\"v\":1e400,\"op\":\"hello\"}".into(),
        "{\"v\":1,\"op\":\"hello\",\"op\":\"pages\"}".into(),
    ] {
        let r = ask_text(&mut ws, &text);
        assert!(r["error"].is_object() || r["ok"].is_object(), "{}: {r}", &text[..text.len().min(40)]);
    }
    // Over 1 MiB of text: refused, the connection stays.
    let big = format!("{{\"v\":1,\"id\":\"big\",\"op\":\"hello\",\"pad\":\"{}\"}}", "a".repeat(1024 * 1024));
    let r = ask_text(&mut ws, &big);
    assert_eq!(what(&r), "too-large", "{r}");
    // A binary frame with no `open` before it, a size mismatch, an oversized announcement, no name.
    ws.send(Message::binary(vec![1u8, 2, 3])).unwrap();
    assert_eq!(what(&read(&mut ws)), "bad-request");
    ws.send(Message::text(json!({"v":1,"id":"o","op":"open","name":"x.ecad","size":10}).to_string())).unwrap();
    ws.send(Message::binary(vec![0u8; 3])).unwrap();
    let r = read(&mut ws);
    assert_eq!((what(&r), r["id"].clone()), ("bad-request", json!("o")), "{r}");
    let r = ask(&mut ws, json!({"v":1,"id":"o2","op":"open","name":"x.ecad","size":(1u64 << 30) + 1}));
    assert_eq!(what(&r), "too-large", "{r}");
    for bad in [
        json!({"v":1,"id":"o3","op":"open","size":3}),
        json!({"v":1,"id":"o4","op":"open","name":"","size":3}),
        json!({"v":1,"id":"o5","op":"open","name":"n".repeat(300),"size":3}),
        json!({"v":1,"id":"o6","op":"open","name":"x.ecad","size":-3}),
    ] {
        let r = ask(&mut ws, bad.clone());
        assert_eq!(what(&r), "bad-request", "{bad} -> {r}");
    }
    // Junk dropped as a file: a message, never a panic; then the old file comes back on reload.
    for junk in [Vec::new(), b"{\"crc\":1}\n".to_vec(), vec![0x50, 0x4B, 3, 4, 0, 0], (0..4096u32).map(|i| (i * 31 % 251) as u8).collect()] {
        ws.send(Message::text(json!({"v":1,"id":"j","op":"open","name":"junk.ecad","size":junk.len()}).to_string())).unwrap();
        ws.send(Message::binary(junk)).unwrap();
        let r = read(&mut ws);
        assert_eq!(r["id"], "j", "{r}");
        for op in [
            "summary",
            "project",
            "pages",
            "devices",
            "nets",
            "wires",
            "cables",
            "branches",
            "log",
            "versions",
            "tables",
            "check_chain",
            "invariants",
            "verify",
        ] {
            let r = ask(&mut ws, json!({"v":1,"id":op,"op":op}));
            assert!(r["ok"].is_object() || r["error"]["what"].is_string(), "{op}: {r}");
        }
    }
    let r = ask(&mut ws, json!({"v":1,"id":"re","op":"reload"}));
    assert!(r["ok"].is_object(), "{r}");
    // Ping frames are not requests.
    ws.send(Message::Ping(vec![1, 2].into())).unwrap();
    let r = ask(&mut ws, json!({"v":1,"id":"after","op":"hello"}));
    assert_eq!(r["id"], "after", "{r}");
    let _ = ws.close(None);
}

#[test]
fn hostile_http_never_stops_the_server() {
    let dir = TempDir::new("t029-http");
    let f = rich(&dir, "T");
    let (port, token, _) = start(Some(&f));
    let host = format!("127.0.0.1:{port}");
    let cases: Vec<(Vec<u8>, Option<u16>)> = vec![
        (vec![0u8, 0xFF, 0xFE, b'\r', b'\n', b'\r', b'\n'], Some(400)),
        (b"\r\n\r\n".to_vec(), Some(400)),
        (format!("GET /?t={token} HTTP/2.0\r\nHost: {host}\r\n\r\n").into_bytes(), Some(400)),
        (format!("GET  /?t={token}  HTTP/1.1\r\nHost: {host}\r\n\r\n").into_bytes(), Some(400)),
        (format!("POST /?t={token} HTTP/1.1\r\nHost: {host}\r\nContent-Length: 3\r\n\r\nabc").into_bytes(), None),
        (format!("GET /../../etc/passwd?t={token} HTTP/1.1\r\nHost: {host}\r\n\r\n").into_bytes(), Some(404)),
        (format!("GET /index.html?t={token} HTTP/1.1\r\nHost: {host}\r\n\r\n").into_bytes(), Some(404)),
        (format!("GET /ws?t={token} HTTP/1.1\r\nHost: {host}\r\n\r\n").into_bytes(), Some(400)),
        (format!("GET /?t={token} HTTP/1.1\r\nHost: {host}\r\nX: {}\r\n\r\n", "a".repeat(20 * 1024)).into_bytes(), Some(431)),
        (format!("GET /?t={token} HTTP/1.1\r\nHost: {host}\r\nbroken header line\r\n\r\n").into_bytes(), Some(400)),
        (format!("GET /?t={token} HTTP/1.1\r\nHost: {host}").into_bytes(), None),
        (format!("GET /ws?t={token} HTTP/1.1\r\nHost: {host}\r\nUpgrade: websocket\r\nConnection: Upgrade\r\n\r\n").into_bytes(), None),
    ];
    for (req, want) in cases {
        let (code, text) = if req.ends_with(b"\r\n\r\n") || req.ends_with(b"abc") || want.is_some() {
            http_raw(port, &req)
        } else {
            // A head that never ends: closed after writing it.
            let mut c = TcpStream::connect(("127.0.0.1", port)).unwrap();
            let _ = c.write_all(&req);
            drop(c);
            (0, String::new())
        };
        if let Some(w) = want {
            assert_eq!(code, w, "{}: {text}", String::from_utf8_lossy(&req[..req.len().min(60)]));
        }
        assert!(!text.contains("Access-Control-Allow"), "no CORS headers: {text}");
    }
    let (code, text) = http_raw(port, format!("GET /?t={token} HTTP/1.1\r\nHost: {host}\r\n\r\n").as_bytes());
    assert_eq!(code, 200, "still serving");
    assert!(text.contains("Content-Security-Policy") && !text.contains("Access-Control-Allow"), "{}", &text[..400.min(text.len())]);
}

/// The escaping fixture (criterion 11): the payload in a page title, a device tag, a property, a
/// wire number, a cable tag, a project property, a branch name and a version name. The protocol
/// sends it back as plain JSON text; the page check is done in a browser (tester report). With
/// `ECAD_TESTER029_EXPORT=<folder>` the file is kept there as `escape.v2.ecad`.
#[test]
fn hostile_text_comes_back_as_plain_json_strings() {
    use ecad_file::v2::{ProjectV2, Store};
    use ecad_model::*;
    const X: &str = "<img src=x onerror=alert(1)>";
    let dir = TempDir::new("t029-escape");
    let mut ids = IdAllocator::seeded(0xE5C);
    let mut l = Ledger::new(3);
    let (p1, layer, k, a, b): (PageId, LayerId, ComponentId, TerminalId, TerminalId) =
        (ids.fresh(), ids.fresh(), ids.fresh(), ids.fresh(), ids.fresh());
    let (w, c): (ConductorId, CableId) = (ids.fresh(), ids.fresh());
    let props = |pairs: &[(&str, &str)]| {
        let mut p = ecad_model::doc::Properties::new();
        for (k, v) in pairs {
            p.insert((*k).into(), (*v).into());
        }
        p
    };
    l.commit(
        Author::User,
        X,
        vec![
            Op::AddPage { id: p1, page: Page::new(X), index: 0 },
            Op::AddLayer { id: layer, layer: Layer { name: X.into() } },
            Op::CreateComponent { id: k, component: Component { properties: props(&[("tag", X), ("note", X)]), definition: None } },
            Op::AddTerminal { id: a, terminal: Terminal { component: k, label: Some(X.into()), key: None } },
            Op::AddTerminal { id: b, terminal: Terminal { component: k, label: Some("2".into()), key: None } },
            Op::CreateConductor { id: w, conductor: Conductor { properties: props(&[("wire_number", X)]), ends: [Some(a), Some(b)] } },
            Op::CreateCable {
                id: c,
                cable: Cable { properties: props(&[("tag", X)]), members: vec![CableMember { conductor: w, core: Some(X.into()) }] },
            },
            Op::SetProperty { object: ObjectRef::Project, key: "project.title".into(), before: None, after: Some(X.into()) },
        ],
    )
    .unwrap();
    let at = l.head();
    l.commit(Author::User, "Version", vec![Op::NameVersion { name: X.into(), at, description: Some(X.into()) }]).unwrap();
    let path = dir.path("escape.v2.ecad");
    let mut proj = ProjectV2::create(Store::create(&path).unwrap(), PROJECT, &l, None, None, Some(1), WRITER, 16).unwrap().0;
    l.mark_persisted(l.head()).unwrap();
    let main = proj.branch();
    let _ = proj.create_branch(X, main, at, None, None).unwrap();
    drop(proj);
    if let Ok(out) = std::env::var("ECAD_TESTER029_EXPORT") {
        std::fs::create_dir_all(&out).unwrap();
        std::fs::copy(&path, std::path::Path::new(&out).join("escape.v2.ecad")).unwrap();
    }
    let (port, token, _) = start(Some(&path));
    let mut ws = connect(port, &token, None, None).unwrap();
    let mut seen = 0;
    for op in ["project", "pages", "devices", "wires", "nets", "cables", "branches", "log", "versions"] {
        let r = ask(&mut ws, json!({"v":1,"id":op,"op":op,"detail":true}));
        assert!(r["ok"].is_object(), "{op}: {r}");
        let text = r.to_string();
        if text.contains(X) {
            seen += 1;
        }
        assert!(!text.contains("&lt;") && !text.contains("<td") && !text.contains("<div"), "the protocol sends no HTML: {op}: {text}");
    }
    assert!(seen >= 6, "the payload comes back as text in most views ({seen})");
}
