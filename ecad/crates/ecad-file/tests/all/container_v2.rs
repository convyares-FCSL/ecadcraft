//! Container version 2 (brief 021, EEP-0005): a project written to a redb file opens from its
//! latest snapshot and tail exactly as the replayed project; saves append only; damage opens
//! read-only; the append-only API refuses rewrites.
#![cfg(not(target_arch = "wasm32"))]

use std::path::PathBuf;

use crate::common;

use common::*;
use ecad_file::v2::schema::{self, BranchId, VersionKind};
use ecad_file::v2::{ProjectV2, Store, StoreError};
use ecad_model::*;

/// A fresh folder under the system temp folder, removed when dropped.
pub struct TempDir(PathBuf);

impl TempDir {
    pub fn new(name: &str) -> Self {
        let p = std::env::temp_dir().join(format!("ecad-v2-{name}-{}", ecad_store::disk::unique()));
        std::fs::create_dir_all(&p).unwrap();
        TempDir(p)
    }
    pub fn path(&self, name: &str) -> PathBuf {
        self.0.join(name)
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

/// Write `ledger` to a new file at `path` with snapshots every `every` transactions.
fn create(path: &std::path::Path, ledger: &Ledger, every: u64) -> ProjectV2 {
    let store = Store::create(path).unwrap();
    ProjectV2::create(store, PROJECT, ledger, None, None, Some(1), WRITER, every).unwrap().0
}

fn open(path: &std::path::Path) -> ecad_file::v2::OpenedV2 {
    ProjectV2::open(Store::open(path, true).unwrap(), SEED ^ 1, WRITER).unwrap()
}

/// A page with `n` lines, one transaction each, and some undo and redo.
fn edits(f: &mut Fixture, n: usize) {
    for i in 0..n {
        let id: ItemId = f.ids.fresh();
        f.commit(
            "LINE",
            vec![Op::AddItem {
                id,
                item: Item {
                    page: f.page,
                    style: style(f.layer),
                    kind: ItemKind::Primitive(Primitive::Line { a: Pt::new(i as f64, 0.0), b: Pt::new(i as f64, 9.0) }),
                },
            }],
        );
        if i % 7 == 3 {
            f.ledger.undo(Author::User).unwrap();
            f.ledger.redo(Author::User).unwrap();
        }
        if i % 7 == 5 {
            f.ledger.undo(Author::User).unwrap();
        }
    }
}

#[test]
fn a_written_project_reopens_identically_and_undoes_like_the_replayed_one() {
    let dir = TempDir::new("reopen");
    let f = fixture();
    let p = dir.path("p.ecad");
    let (_, saved) = {
        let store = Store::create(&p).unwrap();
        ProjectV2::create(store, PROJECT, &f.ledger, None, None, Some(1), WRITER, 2_000).unwrap()
    };
    assert_eq!(saved.last_seq, f.ledger.head());
    let mut o = open(&p);
    assert!(o.report.damage.is_none(), "{:?}", o.report);
    assert_eq!(o.report.snapshot_seq, f.ledger.head(), "opened from the head snapshot");
    assert_eq!(o.report.tail, 0);
    assert_eq!(o.ledger.document(), f.ledger.document());
    assert_eq!(o.ledger.stacks(), f.ledger.stacks());
    assert!(!o.ledger.is_unsaved());
    // Undo everything: every step reads its row from the file on demand.
    let mut r = f.ledger.clone();
    while r.undo(Author::User).is_ok() {
        o.ledger.undo(Author::User).unwrap();
        assert_eq!(o.ledger.document(), r.document());
    }
    assert!(o.ledger.undo(Author::User).is_err());
    for seq in 0..=f.ledger.head() {
        assert_eq!(o.ledger.state_at(seq).unwrap(), f.ledger.state_at(seq).unwrap(), "state_at {seq}");
    }
    // The undos save, and reopen at the same state.
    o.project.save(&mut o.ledger).unwrap();
    let doc = o.ledger.document().clone();
    drop(o);
    let again = open(&p);
    assert_eq!(again.ledger.document(), &doc);
}

#[test]
fn saves_append_and_snapshots_keep_the_tail_short() {
    let dir = TempDir::new("saves");
    let mut f = fixture();
    let p = dir.path("p.ecad");
    let mut proj = create(&p, &f.ledger, 16);
    let base = f.ledger.head();
    f.ledger.mark_persisted(base).unwrap();
    for round in 0..10 {
        edits(&mut f, 5);
        let s = proj.save(&mut f.ledger).unwrap();
        assert_eq!(s.last_seq, f.ledger.head(), "round {round}");
        assert!(!f.ledger.is_unsaved());
    }
    let head = f.ledger.head();
    drop(proj);
    let o = open(&p);
    assert!(o.report.damage.is_none(), "{:?}", o.report);
    assert_eq!(o.ledger.head(), head);
    assert_eq!(o.ledger.document(), f.ledger.document());
    assert_eq!(o.ledger.stacks(), f.ledger.stacks());
    assert!(o.report.snapshot_seq > base, "automatic snapshots were written");
    assert!(o.report.tail < 16, "the tail is shorter than the snapshot interval: {}", o.report.tail);
    // The whole history is still there, row by row.
    let r = o.project.store().reader().unwrap();
    assert_eq!(r.tx_count(o.project.branch()).unwrap(), head);
    let autos = r.versions(o.project.branch(), VersionKind::Auto).unwrap();
    assert!(autos.len() >= 3, "{}", autos.len());
    let v = ecad_file::v2::verify::verify(o.project.store());
    assert!(v.ok, "{:#?}", v.lines);
    assert_eq!(v.transactions, head);
    // Every automatic snapshot equals the replayed state at its point.
    let full = Ledger::replay(f.ledger.full_log().unwrap(), 0).unwrap();
    for v in autos {
        let doc = o.ledger.state_at(v.point.seq).unwrap();
        assert_eq!(doc, full.state_at(v.point.seq).unwrap(), "snapshot at {}", v.point.seq);
    }
}

#[test]
fn named_versions_and_revisions_get_rows_with_their_snapshot() {
    let dir = TempDir::new("versions");
    let mut f = fixture();
    let p = dir.path("p.ecad");
    let mut proj = create(&p, &f.ledger, 2_000);
    f.ledger.mark_persisted(f.ledger.head()).unwrap();
    edits(&mut f, 3);
    let at = f.ledger.head();
    f.commit("Name", vec![Op::NameVersion { name: "Before".into(), at }]);
    edits(&mut f, 2);
    let at2 = f.ledger.head();
    let set = |key: &str, after: &str| Op::SetProperty {
        object: ObjectRef::Project,
        key: key.into(),
        before: None,
        after: Some(after.into()),
    };
    f.commit(
        "Rev fields",
        vec![set("rev.1.rev", "A"), set("rev.1.description", "First issue"), set("rev.1.drawn", "CV")],
    );
    let at3 = f.ledger.head();
    f.commit("Issue", vec![set("rev.1.issued", "Rev A"), Op::NameVersion { name: "Rev A".into(), at: at3 }]);
    edits(&mut f, 2);
    proj.save(&mut f.ledger).unwrap();
    let r = proj.store().reader().unwrap();
    let b = proj.branch();
    let v = r.version(b, VersionKind::User, "Before").unwrap().unwrap();
    assert_eq!(v.point.seq, at);
    let rev = r.version(b, VersionKind::Revision, "Rev A").unwrap().unwrap();
    assert_eq!(rev.point.seq, at3);
    let release = rev.release.unwrap();
    assert_eq!(release.fields.get("rev").map(String::as_str), Some("A"));
    assert_eq!(release.fields.get("drawn").map(String::as_str), Some("CV"));
    assert!(at2 < at3);
    drop(r);
    drop(proj);
    let o = open(&p);
    assert_eq!(o.ledger.document(), f.ledger.document());
    let v = ecad_file::v2::verify::verify(o.project.store());
    assert!(v.ok, "{:#?}", v.lines);
    // A version's snapshot is the state at its point.
    let reader = o.project.store().reader().unwrap();
    for (kind, name, seq) in [(VersionKind::User, "Before", at), (VersionKind::Revision, "Rev A", at3)] {
        let v = reader.version(o.project.branch(), kind, name).unwrap().unwrap();
        let set = ecad_file::v2::chunks::ChunkSet::from_record(&v.chunks).unwrap();
        let bodies = set.chunks.iter().map(|(k, h)| (k.clone(), reader.chunk_body(h).unwrap())).collect::<Vec<_>>();
        let (mut doc, _) = ecad_file::v2::chunks::assemble(&set, bodies).unwrap();
        doc.rebuild_index();
        assert_eq!(doc, f.ledger.state_at(seq).unwrap(), "{name}");
    }
}

#[test]
fn the_append_only_api_refuses_rewrites_and_unknown_meta() {
    let dir = TempDir::new("append-only");
    let f = fixture();
    let p = dir.path("p.ecad");
    let proj = create(&p, &f.ledger, 2_000);
    let b = proj.branch();
    let mut w = proj.store().writer().unwrap();
    for seq in [1, f.ledger.head(), f.ledger.head() + 2] {
        assert!(matches!(w.append_tx(b, seq, &[0; 32], b"{}"), Err(StoreError::Refused(_))), "{seq}");
    }
    assert!(matches!(w.set_meta("title", &serde_json::json!("x")), Err(StoreError::Refused(_))));
    assert!(matches!(w.put_chunk(&[1; 32], b"not this"), Err(StoreError::Refused(_))));
    drop(w);
    // A stored chunk is never replaced: putting it again writes nothing.
    let mut w = proj.store().writer().unwrap();
    let body = b"{\"x\":1}";
    let h = schema::chunk_hash(body);
    assert!(w.put_chunk(&h, body).unwrap() > 0);
    assert_eq!(w.put_chunk(&h, body).unwrap(), 0);
    let asset = w.put_asset("image/png", b"\x89PNG....").unwrap();
    assert_eq!(w.put_asset("image/png", b"\x89PNG....").unwrap(), asset);
    w.commit().unwrap();
    assert_eq!(BranchId::main_of(PROJECT.0), b);
}

#[test]
fn a_damaged_tail_or_snapshot_opens_read_only_at_what_verifies() {
    let dir = TempDir::new("damaged");
    let mut f = fixture();
    let p = dir.path("p.ecad");
    let mut proj = create(&p, &f.ledger, 2_000);
    f.ledger.mark_persisted(f.ledger.head()).unwrap();
    let snap = f.ledger.head();
    edits(&mut f, 6);
    proj.save(&mut f.ledger).unwrap();
    let b = proj.branch();
    // Replace row snap+3's body through redb directly (what damage at rest looks like).
    drop(proj);
    {
        let db = redb::Database::builder().set_concurrency_mode(redb::ConcurrencyMode::SingleWriter).open(&p).unwrap();
        let w = db.begin_write().unwrap();
        {
            use redb::ReadableTable;
            let mut t = w.open_table(schema::TRANSACTIONS).unwrap();
            let key = schema::tx_key(b, snap + 3);
            let mut v = t.get(key).unwrap().unwrap().value().to_vec();
            // In the middle of the body (a Zstandard frame's last bytes are its checksum, which
            // the content's chain value makes redundant).
            let mid = 33 + (v.len() - 33) / 2;
            v[mid] ^= 0x20;
            t.insert(key, v.as_slice()).unwrap();
        }
        w.commit().unwrap();
    }
    let o = open(&p);
    let d = o.report.damage.clone().unwrap();
    assert_eq!(d.seq, Some(snap + 3), "{d:?}");
    assert_eq!(o.ledger.head(), snap + 2);
    assert!(o.project.read_only().is_some());
    assert_eq!(o.ledger.document(), &f.ledger.state_at(snap + 2).unwrap());
    let mut ledger = o.ledger;
    let mut proj = o.project;
    assert!(proj.save(&mut ledger).is_err(), "a damaged file is never written");
    assert!(o.report.describe().contains("read-only"));
}

#[test]
fn a_file_opens_from_its_bytes_in_memory() {
    // EEP-0005 §11: what a wasm build does.
    let dir = TempDir::new("bytes");
    let mut f = fixture();
    let p = dir.path("p.ecad");
    let mut proj = create(&p, &f.ledger, 16);
    f.ledger.mark_persisted(f.ledger.head()).unwrap();
    edits(&mut f, 20);
    proj.save(&mut f.ledger).unwrap();
    drop(proj);
    let bytes = std::fs::read(&p).unwrap();
    let o = ProjectV2::open(Store::from_bytes(bytes).unwrap(), 1, WRITER).unwrap();
    assert!(o.report.damage.is_none(), "{:?}", o.report);
    assert_eq!(o.ledger.document(), f.ledger.document());
    assert_eq!(o.ledger.head(), f.ledger.head());
}

/// Overseer, 2026-11-09: a snapshot records only the top 1 024 entries of each undo and redo
/// stack and how many lie below. Reopened, undo below them reads the whole stacks from the file
/// once and then undoes exactly as the replayed project; snapshots written after it are right.
#[test]
fn undo_below_the_recorded_stack_top_reads_the_history_once() {
    let dir = TempDir::new("stack-cap");
    let mut f = fixture();
    let first = f.ledger.head();
    for i in 0..1_100 {
        f.commit(
            "Property",
            vec![Op::SetProperty {
                object: ObjectRef::Project,
                key: format!("k{i}"),
                before: None,
                after: Some("v".into()),
            }],
        );
    }
    let p = dir.path("p.ecad");
    let proj = create(&p, &f.ledger, 2_000);
    let r = proj.store().reader().unwrap();
    let head = r.auto_at_or_before(proj.branch(), f.ledger.head()).unwrap().unwrap();
    assert_eq!(head.undo.len() as u64, schema::STACK_CAP);
    assert_eq!(head.undo_hidden + schema::STACK_CAP, f.ledger.stacks().0.len() as u64);
    assert_eq!(head.stack_cap, schema::STACK_CAP);
    drop(r);
    drop(proj);
    let mut o = open(&p);
    assert_eq!(o.ledger.stack_depths().0, f.ledger.stacks().0.len() as u64);
    let mut reference = f.ledger.clone();
    // Undo below the 1 024 recorded entries, all the way back.
    let mut n = 0;
    while reference.undo(Author::User).is_ok() {
        o.ledger.undo(Author::User).unwrap();
        n += 1;
        if n % 100 == 0 || reference.head() > first + 2_000 {
            assert_eq!(o.ledger.document(), reference.document(), "after {n} undos");
        }
    }
    assert!(o.ledger.undo(Author::User).is_err());
    assert_eq!(o.ledger.document(), reference.document());
    assert_eq!(o.ledger.stack_rebuilds(), 1, "the history was read whole exactly once");
    // Saving now writes a snapshot whose stacks need the whole history at the base: still right.
    o.project.set_snapshot_every(16).unwrap();
    o.project.save(&mut o.ledger).unwrap();
    let v = ecad_file::v2::verify::verify(o.project.store());
    assert!(v.ok, "{:#?}", v.lines);
    drop(o);
    let again = open(&p);
    assert_eq!(again.ledger.document(), reference.document());
    assert_eq!(again.ledger.stack_depths(), (reference.stacks().0.len() as u64, reference.stacks().1.len() as u64));
}

/// The store walks the history backwards for stack entries below what a snapshot held: the same
/// entries as slicing the stacks replayed from the root, for undos and redos mixed with edits.
#[test]
fn the_backward_walk_finds_the_same_stack_entries_as_a_full_replay() {
    let dir = TempDir::new("walk");
    let mut f = fixture();
    edits(&mut f, 60);
    for _ in 0..9 {
        f.ledger.undo(Author::User).unwrap();
    }
    f.ledger.redo(Author::User).unwrap();
    f.ledger.redo(Author::User).unwrap();
    let p = dir.path("p.ecad");
    drop(create(&p, &f.ledger, 2_000));
    let o = open(&p);
    let h = o.ledger.history().unwrap().clone();
    let at = f.ledger.head();
    let (undo, redo) = f.ledger.stacks();
    for (which, full) in [(true, &undo), (false, &redo)] {
        for (skip, count) in [(0, 1), (0, 5), (2, 3), (3, 10), (0, 10_000), (50, 7), (10_000, 4)] {
            let walked = h.stack_slice(at, which, skip, count).unwrap();
            let end = full.len().saturating_sub(skip as usize);
            let start = end.saturating_sub(count as usize);
            let seqs = |v: &[(u64, TxId)]| v.iter().map(|x| x.0).collect::<Vec<_>>();
            assert_eq!(seqs(&walked), seqs(&full[start..end]), "undo {which}, skip {skip}, count {count}");
        }
    }
}
