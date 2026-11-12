//! Fixtures shared by the inspector's tests: a small history written as a version 1 file and
//! converted to version 2.
#![allow(dead_code)]

use std::path::{Path, PathBuf};

use ecad_model::*;
use ecad_store::{Medium, MemoryMedium};

pub struct TempDir(PathBuf);

impl TempDir {
    pub fn new(name: &str) -> Self {
        let p = std::env::temp_dir().join(format!("ecad-inspector-{name}-{}", ecad_store::disk::unique()));
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

pub const PROJECT: ProjectId = ProjectId(0x00c1_1000_0000_0000_0000_0000_0000_0001);

/// A small history: two pages, lines, a named version, an undo.
pub fn history() -> Ledger {
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

pub fn v1_bytes(l: &Ledger) -> Vec<u8> {
    let f = ecad_file::ProjectFile::write_new(MemoryMedium::default(), PROJECT, l, Some(5), "test").unwrap();
    f.into_medium().read_all().unwrap()
}

/// `ecadinspect <args>` (the text views): its output, or its error.
pub fn run(args: &[&str]) -> std::result::Result<String, String> {
    let args: Vec<String> = args.iter().map(|s| s.to_string()).collect();
    let mut out = Vec::new();
    ecad_inspector::cli::ecadinspect_with(&args, &mut out, &mut std::io::sink()).map(|()| String::from_utf8(out).unwrap())
}

/// `history` written as a version 1 file `p.ecad` in `dir` and converted to `p2.ecad`, as
/// `ecad upgrade` does; the version 2 path.
pub fn upgraded(dir: &TempDir, l: &Ledger) -> PathBuf {
    let src = dir.path("p.ecad");
    std::fs::write(&src, v1_bytes(l)).unwrap();
    let out = dir.path("p2.ecad");
    convert(&src, &out);
    out
}

/// `ecad upgrade <src> --out <out>`.
pub fn convert(src: &Path, out: &Path) {
    ecad_file::v2::convert::convert_file(src, Some(out), "test").unwrap();
}

pub const WRITER: &str = "ecad-inspector tests";

/// The rich fixture (design 029 §8): main with two pages, three devices (two tagged `-K1`, a
/// duplicate), connection points, two wires on one net, a cable, a revision issued as a version,
/// and the tag text `TITLE` in a page title and a property; a branch `Option B` forked from main
/// with edits of its own. Snapshots every 16 transactions. Returns the file's path.
pub fn rich(dir: &TempDir, title: &str) -> PathBuf {
    use ecad_file::v2::{ProjectV2, Store};
    let mut ids = IdAllocator::seeded(0x1A5);
    let mut l = Ledger::new(11);
    let (p1, p2, layer): (PageId, PageId, LayerId) = (ids.fresh(), ids.fresh(), ids.fresh());
    l.commit(
        Author::User,
        "New project",
        vec![
            Op::AddPage { id: p1, page: Page::new(title), index: 0 },
            Op::AddPage { id: p2, page: Page::new("2"), index: 1 },
            Op::AddLayer { id: layer, layer: Layer { name: "0".into() } },
        ],
    )
    .unwrap();
    let device = |ids: &mut IdAllocator, tag: &str, points: &[&str]| -> (Vec<Op>, Vec<TerminalId>) {
        let c: ComponentId = ids.fresh();
        let mut props = ecad_model::doc::Properties::new();
        props.insert("tag".into(), tag.into());
        props.insert("note".into(), title.into());
        let mut ops = vec![Op::CreateComponent { id: c, component: Component { properties: props, definition: None } }];
        let mut ts = Vec::new();
        for p in points {
            let t: TerminalId = ids.fresh();
            ops.push(Op::AddTerminal { id: t, terminal: Terminal { component: c, label: Some((*p).into()), key: None } });
            ts.push(t);
        }
        (ops, ts)
    };
    let (k1, k1t) = device(&mut ids, "-K1", &["A1", "A2"]);
    let (k1b, _) = device(&mut ids, "-K1", &["A1"]);
    let (x1, x1t) = device(&mut ids, "-X1", &["3", "4"]);
    l.commit(Author::User, "Devices", [k1, k1b, x1].concat()).unwrap();
    let wire = |ids: &mut IdAllocator, number: &str, a: TerminalId, b: TerminalId| -> (ConductorId, Op) {
        let w: ConductorId = ids.fresh();
        let mut props = ecad_model::doc::Properties::new();
        props.insert("wire_number".into(), number.into());
        props.insert("cross_section".into(), "1.5".into());
        (w, Op::CreateConductor { id: w, conductor: Conductor { properties: props, ends: [Some(a), Some(b)] } })
    };
    let (w1, op1) = wire(&mut ids, "101", k1t[0], x1t[0]);
    let (_, op2) = wire(&mut ids, "101", x1t[0], x1t[1]);
    l.commit(Author::User, "Wires", vec![op1, op2]).unwrap();
    let cable: CableId = ids.fresh();
    let mut cprops = ecad_model::doc::Properties::new();
    cprops.insert("tag".into(), "-W1".into());
    let cable_op =
        Op::CreateCable { id: cable, cable: Cable { properties: cprops, members: vec![CableMember { conductor: w1, core: Some("1".into()) }] } };
    l.commit(Author::User, "Cable", vec![cable_op]).unwrap();
    for i in 0..20 {
        let id: ItemId = ids.fresh();
        let style = Style { layer, color: Color::ByLayer, linetype: "ByLayer".into(), lineweight: -1 };
        let kind = ItemKind::Primitive(Primitive::Line { a: Pt::new(i as f64, 0.0), b: Pt::new(0.0, i as f64) });
        l.commit(Author::User, "LINE", vec![Op::AddItem { id, item: Item { page: p1, style, kind } }]).unwrap();
    }
    let at = l.head();
    let set = |k: &str, v: &str| Op::SetProperty { object: ObjectRef::Project, key: k.into(), before: None, after: Some(v.into()) };
    l.commit(
        Author::User,
        "Issue A",
        vec![
            set("rev.1.rev", "A"),
            set("rev.1.description", "First issue"),
            set("rev.1.issued", "Rev A"),
            Op::NameVersion { name: "Rev A".into(), at, description: None },
        ],
    )
    .unwrap();
    let path = dir.path("rich.v2.ecad");
    let mut proj = ProjectV2::create(Store::create(&path).unwrap(), PROJECT, &l, None, None, Some(1), WRITER, 16).unwrap().0;
    l.mark_persisted(l.head()).unwrap();
    let main = proj.branch();
    let (b, _) = proj.create_branch("Option B", main, at, None, None).unwrap();
    let (mut bl, _) = proj.switch_to(&l, b, 12).unwrap();
    for i in 0..5 {
        let id: ItemId = ids.fresh();
        let style = Style { layer, color: Color::ByLayer, linetype: "ByLayer".into(), lineweight: -1 };
        let kind = ItemKind::Primitive(Primitive::Line { a: Pt::new(50.0 + i as f64, 0.0), b: Pt::new(0.0, 1.0) });
        bl.commit(Author::User, "LINE", vec![Op::AddItem { id, item: Item { page: p2, style, kind } }]).unwrap();
    }
    proj.save(&mut bl).unwrap();
    let _ = proj.switch_to(&bl, main, 13).unwrap();
    drop(proj);
    path
}

/// One protocol request's reply, parsed.
pub fn ask(src: Option<&dyn ecad_inspector::source::Source>, request: serde_json::Value) -> serde_json::Value {
    let reply = ecad_inspector::protocol::handle(src, &request.to_string());
    serde_json::from_str(&reply).unwrap()
}
