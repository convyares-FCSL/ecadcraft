//! `ecad upgrade` (brief 021): the converter writes a new verified file and never touches the
//! source. `ecad inspect` points at the developer tool `ecadinspect` (brief 029).

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
            l.commit(Author::User, "Name", vec![Op::NameVersion { name: "Ten".into(), at, description: None }]).unwrap();
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
}

#[test]
fn ecad_inspect_points_at_the_developer_tool() {
    let e = run(ecad_cli::ecad, &["inspect", "demo.v2.ecad", "tables"]).unwrap_err();
    assert!(e.contains("ecadinspect") && e.contains("cargo run -p ecad-inspector --bin ecadinspect"), "{e}");
}
