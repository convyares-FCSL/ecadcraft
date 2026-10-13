//! Records, determinism, the golden file, the hash chain seeds, embedded definitions and
//! hostile input (proposal §20.2-§20.3, §20.10-§20.11).

mod common;

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
    let full = f.ledger.log().to_vec();
    let mut prefix = Ledger::replay(full[..5].to_vec(), SEED).unwrap();
    prefix.mark_persisted(0).ok();
    let mut file =
        ProjectFile::create(MemoryMedium::default(), PROJECT, &mut prefix, Some(1_760_000_000_000), WRITER).unwrap();
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
