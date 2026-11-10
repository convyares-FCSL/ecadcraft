//! Branches in a version 2 file (brief 020a, EEP-0005 §3.3, §5): a branch forks from a point of
//! its parent, owns its log, opens from its parent's state at the fork or its own snapshot, and
//! the project reopens on the branch last shown (`meta.active`). Branch actions write at once;
//! readers ignore unknown optional fields (forward compatibility).
#![cfg(not(target_arch = "wasm32"))]

use crate::common::*;
use crate::container_v2::TempDir;
use ecad_file::v2::project::BranchError;
use ecad_file::v2::schema::{BranchId, BranchStatus, VersionKind};
use ecad_file::v2::{ProjectV2, Store};
use ecad_model::*;
use redb::{ReadableDatabase, ReadableTable};

fn create(path: &std::path::Path, ledger: &Ledger, every: u64) -> ProjectV2 {
    let store = Store::create(path).unwrap();
    ProjectV2::create(store, PROJECT, ledger, None, None, Some(1), WRITER, every).unwrap().0
}

fn open(path: &std::path::Path) -> ecad_file::v2::OpenedV2 {
    ProjectV2::open(Store::open(path, true).unwrap(), SEED ^ 1, WRITER).unwrap()
}

/// One line on the fixture's page.
fn line(l: &mut Ledger, ids: &mut IdAllocator, page: PageId, layer: LayerId, x: f64) {
    let id: ItemId = ids.fresh();
    let item = Item { page, style: style(layer), kind: ItemKind::Primitive(Primitive::Line { a: Pt::new(x, 0.0), b: Pt::new(x, 9.0) }) };
    l.commit(Author::User, "LINE", vec![Op::AddItem { id, item }]).unwrap();
}

#[test]
fn a_branch_forks_owns_its_log_and_leaves_main_unchanged() {
    let dir = TempDir::new("branch");
    let mut f = fixture();
    let p = dir.path("p.ecad");
    let mut proj = create(&p, &f.ledger, 2_000);
    f.ledger.mark_persisted(f.ledger.head()).unwrap();
    let main = proj.branch();
    let fork = f.ledger.head();
    let main_doc = f.ledger.document().clone();
    let (b, rec) = proj.create_branch("Option B", main, fork, Some(7), Some("CVA".into())).unwrap();
    assert_eq!(rec.parent.as_deref(), Some(main.hex().as_str()));
    assert_eq!(rec.fork_point.as_ref().map(|p| p.seq), Some(fork));
    assert_eq!(rec.status, BranchStatus::Open);
    assert!(!rec.release_line);
    // Written at once: a second reader sees the row.
    assert_eq!(proj.store().reader().unwrap().branches().unwrap().len(), 2);
    // Switch: the branch starts from main's state at the fork, with empty stacks.
    let (mut bl, report) = proj.switch_to(&f.ledger, b, SEED ^ 2).unwrap();
    assert!(report.damage.is_none(), "{report:?}");
    assert_eq!(proj.branch(), b);
    assert_eq!(bl.head(), 0);
    assert_eq!(bl.document(), &main_doc);
    assert!(bl.undo(Author::User).is_err(), "undo is per branch: nothing to undo on a new branch");
    let mut ids = IdAllocator::seeded(99);
    line(&mut bl, &mut ids, f.page, f.layer, 50.0);
    line(&mut bl, &mut ids, f.page, f.layer, 51.0);
    let branch_doc = bl.document().clone();
    // Switching with unsaved steps is refused; saving first works.
    assert!(matches!(proj.switch_to(&bl, main, 1), Err(BranchError::Refused(_))));
    proj.save(&mut bl).unwrap();
    assert_eq!(proj.store().reader().unwrap().tx_count(b).unwrap(), 2);
    assert_eq!(proj.store().reader().unwrap().tx_count(main).unwrap(), fork, "main's log is untouched");
    // The project reopens on the branch last shown.
    drop(proj);
    let o = open(&p);
    assert_eq!(o.project.branch(), b);
    assert_eq!(o.project.branch_record().name, "Option B");
    assert_eq!(o.ledger.document(), &branch_doc);
    assert_eq!(o.ledger.state_at(0).unwrap(), main_doc, "state_at 0 on a branch is the fork point's state");
    let mut proj = o.project;
    // Back to main: unchanged.
    let (ml, _) = proj.switch_to(&o.ledger, main, 3).unwrap();
    assert_eq!(ml.document(), &main_doc);
    drop(proj);
    assert_eq!(open(&p).project.branch(), main, "main is now the active branch");
    let v = ecad_file::v2::verify::verify(&Store::open(&p, false).unwrap());
    assert!(v.ok, "{:#?}", v.lines);
}

