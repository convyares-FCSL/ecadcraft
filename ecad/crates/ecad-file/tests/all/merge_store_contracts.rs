//! Merges in a version 2 file (brief 020b, FQ1–FQ3): a merge transaction round-trips through
//! the record, the merge index (`versions` kind `merge`) and the source's `merged` status follow
//! the merge, its undo and its redo through Save, and a branch's first save writes its first
//! snapshot (so switching to it costs about an open).
#![cfg(not(target_arch = "wasm32"))]

use crate::common::*;
use crate::container_v2::TempDir;
use ecad_file::v2::schema::{BranchStatus, VersionKind};
use ecad_file::v2::{ProjectV2, Store};
use ecad_model::*;

fn create(path: &std::path::Path, ledger: &Ledger) -> ProjectV2 {
    let store = Store::create(path).unwrap();
    ProjectV2::create(store, PROJECT, ledger, None, None, Some(1), WRITER, 2_000).unwrap().0
}

fn open(path: &std::path::Path) -> ecad_file::v2::OpenedV2 {
    ProjectV2::open(Store::open(path, true).unwrap(), SEED ^ 1, WRITER).unwrap()
}

fn line(l: &mut Ledger, ids: &mut IdAllocator, page: PageId, layer: LayerId, x: f64) -> Op {
    let id: ItemId = ids.fresh();
    let item = Item { page, style: style(layer), kind: ItemKind::Primitive(Primitive::Line { a: Pt::new(x, 0.0), b: Pt::new(x, 9.0) }) };
    let op = Op::AddItem { id, item };
    l.commit(Author::User, "LINE", vec![op.clone()]).unwrap();
    op
}

#[test]
fn the_merge_index_and_the_merged_status_follow_merge_undo_and_redo() {
    let dir = TempDir::new("merge-index");
    let mut f = fixture();
    let p = dir.path("p.ecad");
    let mut proj = create(&p, &f.ledger);
    f.ledger.mark_persisted(f.ledger.head()).unwrap();
    let main = proj.branch();
    let fork = f.ledger.head();
    let (b, _) = proj.create_branch("Option B", main, fork, Some(7), None).unwrap();
    let (mut bl, _) = proj.switch_to(&f.ledger, b, SEED ^ 2).unwrap();
    let mut ids = IdAllocator::seeded(99);
    let added = line(&mut bl, &mut ids, f.page, f.layer, 50.0);
    line(&mut bl, &mut ids, f.page, f.layer, 51.0);
    proj.save(&mut bl).unwrap();
    // The branch's first save wrote its first snapshot.
    let autos = proj.store().reader().unwrap().versions(b, VersionKind::Auto).unwrap();
    assert_eq!(autos.iter().map(|v| v.point.seq).collect::<Vec<_>>(), vec![2], "a snapshot at the branch's first save");
    let source_head = proj.head();
    // On main: one merge transaction.
    let (mut ml, _) = proj.switch_to(&bl, main, SEED ^ 3).unwrap();
    let kind = TxKind::Merge {
        source: b.0,
        source_head: StepPoint { seq: source_head.0, chain: source_head.1 },
        ancestor: BranchPoint { branch: main.0, seq: fork, chain: proj.chain_of(main, fork).unwrap() },
    };
    let tx = ml.commit_merge(Author::User, "Merge \"Option B\"", kind, vec![added], None).unwrap();
    proj.close_on_save(tx, b);
    assert_eq!(proj.merge_index(&ml).unwrap().len(), 1, "an unsaved merge is in the index already");
    proj.save(&mut ml).unwrap();
    let merge_seq = ml.head();
    let rows = proj.merge_rows(main).unwrap();
    assert_eq!(rows.len(), 1);
    assert_eq!((rows[0].seq, rows[0].source, rows[0].source_head.0), (merge_seq, b, 2));
    assert_eq!(rows[0].ancestor, (main, fork, proj.chain_of(main, fork).unwrap()));
    let brow = proj.store().reader().unwrap().branch(b).unwrap().unwrap();
    assert_eq!(brow.status, BranchStatus::Merged);
    assert_eq!(brow.merged.as_ref().map(|m| (m.into.clone(), m.seq)), Some((main.hex(), merge_seq)));
    // Undo: the row goes and the branch reopens; the merge kind round-trips through the file.
    ml.undo(Author::User).unwrap();
    assert!(proj.merge_index(&ml).unwrap().is_empty());
    proj.save(&mut ml).unwrap();
    assert!(proj.merge_rows(main).unwrap().is_empty());
    assert_eq!(proj.store().reader().unwrap().branch(b).unwrap().unwrap().status, BranchStatus::Open);
    // Redo: the row is back at the redo's step, and the branch is closed again.
    ml.redo(Author::User).unwrap();
    proj.save(&mut ml).unwrap();
    let redo_seq = ml.head();
    assert_eq!(proj.merge_rows(main).unwrap().iter().map(|m| m.seq).collect::<Vec<_>>(), vec![redo_seq]);
    assert_eq!(proj.store().reader().unwrap().branch(b).unwrap().unwrap().status, BranchStatus::Merged);
    // Another session: the stored merge reads back with its kind; undo then redo across saves.
    drop(proj);
    let o = open(&p);
    let mut proj = o.project;
    let mut ml = o.ledger;
    let stored = proj.tx_display(main, merge_seq, merge_seq).unwrap();
    assert_eq!(stored[0].kind, kind, "the merge kind round-trips");
    ml.undo(Author::User).unwrap();
    proj.save(&mut ml).unwrap();
    assert!(proj.merge_rows(main).unwrap().is_empty());
    drop(proj);
    let o = open(&p);
    let mut proj = o.project;
    let mut ml = o.ledger;
    ml.redo(Author::User).unwrap();
    proj.save(&mut ml).unwrap();
    assert_eq!(
        proj.merge_rows(main).unwrap().iter().map(|m| m.seq).collect::<Vec<_>>(),
        vec![ml.head()],
        "a redo in a later session finds its merge"
    );
    drop(proj);
    let v = ecad_file::v2::verify::verify(&Store::open(&p, false).unwrap());
    assert!(v.ok, "{:#?}", v.lines);
}

