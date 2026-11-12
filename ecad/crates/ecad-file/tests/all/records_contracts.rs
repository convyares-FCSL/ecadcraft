//! Records, determinism, the golden file, the hash chain seeds, embedded definitions and
//! hostile input (proposal §20.2-§20.3, §20.10-§20.11).

use crate::common;

use common::*;
use ecad_file::chain::{self, ChainKind};
use ecad_file::{Access, Outcome, ProjectFile, open_project};
use ecad_model::*;
use ecad_store::{Medium, MemoryMedium};

fn written(ledger: &mut Ledger) -> Vec<u8> {
    let file = ProjectFile::create(MemoryMedium::default(), PROJECT, ledger, Some(1_760_000_000_000), WRITER).unwrap();
    file.medium().read_all().unwrap()
}

#[test]
fn records_round_trip_exactly() {
    let mut f = fixture();
    let bytes = written(&mut f.ledger);
    let opened = open_project(&bytes, SEED).unwrap();
    assert_eq!(opened.report.outcome, Outcome::Clean);
    assert_eq!(opened.report.access, Access::Writable);
    assert_eq!(opened.ledger.log(), f.ledger.log(), "every transaction, ID, label, author and time");
    assert_eq!(opened.ledger.document(), f.ledger.document());
    assert_eq!(opened.ledger.head(), f.ledger.head());
    assert_eq!(opened.ledger.persisted(), f.ledger.head());
    assert_eq!(opened.report.verified_len, bytes.len() as u64);
    assert_eq!(opened.spans.len() as u64, f.ledger.head());
    for s in &opened.spans {
        let line = &bytes[s.offset as usize..(s.offset + s.len) as usize];
        assert!(line.ends_with(b"\n") && line.starts_with(b"{\"crc\":\""), "span of {}", s.seq);
    }
}

#[test]
fn output_is_deterministic_and_incremental_saves_match_one_write() {
    let a = written(&mut fixture().ledger);
    let b = written(&mut fixture().ledger);
    assert_eq!(a, b, "identical records give identical bytes");
    // Writing a prefix and saving the rest gives the same bytes as writing everything at once.
    let f = fixture();
    let full = f.ledger.full_log().unwrap();
    let mut prefix = Ledger::replay(full[..5].to_vec(), SEED).unwrap();
    prefix.mark_persisted(0).ok();
    let mut file = ProjectFile::create(MemoryMedium::default(), PROJECT, &mut prefix, Some(1_760_000_000_000), WRITER).unwrap();
    let mut rest = Ledger::replay(full.clone(), SEED).unwrap();
    // A ledger with the same history whose last 5.. transactions are not yet persisted.
    rest.mark_persisted(5).unwrap();
    file.save(&mut rest).unwrap();
    assert_eq!(file.medium().read_all().unwrap(), a);
    let _ = f.ledger.head();
}

#[test]
fn the_golden_file_is_stable() {
    let bytes = written(&mut fixture().ledger);
    let path = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/golden/project-v1.ecad");
    if std::env::var_os("ECAD_UPDATE_GOLDEN").is_some() {
        std::fs::write(path, &bytes).unwrap();
    }
    let golden = std::fs::read(path).expect("golden file (regenerate with ECAD_UPDATE_GOLDEN=1)");
    assert_eq!(golden, bytes, "the version 1 format changed; that needs a schema version, not an edit");
    let opened = open_project(&golden, SEED).unwrap();
    assert_eq!(opened.report.outcome, Outcome::Clean);
    assert_eq!(opened.ledger.document(), fixture().ledger.document());
}

#[test]
fn chain_seeds_are_separated_by_kind_and_id() {
    let p = chain::seed(ChainKind::Project, 7);
    assert_ne!(p, chain::seed(ChainKind::Library, 7), "a library and a project with the same ID differ");
    assert_ne!(p, chain::seed(ChainKind::Project, 8));
    assert_eq!(p, chain::seed(ChainKind::Project, 7));
    let header = ecad_file::Header::project(PROJECT, None, "w");
    assert_eq!(header.seed().unwrap(), chain::seed(ChainKind::Project, PROJECT.0));
    // The seed does not depend on the writer or time.
    let other = ecad_file::Header::project(PROJECT, Some(5), "another writer");
    assert_eq!(other.seed().unwrap(), header.seed().unwrap());
}