#[test]
fn a_branch_from_an_older_point_snapshots_and_reads_its_ancestry() {
    let dir = TempDir::new("older");
    let mut f = fixture();
    let p = dir.path("p.ecad");
    let mut proj = create(&p, &f.ledger, 16);
    f.ledger.mark_persisted(f.ledger.head()).unwrap();
    let main = proj.branch();
    let (page, layer) = (f.page, f.layer);
    for i in 0..20 {
        line(&mut f.ledger, &mut f.ids, page, layer, f64::from(i));
    }
    proj.save(&mut f.ledger).unwrap();
    let at = f.ledger.head() - 7;
    let then = f.ledger.state_at(at).unwrap();
    let (b, _) = proj.create_branch("Old", main, at, None, None).unwrap();
    let (mut bl, _) = proj.switch_to(&f.ledger, b, 5).unwrap();
    assert_eq!(bl.document(), &then);
    let mut ids = IdAllocator::seeded(7);
    for i in 0..40 {
        line(&mut bl, &mut ids, page, layer, 100.0 + f64::from(i));
    }
    proj.save(&mut bl).unwrap();
    // A version on the branch, with a description (FQ5).
    let vat = bl.head();
    bl.commit(Author::User, "Create version", vec![Op::NameVersion { name: "Branch v1".into(), at: vat, description: Some("why".into()) }]).unwrap();
    proj.save(&mut bl).unwrap();
    let r = proj.store().reader().unwrap();
    let autos = r.versions(b, VersionKind::Auto).unwrap();
    assert!(!autos.is_empty(), "automatic snapshots on the branch");
    let user = r.versions(b, VersionKind::User).unwrap();
    assert_eq!(user.len(), 1);
    assert_eq!(user[0].description.as_deref(), Some("why"));
    assert!(r.versions(main, VersionKind::User).unwrap().iter().all(|v| v.name != "Branch v1"), "versions are branch-local");
    drop(r);
    let full: Vec<Document> = (0..=bl.head()).map(|s| bl.state_at(s).unwrap()).collect();
    drop(proj);
    // Reopened from the branch's own snapshot; older points read through the ancestry.
    let o = open(&p);
    assert!(o.report.snapshot_seq > 0, "{:?}", o.report);
    for (s, d) in full.iter().enumerate() {
        assert_eq!(&o.ledger.state_at(s as u64).unwrap(), d, "state_at {s}");
        assert_eq!(&o.project.point_state(b, s as u64).unwrap(), d, "point_state {s}");
    }
    assert_eq!(o.project.point_state(main, at).unwrap(), then);
    // A branch of the branch, from its step 3.
    let mut proj = o.project;
    let (c, _) = proj.create_branch("Deeper", b, 3, None, None).unwrap();
    assert_eq!(proj.point_state(c, 0).unwrap(), full[3]);
    assert_eq!(proj.lineage().len(), 2);
    let (cl, _) = proj.switch_to(&o.ledger, c, 9).unwrap();
    assert_eq!(cl.document(), &full[3]);
    assert_eq!(proj.lineage().len(), 3);
    let v = ecad_file::v2::verify::verify(proj.store());
    assert!(v.ok, "{:#?}", v.lines);
}

fn refused<T>(r: std::result::Result<T, BranchError>) -> bool {
    matches!(r, Err(BranchError::Refused(_)))
}

