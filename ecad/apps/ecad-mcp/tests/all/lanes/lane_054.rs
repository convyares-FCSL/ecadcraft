//! Brief 054 (design 054 §12, AC16): page types, folder start numbers, Renumber pages and
//! `deletepage` through MCP. Each write records one transaction by `Author::Agent(<client>)` that
//! one `u` undoes, and is refused with writes off.

use ecad_mcp::policy;
use ecad_model::{Author, Document, Op};
use serde_json::{Value, json};

use crate::common::writer;
use crate::mcp_contracts::{AGENT, Client, reader};

const WRITER_SEED: u64 = 0x054;

fn hex(v: &Value, key: &str) -> String {
    v[key].as_str().unwrap_or_default().to_string()
}

/// `cmd` records one transaction by the agent, holding ops named `names`; `u` restores the
/// document, and `redo` puts it back.
fn one_by_agent(c: &mut Client, cmd: &str, params: Value, names: &[&str]) -> Value {
    let before: Document = c.doc();
    let n = c.log().len();
    let v = c.exec(cmd, params.clone());
    let log = c.log();
    assert_eq!(log.len(), n + 1, "{cmd} {params}: one transaction");
    let last = log.last().cloned().unwrap();
    assert_eq!(last.author, Author::Agent(AGENT.into()), "{cmd}");
    let got: Vec<&str> = last.ops.iter().map(Op::name).collect();
    for want in names {
        assert!(got.contains(want), "{cmd}: {got:?} lacks {want}");
    }
    c.exec("u", json!({}));
    assert!(c.doc() == before, "{cmd} {params}: one undo restores the document");
    c.exec("redo", json!({}));
    v
}

#[test]
fn page_type_commands_record_one_transaction_by_the_agent() {
    let mut c = writer(WRITER_SEED);
    let first = c.doc().page_order[0].hex();
    let v = one_by_agent(&mut c, "newpage", json!({"type": "network", "after": first}), &["AddPage"]);
    let net = hex(&v, "page");
    assert_eq!(c.doc().page_order.get(1).map(|p| p.hex()), Some(net.clone()), "right after the first page");
    let v = one_by_agent(&mut c, "newpage", json!({"type": "front", "generated": "cover"}), &["AddPage"]);
    let cover = hex(&v, "page");
    one_by_agent(&mut c, "setstyle", json!({"catalogue": "page", "id": "schematic", "fields": {"sheet": "A2"}}), &["SetStyle"]);
    one_by_agent(&mut c, "setdefault", json!({"key": "page.type", "level": "page", "page": net, "value": "singleline"}), &["SetProperty"]);
    one_by_agent(&mut c, "projectprops", json!({"props": {"cover.notes": "a\nb"}}), &["SetProperty"]);
    one_by_agent(&mut c, "setdefault", json!({"key": "front.cover.notes", "level": "project", "value": "off"}), &["SetProperty"]);
    let f = hex(&c.exec("newfolder", json!({"name": "Schematic"})), "folder");
    one_by_agent(&mut c, "folderprops", json!({"folder": f, "start": "100"}), &["SetProperty"]);
    c.exec("movepage", json!({"page": first, "folder": f}));
    one_by_agent(&mut c, "renumberpages", json!({}), &["SetProperty"]);
    one_by_agent(&mut c, "deletepage", json!({"page": cover}), &["RemovePage"]);
    // Reads record nothing.
    let n = c.log().len();
    c.exec("styles", json!({"catalogue": "page"}));
    c.exec("folderprops", json!({"folder": f}));
    assert_eq!(c.log().len(), n);
}

#[test]
fn with_writes_off_page_type_writes_are_refused() {
    let mut c = reader();
    let before = c.log();
    let page = c.doc().page_order[0].hex();
    let writes: &[(&str, Value)] = &[
        ("newpage", json!({"type": "schematic"})),
        ("newpage", json!({"type": "front", "generated": "revisions"})),
        ("setstyle", json!({"catalogue": "page", "id": "schematic", "fields": {"zones": "off"}})),
        ("setdefault", json!({"key": "page.type", "level": "page", "page": page, "value": "network"})),
        ("projectprops", json!({"props": {"cover.notes": "a"}})),
        ("setdefault", json!({"key": "front.cover.notes", "level": "project", "value": "off"})),
        ("folderprops", json!({"folder": "0000000000000000000000000000000a", "start": "100"})),
        ("renumberpages", json!({})),
        ("deletepage", json!({"page": page})),
    ];
    for (cmd, params) in writes {
        let m = c.refused("execute", json!({"command": cmd, "params": params}));
        assert_eq!(m, policy::WRITES_OFF, "{cmd}");
        assert_eq!(c.log(), before, "{cmd} moved the head");
    }
}
