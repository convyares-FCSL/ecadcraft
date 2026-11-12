//! Tester 033 through MCP (brief 033 "MCP contract tests", design 033 §15): every new or changed
//! write records one transaction by `Author::Agent(<client>)` and sets the keys design §10 names;
//! the refusals of AC8 and AC13 write nothing; the `contacts` reply carries `table`; reopened with
//! writes off, every write is refused and reads give what they gave. Written from the brief and
//! design before the implementation.

use ecad_mcp::{EcadMcp, Options, policy};
use ecad_model::{Author, Op};
use serde_json::{Value, json};

use crate::mcp_contracts::{AGENT, Client, TempDir};

fn hex(v: &Value, key: &str) -> String {
    v[key].as_str().unwrap().to_string()
}

fn one_by_agent(c: &mut Client, before: usize, what: &str) -> Vec<Op> {
    let log = c.log();
    assert_eq!(log.len(), before + 1, "{what}: one transaction");
    let last = log.last().cloned().unwrap();
    assert_eq!(last.author, Author::Agent(AGENT.into()), "{what}: {}", last.label);
    last.ops
}

fn sets(ops: &[Op], key: &str) -> bool {
    ops.iter().any(|o| matches!(o, Op::SetProperty { key: k, .. } if k == key))
}

/// A refused `execute`: its words; nothing recorded.
fn refused(c: &mut Client, cmd: &str, params: Value) -> String {
    let n = c.log().len();
    let m = c.refused("execute", json!({"command": cmd, "params": params}));
    assert_eq!(c.log().len(), n, "{cmd}: nothing recorded");
    m
}

/// A coil low on page 1 (its automatic table would cross the title block) with four NO contacts
/// on page 2; a BOM page numbered 400 that runs to three sheets.
fn build(c: &mut Client) -> (String, String) {
    let k1 = hex(&c.exec("place", json!({"symbol": "coil", "at": [100, 20]})), "component");
    c.exec("newpage", json!({"title": "Contacts"}));
    for i in 0..4 {
        c.exec("place", json!({"symbol": "no", "at": [60 + 40 * i, 150], "device": k1}));
    }
    c.exec("newpage", json!({"title": "Many"}));
    for i in 0..100 {
        c.exec("place", json!({"symbol": "coil", "at": [20 + (i % 12) * 30, 30 + (i / 12) * 25]}));
    }
    let bom = hex(&c.exec("newpage", json!({"generated": "bom-location", "number": "400"})), "page");
    (k1, bom)
}

