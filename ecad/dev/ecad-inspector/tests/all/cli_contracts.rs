//! `ecadinspect`'s text views (brief 021; moved from `ecad-cli` by brief 029): the views show the
//! tables, branches, log, versions and chunks; `export-jsonl` round-trips into a version 1
//! history; a version 1 file and a damaged history are refused in words.

use crate::common::*;
use ecad_model::*;

#[test]
fn ecadinspect_refuses_a_version_1_file_and_points_at_the_converter() {
    let dir = TempDir::new("v1");
    let src = dir.path("demo.ecad");
    std::fs::write(&src, v1_bytes(&history())).unwrap();
    let e = run(&[src.to_str().unwrap(), "tables"]).unwrap_err();
    assert!(e.contains("ecad upgrade"), "{e}");
}

/// A crafted row at `u64::MAX` (moved from `ecad-file`'s `container_v2`): the export reports the
/// gap instead of walking 2^52 empty ranges.
#[test]
fn export_refuses_a_gap_up_to_the_largest_sequence_number() {
    let dir = TempDir::new("max-seq");
    let out = upgraded(&dir, &history());
    let branch = ecad_file::v2::schema::BranchId::main_of(PROJECT.0);
    {
        let db = redb::Database::open(&out).unwrap();
        let w = db.begin_write().unwrap();
        {
            let mut t = w.open_table(ecad_file::v2::schema::TRANSACTIONS).unwrap();
            let value = ecad_file::v2::schema::tx_value(0, &[0; 32], b"{}");
            t.insert(ecad_file::v2::schema::tx_key(branch, u64::MAX), value.as_slice()).unwrap();
        }
        w.commit().unwrap();
    }
    let e = run(&[out.to_str().unwrap(), "export-jsonl"]).unwrap_err();
    assert!(e.contains("not all stored"), "{e}");
}

#[test]
fn ecadinspect_shows_every_table_and_export_round_trips_into_version_1() {
    let dir = TempDir::new("inspect");
    let l = history();
    let out = upgraded(&dir, &l);
    let o = out.to_str().unwrap();
    let tables = run(&[o, "tables"]).unwrap();
    for t in ["meta", "branches", "transactions", "versions", "chunks", "assets", "link_v1", "json/1"] {
        assert!(tables.contains(t), "{t}: {tables}");
    }
    let branches = run(&[o, "branches"]).unwrap();
    assert!(branches.contains("main") && branches.contains(&format!("head {}", l.head())), "{branches}");
    let log = run(&[o, "log", "main", "--last", "3"]).unwrap();
    assert_eq!(log.matches("\n# ").count() + usize::from(log.starts_with("# ")), 3, "{log}");
    assert!(log.contains("\"type\":\"tx\""), "{log}");
    let versions = run(&[o, "versions"]).unwrap();
    assert!(versions.contains("\"Ten\"") && versions.contains("Auto"), "{versions}");
    let r = ecad_file::v2::Store::open(&out, false).unwrap();
    let (hash, _, _) = r.reader().unwrap().chunk_list().unwrap()[0];
    drop(r);
    let chunk = run(&[o, "chunk", &ecad_file::chain::hex(&hash)]).unwrap();
    assert!(chunk.contains("hash verified"), "{chunk}");
    let verify = run(&[o, "verify"]).unwrap();
    assert!(verify.trim_end().ends_with("OK"), "{verify}");
    // export-jsonl is a version 1 file with the same history and chain.
    let exported = run(&[o, "export-jsonl"]).unwrap();
    let back = ecad_file::open_project(exported.as_bytes(), 0).unwrap();
    assert_eq!(back.report.outcome, ecad_file::Outcome::Clean);
    assert_eq!(back.ledger.log(), l.log());
    assert_eq!(back.ledger.document(), l.document());
    // --with-raw: every stored row, with its raw bytes and readable form.
    let raw_path = dir.path("raw.jsonl");
    let said = run(&[o, "export-jsonl", "--with-raw", "--out", raw_path.to_str().unwrap()]).unwrap();
    assert!(said.contains("wrote"), "{said}");
    let raw = std::fs::read_to_string(&raw_path).unwrap();
    let mut tables_seen = std::collections::BTreeSet::new();
    for line in raw.lines() {
        let v: serde_json::Value = serde_json::from_str(line).unwrap();
        tables_seen.insert(v["table"].as_str().unwrap().to_string());
        assert!(v["stored"].as_str().is_some() && !v["decoded"].is_null(), "{line}");
    }
    for t in ["meta", "branches", "transactions", "versions", "chunks"] {
        assert!(tables_seen.contains(t), "{t}");
    }
}