#[test]
fn embedded_definitions_follow_the_owner_requirements() {
    let mut f = fixture();
    let doc = f.ledger.document().clone();
    // Embed once, many instances: c1 and c2 share d1; every terminal has its own ID.
    assert_eq!(doc.components[&f.c1].definition, Some(f.d1));
    assert_eq!(doc.components[&f.c2].definition, Some(f.d1));
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
        let (rid, rep) = doc.representations_of(Subject::Component(c)).next().map(|(i, r)| (i, r.clone())).unwrap();
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
        ops.push(Op::DeleteComponent { id: c, component: doc.components[&c].clone() });
    }
    ops.push(Op::RemoveDefinition { id: f.d1, definition: doc.definitions[&f.d1].clone() });
    f.commit("Remove old version", ops);
    let bytes = written(&mut f.ledger);
    let opened = open_project(&bytes, SEED).unwrap();
    assert!(!opened.ledger.document().definitions.contains_key(&f.d1));
    let past = opened.ledger.state_at(before).unwrap();
    assert_eq!(past.definitions[&f.d1], doc.definitions[&f.d1], "history keeps the original definition");
    assert_eq!(opened.ledger.document().definitions[&f.d2], doc.definitions[&f.d2]);
}

/// xorshift64*: deterministic mutations without a dependency.
struct Rng(u64);
impl Rng {
    fn next(&mut self) -> u64 {
        self.0 ^= self.0 >> 12;
        self.0 ^= self.0 << 25;
        self.0 ^= self.0 >> 27;
        self.0.wrapping_mul(0x2545_F491_4F6C_DD1D)
    }
    fn below(&mut self, n: usize) -> usize {
        (self.next() % n.max(1) as u64) as usize
    }
}

