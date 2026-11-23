//! `ecadinspect`'s text views: the views show the
//! tables, branches, log, versions and chunks; a file that is not a project and a damaged history
//! are refused in words.

use crate::common::*;
use ecad_model::*;

#[test]
fn ecadinspect_refuses_a_file_that_is_not_a_project() {
    let dir = TempDir::new("not-project");
    let src = dir.path("demo.ecad");
    // A JSON Lines file, as the removed version 1 format was.
    std::fs::write(&src, br#"{"crc":"00000000","rec":{"type":"header"}}"#).unwrap();
    let e = run(&[src.to_str().unwrap(), "tables"]).unwrap_err();
    assert!(e.contains("not an ECAD project file"), "{e}");
}

#[test]
fn ecadinspect_shows_every_table() {
    let dir = TempDir::new("inspect");
    let l = history();
    let out = written(&dir, &l);
    let o = out.to_str().unwrap();
    let tables = run(&[o, "tables"]).unwrap();
    for t in ["meta", "branches", "transactions", "versions", "chunks", "assets", "json/1"] {
        assert!(tables.contains(t), "{t}: {tables}");
    }
    let branches = run(&[o, "branches"]).unwrap();
    assert!(branches.contains("main") && branches.contains(&format!("head {}", l.head())), "{branches}");
    let log = run(&[o, "log", "main", "--last", "3"]).unwrap();
    assert_eq!(log.matches("\n# ").count() + usize::from(log.starts_with("# ")), 3, "{log}");
    assert!(log.contains("\"type\":\"tx\""), "{log}");
    let versions = run(&[o, "versions"]).unwrap();
    assert!(versions.contains("\"Ten\"") && versions.contains("Auto"), "{versions}");
    let r = ecad_file::Store::open(&out, false).unwrap();
    let (hash, _, _) = r.reader().unwrap().chunk_list().unwrap()[0];
    drop(r);
    let chunk = run(&[o, "chunk", &ecad_file::chain::hex(&hash)]).unwrap();
    assert!(chunk.contains("hash verified"), "{chunk}");
    let verify = run(&[o, "verify"]).unwrap();
    assert!(verify.trim_end().ends_with("OK"), "{verify}");
}

/// Tester, brief 020a: `ecadinspect` on a file with branches (one forked from a version, one
/// archived, a branch of a branch): `verify` passes, `branches` shows each fork, `log` reads a
/// branch by name and `versions` lists a branch's own version.
#[test]
fn ecadinspect_verifies_and_shows_a_file_with_branches() {
    use ecad_file::{Project, Store};
    let dir = TempDir::new("inspect-branches");
    let mut l = history();
    let out = written(&dir, &l);
    let o = out.to_str().unwrap();
    let ten = l.document().versions()[&ecad_model::PointName::version("Ten").unwrap()];
    {
        let opened = Project::open(Store::open(&out, true).unwrap(), 3, "tester").unwrap();
        let mut proj = opened.project;
        l = opened.ledger;
        let main = proj.branch();
        let (b, _) = proj.create_branch("Option B", main, ten, None, None).unwrap();
        let (mut bl, _) = proj.switch_to(&l, b, 4).unwrap();
        let page = bl.document().page_order()[0];
        let layer = *bl.document().layers().keys().next().unwrap();
        let mut ids = IdAllocator::seeded(99);
        for i in 0..40 {
            let id: ItemId = ids.fresh();
            let style = Style { layer, color: Color::ByLayer, linetype: "ByLayer".into(), lineweight: -1 };
            let kind = ItemKind::Primitive(Primitive::Line { a: Pt::new(100.0 + i as f64, 0.0), b: Pt::new(0.0, 1.0) });
            bl.commit(Author::User, "LINE", vec![Op::AddItem { id, item: Item { page, style, kind } }]).unwrap();
        }
        let at = bl.head();
        bl.commit(
            Author::User,
            "Name",
            vec![Op::NameVersion { name: ecad_model::PointName::version("On B").unwrap(), at, description: Some("why".into()) }],
        )
        .unwrap();
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
    assert!(versions.contains("Option B Version \"On B\" at 40"), "{versions}");
    // A second branch named "Gone" (the first is archived, so the name is free): a name is no
    // longer enough, an ID is.
    let ids: Vec<(String, String)> = {
        let opened = Project::open(Store::open(&out, true).unwrap(), 6, "tester").unwrap();
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
    fn main(p: &ecad_file::Project) -> ecad_file::schema::BranchId {
        ecad_file::schema::BranchId::main_of(p.project_id().0)
    }
}
