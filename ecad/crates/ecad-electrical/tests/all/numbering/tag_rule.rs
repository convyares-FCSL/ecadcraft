//! Tag rule (brief 029, FP1): a cable tag deleted before a snapshot written by a build before brief
//! 026 (whose cached marks did not count cable tags) is not handed out again after opening.

use ecad_file::chain;
use ecad_file::chunks::{META_KEY, ModelMeta};
use ecad_file::schema::{self, BranchId, RowKind};
use ecad_file::{Project, Store};
use ecad_model::naming::tag::HighWater;
use ecad_model::*;
use {ecad_electrical::devices::tags::TagHigh, ecad_electrical::devices::tags::new_cable_tag};

const PROJECT: ProjectId = ProjectId(0x0029_0000_0000_0000_0000_0000_0000_0007);

fn cable(tag: &str) -> Cable {
    Cable { tag: ecad_model::naming::tag::TagSlot::of_text(Some(tag), Default::default()), cores: Vec::new(), ..Default::default() }
}

#[test]
fn a_cable_tag_deleted_before_a_rule_1_snapshot_is_not_handed_out_again() {
    let dir = std::env::temp_dir().join(format!("ecad-engine-tag-rule-{}", ecad_store::disk::unique()));
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("p.ecad");
    // -W01 and -W02, -W02 deleted, then the snapshot.
    let mut l = Ledger::new(29);
    let (w1, w2) = (CableId(1), CableId(2));
    l.commit(Author::User, "Cables", vec![Op::CreateCable { id: w1, cable: cable("-W01") }, Op::CreateCable { id: w2, cable: cable("-W02") }])
        .unwrap();
    l.commit(Author::User, "Delete", vec![Op::DeleteCable { id: w2, cable: cable("-W02") }]).unwrap();
    Project::create(Store::create(&path).unwrap(), PROJECT, &l, Some(1), "test", 16).unwrap();
    // As a build before brief 026 wrote it: marks without cable tags, and no rule on the row.
    {
        let store = Store::open(&path, true).unwrap();
        let main = BranchId::main_of(PROJECT.0);
        let rows = store.reader().unwrap().versions(main, RowKind::Auto).unwrap();
        assert!(!rows.is_empty());
        let mut w = store.writer().unwrap();
        for mut rec in rows {
            let body = serde_json::to_vec(&ModelMeta { tag_high: HighWater::new() }).unwrap();
            let h = schema::chunk_hash(&body);
            w.put_chunk(&h, &body).unwrap();
            rec.chunks.insert(META_KEY.into(), chain::hex(&h));
            rec.tag_rule = None;
            w.put_version(main, &rec).unwrap();
        }
        w.commit().unwrap();
    }
    let o = Project::open(Store::open(&path, false).unwrap(), 1, "test").unwrap();
    assert!(o.report.snapshot_seq > 0, "opened from the snapshot");
    // With the cached rule 1 marks, -W02 would be given again (the only guard is the mark: no
    // cable holds -W02 now).
    let mut stale = TagHigh::starting_at(HighWater::new());
    assert_eq!(new_cable_tag(&o.ledger, &mut stale, None).unwrap().shown(), "-W02");
    // As opened: the marks are recomputed from the history.
    let mut high = TagHigh::starting_at(o.project.base_tag_high().clone());
    assert_eq!(new_cable_tag(&o.ledger, &mut high, None).unwrap().shown(), "-W03");
    drop(o);
    let _ = std::fs::remove_dir_all(&dir);
}
