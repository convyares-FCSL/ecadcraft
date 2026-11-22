//! Brief 014, acceptance 2, kept as an ordinary test of the current translation: randomised
//! drawing edits on the small benchmark project, made the way a person makes them through the
//! engine.
//!
//! Each edit is either accepted (the log grows) or refused or recorded as nothing, and then the
//! document is exactly as before it. Enough edits of each kind must be accepted, the log must replay
//! to the live document at the end, and a project saved after the edits must reopen to the same
//! document. The sequence is fixed by [`SEED`]; `ECAD_DIFF_SEED=<n>` replays another, and the seed
//! in use is printed first.

use cadcraft_engine::Session;
use cadcraft_engine::doc::Handle;
use ecad_bench::generate::{Variant, generate, tier};
use ecad_engine::{Mapped, ProjectHandle};
use ecad_model::connectivity::derive::connection_point_positions;
use ecad_model::{ItemKind, PageId, Primitive, Pt, Subject};
use serde_json::{Value, json};
use std::collections::BTreeMap;

/// Edits to make at least (brief 014: 1,000 randomised edits).
const EDITS: u64 = 1_200;
/// Edits accepted (the log grew) at least.
const ACCEPTED: u64 = 800;
/// The random sequence used unless `ECAD_DIFF_SEED` (decimal or `0x` hex) names another.
const SEED: u64 = 0x0014_D1FF_E4E7_5EED;
/// Most edits attempted, so a run that accepts too little ends with a failure, not a hang.
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
    /// Polyline routes: handle and the grip index of their last vertex (vertex i is grip 2i).
    polylines: Vec<(Handle, usize)>,
    /// Terminal positions.
    connection_points: Vec<Pt>,
}

fn page_state(project: &ProjectHandle, s: &mut Session, page: PageId) -> Page {
    project
        .read(s, |p, pg| {
            let doc = p.ledger.document();
            let map = &p.pages[&page].map;
            let entities: Vec<Handle> = pg.entities_of(p, page).iter().map(|e| e.handle).collect();
            let mut symbols = Vec::new();
            let mut routes = Vec::new();
            let mut polylines = Vec::new();
            for h in &entities {
                let Some(Mapped::Item(item)) = map.get(*h) else { continue };
                let Some((_, r)) = doc.representation_of_item(item) else { continue };
                match r.subject {
                    Subject::Device(_) if r.items.first() == Some(&item) => symbols.push(*h),
                    // A conductor's drawing is its route's (design 072 F31).
                    Subject::Route(_) => {
                        if let Some(ItemKind::Primitive(prim)) = doc.items.get(&item).map(|i| &i.kind)
                            && let Some((a, b)) = prim.path_ends()
                        {
                            routes.push((*h, a, b));
                            if let Primitive::Polyline { vertices, .. } = prim
                                && vertices.len() >= 2
                            {
                                polylines.push((*h, 2 * (vertices.len() - 1)));
                            }
                        }
                    }
                    _ => {}
                }
            }
            let connection_points = connection_point_positions(doc, page).into_iter().map(|(_, _, p)| p).collect();
            Page { entities, symbols, routes, polylines, connection_points }
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
        0..=19 => ("move component", "move", json!({"handles": hexes(&some(rng, &st.symbols, 3)), "delta": rng.delta()})),
        // Move a component together with some routes and other entities.
        20..=27 => {
            let mut hs = some(rng, &st.symbols, 2);
            hs.extend(some(rng, &st.entities, 3));
            hs.dedup();
            ("move mixed", "move", json!({"handles": hexes(&hs), "delta": rng.delta()}))
        }
        // Move a route so that one end lands on a terminal (reconnection, M3), or anywhere.
        28..=35 => {
            let (h, a, b) = rng.pick(&st.routes)?;
            let to = rng.pick(&st.connection_points)?;
            let from = if rng.chance(50) { a } else { b };
            let delta = if rng.chance(75) { [to.x - from.x, to.y - from.y] } else { rng.delta() };
            ("move route", "move", json!({"handles": [h.hex()], "delta": delta}))
        }
        // Drag one route end by its grip (STRETCH), onto a terminal or into free space (M3).
        36..=45 => {
            let (h, last) = rng.pick(&st.polylines)?;
            let index = if rng.chance(50) { 0 } else { last };
            let to = match rng.pick(&st.connection_points) {
                Some(t) if rng.chance(70) => [t.x, t.y],
                _ => {
                    let d = rng.delta();
                    let (_, a, b) = st.routes.iter().find(|(r, ..)| *r == h).copied()?;
                    let at = if index == 0 { a } else { b };
                    [at.x + d[0] + 1.25, at.y + d[1]]
                }
            };
            ("drag one end", "grip.stretch", json!({"handle": h.hex(), "index": index, "to": to}))
        }
        // Rotate or mirror a component.
        46..=51 => {
            let h = rng.pick(&st.symbols)?;
            let at = rng.pick(&st.connection_points)?;
            if rng.chance(50) {
                let angle = [90.0, 180.0, 270.0][rng.below(3)];
                ("rotate", "rotate", json!({"handles": [h.hex()], "base": [at.x, at.y], "angle": angle}))
            } else {
                ("mirror", "mirror", json!({"handles": [h.hex()], "p1": [at.x, at.y], "p2": [at.x, at.y + 10.0], "erase": true}))
            }
        }
        // Erase anything.
        52..=61 => ("erase", "erase", json!({"handles": hexes(&some(rng, &st.entities, 2))})),
        // Copy a component with some of its neighbours' routes, or anything.
        62..=73 => {
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
            let a = rng.pick(&st.connection_points).unwrap_or(Pt::new(0.0, 0.0));
            let d = rng.delta();
            ("line", "line", json!({"points": [[a.x, a.y], [a.x + d[0] + 1.0, a.y + d[1]]]}))
        }
        // Draw a wire between two terminals (the wire command; no translation, changes the state).
        87..=92 => {
            let (a, b) = (rng.pick(&st.connection_points)?, rng.pick(&st.connection_points)?);
            ("wire", "wire", json!({"points": [[a.x, a.y], [a.x, b.y], [b.x, b.y]]}))
        }
        // Undo (keeps the page from wearing away; no translation).
        _ => ("undo", "undo", json!({"count": 1})),
    })
}

