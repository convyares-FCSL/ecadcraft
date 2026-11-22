//! Fixtures shared by the inspector's tests: small histories written as project files.
#![allow(dead_code)]

use std::path::PathBuf;

use ecad_model::*;

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
            Op::AddPage { id: p1, page: Page::titled("1").unwrap(), index: 0 },
            Op::AddPage { id: p2, page: Page::titled("2").unwrap(), index: 1 },
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

/// `ecadinspect <args>` (the text views): its output, or its error.
pub fn run(args: &[&str]) -> std::result::Result<String, String> {
    let args: Vec<String> = args.iter().map(|s| s.to_string()).collect();
    let mut out = Vec::new();
    ecad_inspector::cli::views(&args, &mut out).map(|()| String::from_utf8(out).unwrap())
}

/// `history` written as the project file `p.ecad` in `dir`; its path.
pub fn written(dir: &TempDir, l: &Ledger) -> PathBuf {
    let path = dir.path("p.ecad");
    ecad_file::Project::create(ecad_file::Store::create(&path).unwrap(), PROJECT, l, Some(5), "test", 16).unwrap();
    path
}

pub const WRITER: &str = "ecad-inspector tests";

/// The rich fixture (design 029 §8): main with two pages, three devices (two tagged `-K1`, a
/// duplicate), connection points, two wires on one net, a cable, a revision issued as a version,
/// and the tag text `TITLE` in a page title and a property; a branch `Option B` forked from main
/// with edits of its own. Snapshots every 16 transactions. Returns the file's path.
pub fn rich(dir: &TempDir, title: &str) -> PathBuf {
    use ecad_file::{Project, Store};
    let mut ids = IdAllocator::seeded(0x1A5);
    let mut l = Ledger::new(11);
    let (p1, p2, layer): (PageId, PageId, LayerId) = (ids.fresh(), ids.fresh(), ids.fresh());
    l.commit(
        Author::User,
        "New project",
        vec![
            Op::AddPage { id: p1, page: Page::titled(title).unwrap(), index: 0 },
            Op::AddPage { id: p2, page: Page::titled("2").unwrap(), index: 1 },
            Op::AddLayer { id: layer, layer: Layer { name: "0".into() } },
        ],
    )
    .unwrap();
    let device = |ids: &mut IdAllocator, tag: &str, points: &[&str]| -> (Vec<Op>, Vec<TerminalId>) {
        let c: DeviceId = ids.fresh();
        let mut props = ecad_model::doc::Properties::new();
        props.insert("note".into(), title.into());
        let mut ops = vec![Op::CreateDevice { id: c, device: Device { properties: props, ..Device::default() }.with_tag(Some(tag)) }];
        let mut ts = Vec::new();
        for p in points {
            let t: TerminalId = ids.fresh();
            ops.push(Op::AddTerminal {
                id: t,
                terminal: Terminal { owner: c.into(), label: Some((*p).into()), key: None, accepts: Default::default() },
            });
            ts.push(t);
        }
        (ops, ts)
    };
    let (k1, k1t) = device(&mut ids, "-K1", &["A1", "A2"]);
    let (k1b, _) = device(&mut ids, "-K1", &["A1"]);
    let (x1, x1t) = device(&mut ids, "-X1", &["3", "4"]);
    l.commit(Author::User, "Devices", [k1, k1b, x1].concat()).unwrap();
    // Whole wires, no routes (design 072 §4.2): one net, its order 1 then 2.
    let net: NetId = ids.fresh();
    let wire = |ids: &mut IdAllocator, number: &str, order: u16, a: TerminalId, b: TerminalId| -> (WireId, Op) {
        let w: WireId = ids.fresh();
        let wire = Wire {
            net_order: order,
            number: ecad_model::text::Text::new(number).ok().map(|text| ecad_model::wire::WireNumber { text, fixed: false }),
            size: ecad_model::wire::WireSize::parse("1.5"),
            ..Wire::new(net, ids.fresh(), a, b)
        };
        (w, Op::CreateWire { id: w, wire })
    };
    let (w1, op1) = wire(&mut ids, "101", 1, k1t[0], x1t[0]);
    let (_, op2) = wire(&mut ids, "101", 2, x1t[0], x1t[1]);
    l.commit(Author::User, "Wires", vec![op1, op2]).unwrap();
    let cable: CableId = ids.fresh();
    let mut cprops = ecad_model::doc::Properties::new();
    cprops.insert("tag".into(), "-W1".into());
    let cable_op = Op::CreateCable {
        id: cable,
        cable: Cable { properties: cprops, cores: vec![Core::on_wire(w1, ecad_model::CoreLabel::new("1").ok())], ..Default::default() },
    };
    l.commit(Author::User, "Cable", vec![cable_op]).unwrap();
    for i in 0..20 {
        let id: ItemId = ids.fresh();
        let style = Style { layer, color: Color::ByLayer, linetype: "ByLayer".into(), lineweight: -1 };
        let kind = ItemKind::Primitive(Primitive::Line { a: Pt::new(i as f64, 0.0), b: Pt::new(0.0, i as f64) });
        l.commit(Author::User, "LINE", vec![Op::AddItem { id, item: Item { page: p1, style, kind } }]).unwrap();
    }
    let at = l.head();
    l.commit(
        Author::User,
        "Issue A",
        vec![
            ecad_model::Op::ModifyProject {
                before: ecad_model::Project::default(),
                after: ecad_model::Project {
                    revisions: vec![ecad_model::Revision {
                        rev: ecad_model::Rev::new("A").ok(),
                        description: ecad_model::RevValue::new("First issue").ok(),
                        drawn: ecad_model::RevValue::new("CV").ok(),
                        issued: ecad_model::RevValue::new("Rev A").ok(),
                        ..ecad_model::Revision::default()
                    }],
                    ..ecad_model::Project::default()
                },
            },
            Op::SetProperty { object: ecad_model::ObjectRef::Project, key: "note".into(), before: None, after: Some("issued".into()) },
            Op::NameVersion { name: "Rev A".into(), at, description: None },
        ],
    )
    .unwrap();
    let path = dir.path("rich.v2.ecad");
    let mut proj = Project::create(Store::create(&path).unwrap(), PROJECT, &l, Some(1), WRITER, 16).unwrap().0;
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