/// Tester, brief 020a: `ecadinspect` on a file with branches (one forked from a version, one
/// archived, a branch of a branch): `verify` passes, `branches` shows each fork, `log` reads a
/// branch by name and `versions` lists a branch's own version.
#[test]
fn ecadinspect_verifies_and_shows_a_file_with_branches() {
    use ecad_file::v2::{ProjectV2, Store};
    let dir = TempDir::new("inspect-branches");
    let mut l = history();
    let out = upgraded(&dir, &l);
    let o = out.to_str().unwrap();
    let ten = l.document().versions["Ten"];
    {
        let opened = ProjectV2::open(Store::open(&out, true).unwrap(), 3, "tester").unwrap();
        let mut proj = opened.project;
        l = opened.ledger;
        let main = proj.branch();
        let (b, _) = proj.create_branch("Option B", main, ten, None, None).unwrap();
        let (mut bl, _) = proj.switch_to(&l, b, 4).unwrap();
        let page = bl.document().page_order[0];
        let layer = *bl.document().layers.keys().next().unwrap();
        let mut ids = IdAllocator::seeded(99);
        for i in 0..40 {
            let id: ItemId = ids.fresh();
            let style = Style { layer, color: Color::ByLayer, linetype: "ByLayer".into(), lineweight: -1 };
            let kind = ItemKind::Primitive(Primitive::Line { a: Pt::new(100.0 + i as f64, 0.0), b: Pt::new(0.0, 1.0) });
            bl.commit(Author::User, "LINE", vec![Op::AddItem { id, item: Item { page, style, kind } }]).unwrap();
        }
        let at = bl.head();
        bl.commit(Author::User, "Name", vec![Op::NameVersion { name: "On B".into(), at, description: Some("why".into()) }]).unwrap();
        proj.save(&mut bl).unwrap();
        let (c, _) = proj.create_branch("Deeper", b, 5, None, None).unwrap();
        let (gone, _) = proj.create_branch("Gone", main, 3, None, None).unwrap();
        proj.set_branch_open(gone, false).unwrap();
        let (mut cl, _) = proj.switch_to(&bl, c, 5).unwrap();
        let id: ItemId = ids.fresh();
        let style = Style { layer, color: Color::ByLayer, linetype: "ByLayer".into(), lineweight: -1 };
        cl.commit(
            Author::User,
            "LINE",
            vec![Op::AddItem {
                id,
                item: Item { page, style, kind: ItemKind::Primitive(Primitive::Line { a: Pt::new(1.0, 1.0), b: Pt::new(2.0, 2.0) }) },
            }],
        )
        .unwrap();
        proj.save(&mut cl).unwrap();
    }
    let verify = run(&[o, "verify"]).unwrap();
    assert!(verify.trim_end().ends_with("OK"), "{verify}");
    let branches = run(&[o, "branches"]).unwrap();
    assert!(branches.contains("Option B") && branches.contains(&format!("after {ten}")), "{branches}");
    assert!(branches.contains("Deeper") && branches.contains("after 5") && branches.contains("Archived"), "{branches}");
    let log = run(&[o, "log", "Option B", "--last", "2"]).unwrap();
    assert!(log.contains("# 41:") && log.contains("# 40:"), "{log}");
    let versions = run(&[o, "versions"]).unwrap();
    assert!(versions.contains("Option B User \"On B\" at 40"), "{versions}");
    // export-jsonl of a branch: its own records, the others named on stderr.
    let (mut lines, mut notes) = (Vec::new(), Vec::new());
    ecad_inspector::cli::ecadinspect_with(&[o.into(), "export-jsonl".into(), "--branch".into(), "Option B".into()], &mut lines, &mut notes).unwrap();
    assert_eq!(String::from_utf8(lines).unwrap().lines().count(), 1 + 41, "a header and the branch's 41 records");
    let notes = String::from_utf8(notes).unwrap();
    assert!(notes.contains("left out: 3 other branch(es)") && notes.contains("main (") && notes.contains("Gone ("), "{notes}");
    // A second branch named "Gone" (the first is archived, so the name is free): a name is no
    // longer enough, an ID is.
    let ids: Vec<(String, String)> = {
        let opened = ProjectV2::open(Store::open(&out, true).unwrap(), 6, "tester").unwrap();
        let mut proj = opened.project;
        let main = BranchIdOf::main(&proj);
        proj.create_branch("Gone", main, 2, None, None).unwrap();
        proj.branches().unwrap().into_iter().map(|(id, b)| (id.hex(), b.name)).collect()
    };
    let e = run(&[o, "log", "Gone"]).unwrap_err();
    let gone: Vec<&String> = ids.iter().filter(|(_, n)| n == "Gone").map(|(id, _)| id).collect();
    assert_eq!(gone.len(), 2);
    assert!(e.contains("2 branches are named \"Gone\"") && gone.iter().all(|id| e.contains(id.as_str())), "{e}");
    let by_id = run(&[o, "log", gone[0].as_str()]).unwrap();
    assert!(!by_id.contains("no branch"), "{by_id}");
    let b_id = ids.iter().find(|(_, n)| n == "Option B").map(|(id, _)| id.clone()).unwrap();
    let log = run(&[o, "log", b_id.as_str(), "--last", "1"]).unwrap();
    assert!(log.contains("# 41:"), "{log}");
}

/// `main`'s ID of a project.
struct BranchIdOf;
impl BranchIdOf {
    fn main(p: &ecad_file::v2::ProjectV2) -> ecad_file::v2::schema::BranchId {
        ecad_file::v2::schema::BranchId::main_of(p.project_id().0)
    }
}