#[test]
#[ignore = "slow: 3,000 mutated files, about 6 s"]
fn mutated_files_never_panic_and_are_always_classified() {
    let good = written(&mut fixture().ledger);
    let mut rng = Rng(0x9E37_79B9_7F4A_7C15);
    for round in 0..3000 {
        let mut b = good.clone();
        for _ in 0..1 + rng.below(3) {
            match rng.below(6) {
                0 => {
                    let i = rng.below(b.len());
                    b[i] ^= 1 << rng.below(8);
                }
                1 => b.truncate(rng.below(b.len() + 1)),
                2 => {
                    let i = rng.below(b.len() + 1);
                    let junk: Vec<u8> = (0..1 + rng.below(16)).map(|_| rng.next() as u8).collect();
                    b.splice(i..i, junk);
                }
                3 => {
                    let i = rng.below(b.len());
                    let j = (i + rng.below(64)).min(b.len());
                    b.drain(i..j);
                }
                4 => b.extend_from_slice(&good[rng.below(good.len())..]),
                _ => {
                    let i = rng.below(b.len());
                    b[i] = b'\n';
                }
            }
        }
        match open_project(&b, SEED) {
            Ok(o) => {
                assert!(o.report.verified_len <= b.len() as u64, "round {round}");
                assert!(o.ledger.document().check().is_ok(), "round {round}");
                assert_eq!(o.report.access == Access::ReadOnly, matches!(o.report.outcome, Outcome::Corrupt(_)));
                assert_eq!(o.ledger.log(), &fixture().ledger.log()[..o.ledger.log().len()], "round {round}: a prefix");
                if let Outcome::IncompleteTail { offset, len, .. } = o.report.outcome {
                    assert_eq!(offset + len, b.len() as u64);
                    assert!(!b[offset as usize..].contains(&b'\n'));
                }
            }
            Err(e) => assert!(!e.to_string().is_empty()),
        }
    }
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
    after.tag_padding = 3;
    after.tag_prefixes.insert("coil".into(), "KA".into());
    let op = Op::SetProjectSettings { before, after: after.clone() };
    let line = ecad_file::record::op_value(&op).unwrap().to_string();
    assert!(line.contains("\"tag_padding\":3") && line.contains("\"coil\":\"KA\""), "{line}");
    f.ledger.commit(Author::User, "Tag settings", vec![op]).unwrap();
    let bytes = written(&mut f.ledger);
    let opened = open_project(&bytes, SEED).unwrap();
    assert_eq!(opened.report.outcome, Outcome::Clean);
    assert_eq!(opened.ledger.document().settings, after);
    assert_eq!(opened.ledger.log(), f.ledger.log());
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
    after.wiring.digits = 3;
    after.wiring.colour_style = ecad_model::wiring::ColourStyle::ThreeLetter;
    after.wiring.label_height = 2.5;
    // Brief 032 (FP-W1): the label rule and the shortest labelled wire.
    after.wiring.label_rule = ecad_model::wiring::LABEL_RULE_EACH.into();
    after.wiring.label_min_length = 10.0;
    after.wiring.potentials.retain(|p| p.name != "PE");
    let op = Op::SetProjectSettings { before, after: after.clone() };
    let line = ecad_file::record::op_value(&op).unwrap().to_string();
    assert!(line.contains("\"page-column\"") && line.contains("\"three-letter\""), "{line}");
    f.ledger.commit(Author::User, "Wire settings", vec![op]).unwrap();

    let bare = ecad_model::Definition { name: "t".into(), origin: None, graphics: vec![], terminals: vec![], bridges: vec![] };
    let bridged = ecad_model::Definition { bridges: vec![vec!["1".into(), "2".into()]], ..bare.clone() };
    let bare_line = serde_json::to_string(&ecad_file::v1::Definition::from(&bare)).unwrap();
    assert!(!bare_line.contains("bridges"), "{bare_line}");
    assert_ne!(ecad_file::definition_hash(&bare).unwrap(), ecad_file::definition_hash(&bridged).unwrap());
    let id: ecad_model::DefinitionId = ecad_model::DefinitionId(0xb1d9e);
    f.ledger.commit(Author::User, "Bridge", vec![Op::EmbedDefinition { id, definition: bridged.clone() }]).unwrap();

    let bytes = written(&mut f.ledger);
    let opened = open_project(&bytes, SEED).unwrap();
    assert_eq!(opened.report.outcome, Outcome::Clean);
    assert_eq!(opened.ledger.document().settings, after);
    assert_eq!(opened.ledger.document().definitions.get(&id), Some(&bridged));
    assert_eq!(opened.ledger.log(), f.ledger.log());
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
    after.sheet = SheetSpec::parse("A2 12x8 numbers letters").unwrap();
    after.page_number_step = 5;
    after.reference_style = ReferenceStyle::PageColumn;
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
    let bytes = written(&mut f.ledger);
    let opened = open_project(&bytes, SEED).unwrap();
    assert_eq!(opened.report.outcome, Outcome::Clean);
    assert_eq!(opened.ledger.log(), f.ledger.log());
    assert_eq!(opened.ledger.document(), f.ledger.document());
    assert_eq!(opened.ledger.document().settings, after);
    assert_eq!(opened.ledger.document().folders[&folder], renamed);
}

/// Brief 032 (FP-W1): wiring settings written before the label rule existed read with its
/// defaults.
#[test]
fn wiring_settings_without_the_label_rule_read_with_its_defaults() {
    // At their defaults the two fields are left out (review of #65, finding 4), like their
    // neighbours; such a record reads back with the defaults.
    let v = serde_json::to_value(ecad_file::v1::WiringSettings::from(&ecad_model::wiring::WiringSettings::default())).unwrap();
    let o = v.as_object().unwrap();
    assert!(!o.contains_key("label_rule") && !o.contains_key("label_min_length"), "{v}");
    let read: ecad_file::v1::WiringSettings = serde_json::from_value(v).unwrap();
    let w = ecad_model::wiring::WiringSettings::from(read);
    assert_eq!(w.label_rule, ecad_model::wiring::DEFAULT_LABEL_RULE);
    assert_eq!(w.label_min_length, ecad_model::wiring::DEFAULT_LABEL_MIN_LENGTH);
}
