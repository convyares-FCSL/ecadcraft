//! The inspector protocol (brief 029, design 029 §2, §6, §8): every op's reply shape, counts and
//! paging on the rich fixture; every mismatch row gives its `what` and `way_out`; no input
//! panics; serving never writes the file.

use std::time::Duration;

use ecad_inspector::source::{FileSource, Source, VerifyState};
use redb::ReadableTable;
use serde_json::{Value, json};

use crate::common::*;

fn src(f: &FileSource) -> Option<&dyn Source> {
    Some(f as &dyn Source)
}

fn ok(v: &Value) -> &Value {
    assert!(v.get("error").is_none(), "{v:#}");
    &v["ok"]
}

fn what(v: &Value) -> &str {
    v["error"]["what"].as_str().unwrap_or_else(|| panic!("not an error: {v:#}"))
}

/// The rich fixture, opened and verified.
fn opened(dir: &TempDir) -> FileSource {
    let f = FileSource::from_path(&rich(dir, "Main"));
    assert!(matches!(f.wait_verified(Duration::from_secs(60)), VerifyState::Done { .. }));
    f
}

#[test]
fn every_op_answers_in_shape_on_the_rich_fixture() {
    let dir = TempDir::new("ops");
    let f = opened(&dir);
    let s = src(&f);
    // (request, check of the `ok` value)
    type Check = fn(&Value);
    let rows: Vec<(Value, Check)> = vec![
        (json!({"v": 1, "id": "1", "op": "hello"}), |o| {
            assert_eq!(o["protocol"], 1);
            assert_eq!(o["source"]["kind"], "file");
            assert_eq!(o["capabilities"], json!(["store", "verify", "model"]));
            assert!(o["ops"].as_array().unwrap().len() >= 18);
        }),
        (json!({"op": "summary"}), |o| {
            assert_eq!(o["open"]["state"], "clean");
            assert_eq!(o["open"]["branch"], "main");
            assert_eq!(o["meta"]["container_version"], 2);
            assert_eq!(o["meta"]["codec"], "json/1");
            assert_eq!(o["chain"]["ok"], true);
            assert_eq!(o["counts"]["devices"], 3);
            assert_eq!(o["counts"]["wires"], 2);
            // The wired net, and one net for each connection point no wire reaches.
            assert_eq!(o["counts"]["nets"], 3);
            assert_eq!(o["branches"], 2);
        }),
        (json!({"op": "project"}), |o| {
            assert_eq!(o["revisions"]["1"]["rev"], "A");
            assert_eq!(o["revisions"]["1"]["issued"], "Rev A");
            assert_eq!(o["counts"]["pages"], 2);
            assert!(o["properties"].get("rev.1.rev").is_none(), "revision rows are grouped");
        }),
        (json!({"op": "pages"}), |o| {
            assert_eq!(o["total"], 2);
            assert_eq!(o["rows"][0]["title"], "Main");
            assert_eq!(o["rows"][0]["items"], 20);
        }),
        (json!({"op": "devices"}), |o| {
            assert_eq!(o["total"], 3);
            let k1: Vec<&Value> = o["rows"].as_array().unwrap().iter().filter(|r| r["tag"] == "-K1").collect();
            assert_eq!(k1.len(), 2);
            assert!(k1.iter().all(|r| r["duplicate"] == true));
            let x1 = o["rows"].as_array().unwrap().iter().find(|r| r["tag"] == "-X1").unwrap();
            assert_eq!(x1["points"][0], json!({"name": "3", "wires": ["101", "101"]}));
            assert!(x1.get("properties").is_none(), "properties only with detail");
        }),
        (json!({"op": "devices", "detail": true, "q": "-x1"}), |o| {
            assert_eq!(o["total"], 1);
            assert_eq!(o["rows"][0]["properties"]["tag"], "-X1");
        }),
        (json!({"op": "nets"}), |o| {
            assert_eq!(o["total"], 3);
            assert_eq!(o["rows"][0]["number"], "101", "numbered nets first");
            assert_eq!(o["rows"][0]["wires"], 2);
            assert_eq!(o["rows"][0]["points"], json!(["-K1:A1", "-X1:3", "-X1:4"]));
        }),
        (json!({"op": "wires"}), |o| {
            assert_eq!(o["total"], 2);
            let w = o["rows"].as_array().unwrap().iter().find(|r| r["cable"] == "-W1").unwrap();
            assert_eq!(w["ends"], json!(["-K1:A1", "-X1:3"]));
            assert_eq!(w["core"], "1");
            assert_eq!(w["cross_section"], "1.5");
        }),
        (json!({"op": "cables"}), |o| {
            assert_eq!(o["total"], 1);
            assert_eq!(o["rows"][0]["tag"], "-W1");
            assert_eq!(o["rows"][0]["members"], json!([{"wire": "101", "core": "1"}]));
        }),
        (json!({"op": "branches"}), |o| {
            assert_eq!(o["total"], 2);
            let b = o["rows"].as_array().unwrap().iter().find(|r| r["name"] == "Option B").unwrap();
            assert_eq!(b["fork"]["parent"], "main");
            assert_eq!(b["rows"], 5);
        }),
        (json!({"op": "log", "count": 3}), |o| {
            assert_eq!(o["branch"], "main");
            let rows = o["rows"].as_array().unwrap();
            assert_eq!(rows.len(), 3);
            assert_eq!(rows[0]["seq"], o["total"], "newest first");
            assert_eq!(rows[0]["label"], "Issue A");
            assert!(rows[0].get("record").is_none());
        }),
        (json!({"op": "log", "branch": "Option B", "from": 4, "count": 10, "detail": true}), |o| {
            let rows = o["rows"].as_array().unwrap();
            assert_eq!((o["total"].as_u64(), rows.len()), (Some(5), 1));
            assert_eq!(rows[0]["seq"], 1);
            assert_eq!(rows[0]["record"]["type"], "tx");
        }),
        (json!({"op": "versions"}), |o| {
            let rows = o["rows"].as_array().unwrap();
            assert!(rows.iter().any(|r| r["kind"] == "Revision" && r["name"] == "Rev A" && r["release"]["rev"] == "A"), "{rows:#?}");
            assert!(rows.iter().any(|r| r["kind"] == "Auto"));
        }),
        (json!({"op": "tables"}), |o| {
            let names: Vec<&str> = o["tables"].as_array().unwrap().iter().map(|t| t["name"].as_str().unwrap()).collect();
            assert_eq!(names, ["meta", "branches", "transactions", "versions", "chunks", "assets"]);
            assert_eq!(o["meta"]["format"], "ecad-project");
        }),
        (json!({"op": "rows", "table": "transactions", "count": 2}), |o| {
            assert!(o["total"].as_u64().unwrap() > 20);
            assert_eq!(o["rows"].as_array().unwrap().len(), 2);
            assert!(o["rows"][0]["compression"].is_string());
            assert!(o["rows"][0]["key_text"].as_str().unwrap().ends_with("#1"));
        }),
        (json!({"op": "check_chain"}), |o| {
            assert_eq!(o["ok"], true);
            assert_eq!(o["full_checks"], 0);
        }),
        (json!({"op": "invariants"}), |o| {
            assert_eq!(o["ok"], true);
            assert_eq!(o["mismatches"], 0);
        }),
        (json!({"op": "verify"}), |o| {
            assert_eq!(o["state"], "done");
            assert_eq!(o["report"]["ok"], true);
            assert!(o["elapsed_ms"].is_u64());
        }),
        (json!({"op": "export_jsonl"}), |o| {
            assert_eq!(o["branch"], "main");
            let text = o["text"].as_str().unwrap();
            assert_eq!(text.lines().count() as u64, o["lines"].as_u64().unwrap());
            assert_eq!(o["left_out"].as_array().unwrap().len(), 1);
        }),
    ];
    for (i, (req, check)) in rows.iter().enumerate() {
        let mut req = req.clone();
        req["id"] = json!(format!("r{i}"));
        let reply = ask(s, req.clone());
        assert_eq!(reply["v"], 1, "{req}");
        assert_eq!(reply["id"], json!(format!("r{i}")), "replies echo the id: {req}");
        check(ok(&reply));
    }
}