#[test]
fn an_older_reader_refuses_a_merge_record_and_a_bad_merge_kind_is_refused() {
    let mut l = Ledger::new(SEED);
    let kind = TxKind::Edit;
    assert!(l.commit_merge(Author::User, "x", kind, vec![Op::AddPage { id: PageId(1), page: Page::new("1"), index: 0 }], None).is_err());
    let merge = TxKind::Merge {
        source: [1; 16],
        source_head: StepPoint { seq: 3, chain: [2; 32] },
        ancestor: BranchPoint { branch: [3; 16], seq: 1, chain: [4; 32] },
    };
    l.commit_merge(Author::User, "Merge", merge, vec![Op::AddPage { id: PageId(1), page: Page::new("1"), index: 0 }], None).unwrap();
    assert_eq!(l.undo_top().map(|t| t.kind), Some(merge));
    let tx = l.log()[0].clone();
    let v = ecad_file::record::tx_value(&tx, &[0; 32]).unwrap();
    assert_eq!(v["kind"]["merge"]["source"], serde_json::json!("01".repeat(16)));
    assert_eq!(v["kind"]["merge"]["source_head"]["seq"], serde_json::json!(3));
    let back = ecad_file::record::decode_tx(&v).unwrap();
    assert_eq!(back.kind, merge);
    let mut bad = v.clone();
    bad["kind"]["merge"]["source"] = serde_json::json!("xyz");
    assert!(ecad_file::record::decode_tx(&bad).is_err());
}

#[test]
fn verify_refuses_a_main_row_that_open_refuses() {
    let dir = TempDir::new("merge-verify-main");
    let f = fixture();
    let p = dir.path("p.ecad");
    let proj = create(&p, &f.ledger);
    let main = proj.branch();
    let mut rec = proj.store().reader().unwrap().branch(main).unwrap().unwrap();
    rec.status = BranchStatus::Merged;
    let mut w = proj.store().writer().unwrap();
    w.put_branch(main, &rec).unwrap();
    w.commit().unwrap();
    drop(proj);
    let v = ecad_file::v2::verify::verify(&Store::open(&p, false).unwrap());
    assert!(!v.ok && v.lines.iter().any(|l| l.contains("main")), "{:#?}", v.lines);
    assert!(ProjectV2::open(Store::open(&p, true).unwrap(), 1, WRITER).is_err());
}
