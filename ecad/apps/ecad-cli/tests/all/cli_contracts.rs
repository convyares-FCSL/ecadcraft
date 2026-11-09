//! `ecad upgrade` and `ecadinspect` (brief 021): the converter writes a new verified file and
//! never touches the source; the views show the tables, branches, log, versions and chunks;
//! `export-jsonl` round-trips into a version 1 history.

use std::path::PathBuf;

use ecad_model::*;
use ecad_store::{Medium, MemoryMedium};

struct TempDir(PathBuf);

impl TempDir {
    fn new(name: &str) -> Self {
        let p = std::env::temp_dir().join(format!("ecad-cli-{name}-{}", ecad_store::disk::unique()));
        std::fs::create_dir_all(&p).unwrap();
        TempDir(p)
    }
    fn path(&self, name: &str) -> PathBuf {
        self.0.join(name)
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

const PROJECT: ProjectId = ProjectId(0x00c1_1000_0000_0000_0000_0000_0000_0001);

/// A small history: two pages, lines, a named version, an undo.
fn history() -> Ledger {
    let mut ids = IdAllocator::seeded(7);
    let mut l = Ledger::new(7);
    let (p1, p2, layer): (PageId, PageId, LayerId) = (ids.fresh(), ids.fresh(), ids.fresh());
    l.commit(
        Author::User,
        "New project",
        vec![
            Op::AddPage { id: p1, page: Page::new("1"), index: 0 },
            Op::AddPage { id: p2, page: Page::new("2"), index: 1 },
            Op::AddLayer { id: layer, layer: Layer { name: "0".into() } },
        ],
    )
    .unwrap();
    for i in 0..30 {
        let id: ItemId = ids.fresh();
        let page = if i % 2 == 0 { p1 } else { p2 };
        let style = Style { layer, color: Color::ByLayer, linetype: "ByLayer".into(), lineweight: -1 };
        let kind = ItemKind::Primitive(Primitive::Line { a: Pt::new(i as f64, 0.0), b: Pt::new(0.0, i as f64) });
        l.commit(Author::User, "LINE", vec![Op::AddItem { id, item: Item { page, style, kind } }]).unwrap();
        if i == 10 {
            let at = l.head();
            l.commit(Author::User, "Name", vec![Op::NameVersion { name: "Ten".into(), at }]).unwrap();
        }
    }
    l.undo(Author::User).unwrap();
    l
}

fn v1_bytes(l: &Ledger) -> Vec<u8> {
    let f = ecad_file::ProjectFile::write_new(MemoryMedium::default(), PROJECT, l, Some(5), "test").unwrap();
    f.into_medium().read_all().unwrap()
}

fn run(f: fn(&[String], &mut dyn std::io::Write) -> std::result::Result<(), String>, args: &[&str]) -> std::result::Result<String, String> {
    let args: Vec<String> = args.iter().map(|s| s.to_string()).collect();
    let mut out = Vec::new();
    f(&args, &mut out).map(|()| String::from_utf8(out).unwrap())
}

#[test]
fn upgrade_writes_a_new_verified_file_and_never_touches_the_source() {
    let dir = TempDir::new("upgrade");
    let l = history();
    let src = dir.path("demo.ecad");
    let bytes = v1_bytes(&l);
    std::fs::write(&src, &bytes).unwrap();
    let s = src.to_str().unwrap();
    let said = run(ecad_cli::ecad, &["upgrade", s]).unwrap();
    assert!(said.contains("Verified"), "{said}");
    assert_eq!(std::fs::read(&src).unwrap(), bytes, "the source is unchanged");
    let out = dir.path("demo.v2.ecad");
    assert!(out.is_file());
    // Never overwritten.
    let again = run(ecad_cli::ecad, &["upgrade", s]).unwrap_err();
    assert!(again.contains("never overwritten"), "{again}");
    // It opens as version 2 with the same document and the same final hash.
    let o = ecad_file::v2::disk::DiskProjectV2::open(&out, 1, "test").unwrap();
    assert_eq!(o.ledger.document(), l.document());
    let link = o.project.project().link_v1().unwrap().clone();
    assert_eq!(link.transactions, l.head());
    let v1 = ecad_file::open_project(&bytes, 0).unwrap();
    assert_eq!(link.hash, ecad_file::chain::hex(&v1.chain_head));
    drop(o);
    // A corrupt source is refused, and nothing is written.
    let mut bad = bytes.clone();
    let mid = bad.len() / 2;
    bad[mid] ^= 0x01;
    let bad_src = dir.path("bad.ecad");
    std::fs::write(&bad_src, &bad).unwrap();
    let e = run(ecad_cli::ecad, &["upgrade", bad_src.to_str().unwrap()]).unwrap_err();
    assert!(e.contains("damaged"), "{e}");
    assert!(!dir.path("bad.v2.ecad").exists());
    // ecadinspect refuses a version 1 file and points at the converter.
    let e = run(ecad_cli::ecadinspect, &[s, "tables"]).unwrap_err();
    assert!(e.contains("ecad upgrade"), "{e}");
}

#[test]
fn ecadinspect_shows_every_table_and_export_round_trips_into_version_1() {
    let dir = TempDir::new("inspect");
    let l = history();
    let src = dir.path("p.ecad");
    std::fs::write(&src, v1_bytes(&l)).unwrap();
    let out = dir.path("p2.ecad");
    let o = out.to_str().unwrap();
    run(ecad_cli::ecad, &["upgrade", src.to_str().unwrap(), "--out", o]).unwrap();
    let tables = run(ecad_cli::ecadinspect, &[o, "tables"]).unwrap();
    for t in ["meta", "branches", "transactions", "versions", "chunks", "assets", "link_v1", "json/1"] {
        assert!(tables.contains(t), "{t}: {tables}");
    }
    let branches = run(ecad_cli::ecadinspect, &[o, "branches"]).unwrap();
    assert!(branches.contains("main") && branches.contains(&format!("head {}", l.head())), "{branches}");
    let log = run(ecad_cli::ecadinspect, &[o, "log", "main", "--last", "3"]).unwrap();
    assert_eq!(log.matches("\n# ").count() + usize::from(log.starts_with("# ")), 3, "{log}");
    assert!(log.contains("\"type\":\"tx\""), "{log}");
    let versions = run(ecad_cli::ecadinspect, &[o, "versions"]).unwrap();
    assert!(versions.contains("\"Ten\"") && versions.contains("Auto"), "{versions}");
    let r = ecad_file::v2::Store::open(&out, false).unwrap();
    let (hash, _, _) = r.reader().unwrap().chunk_list().unwrap()[0];
    drop(r);
    let chunk = run(ecad_cli::ecadinspect, &[o, "chunk", &ecad_file::chain::hex(&hash)]).unwrap();
    assert!(chunk.contains("hash verified"), "{chunk}");
    let verify = run(ecad_cli::ecadinspect, &[o, "verify"]).unwrap();
    assert!(verify.trim_end().ends_with("OK"), "{verify}");
    // export-jsonl is a version 1 file with the same history and chain.
    let exported = run(ecad_cli::ecadinspect, &[o, "export-jsonl"]).unwrap();
    let back = ecad_file::open_project(exported.as_bytes(), 0).unwrap();
    assert_eq!(back.report.outcome, ecad_file::Outcome::Clean);
    assert_eq!(back.ledger.log(), l.log());
    assert_eq!(back.ledger.document(), l.document());
    // --with-raw: every stored row, with its raw bytes and readable form.
    let raw_path = dir.path("raw.jsonl");
    let said = run(ecad_cli::ecadinspect, &[o, "export-jsonl", "--with-raw", "--out", raw_path.to_str().unwrap()]).unwrap();
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
