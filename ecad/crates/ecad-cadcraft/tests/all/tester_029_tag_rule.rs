//! Tester contract for brief 029 FP1 (design §10), through the app: a v2 file whose snapshot was
//! written by a build before 026 (marks without cable tags, no rule in the row) opens with
//! `ProjectHandle::open_file`, and the app's `cableadd` gives the next unused cable number, not
//! the number of a cable deleted before that snapshot.

use ecad_file::chain;
use ecad_file::v2::chunks::{META_KEY, ModelMeta};
use ecad_file::v2::schema::{self, BranchId, VersionKind};
use ecad_file::v2::{ProjectV2, Store};
use ecad_model::tag::HighWater;
use ecad_model::*;
use serde_json::json;

const PROJECT: ProjectId = ProjectId(0x0029_7e57_0000_0000_0000_0000_0000_0001);

fn tagged(tag: &str) -> ecad_model::doc::Properties {
    let mut p = ecad_model::doc::Properties::new();
    p.insert("tag".into(), tag.into());
    p
}

#[test]
fn cableadd_after_opening_a_rule_1_file_skips_the_deleted_cable_number() {
    let dir = std::env::temp_dir().join(format!("ecad-t029-tagrule-{}", ecad_store::disk::unique()));
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("old.ecad");
    let mut ids = IdAllocator::seeded(0x7029);
    let mut l = Ledger::new(0x7029);
    let (page, layer): (PageId, LayerId) = (ids.fresh(), ids.fresh());
    let (k1, x1): (ComponentId, ComponentId) = (ids.fresh(), ids.fresh());
    let (a1, t3): (TerminalId, TerminalId) = (ids.fresh(), ids.fresh());
    let wire: ConductorId = ids.fresh();
    l.commit(
        Author::User,
        "Setup",
        vec![
            Op::AddPage { id: page, page: Page::new("1"), index: 0 },
            Op::AddLayer { id: layer, layer: Layer { name: "0".into() } },
            Op::CreateComponent { id: k1, component: Component { properties: tagged("-K1"), definition: None } },
            Op::CreateComponent { id: x1, component: Component { properties: tagged("-X1"), definition: None } },
            Op::AddTerminal { id: a1, terminal: Terminal { component: k1, label: Some("A1".into()), key: None } },
            Op::AddTerminal { id: t3, terminal: Terminal { component: x1, label: Some("3".into()), key: None } },
            Op::CreateConductor { id: wire, conductor: Conductor { properties: Default::default(), ends: [Some(a1), Some(t3)] } },
        ],
    )
    .unwrap();
    let (w1, w2): (CableId, CableId) = (ids.fresh(), ids.fresh());
    l.commit(
        Author::User,
        "Cables",
        vec![
            Op::CreateCable { id: w1, cable: Cable { properties: tagged("-W01"), members: Vec::new() } },
            Op::CreateCable { id: w2, cable: Cable { properties: tagged("-W02"), members: Vec::new() } },
        ],
    )
    .unwrap();
    l.commit(Author::User, "Delete", vec![Op::DeleteCable { id: w2, cable: Cable { properties: tagged("-W02"), members: Vec::new() } }]).unwrap();
    ProjectV2::create(Store::create(&path).unwrap(), PROJECT, &l, None, None, Some(1), "tester 029", 16).unwrap();
    // As a build before 026 wrote it: device marks only (-K, -X), no rule in the row.
    {
        let store = Store::open(&path, true).unwrap();
        let main = BranchId::main_of(PROJECT.0);
        let rows = store.reader().unwrap().versions(main, VersionKind::Auto).unwrap();
        assert!(!rows.is_empty(), "the create wrote a snapshot");
        let mut w = store.writer().unwrap();
        for mut rec in rows {
            let mut high = HighWater::new();
            high.insert("K".into(), 1);
            high.insert("X".into(), 1);
            let body = serde_json::to_vec(&ModelMeta { tag_high: high }).unwrap();
            let h = schema::chunk_hash(&body);
            w.put_chunk(&h, &body).unwrap();
            rec.chunks.insert(META_KEY.into(), chain::hex(&h));
            rec.tag_rule = None;
            w.put_version(main, &rec).unwrap();
        }
        w.commit().unwrap();
    }
    ecad_cadcraft::install();
    ecad_cadcraft::views::isolate_for_tests();
    let mut s = cadcraft_engine::Session::empty();
    let (project, _) = ecad_cadcraft::ProjectHandle::open_file(&mut s, &path.display().to_string(), 5).unwrap();
    let r = s.execute("cableadd", &json!({"conductors": [wire.hex()]})).unwrap();
    let cable = r["cable"].as_str().unwrap().to_string();
    let tag = project
        .read(&mut s, |p, _| p.ledger.document().cables.iter().find(|(id, _)| id.hex() == cable).and_then(|(_, c)| c.properties.get("tag").cloned()))
        .unwrap();
    assert_eq!(tag.as_deref(), Some("-W03"), "-W02 was used before the snapshot");
    while !s.docs.is_empty() {
        s.execute("close", &json!({"index": 0})).unwrap();
    }
    drop(project);
    // Opening (and an unsaved edit) did not rewrite the old rows.
    let after = Store::open(&path, false).unwrap();
    let main = BranchId::main_of(PROJECT.0);
    assert!(after.reader().unwrap().versions(main, VersionKind::Auto).unwrap().iter().all(|v| v.tag_rule.is_none()));
    drop(after);
    let _ = std::fs::remove_dir_all(&dir);
}
