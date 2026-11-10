//! Tester contract for brief 020b (FQ2, forward compatibility): a merge index row (`versions`
//! kind 4) carrying a field a newer build added keeps it through this build's saves, and a
//! version row of a kind this build does not know (5) is ignored on open and kept on save.
#![cfg(not(target_arch = "wasm32"))]

use crate::common::*;
use crate::container_v2::TempDir;
use ecad_file::v2::schema::{VERSIONS, VersionKind, auto_name, version_key};
use ecad_file::v2::{ProjectV2, Store};
use ecad_model::*;
use redb::{ReadableDatabase, ReadableTable};

fn line(l: &mut Ledger, ids: &mut IdAllocator, page: PageId, layer: LayerId, x: f64) -> Op {
    let id: ItemId = ids.fresh();
    let item = Item { page, style: style(layer), kind: ItemKind::Primitive(Primitive::Line { a: Pt::new(x, 0.0), b: Pt::new(x, 9.0) }) };
    let op = Op::AddItem { id, item };
    l.commit(Author::User, "LINE", vec![op.clone()]).unwrap();
    op
}

fn row(p: &std::path::Path, key: &[u8]) -> Option<serde_json::Value> {
    let db = redb::ReadOnlyDatabase::open(p).unwrap();
    let r = db.begin_read().unwrap();
    let t = r.open_table(VERSIONS).unwrap();
    t.get(key).unwrap().map(|v| serde_json::from_slice(v.value()).unwrap())
}

#[test]
fn a_merge_row_keeps_a_newer_builds_field_and_an_unknown_version_kind_is_kept() {
    let dir = TempDir::new("merge-forward");
    let mut f = fixture();
    let p = dir.path("p.ecad");
    let store = Store::create(&p).unwrap();
    let mut proj = ProjectV2::create(store, PROJECT, &f.ledger, None, None, Some(1), WRITER, 2_000).unwrap().0;
    f.ledger.mark_persisted(f.ledger.head()).unwrap();
    let main = proj.branch();
    let fork = f.ledger.head();
    let (b, _) = proj.create_branch("Option B", main, fork, Some(7), None).unwrap();
    let (mut bl, _) = proj.switch_to(&f.ledger, b, SEED ^ 2).unwrap();
    let mut ids = IdAllocator::seeded(99);
    let added = line(&mut bl, &mut ids, f.page, f.layer, 50.0);
    proj.save(&mut bl).unwrap();
    let source_head = proj.head();
    let (mut ml, _) = proj.switch_to(&bl, main, SEED ^ 3).unwrap();
    let kind = TxKind::Merge {
        source: b.0,
        source_head: StepPoint { seq: source_head.0, chain: source_head.1 },
        ancestor: BranchPoint { branch: main.0, seq: fork, chain: proj.chain_of(main, fork).unwrap() },
    };
    let tx = ml.commit_merge(Author::User, "Merge \"Option B\"", kind, vec![added], None).unwrap();
    proj.close_on_save(tx, b);
    proj.save(&mut ml).unwrap();
    let merge_seq = ml.head();
    let doc = ml.document().clone();
    drop(proj);
    // A newer build's additions: a field on the merge row, and a version row of kind 5.
    let mkey = version_key(main, VersionKind::Merge, &auto_name(merge_seq));
    let mut fkey = main.0.to_vec();
    fkey.push(5);
    fkey.extend_from_slice(b"future");
    {
        let db = redb::Database::open(&p).unwrap();
        let w = db.begin_write().unwrap();
        {
            let mut t = w.open_table(VERSIONS).unwrap();
            let mut v: serde_json::Value = serde_json::from_slice(t.get(mkey.as_slice()).unwrap().expect("the merge row").value()).unwrap();
            v["future_merge"] = serde_json::json!({"kept": [1, 2]});
            t.insert(mkey.as_slice(), serde_json::to_vec(&v).unwrap().as_slice()).unwrap();
            t.insert(fkey.as_slice(), br#"{"anything": true}"#.as_slice()).unwrap();
        }
        w.commit().unwrap();
    }
    // This build opens it undamaged, edits and saves.
    let o = ProjectV2::open(Store::open(&p, true).unwrap(), SEED ^ 4, WRITER).unwrap();
    assert!(o.report.damage.is_none(), "{:?}", o.report);
    assert_eq!(o.ledger.document(), &doc);
    let mut proj = o.project;
    let mut ml = o.ledger;
    assert_eq!(proj.merge_rows(main).unwrap().len(), 1, "the merge row still reads with an extra field");
    line(&mut ml, &mut ids, f.page, f.layer, 70.0);
    proj.save(&mut ml).unwrap();
    drop(proj);
    let kept = row(&p, &mkey).expect("the merge row survives the save");
    assert_eq!(kept["future_merge"], serde_json::json!({"kept": [1, 2]}), "{kept}");
    assert_eq!(row(&p, &fkey), Some(serde_json::json!({"anything": true})), "the unknown kind's row is kept");
    let v = ecad_file::v2::verify::verify(&Store::open(&p, false).unwrap());
    assert!(v.ok, "{:#?}", v.lines);
}
