//! Brief 014, acceptance 2: the scoped drawing-edit translation gives exactly the ops of the
//! earlier whole-document code, on a randomised set of edits on the small benchmark project.
//!
//! The comparison itself is `ecad-cadcraft`'s `translate-oracle` feature, on for this test through
//! the dev-dependencies: every translation also runs the pre-014 code on the same inputs and
//! aborts the process on any difference (ops, new handles, amended handles, summary or refusal,
//! ID allocator state). This test makes the edits, the way a person makes them through the engine,
//! and checks that enough translations were compared. The same oracle runs under every existing
//! `ecad-cadcraft` test.

use cadcraft_engine::Session;
use cadcraft_engine::doc::Handle;
use ecad_bench::generate::{Variant, generate, tier};
use ecad_cadcraft::{Mapped, ProjectHandle, translate_oracle};
use ecad_model::derive::{route_on_page, terminal_positions};
use ecad_model::{PageId, Pt, Subject};
use serde_json::{Value, json};

/// Translations to compare at least (brief 014: 1,000 randomised edits).
const EDITS: u64 = 1_200;
/// Most edits attempted, so a run that compares too little ends with a failure, not a hang.
const MAX_ATTEMPTS: usize = 20_000;

/// A small deterministic generator (xorshift64*), so a failure can be replayed.
struct Rng(u64);

impl Rng {
    fn next(&mut self) -> u64 {
        self.0 ^= self.0 >> 12;
        self.0 ^= self.0 << 25;
        self.0 ^= self.0 >> 27;
        self.0.wrapping_mul(0x2545_F491_4F6C_DD1D)
    }

    fn below(&mut self, n: usize) -> usize {
        if n == 0 { 0 } else { (self.next() % n as u64) as usize }
    }

    fn pick<T: Clone>(&mut self, v: &[T]) -> Option<T> {
        v.get(self.below(v.len())).cloned()
    }

    fn chance(&mut self, percent: u64) -> bool {
        self.next() % 100 < percent
    }

    fn delta(&mut self) -> [f64; 2] {
        const STEPS: [f64; 7] = [-10.0, -5.0, -2.5, 0.0, 2.5, 5.0, 10.0];
        [STEPS[self.below(STEPS.len())], STEPS[self.below(STEPS.len())]]
    }
}

/// What the edits pick from on page 1.
struct Page {
    /// Every model-space entity: symbols, routes, markers, slashes, plain lines.
    entities: Vec<Handle>,
    /// Symbol handles of components.
    symbols: Vec<Handle>,
    /// Route handles of conductors with their path ends.
    routes: Vec<(Handle, Pt, Pt)>,
    /// Terminal positions.
    terminals: Vec<Pt>,
}

fn page_state(project: &ProjectHandle, s: &mut Session, page: PageId) -> Page {
    project
        .read(s, |p, pg| {
            let doc = p.ledger.document();
            let map = &p.pages[&page].map;
            let entities: Vec<Handle> = pg.entities_of(p, page).iter().map(|e| e.handle).collect();
            let mut symbols = Vec::new();
            let mut routes = Vec::new();
            for h in &entities {
                let Some(Mapped::Item(item)) = map.get(*h) else { continue };
                let Some((_, r)) = doc.representation_of_item(item) else { continue };
                match r.subject {
                    Subject::Component(_) if r.items.first() == Some(&item) => symbols.push(*h),
                    Subject::Conductor(c) => {
                        if let Some((id, prim)) = route_on_page(doc, c, page)
                            && id == item
                            && let Some((a, b)) = prim.path_ends()
                        {
                            routes.push((*h, a, b));
                        }
                    }
                    _ => {}
                }
            }
            let terminals = terminal_positions(doc, page).into_iter().map(|(_, _, p)| p).collect();
            Page { entities, symbols, routes, terminals }
        })
        .unwrap()
}

fn hexes(hs: &[Handle]) -> Vec<String> {
    hs.iter().map(|h| h.hex()).collect()
}

/// A few distinct handles from `from`.
fn some(rng: &mut Rng, from: &[Handle], most: usize) -> Vec<Handle> {
    let mut out = Vec::new();
    for _ in 0..1 + rng.below(most) {
        if let Some(h) = rng.pick(from)
            && !out.contains(&h)
        {
            out.push(h);
        }
    }
    out
}

