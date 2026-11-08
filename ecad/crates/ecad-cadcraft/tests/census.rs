//! Command census (verification V4): every CADCraft command's JSON form, with default parameters,
//! on an ECAD page holding a component, a symbol and a wire. Each outcome is classified; whatever
//! happens, the checks of §17.4 must still hold. The table is written to
//! `$CARGO_TARGET_TMPDIR/ecad-census.md` for the experiment report.

mod common;

use std::collections::BTreeMap;
use std::panic::{AssertUnwindSafe, catch_unwind};

use common::{H, pts};
use serde_json::{Value, json};

/// Commands that leave the page (open, close, new documents, quit) are not about the boundary.
const SKIP: &[&str] =
    &["close", "closeall", "new", "qnew", "open", "quit", "exit", "newsheetset", "recover", "recoverall", "delay"];

fn fixture() -> H {
    let mut h = H::new(5);
    h.cmdline("line 0,0 40,0 40,20 0,20 c");
    let first = h.first_page();
    let lines = h.lines_on(first);
    h.run("convertcomponent", json!({"handles": lines.iter().map(|x| x.hex()).collect::<Vec<_>>()})).unwrap();
    h.run("addterminal", json!({"handle": lines[1].hex(), "at": [40, 10]})).unwrap();
    h.run("place", json!({"at": [100, 0]})).unwrap();
    h.run("wire", json!({"points": pts(&[(40.0, 10.0), (100.0, 10.0)])})).unwrap();
    h.cmdline("line 0,40 10,40 ");
    // Select everything so commands that act on the selection have something to act on.
    let page = h.first_page();
    let all: Vec<_> = h.pv(|p, pg| pg.entities_of(p, page)).iter().map(|e| e.handle).collect();
    h.s.set_selection(all);
    h
}

#[test]
fn every_command_leaves_the_project_consistent() {
    ecad_cadcraft::install();
    let ids: Vec<&'static str> = cadcraft_engine::command_specs().iter().map(|c| c.id).collect();
    let ours: Vec<&str> = ecad_cadcraft::commands::specs().iter().map(|c| c.id).collect();
    let mut rows: BTreeMap<&'static str, Vec<String>> = BTreeMap::new();
    let mut broken = Vec::new();
    for id in ids.iter().copied().filter(|id| !SKIP.contains(id) && !ours.contains(id)) {
        let mut h = fixture();
        let head = h.head();
        let refused_before = h.p(|p| p.events.count(ecad_inspect::EventKind::Refused));
        let r = h.s.execute(id, &Value::Null);
        let refused = h.p(|p| p.events.count(ecad_inspect::EventKind::Refused)) > refused_before;
        let class = match (&r, h.head() > head, refused) {
            (_, true, _) => "recorded",
            (_, _, true) => "refused by the owner",
            (Ok(_), false, false) => "no recorded change",
            (Err(_), false, false) => "command error (no change)",
        };
        if let Some(st) = h.s.running.as_ref().map(|r| r.id.clone()) {
            let _ = st;
            h.s.cancel();
        }
        let note = match &r {
            Err(e) if refused => e.to_string(),
            _ => String::new(),
        };
        rows.entry(class).or_default().push(if note.is_empty() { id.to_string() } else { format!("{id} ({note})") });
        if catch_unwind(AssertUnwindSafe(|| h.check())).is_err() {
            broken.push(id);
        }
    }
    let mut md = String::from(
        "# Command census\n\nEvery CADCraft command's JSON form with default parameters on an ECAD page.\n\n",
    );
    for (class, list) in &rows {
        md.push_str(&format!("## {class} ({})\n\n{}\n\n", list.len(), list.join(", ")));
    }
    if let Ok(dir) = std::env::var("CARGO_TARGET_TMPDIR") {
        let _ = std::fs::write(std::path::Path::new(&dir).join("ecad-census.md"), &md);
    }
    println!("{md}");
    assert!(
        ours.iter().all(|o| cadcraft_engine::find_command(o)
            .is_some_and(|c| ecad_cadcraft::commands::specs().iter().any(|s| s.id == c.id && s.label == c.label))),
        "an ECAD command id is shadowed by a built-in"
    );
    assert!(broken.is_empty(), "checks failed after: {broken:?}");
}