/// AC5, AC7–AC11, AC13 through MCP, then writes off.
#[test]
#[ignore = "tester 033: awaiting build"]
fn print_and_project_writes_by_the_agent_then_refused_with_writes_off() {
    let dir = TempDir::new("t033 Schaltpl\u{e4}ne mit Leerzeichen");
    let path = dir.file("Druck (P).ecad");
    let (k1, bom, before) = {
        let mut c = Client::new(EcadMcp::create(&path, Options { write: true, agent: None }).unwrap());
        let (k1, bom) = build(&mut c);
        // AC5: the contacts reply's table.
        let v = c.exec("contacts", json!({"component": k1}));
        assert_eq!(v["table"]["shown"], json!("right"), "{v}");
        assert!(v["table"]["bottom"].as_f64().unwrap() >= 2.0 - 1e-9, "{v}");
        assert!(v["table"]["top"].as_array().is_some_and(|a| a.len() == 2), "{v}");
        // AC7: sheet numbers of the BOM.
        let v = c.exec("pageprops", json!({"page": bom}));
        let nums: Vec<&str> = v["sheet_numbers"].as_array().unwrap().iter().map(|x| x.as_str().unwrap()).collect();
        assert!(nums.len() >= 3, "{v}");
        assert_eq!(nums[..3], ["400", "401", "402"]);
        // AC8: a continuation number is taken.
        let m = refused(&mut c, "newpage", json!({"title": "X", "number": "401"}));
        assert_eq!(m, "Number 401 is sheet 2 of page 400 (BOM by location).");
        let other = hex(&c.exec("newpage", json!({"title": "Other"})), "page");
        let m = refused(&mut c, "pageprops", json!({"page": other, "number": "402"}));
        assert_eq!(m, "Number 402 is sheet 3 of page 400 (BOM by location).");
        // AC13: `table`.
        let m = refused(&mut c, "table", json!({"at": [50, 50], "rows": 2, "cols": 2}));
        assert_eq!(
            m,
            "Tables can't be placed on ECAD pages yet. Draw it with lines and text, or add a generated page for lists (New Page \u{25b8} Generated)."
        );
        // Every new write: one transaction by the agent, with its keys.
        c.exec("revision", json!({"description": "First issue", "drawn": "T"}));
        let writes: Vec<(&str, Value, &[&str])> = vec![
            ("projectprops", json!({"revision_columns": "rev, date, ecr:ECR, description, drawn"}), &["revisions.columns"]),
            ("revision", json!({"fields": {"ecr": "ECR-1"}}), &["rev.1.ecr"]),
            ("issuerevision", json!({"date": "2026-11-10"}), &["rev.1.issued"]),
            ("projectprops", json!({"revision_pending": "draft"}), &["revisions.pending"]),
            ("projectprops", json!({"tag_dash": false}), &["tags.dash", "tag"]),
            ("projectprops", json!({"tag_case": "typed"}), &["tags.case"]),
        ];
        for (cmd, params, keys) in writes {
            let n = c.log().len();
            let v = c.exec(cmd, params.clone());
            let ops = one_by_agent(&mut c, n, &format!("{cmd} {params} → {v}"));
            for k in keys {
                assert!(sets(&ops, k), "{cmd} {params}: sets {k}: {ops:?}");
            }
        }
        let d = c.doc();
        assert!(d.components.values().filter_map(|x| x.properties.get("tag")).all(|t| !t.starts_with('-')), "tags without the dash");
        // `u` takes back the case setting in one step.
        let n = c.log().len();
        c.exec("u", Value::Null);
        assert_eq!(c.log().len(), n + 1);
        assert!(!c.doc().properties.contains_key("tags.case"));
        // Reads record nothing.
        let n = c.log().len();
        let before = json!({
            "contacts": c.exec("contacts", json!({"component": k1})),
            "bom_page": c.exec("pageprops", json!({"page": bom})),
            "project": c.exec("projectprops", json!({})),
        });
        assert_eq!(c.log().len(), n);
        assert_eq!(before["project"]["tag_dash"], json!(false), "{}", before["project"]);
        assert_eq!(c.ok("save", json!({}))["saved"], json!(true));
        (k1, bom, before)
    };
    let (s, _) = EcadMcp::open(&path, Options::default()).unwrap();
    let mut c = Client::new(s);
    assert!(!c.s.writes());
    let log = c.log();
    for (cmd, params) in [
        ("projectprops", json!({"tag_dash": true})),
        ("projectprops", json!({"tag_case": "typed"})),
        ("projectprops", json!({"revision_columns": "rev, description"})),
        ("projectprops", json!({"revision_pending": "off"})),
        ("revision", json!({"fields": {"ecr": "ECR-2"}})),
        ("newpage", json!({"title": "X", "number": "401"})),
        ("pageprops", json!({"page": bom, "number": "500"})),
        ("mirrormove", json!({"component": k1, "at": [100, -10]})),
        ("table", json!({"at": [50, 50], "rows": 2, "cols": 2})),
    ] {
        let m = c.refused("execute", json!({"command": cmd, "params": params}));
        assert_eq!(m, policy::WRITES_OFF, "{cmd} {params}");
    }
    let after = json!({
        "contacts": c.exec("contacts", json!({"component": k1})),
        "bom_page": c.exec("pageprops", json!({"page": bom})),
        "project": c.exec("projectprops", json!({})),
    });
    assert_eq!(after, before, "reads after reopen with writes off");
    assert_eq!(c.log(), log, "nothing recorded with writes off");
}