/// The small project, written in the version 2 format the app opens, and opened from it.
struct Opened {
    dir: std::path::PathBuf,
    page: PageId,
    project: ProjectHandle,
    s: Session,
}

fn open_small(tag: &str) -> Opened {
    ecad_engine::install();
    ecad_engine::cables::views::isolate_for_tests();
    let g = generate(tier("small").unwrap(), Variant::Lean).unwrap();
    let dir = std::env::temp_dir().join(format!("ecad-014-{tag}-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let v2 = dir.join("small-lean.ecad");
    ecad_bench::container::write(&g, &v2).unwrap();
    let mut s = Session::empty();
    let (project, _) = ProjectHandle::open_file_in(&mut s, v2.to_str().unwrap(), 14, ecad_engine::cables::views::Settings::none()).unwrap();
    Opened { dir, page: g.pages[0], project, s }
}

/// The log's head and the document's text.
fn state(o: &mut Opened) -> (u64, String) {
    o.project.read(&mut o.s, |p, _| (p.ledger.head(), format!("{:?}", p.ledger.document()))).unwrap()
}

/// Make random edits until at least `edits` were tried and `accepted` accepted (or
/// [`MAX_ATTEMPTS`]), checking that every edit not accepted left the document as it was. Returns
/// the number tried and accepted, overall and by kind.
fn edit_randomly(o: &mut Opened, seed: u64, edits: u64, accepted: u64) -> (usize, u64, u64, BTreeMap<&'static str, (u64, u64)>) {
    let mut rng = Rng(seed | 1);
    let mut per_kind: BTreeMap<&str, (u64, u64)> = BTreeMap::new();
    let (mut attempts, mut tried, mut took) = (0, 0, 0);
    while (tried < edits || took < accepted) && attempts < MAX_ATTEMPTS {
        attempts += 1;
        let st = page_state(&o.project, &mut o.s, o.page);
        let Some((name, cmd, params)) = edit(&mut rng, &st, &o.s) else { continue };
        let (head, before) = state(o);
        // Refusals are expected; they must change nothing.
        let _ = o.s.execute(cmd, &params);
        let (now, after) = state(o);
        let e = per_kind.entry(name).or_default();
        e.0 += 1;
        tried += 1;
        if now > head {
            e.1 += 1;
            took += 1;
        } else {
            assert_eq!(before, after, "seed {seed:#x}: {name} {params} was not recorded but changed the document");
        }
    }
    (attempts, tried, took, per_kind)
}

fn print_seed(seed: u64) {
    use std::io::Write;
    let _ = writeln!(std::io::stderr(), "translate_differential: seed {seed:#x} (rerun with ECAD_DIFF_SEED={seed:#x})");
}

#[test]
#[ignore = "slow: 1,200 random edits, about 110 s; runs at the QA gate"]
fn randomised_edits_on_the_small_project_are_recorded_or_change_nothing() {
    let mut o = open_small("differential");
    let seed = seed();
    print_seed(seed);
    let (attempts, tried, accepted, per_kind) = edit_randomly(&mut o, seed, EDITS, ACCEPTED);
    eprintln!("seed {seed:#x}: {attempts} edits attempted, {tried} made, {accepted} of them accepted");
    for (name, (n, ok)) in &per_kind {
        eprintln!("  {name:<16} {n:>5} tried {ok:>5} accepted");
    }
    assert!(tried >= EDITS, "seed {seed:#x}: only {tried} edits made in {attempts} attempts");
    assert!(accepted >= ACCEPTED, "seed {seed:#x}: only {accepted} edits accepted");
    for kind in ["move component", "move route", "drag one end", "erase", "copy", "pasteclip", "line", "rotate", "mirror"] {
        assert!(per_kind.get(kind).is_some_and(|(_, ok)| *ok > 0), "seed {seed:#x}: no {kind} was accepted");
    }
    // The project is still whole: the log replays to the live document.
    assert!(o.project.read(&mut o.s, |p, _| p.ledger.replay_matches().unwrap()).unwrap());
    let dir = o.dir.clone();
    drop(o);
    let _ = std::fs::remove_dir_all(&dir);
}

/// Edits made one at a time, saved and reopened, give the document the edits left.
#[test]
fn incremental_edits_saved_and_reopened_give_the_same_document() {
    let mut o = open_small("reopen");
    let seed = seed();
    print_seed(seed);
    let (_, _, accepted, _) = edit_randomly(&mut o, seed, 12, 8);
    assert!(accepted >= 8, "seed {seed:#x}: only {accepted} edits accepted");
    let saved = o.dir.join("saved.ecad");
    o.s.execute("saveas", &json!({"path": saved.to_string_lossy()})).unwrap();
    let live = o.project.read(&mut o.s, |p, _| serde_json::to_value(p.ledger.document()).unwrap()).unwrap();
    let mut s = Session::empty();
    let (reopened, _) = ProjectHandle::open_file_in(&mut s, saved.to_str().unwrap(), 14, ecad_engine::cables::views::Settings::none()).unwrap();
    let read = reopened.read(&mut s, |p, _| serde_json::to_value(p.ledger.document()).unwrap()).unwrap();
    assert!(live == read, "seed {seed:#x}: the reopened document differs from the one saved");
    drop(reopened);
    drop(s);
    let dir = o.dir.clone();
    drop(o);
    let _ = std::fs::remove_dir_all(&dir);
}

/// `ECAD_DIFF_SEED` if set (decimal or `0x` hex), else [`SEED`].
fn seed() -> u64 {
    let Ok(text) = std::env::var("ECAD_DIFF_SEED") else { return SEED };
    let text = text.trim();
    let parsed = match text.strip_prefix("0x").or_else(|| text.strip_prefix("0X")) {
        Some(hex) => u64::from_str_radix(&hex.replace('_', ""), 16),
        None => text.replace('_', "").parse(),
    };
    parsed.unwrap_or_else(|_| panic!("ECAD_DIFF_SEED={text:?} is not a number"))
}
