//! Brief 045c in the trial: Lists ▸ I/O is a read-only column set over the I/O rows; after a
//! commit it patches only the cards whose rows changed, and a commit touching no PLC data patches
//! nothing.

use cadcraft_engine::Session;
use ecad_cadcraft::{Project, ProjectHandle};
use ecad_trial::lists::plc::IoView;
use ecad_ui::table::TableSource;
use serde_json::json;

fn run(s: &mut Session, cmd: &str, params: serde_json::Value) -> serde_json::Value {
    s.execute(cmd, &params).unwrap_or_else(|e| panic!("{cmd}: {e}"))
}

fn sync(view: &mut IoView, s: &mut Session, p: &ProjectHandle) {
    p.configure(s, |p| {
        p.outputs();
        if let Some(o) = p.outputs.as_ref() {
            view.sync(1, p.ledger.document(), o);
        }
    })
    .unwrap();
}

#[test]
fn lists_io_shows_every_channel_and_patches_only_the_touched_card() {
    ecad_cadcraft::install();
    let mut s = Session::empty();
    let p = Project::create(&mut s, "IO", 4561).unwrap();
    run(&mut s, "place", json!({"symbol": "plccpu", "at": [50, 400]}));
    let di = run(&mut s, "place", json!({"symbol": "plcdi", "at": [100, 400]}));
    run(&mut s, "place", json!({"symbol": "plcai", "at": [140, 400]}));
    let di = di["component"].as_str().unwrap().to_string();
    let mut view = IoView::default();
    sync(&mut view, &mut s, &p);
    assert_eq!(view.source.keys().len(), 16 + 4);
    assert_eq!(view.rebuilds, 1);
    run(&mut s, "plcchannel", json!({"card": di, "channel": 2, "text": "Door closed"}));
    sync(&mut view, &mut s, &p);
    assert_eq!((view.rebuilds, view.patched), (1, 1), "one card patched");
    let key = view.source.keys()[2];
    assert_eq!(view.source.row(key).and_then(|r| r.text.clone()).as_deref(), Some("Door closed"));
    run(&mut s, "wire", json!({"points": [[0, 0], [0, 50]]}));
    sync(&mut view, &mut s, &p);
    assert_eq!((view.rebuilds, view.patched), (1, 1), "no PLC data: nothing patched");
}
