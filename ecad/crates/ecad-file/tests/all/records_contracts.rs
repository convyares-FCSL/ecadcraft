//! Records, the hash chain seeds, embedded definitions and what is left out while it holds its
//! defaults (proposal §20.2-§20.3, §20.10-§20.11).

use crate::common;

use common::*;
use ecad_file::chain::{self, ChainKind};
use ecad_model::*;

#[test]
fn records_round_trip_exactly() {
    let mut f = fixture();
    let opened = reopened(&mut f.ledger);
    assert!(opened.report.damage.is_none());
    assert!(opened.project.read_only().is_none());
    assert_eq!(opened.ledger.full_log().unwrap(), f.ledger.full_log().unwrap(), "every transaction, ID, label, author and time");
    assert_eq!(opened.ledger.document(), f.ledger.document());
    assert_eq!(opened.ledger.head(), f.ledger.head());
    assert_eq!(opened.ledger.persisted(), f.ledger.head());
}

#[test]
fn records_are_deterministic() {
    let a = fixture().ledger;
    let b = fixture().ledger;
    let prev = [0; 32];
    for (x, y) in a.log().iter().zip(b.log()) {
        let (vx, vy) = (ecad_file::record::tx_value(x, &prev).unwrap(), ecad_file::record::tx_value(y, &prev).unwrap());
        assert_eq!(
            ecad_file::canonical::canonical(&vx).unwrap(),
            ecad_file::canonical::canonical(&vy).unwrap(),
            "identical records give identical bytes"
        );
    }
}

#[test]
fn chain_seeds_are_separated_by_kind_and_id() {
    let p = chain::seed(ChainKind::Project, 7);
    assert_ne!(p, chain::seed(ChainKind::Library, 7), "a library and a project with the same ID differ");
    assert_ne!(p, chain::seed(ChainKind::Project, 8));
    assert_eq!(p, chain::seed(ChainKind::Project, 7));
}

#[test]
fn embedded_definitions_follow_the_owner_requirements() {
    let mut f = fixture();
    let doc = f.ledger.document().clone();
    // Embed once, many instances: c1 and c2 share d1; every terminal has its own ID.
    assert_eq!(doc.devices[&f.c1].definition, Some(f.d1));
    assert_eq!(doc.devices[&f.c2].definition, Some(f.d1));
    let ids: std::collections::BTreeSet<_> = f.t.iter().collect();
    assert_eq!(ids.len(), 6);
    // Versions coexist: d1 (library version 7) and d2 (version 9), different content hashes.
    let (o1, o2) = (doc.definitions[&f.d1].origin.clone().unwrap(), doc.definitions[&f.d2].origin.clone().unwrap());
    assert_eq!((o1.library_seq, o2.library_seq), (7, 9));
    assert_eq!(o1.definition, o2.definition);
    assert_ne!(o1.content_hash, o2.content_hash);
    // The content hash ignores the origin it is recorded in.
    assert_eq!(ecad_file::definition_hash(&doc.definitions[&f.d1]).unwrap(), o1.content_hash);
    // A referenced definition cannot be removed.
    let d2 = doc.definitions[&f.d2].clone();
    assert!(f.ledger.commit(Author::User, "remove", vec![Op::RemoveDefinition { id: f.d2, definition: d2 }]).is_err());
    // Remove the d1 instances, then d1 itself: earlier revisions keep the original content.
    let before = f.ledger.head();
    let mut ops = Vec::new();
    for c in [f.c1, f.c2] {
        let doc = f.ledger.document();
        let (rid, rep) = doc.representations_of(Subject::Device(c)).next().map(|(i, r)| (i, r.clone())).unwrap();
        for (w, cond) in doc.conductors.iter() {
            for end in End::BOTH {
                if let Some(t) = cond.end(end).filter(|t| doc.terminals[t].component == c) {
                    ops.push(Op::AttachEnd { conductor: *w, end, before: Some(t), after: None });
                }
            }
        }
        ops.push(Op::UnlinkRepresentation { id: rid, rep: rep.clone() });
        ops.push(Op::RemoveItem { id: rep.items[0], item: doc.items[&rep.items[0]].clone() });
        for (t, term) in doc.terminals_of(c) {
            ops.push(Op::RemoveTerminal { id: t, terminal: term.clone() });
        }
        ops.push(Op::DeleteDevice { id: c, device: doc.devices[&c].clone() });
    }
    ops.push(Op::RemoveDefinition { id: f.d1, definition: doc.definitions[&f.d1].clone() });
    f.commit("Remove old version", ops);
    let opened = reopened(&mut f.ledger);
    assert!(!opened.ledger.document().definitions.contains_key(&f.d1));
    let past = opened.ledger.state_at(before).unwrap();
    assert_eq!(past.definitions[&f.d1], doc.definitions[&f.d1], "history keeps the original definition");
    assert_eq!(opened.ledger.document().definitions[&f.d2], doc.definitions[&f.d2]);
}