#[test]
fn row_is_readable_or_raw_and_raw_is_the_stored_bytes() {
    let dir = TempDir::new("row");
    let f = opened(&dir);
    let s = src(&f);
    let first = ask(s, json!({"op": "rows", "table": "transactions", "count": 1}));
    let key = ok(&first)["rows"][0]["key"].as_str().unwrap().to_string();
    let readable = ask(s, json!({"op": "row", "table": "transactions", "key": key, "form": "readable"}));
    assert_eq!(ok(&readable)["decoded"]["record"]["seq"], 1);
    let raw = ask(s, json!({"op": "row", "table": "transactions", "key": key, "form": "raw"}));
    let r = f.store().unwrap().reader().unwrap();
    let mut bytes = None;
    r.raw_rows(|t, k, v| {
        if t == "transactions" && ecad_inspector::text::hex(k) == key {
            bytes = Some(v.to_vec());
        }
        Ok(())
    })
    .unwrap();
    let bytes = bytes.unwrap();
    assert_eq!(ok(&raw)["hex"], ecad_inspector::text::hex(&bytes));
    assert_eq!(ok(&raw)["size"], bytes.len());
    assert_eq!(ok(&raw)["truncated"], false);
    assert_eq!(what(&ask(s, json!({"op": "row", "table": "transactions", "key": "00"}))), "not-found");
    assert_eq!(what(&ask(s, json!({"op": "rows", "table": "nope"}))), "bad-request");
}