/// One random edit: its name and JSON command, or `None` when page 1 offers nothing to edit.
fn edit(rng: &mut Rng, st: &Page, s: &Session) -> Option<(&'static str, &'static str, Value)> {
    let kind = rng.below(100);
    Some(match kind {
        // Move components (their wires follow, M2).
        0..=21 => {
            ("move component", "move", json!({"handles": hexes(&some(rng, &st.symbols, 3)), "delta": rng.delta()}))
        }
        // Move a component together with some routes and other entities.
        22..=29 => {
            let mut hs = some(rng, &st.symbols, 2);
            hs.extend(some(rng, &st.entities, 3));
            hs.dedup();
            ("move mixed", "move", json!({"handles": hexes(&hs), "delta": rng.delta()}))
        }
        // Move a route so that one end lands on a terminal (reconnection, M3), or anywhere.
        30..=41 => {
            let (h, a, b) = rng.pick(&st.routes)?;
            let to = rng.pick(&st.terminals)?;
            let from = if rng.chance(50) { a } else { b };
            let delta = if rng.chance(75) { [to.x - from.x, to.y - from.y] } else { rng.delta() };
            ("move route", "move", json!({"handles": [h.hex()], "delta": delta}))
        }
        // Rotate or mirror a component.
        42..=47 => {
            let h = rng.pick(&st.symbols)?;
            let at = rng.pick(&st.terminals)?;
            if rng.chance(50) {
                let angle = [90.0, 180.0, 270.0][rng.below(3)];
                ("rotate", "rotate", json!({"handles": [h.hex()], "base": [at.x, at.y], "angle": angle}))
            } else {
                (
                    "mirror",
                    "mirror",
                    json!({"handles": [h.hex()], "p1": [at.x, at.y], "p2": [at.x, at.y + 10.0], "erase": true}),
                )
            }
        }
        // Erase anything.
        48..=59 => ("erase", "erase", json!({"handles": hexes(&some(rng, &st.entities, 2))})),
        // Copy a component with some of its neighbours' routes, or anything.
        60..=73 => {
            let mut hs = some(rng, &st.symbols, 2);
            for _ in 0..rng.below(4) {
                if let Some((h, ..)) = rng.pick(&st.routes) {
                    hs.push(h);
                }
            }
            if rng.chance(25) {
                hs = some(rng, &st.entities, 4);
            }
            hs.sort();
            hs.dedup();
            ("copy", "copy", json!({"handles": hexes(&hs), "delta": [rng.delta()[0] * 4.0, -60.0]}))
        }
        // Copy to the clipboard and paste.
        74..=77 => {
            let hs = some(rng, &st.symbols, 2);
            let _ = s;
            ("copyclip", "copyclip", json!({"handles": hexes(&hs)}))
        }
        78..=81 => ("pasteclip", "pasteclip", json!({"at": [s.clipboard_base.x + 20.0, s.clipboard_base.y - 80.0]})),
        // Draw a plain line, sometimes from a terminal.
        82..=86 => {
            let a = rng.pick(&st.terminals).unwrap_or(Pt::new(0.0, 0.0));
            let d = rng.delta();
            ("line", "line", json!({"points": [[a.x, a.y], [a.x + d[0] + 1.0, a.y + d[1]]]}))
        }
        // Draw a wire between two terminals (the wire command; no translation, changes the state).
        87..=92 => {
            let (a, b) = (rng.pick(&st.terminals)?, rng.pick(&st.terminals)?);
            ("wire", "wire", json!({"points": [[a.x, a.y], [a.x, b.y], [b.x, b.y]]}))
        }
        // Undo (keeps the page from wearing away; no translation).
        _ => ("undo", "undo", json!({"count": 1})),
    })
}

#[test]
fn randomised_edits_on_the_small_project_give_the_ops_of_the_old_translation() {
    ecad_cadcraft::install();
    ecad_cadcraft::views::isolate_for_tests();
    let g = generate(tier("small").unwrap(), Variant::Lean).unwrap();
    let dir = std::env::temp_dir().join(format!("ecad-014-differential-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("small-lean.ecad");
    std::fs::write(&path, g.file_bytes().unwrap()).unwrap();

    let mut s = Session::empty();
    let (project, _) =
        ProjectHandle::open_file_in(&mut s, path.to_str().unwrap(), 14, ecad_cadcraft::views::Settings::none())
            .unwrap();
    let page = g.pages[0];
    let mut rng = Rng(0x0014_D1FF_E4E7_5EED);
    let start = translate_oracle::comparisons();
    let mut per_kind: std::collections::BTreeMap<&str, (u64, u64)> = Default::default();
    let mut attempts = 0;
    while translate_oracle::comparisons() - start < EDITS && attempts < MAX_ATTEMPTS {
        attempts += 1;
        let st = page_state(&project, &mut s, page);
        let Some((name, cmd, params)) = edit(&mut rng, &st, &s) else { continue };
        let before = translate_oracle::comparisons();
        // Refusals are expected; the oracle compares them too.
        let _ = s.execute(cmd, &params);
        let e = per_kind.entry(name).or_default();
        e.0 += 1;
        e.1 += translate_oracle::comparisons() - before;
    }
    let compared = translate_oracle::comparisons() - start;
    eprintln!("{attempts} edits attempted, {compared} translations compared with the oracle");
    for (name, (tried, cmp)) in &per_kind {
        eprintln!("  {name:<16} {tried:>5} tried {cmp:>5} compared");
    }
    assert!(compared >= EDITS, "only {compared} translations compared in {attempts} edits");
    for kind in ["move component", "move route", "erase", "copy", "pasteclip", "line", "rotate", "mirror"] {
        assert!(per_kind.get(kind).is_some_and(|(_, c)| *c > 0), "no {kind} was translated");
    }
    // The project is still whole: the log replays to the live document.
    assert!(project.read(&mut s, |p, _| p.ledger.replay_matches().unwrap()).unwrap());
    drop(project);
    drop(s);
    let _ = std::fs::remove_dir_all(&dir);
}