/// Brief 018 (owner decision 2026-11-08): the tag settings are left out of a record while they
/// hold their defaults, so files that never set them are unchanged; set, they round-trip.
#[test]
fn tag_settings_are_written_only_when_set_and_round_trip() {
    let defaults = ProjectSettings::default();
    let plain = ecad_file::record::op_value(&Op::SetProjectSettings { before: defaults.clone(), after: defaults.clone() }).unwrap().to_string();
    assert!(!plain.contains("tag_padding") && !plain.contains("tag_prefixes"), "{plain}");

    let mut f = fixture();
    let before = f.ledger.document().settings.clone();
    let mut after = before.clone();
    after.tag_padding = Some(3);
    after.tag_prefixes.insert(ecad_model::prefix::PrefixOf::Starter("coil".into()), ecad_model::prefix::TagPrefix::typed("KA").unwrap());
    let op = Op::SetProjectSettings { before, after: after.clone() };
    let line = ecad_file::record::op_value(&op).unwrap().to_string();
    assert!(line.contains("\"tag_padding\":3") && line.contains("\"starter:coil\":\"KA\""), "{line}");
    f.ledger.commit(Author::User, "Tag settings", vec![op]).unwrap();
    let opened = reopened(&mut f.ledger);
    assert!(opened.report.damage.is_none());
    assert_eq!(opened.ledger.document().settings, after);
    assert_eq!(opened.ledger.full_log().unwrap(), f.ledger.full_log().unwrap());
}

/// Brief 022 (FP1, FP5): the wiring settings and a definition's bridges are left out while they
/// hold their defaults, so files and content hashes are unchanged; set, they round-trip.
#[test]
fn wiring_settings_and_bridges_are_written_only_when_set_and_round_trip() {
    let defaults = ProjectSettings::default();
    let plain = ecad_file::record::op_value(&Op::SetProjectSettings { before: defaults.clone(), after: defaults.clone() }).unwrap().to_string();
    assert!(!plain.contains("wiring"), "{plain}");

    let mut f = fixture();
    let before = f.ledger.document().settings.clone();
    let mut after = before.clone();
    after.wiring.scheme = ecad_model::wiring::Scheme::PageColumn;
    after.wiring.digits = Some(3);
    after.wiring.colour_style = Some(ecad_model::wiring::ColourStyle::ThreeLetter);
    after.wiring.label_height = Some(2.5);
    // Brief 032 (FP-W1): the label rule and the shortest labelled wire.
    after.wiring.label_rule = Some(ecad_model::wiring::LABEL_RULE_EACH.into());
    after.wiring.label_min_length = Some(10.0);
    after.wiring.potentials.retain(|p| p.name != "PE");
    let op = Op::SetProjectSettings { before, after: after.clone() };
    let line = ecad_file::record::op_value(&op).unwrap().to_string();
    assert!(line.contains("\"page-column\"") && line.contains("\"three-letter\""), "{line}");
    f.ledger.commit(Author::User, "Wire settings", vec![op]).unwrap();

    let bare = ecad_model::Definition { name: "t".into(), origin: None, graphics: vec![], terminals: vec![], bridges: vec![], ..Default::default() };
    // Brief 044 (FP2, invariant I7): a bridge names keys the definition has terminals for.
    let points = ["1", "2"].iter().map(|k| ecad_model::TerminalDef { key: (*k).into(), label: None, at: ecad_model::Pt::new(0.0, 0.0) }).collect();
    let bridged = ecad_model::Definition { terminals: points, bridges: vec![vec!["1".into(), "2".into()]], ..bare.clone() };
    let bare_line = serde_json::to_string(&ecad_file::records::Definition::from(&bare)).unwrap();
    assert!(!bare_line.contains("bridges"), "{bare_line}");
    assert_ne!(ecad_file::definition_hash(&bare).unwrap(), ecad_file::definition_hash(&bridged).unwrap());
    let id: ecad_model::DefinitionId = ecad_model::DefinitionId(0xb1d9e);
    f.ledger.commit(Author::User, "Bridge", vec![Op::EmbedDefinition { id, definition: bridged.clone() }]).unwrap();

    let opened = reopened(&mut f.ledger);
    assert!(opened.report.damage.is_none());
    assert_eq!(opened.ledger.document().settings, after);
    assert_eq!(opened.ledger.document().definitions.get(&id), Some(&bridged));
    assert_eq!(opened.ledger.full_log().unwrap(), f.ledger.full_log().unwrap());
}