#[test]
fn export_jsonl_is_byte_for_byte_the_text_view() {
    let dir = TempDir::new("export");
    let f = opened(&dir);
    let reply = ask(src(&f), json!({"op": "export_jsonl", "branch": "Option B"}));
    let mut want = Vec::new();
    ecad_inspector::text::export_branch_jsonl(f.store().unwrap(), false, "Option B", &mut want).unwrap();
    assert_eq!(ok(&reply)["text"].as_str().unwrap().as_bytes(), want.as_slice());
}

#[test]
fn paging_counts_and_filters() {
    let dir = TempDir::new("paging");
    let f = opened(&dir);
    let s = src(&f);
    let all = ask(s, json!({"op": "devices"}));
    assert_eq!(ok(&all)["rows"].as_array().unwrap().len(), 3);
    for (from, count, rows) in [(0, 1, 1), (1, 1, 1), (2, 5, 1), (3, 5, 0), (99, 5, 0)] {
        let r = ask(s, json!({"op": "devices", "from": from, "count": count}));
        assert_eq!(ok(&r)["total"], 3);
        assert_eq!(ok(&r)["rows"].as_array().unwrap().len(), rows, "from {from} count {count}");
    }
    // A huge count is capped at 1 000; a negative or non-numeric one takes the default.
    for count in [json!(u64::MAX), json!(-5), json!("many")] {
        let r = ask(s, json!({"op": "log", "count": count}));
        assert!(ok(&r)["rows"].as_array().unwrap().len() <= 1_000);
    }
    let q = ask(s, json!({"op": "devices", "q": "-K1"}));
    assert_eq!(ok(&q)["total"], 2, "q is case-insensitive");
}

