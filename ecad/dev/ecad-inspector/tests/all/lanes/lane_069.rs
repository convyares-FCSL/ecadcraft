//! Brief 069 (one file format; owner, 2026-11-15): with the version 1 export gone, the inspector's
//! **Raw** and **Readable** views are what keep a `.ecad` file from being a black box. On a project
//! holding every op kind (the list is read from `Op` itself, so a new op without a rendering fails
//! here), every transaction renders in both forms through the protocol: the Tables view's row
//! expand (`row`, raw and readable) and the History log's record (`log` with `detail`).

use std::collections::BTreeSet;
use std::time::Duration;

use ecad_inspector::source::{FileSource, Source, VerifyState};
use ecad_model::{
    Author, Cable, CableMember, Color, Definition, Device, End, Folder, Form, IdAllocator, Item, ItemKind, Layer, LayerId, Ledger, LengthUnit,
    ObjectRef, Op, Page, PageId, Placement, Primitive, ProjectSettings, Pt, Representation, StandardHeader, Style, StyleLayer, StyleRecord, StyleRef,
    Subject, SymbolInstance, Terminal, TerminalDef, Wire,
};
use serde_json::{Value, json};

use crate::common::*;

/// Every `Op` variant's name, read from `Op` itself: serde's derived `Deserialize` hands every
/// variant name to the deserializer, so a new op is in this list without anyone adding it.
fn all_ops() -> BTreeSet<&'static str> {
    use serde::de::{self, Deserialize, Visitor};

    struct Names(Option<&'static [&'static str]>);

    #[derive(Debug)]
    struct Stop;
    impl std::fmt::Display for Stop {
        fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            f.write_str("variant names recorded")
        }
    }
    impl std::error::Error for Stop {}
    impl de::Error for Stop {
        fn custom<T: std::fmt::Display>(_: T) -> Self {
            Stop
        }
    }

    impl<'de> de::Deserializer<'de> for &mut Names {
        type Error = Stop;
        fn deserialize_any<V: Visitor<'de>>(self, _: V) -> Result<V::Value, Stop> {
            Err(Stop)
        }
        fn deserialize_enum<V: Visitor<'de>>(self, _: &'static str, variants: &'static [&'static str], _: V) -> Result<V::Value, Stop> {
            self.0 = Some(variants);
            Err(Stop)
        }
        serde::forward_to_deserialize_any! {
            bool i8 i16 i32 i64 i128 u8 u16 u32 u64 u128 f32 f64 char str string bytes byte_buf option unit unit_struct
            newtype_struct seq tuple tuple_struct map struct identifier ignored_any
        }
    }

    let mut names = Names(None);
    assert!(Op::deserialize(&mut names).is_err());
    let all: BTreeSet<&'static str> = names.0.unwrap_or_default().iter().copied().collect();
    assert!(all.len() >= 33, "Op's variants: {all:?}");
    all
}

fn style(layer: LayerId) -> Style {
    Style { layer, color: Color::ByLayer, linetype: "ByLayer".into(), lineweight: -1 }
}

fn line(page: PageId, layer: LayerId, x: f64) -> Item {
    Item { page, style: style(layer), kind: ItemKind::Primitive(Primitive::Line { a: Pt::new(x, 0.0), b: Pt::new(x, 10.0) }) }
}