/// Brief 019 (FP1–FP3): page and project properties, folders and sheet settings are written and
/// read back exactly; a page without properties and default settings are written as before.
#[test]
fn sheet_records_round_trip_and_defaults_are_left_out() {
    let plain = ecad_file::record::op_value(&Op::AddPage { id: PageId(1), page: Page::new("1"), index: 0 }).unwrap().to_string();
    assert!(!plain.contains("properties"), "{plain}");
    let defaults = ProjectSettings::default();
    let line = ecad_file::record::op_value(&Op::SetProjectSettings { before: defaults.clone(), after: defaults }).unwrap().to_string();
    for k in ["sheet", "page_number_step", "reference_style"] {
        assert!(!line.contains(k), "{k} in {line}");
    }

    let mut f = fixture();
    let folder: FolderId = f.ids.fresh();
    let before = f.ledger.document().settings.clone();
    let mut after = before.clone();
    after.sheet = Some(SheetSpec::parse("A2 12x8 numbers letters").unwrap());
    after.page_number_step = Some(5);
    after.reference_style = Some(ReferenceStyle::PageColumn);
    let ops = vec![
        Op::AddFolder { id: folder, folder: Folder { name: "Layouts".into(), parent: None }, index: 0 },
        Op::SetProperty { object: ObjectRef::Page(f.page), key: "folder".into(), before: None, after: Some(folder.hex()) },
        Op::SetProperty { object: ObjectRef::Project, key: "title".into(), before: None, after: Some("P7".into()) },
        Op::SetProjectSettings { before, after: after.clone() },
    ];
    f.ledger.commit(Author::User, "Sheets", ops).unwrap();
    let renamed = Folder { name: "Schematics".into(), parent: None };
    let modify = Op::ModifyFolder { id: folder, before: Folder { name: "Layouts".into(), parent: None }, after: renamed.clone() };
    f.ledger.commit(Author::User, "Rename folder", vec![modify]).unwrap();
    let opened = reopened(&mut f.ledger);
    assert!(opened.report.damage.is_none());
    assert_eq!(opened.ledger.full_log().unwrap(), f.ledger.full_log().unwrap());
    assert_eq!(opened.ledger.document(), f.ledger.document());
    assert_eq!(opened.ledger.document().settings, after);
    assert_eq!(opened.ledger.document().folders[&folder], renamed);
}

/// Brief 032 (FP-W1): wiring settings written before the label rule existed read with its
/// defaults.
#[test]
fn wiring_settings_without_the_label_rule_read_with_its_defaults() {
    // Not chosen, the fields are left out (brief 066, F2); such a record reads back not chosen.
    let v = serde_json::to_value(ecad_file::records::WiringSettings::from(&ecad_model::wiring::WiringSettings::default())).unwrap();
    let o = v.as_object().unwrap();
    assert!(!o.contains_key("label_rule") && !o.contains_key("label_min_length"), "{v}");
    let read: ecad_file::records::WiringSettings = serde_json::from_value(v).unwrap();
    let w = ecad_model::wiring::WiringSettings::from(read);
    // Brief 066 (F2): absent is "not chosen", which follows the built-in.
    assert_eq!((w.label_rule, w.label_min_length, w.label_height), (None, None, None));
}

/// The golden project (`tests/golden/project.ecad`, written once by this build from the fixture;
/// regenerate with `ECAD_UPDATE_GOLDEN=1` only for a new format version) still opens to the same
/// history and document: a file written by an older build keeps working.
#[test]
fn the_golden_project_opens() {
    let path = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/golden/project.ecad");
    let mut f = fixture();
    if std::env::var_os("ECAD_UPDATE_GOLDEN").is_some() {
        std::fs::write(path, written(&mut f.ledger)).unwrap();
    }
    let golden = std::fs::read(path).expect("golden file (regenerate with ECAD_UPDATE_GOLDEN=1)");
    let o = ecad_file::ProjectV2::open(ecad_file::Store::from_bytes(golden).unwrap(), SEED, WRITER).unwrap();
    assert!(o.report.damage.is_none(), "{:?}", o.report);
    assert_eq!(o.project.project_id(), PROJECT);
    assert_eq!(o.ledger.full_log().unwrap(), f.ledger.full_log().unwrap());
    assert_eq!(o.ledger.document(), f.ledger.document());
}