#[test]
fn branch_names_and_states_follow_the_rules() {
    let dir = TempDir::new("names");
    let f = fixture();
    let p = dir.path("p.ecad");
    let mut proj = create(&p, &f.ledger, 2_000);
    let main = proj.branch();
    let head = f.ledger.head();
    assert!(refused(proj.create_branch("  ", main, head, None, None)));
    assert!(refused(proj.create_branch("Main", main, head, None, None)));
    assert!(refused(proj.create_branch(&"x".repeat(61), main, head, None, None)));
    assert!(refused(proj.create_branch("a\u{7}b", main, head, None, None)));
    assert!(refused(proj.create_branch("B", main, head + 1, None, None)), "no such step");
    let (b, rec) = proj.create_branch("  Option B ", main, head, None, None).unwrap();
    assert_eq!(rec.name, "Option B", "trimmed");
    assert!(refused(proj.create_branch("option b", main, head, None, None)), "unique among open branches, any case");
    assert!(refused(proj.rename_branch(main, "Trunk")));
    assert!(refused(proj.set_branch_open(main, false)));
    proj.rename_branch(b, "Option C").unwrap();
    proj.set_branch_open(b, false).unwrap();
    assert_eq!(proj.branches().unwrap()[1].1.status, BranchStatus::Archived);
    assert!(refused(proj.switch_to(&f.ledger, b, 1)), "an archived branch is reopened first");
    // Another open branch takes the name: reopening asks for a new one.
    proj.create_branch("Option C", main, head, None, None).unwrap();
    assert_eq!(proj.set_branch_open(b, true), Err(BranchError::NameTaken("Option C".into())));
    proj.rename_branch(b, "Option D").unwrap();
    assert_eq!(proj.set_branch_open(b, true).unwrap().status, BranchStatus::Open);
    // The branch shown cannot be archived.
    let (_, _) = proj.switch_to(&f.ledger, b, 1).unwrap();
    assert!(refused(proj.set_branch_open(b, false)));
    let ids: Vec<BranchId> = proj.branches().unwrap().into_iter().map(|(id, _)| id).collect();
    assert_eq!(ids.first(), Some(&main), "main is listed first");
}

#[test]
fn readers_ignore_unknown_optional_fields_in_meta_branch_and_version_rows() {
    let dir = TempDir::new("forward");
    let f = fixture();
    let p = dir.path("p.ecad");
    let proj = create(&p, &f.ledger, 2_000);
    let main = proj.branch();
    drop(proj);
    {
        // A newer build's additions, written straight into the tables.
        let db = redb::Database::open(&p).unwrap();
        let w = db.begin_write().unwrap();
        {
            let mut meta = w.open_table(ecad_file::v2::schema::META).unwrap();
            meta.insert("future_name", br#"{"anything": 1}"#.as_slice()).unwrap();
            let mut branches = w.open_table(ecad_file::v2::schema::BRANCHES).unwrap();
            let row = branches.get(main.0).unwrap().unwrap().value().to_vec();
            let mut v: serde_json::Value = serde_json::from_slice(&row).unwrap();
            v["future_field"] = serde_json::json!({"x": [1, 2]});
            branches.insert(main.0, serde_json::to_vec(&v).unwrap().as_slice()).unwrap();
            let mut versions = w.open_table(ecad_file::v2::schema::VERSIONS).unwrap();
            let rows: Vec<(Vec<u8>, Vec<u8>)> =
                versions.iter().unwrap().map(|r| r.unwrap()).map(|(k, v)| (k.value().to_vec(), v.value().to_vec())).collect();
            for (k, row) in rows {
                let mut v: serde_json::Value = serde_json::from_slice(&row).unwrap();
                v["future_field"] = serde_json::json!("later");
                v["release_future"] = serde_json::json!(true);
                versions.insert(k.as_slice(), serde_json::to_vec(&v).unwrap().as_slice()).unwrap();
            }
        }
        w.commit().unwrap();
    }
    let o = open(&p);
    assert!(o.report.damage.is_none(), "{:?}", o.report);
    assert_eq!(o.ledger.document(), f.ledger.document());
    let v = ecad_file::v2::verify::verify(o.project.store());
    assert!(v.ok, "{:#?}", v.lines);
}

/// Put `field` into the JSON row `key` of `table` (a newer build's optional addition).
fn add_field<K: redb::Key + 'static>(p: &std::path::Path, table: redb::TableDefinition<K, &[u8]>, key: K::SelfType<'_>, field: &str)
where
    for<'a> K::SelfType<'a>: Clone,
{
    let db = redb::Database::open(p).unwrap();
    let w = db.begin_write().unwrap();
    {
        let mut t = w.open_table(table).unwrap();
        let row = t.get(key.clone()).unwrap().unwrap().value().to_vec();
        let mut v: serde_json::Value = serde_json::from_slice(&row).unwrap();
        v[field] = serde_json::json!({"kept": [1, 2]});
        t.insert(key, serde_json::to_vec(&v).unwrap().as_slice()).unwrap();
    }
    w.commit().unwrap();
}

/// Whether row `key` of `table` still holds `field`.
fn has_field<K: redb::Key + 'static>(p: &std::path::Path, table: redb::TableDefinition<K, &[u8]>, key: K::SelfType<'_>, field: &str) -> bool {
    let db = redb::ReadOnlyDatabase::open(p).unwrap();
    let r = db.begin_read().unwrap();
    let t = r.open_table(table).unwrap();
    let row = t.get(key).unwrap().unwrap().value().to_vec();
    let v: serde_json::Value = serde_json::from_slice(&row).unwrap();
    v.get(field) == Some(&serde_json::json!({"kept": [1, 2]}))
}

