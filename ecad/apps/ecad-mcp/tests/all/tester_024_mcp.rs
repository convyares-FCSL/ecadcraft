//! Tester probe for brief 024 over MCP: Print Book and the reports never write a file for an
//! agent, in any spelling of the file parameter or mode; a book of more than 10 pages comes back
//! as one base64 PDF; the location writes are refused with writes off.

use ecad_mcp::{EcadMcp, Options};
use serde_json::{Value, json};

use crate::mcp_contracts::{Client, reader};

fn writer() -> Client {
    Client::new(EcadMcp::in_memory("mcp-t024", 0x24_7701, Options { write: true, agent: None }).unwrap())
}

fn b64(s: &str) -> Vec<u8> {
    let val = |c: u8| -> Option<u32> {
        match c {
            b'A'..=b'Z' => Some(u32::from(c - b'A')),
            b'a'..=b'z' => Some(u32::from(c - b'a') + 26),
            b'0'..=b'9' => Some(u32::from(c - b'0') + 52),
            b'+' => Some(62),
            b'/' => Some(63),
            _ => None,
        }
    };
    let (mut out, mut acc, mut bits) = (Vec::new(), 0u32, 0);
    for c in s.bytes() {
        let Some(v) = val(c) else { continue };
        acc = (acc << 6) | v;
        bits += 6;
        if bits >= 8 {
            bits -= 8;
            out.push((acc >> bits) as u8);
            acc &= (1 << bits) - 1;
        }
    }
    out
}

#[test]
fn printbook_and_reports_refuse_every_file_parameter_in_every_mode() {
    let dir = std::env::temp_dir().join(format!("ecad-t024-mcp-{}", std::process::id()));
    let target = dir.join("x.pdf");
    for mut c in [writer(), reader()] {
        let before = c.log().len();
        for (cmd, params) in [
            ("printbook", json!({"path": target.to_string_lossy()})),
            ("printbook", json!({"path": ""})),
            ("printbook", json!({"path": "x.pdf", "scope": "all", "colour": "mono"})),
            ("printbook", json!({"scope": {"pages": "1"}, "path": "../x.pdf"})),
            ("printbook", json!({"file": "x.pdf"})),
            ("printbook", json!({"output": "x.pdf"})),
            ("bom", json!({"path": "x.csv", "group": "part"})),
            ("wirelist", json!({"path": "x.csv"})),
            ("cablelist", json!({"path": "x.csv"})),
        ] {
            let m = c.refused("execute", json!({"command": cmd, "params": params.clone()}));
            assert!(m.starts_with("Refused"), "{cmd} {params}: {m}");
        }
        assert_eq!(c.log().len(), before, "nothing recorded");
    }
    assert!(!target.exists(), "no file written");
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn printbook_of_twelve_pages_returns_one_base64_pdf() {
    let mut c = writer();
    c.exec("projectprops", json!({"title": "Twelve", "location": "+CP1"}));
    c.exec("newpage", json!({"generated": "cover"}));
    c.exec("newpage", json!({"generated": "contents"}));
    c.exec("newpage", json!({"generated": "bom-location"}));
    for i in 0..9 {
        c.exec("newpage", json!({"title": format!("P{i}")}));
        c.exec("place", json!({"symbol": "coil", "at": [100, 100]}));
    }
    let before = c.log().len();
    let v = c.exec("printbook", json!({}));
    assert_eq!(v["sheets"], 13, "{}", v["message"]);
    let files = v["files"].as_array().unwrap();
    assert_eq!(files.len(), 1);
    assert!(files[0]["path"].is_null(), "no file under MCP: {}", files[0]["path"]);
    let bytes = b64(files[0]["data"].as_str().unwrap());
    assert!(bytes.starts_with(b"%PDF"));
    assert_eq!(files[0]["bytes"].as_u64(), Some(bytes.len() as u64));
    assert!(bytes.windows(5).rev().take(64).any(|w| w == b"%%EOF"), "a whole PDF");
    assert_eq!(c.log().len(), before, "printing records nothing");
    // The reports page over MCP too.
    let v = c.exec("bom", json!({"limit": 3}));
    assert_eq!(v["rows"].as_array().unwrap().len(), 3, "{v}");
    assert!(v["next"].as_u64().is_some(), "{v}");
    let v: Value = c.exec("bom", json!({"limit": 3, "after": v["next"]}));
    assert!(!v["rows"].as_array().unwrap().is_empty(), "{v}");
}