/// A valid project holding one of everything, then one transaction per op kind: the op and its
/// inverse together (the end state is valid even where the op alone would break an invariant).
/// The ops are those of `ecad-model`'s `incremental_contracts` (one of each that applies).
fn every_op_ledger() -> Ledger {
    let mut ids = IdAllocator::seeded(0x069);
    let mut ledger = Ledger::new(0x069);
    let page: PageId = ids.fresh();
    let page_v = Page::titled("1").unwrap();
    let layer: LayerId = ids.fresh();
    let layer_v = Layer { name: "0".into() };
    let def: ecad_model::DefinitionId = ids.fresh();
    let def_v = Definition {
        name: "K".into(),
        graphics: vec![Primitive::Circle { center: Pt::new(0.0, 0.0), radius: 2.0 }],
        terminals: vec![TerminalDef { key: "1".parse().unwrap(), label: None, at: Pt::new(0.0, 2.0) }],
        ..Default::default()
    };
    let comp: ecad_model::DeviceId = ids.fresh();
    let mut comp_v = Device { definition: Some(def), ..Device::default() };
    comp_v.properties.insert("note".into(), "K1".into());
    let term: ecad_model::TerminalId = ids.fresh();
    let term_v = Terminal { component: comp, label: Some("1".into()), key: Some("1".parse().unwrap()) };
    let sym: ecad_model::ItemId = ids.fresh();
    let sym_v = Item { page, style: style(layer), kind: ItemKind::Symbol(SymbolInstance { definition: def, placement: Placement::default() }) };
    let item: ecad_model::ItemId = ids.fresh();
    let item_v = line(page, layer, 5.0);
    let rep_sym: ecad_model::RepresentationId = ids.fresh();
    let rep_sym_v = Representation {
        subject: Subject::Device(comp),
        page,
        form: Form::Symbol,
        role: "main".into(),
        items: vec![sym],
        connection_points: Default::default(),
    };
    let wire: ecad_model::WireId = ids.fresh();
    let wire_v = Wire { ends: [Some(term), None], ..Wire::default() };
    let wire2: ecad_model::WireId = ids.fresh();
    let rep_wire: ecad_model::RepresentationId = ids.fresh();
    let rep_wire_v = Representation {
        subject: Subject::Wire(wire),
        page,
        form: Form::Drawn,
        role: "route".into(),
        items: vec![item],
        connection_points: Default::default(),
    };
    let cable: ecad_model::CableId = ids.fresh();
    let cable_v = Cable { members: vec![CableMember { conductor: wire, core: None }], ..Cable::default() };
    let folder: ecad_model::FolderId = ids.fresh();
    let folder_v = Folder { name: "F".into(), parent: None, start: None };
    ledger
        .commit(
            Author::User,
            "base",
            vec![
                Op::AddPage { id: page, page: page_v.clone(), index: 0 },
                Op::AddLayer { id: layer, layer: layer_v.clone() },
                Op::EmbedDefinition { id: def, definition: def_v.clone() },
                Op::CreateDevice { id: comp, device: comp_v.clone() },
                Op::AddTerminal { id: term, terminal: term_v.clone() },
                Op::AddItem { id: sym, item: sym_v },
                Op::AddItem { id: item, item: item_v.clone() },
                Op::LinkRepresentation { id: rep_sym, rep: rep_sym_v.clone() },
                Op::CreateWire { id: wire, conductor: wire_v.clone() },
                Op::CreateWire { id: wire2, conductor: Wire::default() },
                Op::LinkRepresentation { id: rep_wire, rep: rep_wire_v.clone() },
                Op::CreateCable { id: cable, cable: cable_v.clone() },
                Op::AddFolder { id: folder, folder: folder_v.clone(), index: 0 },
            ],
        )
        .unwrap();
    ledger.commit(Author::User, "bookmark", vec![Op::NameVersion { name: "v".into(), at: 1, description: None }]).unwrap();
    let moved = line(page, layer, 7.0);
    let mut rep_after = rep_wire_v.clone();
    rep_after.role = "moved".into();
    let header = StandardHeader { id: ids.fresh(), name: "S".into(), based_on: "iec".into(), origin: None };
    let ops = vec![
        Op::AddPage { id: ids.fresh(), page: Page::titled("2").unwrap(), index: 1 },
        Op::RemovePage { id: page, page: page_v.clone(), index: 0 },
        // Lane 072 E (A15): a page changed in place.
        Op::ModifyPage {
            id: page,
            before: page_v.clone(),
            after: Page { number: ecad_model::PageNo::parse("7"), sheet: ecad_model::SheetSpec::parse("A2").ok(), ..page_v.clone() },
        },
        Op::AddLayer { id: ids.fresh(), layer: Layer { name: "x".into() } },
        Op::RemoveLayer { id: layer, layer: layer_v.clone() },
        Op::AddItem { id: ids.fresh(), item: line(page, layer, 1.0) },
        Op::ModifyItem { id: item, before: item_v.clone(), after: moved },
        Op::RemoveItem { id: item, item: item_v.clone() },
        Op::CreateDevice { id: ids.fresh(), device: Device::default() },
        Op::DeleteDevice { id: comp, device: comp_v.clone() },
        Op::ModifyDevice {
            id: comp,
            before: comp_v.clone(),
            after: Device { device_type: ecad_model::DeviceType::of_group(ecad_model::DeviceGroup::Motor), ..comp_v.clone() },
        },
        Op::AddTerminal { id: ids.fresh(), terminal: Terminal { component: comp, label: None, key: None } },
        Op::RemoveTerminal { id: term, terminal: term_v.clone() },
        Op::LinkRepresentation { id: ids.fresh(), rep: rep_wire_v.clone() },
        Op::ModifyRepresentation { id: rep_wire, before: rep_wire_v.clone(), after: rep_after },
        Op::UnlinkRepresentation { id: rep_sym, rep: rep_sym_v.clone() },
        Op::CreateWire { id: ids.fresh(), conductor: Wire { ends: [Some(term), Some(term)], ..Wire::default() } },
        Op::DeleteWire { id: wire, conductor: wire_v.clone() },
        Op::ModifyWire { id: wire2, before: Wire::default(), after: Wire { size: ecad_model::wire::WireSize::parse("2.5"), ..Wire::default() } },
        Op::AttachEnd { conductor: wire2, end: End::B, before: None, after: Some(term) },
        Op::CreateCable { id: ids.fresh(), cable: cable_v.clone() },
        Op::DeleteCable { id: cable, cable: cable_v.clone() },
        Op::AddCableMember { cable, index: 1, member: CableMember { conductor: wire2, core: Some("2".into()) } },
        Op::RemoveCableMember { cable, index: 0, member: CableMember { conductor: wire, core: None } },
        Op::SetProperty { object: ObjectRef::Device(comp), key: "note".parse().unwrap(), before: Some("K1".into()), after: None },
        Op::EmbedDefinition { id: ids.fresh(), definition: def_v.clone() },
        Op::RemoveDefinition { id: def, definition: def_v.clone() },
        Op::SetProjectSettings {
            before: ProjectSettings::default(),
            after: ProjectSettings { length_unit: LengthUnit::Inch, ..ProjectSettings::default() },
        },
        Op::NameVersion { name: "w".into(), at: 2, description: None },
        Op::UnnameVersion { name: "v".into(), at: 1, description: None },
        Op::SetDeviceDefinition { id: comp, before: Some(def), after: None },
        Op::AddFolder { id: ids.fresh(), folder: Folder { name: "G".into(), parent: Some(folder), start: None }, index: 1 },
        Op::RemoveFolder { id: folder, folder: folder_v.clone(), index: 0 },
        Op::ModifyFolder { id: folder, before: folder_v.clone(), after: Folder { name: "Renamed".into(), parent: None, start: None } },
        Op::SetStyle {
            layer: StyleLayer::Project,
            catalogue: ecad_model::StyleCatalogue::Wire,
            id: StyleRef::Builtin("thin".into()),
            before: None,
            after: Some(StyleRecord { name: "Thin".into(), ..StyleRecord::default() }),
        },
        Op::SetStandard { before: None, after: Some(header) },
        // Lane 072 E (A15): the project's lists changed.
        Op::ModifyProject {
            before: ecad_model::Project::default(),
            after: ecad_model::Project {
                locations: vec![ecad_model::Location::new(ids.fresh(), ecad_model::ListCode::new("+CP1").unwrap())],
                ..ecad_model::Project::default()
            },
        },
    ];
    for op in ops {
        let name = op.name();
        let back = op.inverse();
        ledger.commit(Author::User, name, vec![op, back]).unwrap_or_else(|e| panic!("{name}: {e}"));
    }
    ledger
}