#[test]
fn writers_keep_the_unknown_fields_a_newer_build_wrote_through_save_switch_and_rename() {
    use ecad_file::v2::schema::{BRANCHES, META, VERSIONS, version_key};
    let dir = TempDir::new("preserve");
    let mut f = fixture();
    let p = dir.path("p.ecad");
    let proj = create(&p, &f.ledger, 16);
    let main = proj.branch();
    let named = proj.store().reader().unwrap().versions(main, VersionKind::User).unwrap();
    let vname = named.first().map(|v| v.name.clone()).unwrap();
    drop(proj);
    let vkey = version_key(main, VersionKind::User, &vname);
    add_field(&p, BRANCHES, main.0, "future_main");
    add_field(&p, META, "cache", "future_cache");
    add_field(&p, VERSIONS, vkey.as_slice(), "future_version");
    // This build: edits saved (an automatic snapshot rewrites meta.cache), a branch made, the
    // future field put on it too, then switch, rename, switch back and save again.
    let mut o = open(&p);
    let base = o.ledger.head();
    f.ledger = o.ledger;
    for i in 0..20 {
        line(&mut f.ledger, &mut f.ids, f.page, f.layer, 200.0 + f64::from(i));
    }
    o.project.save(&mut f.ledger).unwrap();
    assert!(o.project.snapshot_seq() > base, "an automatic snapshot was written, so meta.cache was");
    let (b, _) = o.project.create_branch("Next", main, f.ledger.head(), None, None).unwrap();
    drop(o.project);
    add_field(&p, BRANCHES, b.0, "future_branch");
    let mut o = open(&p);
    let (bl, _) = o.project.switch_to(&o.ledger, b, 3).unwrap();
    o.project.rename_branch(b, "Later").unwrap();
    let (mut ml, _) = o.project.switch_to(&bl, main, 4).unwrap();
    line(&mut ml, &mut f.ids, f.page, f.layer, 300.0);
    o.project.save(&mut ml).unwrap();
    drop(o.project);
    assert!(has_field(&p, BRANCHES, main.0, "future_main"), "main's row");
    assert!(has_field(&p, BRANCHES, b.0, "future_branch"), "the renamed branch's row");
    assert!(has_field(&p, META, "cache", "future_cache"), "meta.cache");
    assert!(has_field(&p, VERSIONS, vkey.as_slice(), "future_version"), "the version row");
    let v = ecad_file::v2::verify::verify(&Store::open(&p, false).unwrap());
    assert!(v.ok, "{:#?}", v.lines);
}

/// Showing a point costs its nearest snapshot plus a bounded replay, however long the history
/// (reviewer finding 2), on main and on a branch: snapshots are written by the save at or after
/// each interval, so the replay is at most the interval plus one save's steps per level.
#[test]
fn a_point_is_read_from_its_nearest_snapshot_never_further_back_than_the_interval() {
    let dir = TempDir::new("bounded");
    let mut f = fixture();
    let p = dir.path("p.ecad");
    let every = 16;
    let mut proj = create(&p, &f.ledger, every);
    f.ledger.mark_persisted(f.ledger.head()).unwrap();
    let main = proj.branch();
    let (page, layer) = (f.page, f.layer);
    for i in 0..150 {
        line(&mut f.ledger, &mut f.ids, page, layer, f64::from(i));
        if i % 10 == 9 {
            proj.save(&mut f.ledger).unwrap();
        }
    }
    proj.save(&mut f.ledger).unwrap();
    let head = f.ledger.head();
    let (b, _) = proj.create_branch("Late", main, head - 3, None, None).unwrap();
    let (mut bl, _) = proj.switch_to(&f.ledger, b, 2).unwrap();
    let mut ids = IdAllocator::seeded(5);
    for i in 0..60 {
        line(&mut bl, &mut ids, page, layer, 500.0 + f64::from(i));
        if i % 10 == 9 {
            proj.save(&mut bl).unwrap();
        }
    }
    for seq in (0..=head).step_by(23) {
        let n = proj.point_replay(main, seq).unwrap();
        assert!(n <= every + 10, "main step {seq}: {n} steps replayed");
        assert_eq!(proj.point_state(main, seq).unwrap(), f.ledger.state_at(seq).unwrap(), "main step {seq}");
    }
    for seq in 0..=bl.head() {
        let n = proj.point_replay(b, seq).unwrap();
        assert!(n <= 2 * (every + 10), "branch step {seq}: {n} steps replayed");
    }
}
