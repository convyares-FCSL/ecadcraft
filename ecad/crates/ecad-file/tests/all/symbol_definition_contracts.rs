//! Brief 048 (symbol builder): a definition's text slots, defaults and `derived_from` (FP1, FP2,
//! FP4) are left out of the record when empty, so existing content hashes hold, and round-trip
//! when set; the op `SetComponentDefinition` (FP5) round-trips; a project symbol's content hash is
//! checked on read like any embedded definition's.

use crate::common::*;
use ecad_file::{Outcome, ProjectFile, open_project};
use ecad_model::*;
use ecad_store::{Medium, MemoryMedium};

fn written(ledger: &mut Ledger) -> Vec<u8> {
    let file = ProjectFile::create(MemoryMedium::default(), PROJECT, ledger, Some(1_760_000_000_000), WRITER).unwrap();
    file.medium().read_all().unwrap()
}

fn symbol() -> Definition {
    let mut d = coil(3.0);
    d.texts = vec![
        TextSlot { text: "tag".into(), at: Pt::new(-6.0, 0.0), h: TextHAlign::Right, v: TextVAlign::Middle, rotation: 0.0 },
        TextSlot { text: "rating".into(), at: Pt::new(6.0, -2.5), h: TextHAlign::Left, v: TextVAlign::Top, rotation: -0.0 },
    ];
    d.defaults =
        [("key", "float1"), ("prefix", "B"), ("description", "Float switch"), ("join", "device")].map(|(k, v)| (k.to_string(), v.to_string())).into();
    d.derived_from = from_library(coil(3.0), DefinitionId(0x77), 7).origin;
    d
}

#[test]
fn empty_new_fields_are_left_out_and_old_hashes_hold() {
    let bare = coil(3.0);
    let line = serde_json::to_string(&ecad_file::v1::Definition::from(&bare)).unwrap();
    for word in ["texts", "defaults", "derived_from"] {
        assert!(!line.contains(word), "{word} in {line}");
    }
    // The golden file (`records_contracts::the_golden_file_is_stable`) holds the old record and
    // hash; here: a slot, a default or a source each change the hash.
    let h = ecad_file::definition_hash(&bare).unwrap();
    let full = symbol();
    for d in [
        Definition { texts: full.texts.clone(), ..bare.clone() },
        Definition { defaults: full.defaults.clone(), ..bare.clone() },
        Definition { derived_from: full.derived_from.clone(), ..bare.clone() },
    ] {
        assert_ne!(ecad_file::definition_hash(&d).unwrap(), h);
    }
    // The origin is outside the hash; `derived_from` is inside it.
    let mut other = full.clone();
    other.origin = from_library(full.clone(), DefinitionId(0x99), 3).origin;
    assert_eq!(ecad_file::definition_hash(&other).unwrap(), ecad_file::definition_hash(&full).unwrap());
}

#[test]
fn a_project_symbol_and_set_component_definition_round_trip() {
    let mut f = fixture();
    let sym = from_library(symbol(), DefinitionId(0x48), 1);
    let id = DefinitionId(0x480);
    // A component with no placed symbol (a placed one must show its component's definition, I9).
    let c = ComponentId(0x4801);
    f.commit(
        "Symbol",
        vec![
            Op::EmbedDefinition { id, definition: sym.clone() },
            Op::CreateComponent { id: c, component: Component { definition: Some(f.d1), ..Default::default() } },
        ],
    );
    let op = Op::SetComponentDefinition { id: c, before: Some(f.d1), after: Some(id) };
    let v = ecad_file::record::op_value(&op).unwrap();
    assert_eq!(v["op"], "SetComponentDefinition");
    assert_eq!(v["v"], 1);
    assert!(v.get("before").is_some() && v.get("after").is_some(), "{v}");
    f.commit("Symbol changed", vec![op.clone()]);
    let none = Op::SetComponentDefinition { id: c, before: Some(id), after: None };
    f.commit("No symbol", vec![none.clone()]);
    f.commit("Back", vec![none.inverse()]);

    let bytes = written(&mut f.ledger);
    let opened = open_project(&bytes, SEED).unwrap();
    assert_eq!(opened.report.outcome, Outcome::Clean);
    assert_eq!(opened.ledger.log(), f.ledger.log());
    assert_eq!(opened.ledger.document(), f.ledger.document());
    let read = &opened.ledger.document().definitions[&id];
    assert_eq!(read, &sym);
    // A slot turned by -0.0 is one turned by 0.0: same record, same hash (it was written once
    // and failed the content-hash check on read).
    let mut zero = symbol();
    zero.texts[1].rotation = 0.0;
    assert_eq!(ecad_file::definition_hash(&zero).unwrap(), ecad_file::definition_hash(&symbol()).unwrap());
    assert_eq!(opened.ledger.document().components[&c].definition, Some(id));
}

#[test]
fn an_edited_project_symbol_is_refused_on_read() {
    let sym = from_library(symbol(), DefinitionId(0x48), 1);
    let tx = Transaction {
        id: TxId(1),
        seq: 1,
        author: Author::User,
        label: "Symbol".into(),
        kind: TxKind::Edit,
        time_ms: None,
        ops: vec![Op::EmbedDefinition { id: DefinitionId(0x480), definition: sym }],
    };
    let mut v = ecad_file::record::tx_value(&tx, &[0; 32]).unwrap();
    assert!(ecad_file::record::decode_tx(&v).is_ok());
    // A hand edit of a default inside the record: the recorded hash no longer matches.
    v["ops"][0]["definition"]["defaults"]["description"] = "Float switcH".into();
    let e = ecad_file::record::decode_tx(&v).unwrap_err();
    assert!(format!("{e:?}").contains("content hash"), "{e:?}");
}