fn ok(v: &Value) -> &Value {
    assert!(v.get("error").is_none(), "{v:#}");
    &v["ok"]
}

fn ops_of(record: &Value) -> Vec<String> {
    record["ops"].as_array().map(|a| a.iter().filter_map(|o| o["op"].as_str().map(String::from)).collect()).unwrap_or_default()
}

#[test]
fn every_op_kind_renders_raw_and_readable() {
    let dir = TempDir::new("lane-069");
    let ledger = every_op_ledger();
    let path = written(&dir, &ledger);
    let f = FileSource::from_path(&path);
    assert!(matches!(f.wait_verified(Duration::from_secs(60)), VerifyState::Done { .. }));
    let s = Some(&f as &dyn Source);
    let all = all_ops();

    // The Tables view: every transactions row, expanded Readable and Raw.
    let rows = ask(s, json!({"op": "rows", "table": "transactions", "count": 1000}));
    let rows = ok(&rows)["rows"].as_array().unwrap().clone();
    assert_eq!(rows.len() as u64, ledger.head(), "one row per transaction");
    let mut readable_ops = BTreeSet::new();
    for r in &rows {
        let key = r["key"].as_str().unwrap();
        let readable = ask(s, json!({"op": "row", "table": "transactions", "key": key, "form": "readable"}));
        let readable = ok(&readable);
        assert!(readable.get("decode_error").is_none(), "{key}: {readable:#}");
        let record = &readable["decoded"]["record"];
        assert_eq!(record["type"], "tx", "{key}: {readable:#}");
        // Readable is the record itself: it decodes back to exactly the transaction in the ledger.
        let tx = ecad_file::record::decode_tx(record).unwrap_or_else(|e| panic!("{key}: {e:?}"));
        assert_eq!(Some(&tx), ledger.log().get(tx.seq as usize - 1), "{key}");
        readable_ops.extend(ops_of(record));
        let raw = ask(s, json!({"op": "row", "table": "transactions", "key": key, "form": "raw"}));
        let raw = ok(&raw);
        let hex = raw["hex"].as_str().unwrap();
        assert_eq!(raw["form"], "raw");
        assert!(!hex.is_empty() && hex.len() == 2 * raw["size"].as_u64().unwrap() as usize && raw["truncated"] == false, "{key}: {raw:#}");
    }
    let readable_ops: BTreeSet<&str> = readable_ops.iter().map(String::as_str).collect();
    assert_eq!(readable_ops, all, "every op kind renders in Readable");

    // The History log: every transaction's record, as the row expand shows it.
    let log = ask(s, json!({"op": "log", "branch": "main", "from": 0, "count": 1000, "detail": true}));
    let log = ok(&log)["rows"].as_array().unwrap().clone();
    assert_eq!(log.len() as u64, ledger.head());
    let mut logged = BTreeSet::new();
    for r in &log {
        assert_eq!(r["decodes"], true, "{r:#}");
        let tx = ecad_file::record::decode_tx(&r["record"]).unwrap_or_else(|e| panic!("log {}: {e:?}", r["seq"]));
        assert_eq!(Some(&tx), ledger.log().get(tx.seq as usize - 1));
        logged.extend(ops_of(&r["record"]));
    }
    let logged: BTreeSet<&str> = logged.iter().map(String::as_str).collect();
    assert_eq!(logged, all, "every op kind renders in the log");
}