#[test]
fn envelope_errors_have_a_stable_what() {
    let dir = TempDir::new("envelope");
    let f = opened(&dir);
    let s = src(&f);
    let raw = |text: &str| -> Value { serde_json::from_str(&ecad_inspector::protocol::handle(s, text)).unwrap() };
    let rows: &[(&str, &str)] = &[
        ("not json", "bad-request"),
        ("[1, 2]", "bad-request"),
        ("{}", "bad-request"),
        (r#"{"op": 7}"#, "bad-request"),
        (r#"{"v": "one", "op": "hello"}"#, "bad-request"),
        (r#"{"v": 2, "op": "hello"}"#, "protocol"),
        (r#"{"op": "subscribe", "topic": "commits"}"#, "unknown-op"),
        (r#"{"op": "unsubscribe"}"#, "unknown-op"),
        (r#"{"op": "write_everything"}"#, "unknown-op"),
        (r#"{"op": "open"}"#, "bad-request"),
    ];
    for (text, want) in rows {
        let r = raw(text);
        assert_eq!(what(&r), *want, "{text}");
        assert!(r["error"]["why"].as_str().is_some_and(|w| !w.is_empty()) && r["error"]["way_out"].as_str().is_some_and(|w| !w.is_empty()), "{r}");
    }
    let long_id = "x".repeat(65);
    assert_eq!(what(&raw(&json!({"op": "hello", "id": long_id}).to_string())), "bad-request");
    let long_q = "q".repeat(257);
    assert_eq!(what(&raw(&json!({"op": "devices", "q": long_q}).to_string())), "bad-request");
    // Unknown fields are ignored.
    assert!(raw(r#"{"v": 1, "op": "hello", "later": {"x": 1}}"#).get("ok").is_some());
    // Nothing open: every op but hello says so.
    assert_eq!(what(&ask(None, json!({"op": "devices"}))), "not-open");
    assert!(ask(None, json!({"op": "hello"}))["ok"]["source"].is_null());
}

// ------------------------------------------------------------------ mismatches (design 029 §6)

/// Rewrite one `meta` row of the file at `path`.
fn set_meta(path: &std::path::Path, name: &str, value: Value) {
    let db = redb::Database::open(path).unwrap();
    let w = db.begin_write().unwrap();
    {
        let mut t = w.open_table(ecad_file::v2::schema::META).unwrap();
        t.insert(name, serde_json::to_vec(&value).unwrap().as_slice()).unwrap();
    }
    w.commit().unwrap();
}

#[test]
fn every_mismatch_row_gives_its_what_and_a_way_out() {
    let dir = TempDir::new("mismatch");
    let good = std::fs::read(rich(&dir, "Main")).unwrap();
    let v1 = v1_bytes(&history());
    let mut redb_junk = b"redb".to_vec();
    redb_junk.extend(std::iter::repeat_n(0xA5u8, 5_000));
    let with_meta = |name: &str, value: Value| {
        let p = dir.path(&format!("meta-{name}.ecad"));
        std::fs::write(&p, &good).unwrap();
        set_meta(&p, name, value);
        std::fs::read(&p).unwrap()
    };
    // (case, name, bytes, what, an op that still works, an op that does not)
    let rows: Vec<(&str, &str, Vec<u8>, &str, Option<&str>, &str)> = vec![
        ("1 empty", "x.ecad", Vec::new(), "empty", None, "tables"),
        ("1 lock file", "x.ecad.lock", b"pid 7".to_vec(), "empty", None, "tables"),
        ("2 version 1", "old.ecad", v1.clone(), "version-1", None, "devices"),
        ("3 snapshot", "s.json", br#"{"format":"ecad-inspect/2","seq":1}"#.to_vec(), "snapshot", None, "devices"),
        ("4 not ecad", "z.ecad", b"PK\x03\x04 a zip".to_vec(), "not-ecad", None, "tables"),
        ("5 not a project", "lib.ecad", with_meta("format", json!("ecad-library")), "not-project", Some("tables"), "devices"),
        ("6 container version", "c3.ecad", with_meta("container_version", json!(3)), "container-version", Some("tables"), "devices"),
        ("7 schema version", "s2.ecad", with_meta("schema_version", json!(2)), "schema-version", Some("rows"), "devices"),
        ("8 unreadable", "r.ecad", redb_junk, "unreadable", None, "tables"),
    ];
    for (case, name, bytes, want, works, refused) in rows {
        let f = FileSource::from_bytes(name, bytes);
        let s = src(&f);
        let summary = ask(s, json!({"op": "summary"}));
        let p = &ok(&summary)["problem"];
        assert_eq!(p["what"], want, "{case}: {summary:#}");
        assert!(p["why"].as_str().is_some_and(|w| !w.is_empty()) && p["way_out"].as_str().is_some_and(|w| !w.is_empty()), "{case}: {p}");
        let r = ask(s, json!({"op": refused}));
        assert_eq!(what(&r), want, "{case}: {refused}");
        if let Some(op) = works {
            let r = ask(s, json!({"op": op, "table": "meta"}));
            assert!(r.get("ok").is_some(), "{case}: {op} still works: {r}");
        }
        f.wait_verified(Duration::from_secs(10));
    }
    // Row 2 shows the version 1 header line; row 4 the first bytes.
    let f = FileSource::from_bytes("old.ecad", v1);
    let s = ask(src(&f), json!({"op": "summary"}));
    assert!(ok(&s)["v1_header"].as_str().unwrap().starts_with('{'), "{s}");
    assert!(ok(&s)["problem"]["way_out"].as_str().unwrap().contains("ecad upgrade old.ecad"));
    let f = FileSource::from_bytes("z.ecad", b"PK\x03\x04 a zip".to_vec());
    assert_eq!(ok(&ask(src(&f), json!({"op": "summary"})))["head_hex"].as_str().unwrap().get(..8), Some("504b0304"));
}

#[test]
fn a_damaged_history_opens_read_only_at_the_last_good_transaction() {
    // Row 11: the summary says where, the model views show the state before it, and the Checks
    // views (chain, Verify) name it.
    let dir = TempDir::new("damage");
    let path = rich(&dir, "Main");
    let main = ecad_file::v2::schema::BranchId::main_of(PROJECT.0);
    let last = ecad_file::v2::Store::open(&path, false).unwrap().reader().unwrap().last_seq(main).unwrap().unwrap();
    {
        let db = redb::Database::open(&path).unwrap();
        let w = db.begin_write().unwrap();
        {
            let mut t = w.open_table(ecad_file::v2::schema::TRANSACTIONS).unwrap();
            let k = ecad_file::v2::schema::tx_key(main, last);
            let mut v = t.get(k).unwrap().unwrap().value().to_vec();
            let end = v.len() - 1;
            v[end] ^= 1;
            t.insert(k, v.as_slice()).unwrap();
        }
        w.commit().unwrap();
    }
    let f = FileSource::from_path(&path);
    let s = ask(src(&f), json!({"op": "summary"}));
    assert_eq!(ok(&s)["open"]["state"], "damaged", "{s:#}");
    assert_eq!(ok(&s)["chain"]["ok"], false);
    assert!(ask(src(&f), json!({"op": "devices"})).get("ok").is_some(), "the model views still answer");
    match f.wait_verified(Duration::from_secs(60)) {
        VerifyState::Done { report, .. } => assert!(!report.ok && report.failures.iter().any(|l| l.contains(&format!("transaction {last}")))),
        other => panic!("{other:?}"),
    }
}

#[test]
fn a_file_changed_on_disk_is_refused_until_reloaded() {
    // Row 9: the identity (size and modified time) is checked on each request.
    let dir = TempDir::new("changed");
    let path = rich(&dir, "Main");
    let f = FileSource::from_path(&path);
    f.wait_verified(Duration::from_secs(60));
    assert!(ask(src(&f), json!({"op": "summary"})).get("ok").is_some());
    // The file grows, as a save in the app makes it.
    let mut bytes = std::fs::read(&path).unwrap();
    bytes.extend_from_slice(&[0u8; 4096]);
    std::fs::write(&path, &bytes).unwrap();
    let r = ask(src(&f), json!({"op": "devices"}));
    assert_eq!(what(&r), "changed", "{r}");
    assert!(ask(src(&f), json!({"op": "hello"})).get("ok").is_some(), "hello still answers");
}

// ------------------------------------------------------------------ hostile input

/// A small deterministic generator (fixed seed).
fn noise(seed: u64, n: usize) -> Vec<u8> {
    let mut x = seed;
    (0..n)
        .map(|_| {
            x ^= x << 13;
            x ^= x >> 7;
            x ^= x << 17;
            (x >> 24) as u8
        })
        .collect()
}

/// Every op on `f`: a reply, never a panic.
fn every_op(f: &FileSource) {
    for op in ecad_inspector::protocol::OPS {
        for extra in [json!({}), json!({"table": "transactions", "key": "00", "detail": true, "branch": "main"})] {
            let mut req = json!({"v": 1, "id": "h", "op": op});
            for (k, v) in extra.as_object().unwrap() {
                req[k] = v.clone();
            }
            let reply = ask(src(f), req);
            assert!(reply.get("ok").is_some() || reply.get("error").is_some());
        }
    }
}

#[test]
fn hostile_files_get_a_reply_never_a_panic() {
    let dir = TempDir::new("hostile");
    let path = rich(&dir, "Main");
    let good = std::fs::read(&path).unwrap();
    // Truncations, at a stride (every byte is in the slow set below).
    let stride = (good.len() / 97).max(1);
    for cut in (0..good.len()).step_by(stride) {
        let f = FileSource::from_bytes("cut.ecad", good.get(..cut).unwrap().to_vec());
        every_op(&f);
        f.wait_verified(Duration::from_secs(30));
    }
    // Random bytes, with and without the database magic.
    for seed in 1..=4u64 {
        let mut b = noise(seed, 70_000);
        if seed % 2 == 0 {
            b.splice(0..4, *b"redb");
        }
        every_op(&FileSource::from_bytes("noise.ecad", b));
    }
    // A flipped byte in one row of each table.
    for table in ecad_file::v2::store::TABLES {
        let p = dir.path(&format!("flip-{table}.ecad"));
        std::fs::write(&p, &good).unwrap();
        let db = redb::Database::open(&p).unwrap();
        let w = db.begin_write().unwrap();
        let flip = |mut v: Vec<u8>| {
            if let Some(b) = v.last_mut() {
                *b ^= 0x5A;
            }
            v
        };
        let mut flipped = true;
        {
            use ecad_file::v2::schema::{BRANCHES, CHUNKS, META, TRANSACTIONS, VERSIONS};
            match table {
                "meta" => {
                    let mut t = w.open_table(META).unwrap();
                    let v = flip(t.get("format").unwrap().unwrap().value().to_vec());
                    t.insert("format", v.as_slice()).unwrap();
                }
                "branches" => {
                    let mut t = w.open_table(BRANCHES).unwrap();
                    let (k, v) = t.iter().unwrap().next().unwrap().map(|(k, v)| (k.value(), v.value().to_vec())).unwrap();
                    t.insert(k, flip(v).as_slice()).unwrap();
                }
                "transactions" => {
                    let mut t = w.open_table(TRANSACTIONS).unwrap();
                    let (k, v) = t.iter().unwrap().next().unwrap().map(|(k, v)| (k.value(), v.value().to_vec())).unwrap();
                    t.insert(k, flip(v).as_slice()).unwrap();
                }
                "versions" => {
                    let mut t = w.open_table(VERSIONS).unwrap();
                    let (k, v) = t.iter().unwrap().next().unwrap().map(|(k, v)| (k.value().to_vec(), v.value().to_vec())).unwrap();
                    t.insert(k.as_slice(), flip(v).as_slice()).unwrap();
                }
                "chunks" => {
                    let mut t = w.open_table(CHUNKS).unwrap();
                    let (k, v) = t.iter().unwrap().next().unwrap().map(|(k, v)| (k.value(), v.value().to_vec())).unwrap();
                    t.insert(k, flip(v).as_slice()).unwrap();
                }
                _ => flipped = false,
            }
        }
        w.commit().unwrap();
        drop(db);
        let f = FileSource::from_path(&p);
        every_op(&f);
        if flipped {
            let v = f.wait_verified(Duration::from_secs(60));
            assert!(!matches!(v, VerifyState::Running { .. }));
        }
    }
}

#[test]
#[ignore = "slow: every truncation of the rich fixture"]
fn every_truncation_gets_a_reply() {
    let dir = TempDir::new("hostile-all");
    let good = std::fs::read(rich(&dir, "Main")).unwrap();
    for cut in 0..good.len() {
        every_op(&FileSource::from_bytes("cut.ecad", good.get(..cut).unwrap().to_vec()));
    }
}

#[test]
fn serving_never_writes_the_file() {
    let dir = TempDir::new("never-writes");
    let path = rich(&dir, "Main");
    let before = (std::fs::read(&path).unwrap(), std::fs::metadata(&path).unwrap().modified().unwrap());
    let f = FileSource::from_path(&path);
    f.wait_verified(Duration::from_secs(60));
    every_op(&f);
    drop(f);
    let after = (std::fs::read(&path).unwrap(), std::fs::metadata(&path).unwrap().modified().unwrap());
    assert!(before == after, "the bytes and modified time are unchanged");
}

#[test]
fn markup_in_values_is_sent_as_plain_text() {
    // Design 029 §5.3: the protocol never sends HTML; the page escapes every value (the tester
    // checks the page in a browser).
    let dir = TempDir::new("escape");
    let evil = "<img src=x onerror=alert(1)>";
    let f = FileSource::from_path(&rich(&dir, evil));
    let pages = ask(src(&f), json!({"op": "pages"}));
    assert_eq!(ok(&pages)["rows"][0]["title"], evil);
    let devices = ask(src(&f), json!({"op": "devices", "detail": true}));
    assert_eq!(ok(&devices)["rows"][0]["properties"]["note"], evil);
}
