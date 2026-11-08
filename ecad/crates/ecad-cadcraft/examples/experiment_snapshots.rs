//! Write ecad-inspect snapshots of a short experiment run, for the static inspector
//! (`ecad/tools/inspector/index.html`).
//!
//! Usage: `cargo run -p ecad-cadcraft --example experiment_snapshots -- <output dir>`
//!
//! Runs headless (no display needed): draws, converts, places, wires, moves, copies, refuses one
//! command, saves, fails one save, and writes one live snapshot plus historical snapshots at a
//! few sequence numbers. Historical snapshots are reconstructed without touching the live project.

use std::path::PathBuf;

use cadcraft_engine::Session;
use ecad_cadcraft::{Project, session_seed};
use serde_json::{Value, json};

fn main() -> Result<(), String> {
    let dir = PathBuf::from(std::env::args().nth(1).unwrap_or_else(|| "ecad-snapshots".into()));
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    ecad_cadcraft::install();
    let mut s = Session::empty();
    let project = Project::create(&mut s, "Inspector demo", session_seed()).map_err(|e| e.to_string())?;
    let run = |s: &mut Session, cmd: &str, p: Value| -> Value {
        match s.execute(cmd, &p) {
            Ok(v) => v,
            Err(e) => {
                eprintln!("{cmd}: {e} (expected for the deliberate refusal)");
                Value::Null
            }
        }
    };
    s.cmdline("line 0,0 40,0 40,20 0,20 c").map_err(|e| e.to_string())?;
    let lines: Vec<String> = s.doc().map_err(|e| e.to_string())?.model.iter().map(|e| e.handle.hex()).collect();
    let c = run(&mut s, "convertcomponent", json!({"handles": lines}));
    run(&mut s, "setprop", json!({"component": c["component"], "key": "tag", "value": "-K1"}));
    let t1 = run(&mut s, "addterminal", json!({"handle": lines[1], "at": [40, 15], "label": "13"}));
    run(&mut s, "addterminal", json!({"handle": lines[1], "at": [40, 5], "label": "14"}));
    let sym = run(&mut s, "place", json!({"at": [100, 10], "rotation": 90}));
    run(&mut s, "setprop", json!({"component": sym["component"], "key": "tag", "value": "-K2"}));
    let w1 = run(&mut s, "wire", json!({"points": [[40, 15], [65, 15], [65, 10], [90, 10]]}));
    let w2 = run(&mut s, "wire", json!({"points": [[40, 5], [60, -10]]}));
    run(&mut s, "cableadd", json!({"conductors": [w1["conductor"], w2["conductor"]], "tag": "-W1"}));
    let handles = project.read(&mut s, |p, _| -> Result<Vec<String>, String> {
        let doc = p.ledger.document();
        let page = *doc.page_order.first().ok_or("page")?;
        let map = &p.pages.get(&page).ok_or("page")?.map;
        let comp = ecad_model::ComponentId::parse(c["component"].as_str().unwrap_or_default()).ok_or("id")?;
        let mut out = Vec::new();
        for (rid, r) in doc.representations_of(ecad_model::Subject::Component(comp)) {
            out.extend(r.items.iter().filter_map(|i| map.item(*i)).map(|h| h.hex()));
            out.extend(r.connection_points.keys().filter_map(|t| map.marker(rid, *t)).map(|h| h.hex()));
        }
        Ok(out)
    })??;
    run(&mut s, "move", json!({"handles": handles, "delta": [0, -5]}));
    run(&mut s, "layer.new", json!({"name": "NEW"})); // refused by the owner: layer edits are not recorded yet
    let _ = t1;
    s.execute("qsave", &Value::Null).map_err(|e| e.to_string())?;
    run(&mut s, "setprop", json!({"component": c["component"], "key": "description", "value": "Contactor"}));
    project.configure(&mut s, |p| p.store.medium_mut().fail_next_flush = true)?;
    run(&mut s, "qsave", Value::Null); // fails: injected
    let head = project.read(&mut s, |p, _| p.ledger.head())?;
    let mut written = Vec::new();
    for at in [None, Some(3), Some(head / 2), Some(head.saturating_sub(1))] {
        let snap = project.snapshot(&mut s, at)?;
        let name = match at {
            None => format!("live-seq{head:03}.json"),
            Some(n) => format!("history-seq{n:03}.json"),
        };
        let path = dir.join(&name);
        std::fs::write(&path, serde_json::to_vec_pretty(&snap).map_err(|e| e.to_string())?)
            .map_err(|e| e.to_string())?;
        written.push(path.display().to_string());
    }
    println!("wrote:\n  {}", written.join("\n  "));
    Ok(())
}
