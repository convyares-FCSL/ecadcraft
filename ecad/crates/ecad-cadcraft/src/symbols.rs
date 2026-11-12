//! The starter symbols (brief 018, design 018 §4): about 18 IEC 60617-style definitions built into
//! the app, embedded into a project the first time they are used.
//!
//! # Semantics
//! - Each starter symbol is a [`Definition`] with an [`Origin`]: one fixed library ID for "ECAD
//!   starter symbols", a fixed definition ID per symbol, `library_seq` 1 and the content hash of
//!   proposal §20.10. A project's embedded copy is found by that origin, never by name.
//! - What a symbol *is* (its kind, default tag prefix, aliases and default description) is kept
//!   here, in the catalogue, keyed by origin; the stored `Definition` holds none of it.
//! - Geometry is in millimetres, local frame, y up, symbol centre at (0,0), terminals on a 5 mm
//!   grid. The shapes follow IEC 60617 forms but are drawn new for ECAD.
//!
//! This module reads no project and changes nothing.

use std::collections::BTreeMap;
use std::sync::OnceLock;

use ecad_model::{Definition, DefinitionId, Document, LibraryId, Origin, Primitive, Pt, TerminalDef};

/// The library ID of the starter symbols.
pub const STARTER_LIBRARY: LibraryId = LibraryId(0x0000_ecad_0018_5ea1_0000_0000_0000_0001);

/// The terminal grid of the starter symbols, in millimetres (`ecad.symbol-grid`).
pub const SYMBOL_GRID_MM: f64 = 5.0;

/// How a symbol joins a device (design 018 §3.1).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    /// A contact: its labels follow the IEC sequence digit (`13`/`14`, `21`/`22`…). `make` for
    /// a normally-open contact.
    Contact { make: bool },
    /// A single terminal: numbered 1, 2, 3…
    Terminal,
    /// A PLC channel: the next address of its letter (`I0.0`…`I0.7`, `I1.0`…).
    Plc { letter: char },
    /// Fixed labels from the definition. `exclusive`: a device holds at most one of these.
    Fixed { exclusive: bool },
    /// A tagless component of its own, never part of a device: a junction or a continuation
    /// (brief 022, FP4).
    Tagless,
}

/// One starter symbol in the catalogue.
#[derive(Debug, Clone)]
pub struct Starter {
    /// The short key typed at prompts and used in JSON (`coil`, `no`…).
    pub key: &'static str,
    /// Its name and default description.
    pub name: &'static str,
    /// Words that also find it.
    pub aliases: &'static [&'static str],
    /// The default tag prefix (IEC 81346), without `-`.
    pub prefix: &'static str,
    pub kind: Kind,
    /// Its fixed definition ID inside [`STARTER_LIBRARY`].
    pub id: DefinitionId,
}

impl Starter {
    /// The definition with its origin, built and hashed once per run (the Place preview asks for it
    /// every frame).
    ///
    /// # Errors
    /// Its content hash could not be computed (never for the built-in symbols; a definition is
    /// never stored with a made-up origin hash).
    pub fn definition(&self) -> Result<&'static Definition, String> {
        static BUILT: OnceLock<BTreeMap<&'static str, Result<Definition, String>>> = OnceLock::new();
        let built = BUILT.get_or_init(|| STARTERS.iter().map(|s| (s.key, s.build())).collect());
        match built.get(self.key) {
            Some(Ok(d)) => Ok(d),
            Some(Err(e)) => Err(e.clone()),
            None => Err(format!("{} is not a starter symbol", self.key)),
        }
    }

    fn build(&self) -> Result<Definition, String> {
        let mut d =
            Definition { bridges: Vec::new(), name: self.name.into(), origin: None, graphics: graphics(self.key), terminals: terminals(self.key) };
        let content_hash = ecad_file::definition_hash(&d).map_err(|e| format!("The symbol {} could not be prepared ({e}).", self.name))?;
        d.origin = Some(Origin { library: STARTER_LIBRARY, definition: self.id, library_seq: 1, content_hash });
        Ok(d)
    }

    /// Whether it can join an existing device (every starter symbol can, within §3.1's rules).
    pub fn joinable(&self) -> bool {
        true
    }

    /// The labels of its terminals as the definition gives them.
    pub fn labels(&self) -> Vec<String> {
        terminals(self.key).into_iter().map(|t| t.key).collect()
    }

    /// Its box (width, height) in millimetres.
    pub fn size(&self) -> (f64, f64) {
        let Ok(d) = self.definition() else { return (0.0, 0.0) };
        let (lo, hi) = bounds(d);
        ((hi.x - lo.x).max(0.0), (hi.y - lo.y).max(0.0))
    }
}

const fn def_id(n: u128) -> DefinitionId {
    DefinitionId(0x0000_ecad_0018_5ea1_0000_0000_0001_0000 + n)
}

/// The catalogue, in table order (design 018 §4).
pub const STARTERS: &[Starter] = &[
    Starter {
        key: "coil",
        name: "Contactor / relay coil",
        aliases: &["coil", "relay", "contactor"],
        prefix: "K",
        kind: Kind::Fixed { exclusive: true },
        id: def_id(1),
    },
    Starter {
        key: "no",
        name: "NO contact",
        aliases: &["make", "normally open", "contact"],
        prefix: "K",
        kind: Kind::Contact { make: true },
        id: def_id(2),
    },
    Starter {
        key: "nc",
        name: "NC contact",
        aliases: &["break", "normally closed", "contact"],
        prefix: "K",
        kind: Kind::Contact { make: false },
        id: def_id(3),
    },
    Starter {
        key: "power3",
        name: "Power contacts, 3-pole",
        aliases: &["main contacts", "contactor"],
        prefix: "K",
        kind: Kind::Fixed { exclusive: false },
        id: def_id(4),
    },
    Starter {
        key: "mcb1",
        name: "MCB 1-pole",
        aliases: &["breaker", "circuit breaker"],
        prefix: "Q",
        kind: Kind::Fixed { exclusive: true },
        id: def_id(5),
    },
    Starter {
        key: "mcb2",
        name: "MCB 2-pole",
        aliases: &["breaker", "circuit breaker"],
        prefix: "Q",
        kind: Kind::Fixed { exclusive: true },
        id: def_id(6),
    },
    Starter { key: "fuse", name: "Fuse", aliases: &["fuse"], prefix: "F", kind: Kind::Fixed { exclusive: true }, id: def_id(7) },
    Starter { key: "psu", name: "Power supply", aliases: &["psu", "ac/dc"], prefix: "G", kind: Kind::Fixed { exclusive: true }, id: def_id(8) },
    Starter {
        key: "pbno",
        name: "Push button, NO",
        aliases: &["push", "button", "start"],
        prefix: "S",
        kind: Kind::Contact { make: true },
        id: def_id(9),
    },
    Starter {
        key: "pbnc",
        name: "Push button, NC",
        aliases: &["push", "button", "stop"],
        prefix: "S",
        kind: Kind::Contact { make: false },
        id: def_id(10),
    },
    Starter {
        key: "sel",
        name: "Selector switch, 2-position",
        aliases: &["selector", "switch"],
        prefix: "S",
        kind: Kind::Contact { make: true },
        id: def_id(11),
    },
    Starter {
        key: "lamp",
        name: "Indicator lamp",
        aliases: &["lamp", "light", "pilot"],
        prefix: "H",
        kind: Kind::Fixed { exclusive: true },
        id: def_id(12),
    },
    Starter { key: "motor3", name: "Motor, 3-phase", aliases: &["motor"], prefix: "M", kind: Kind::Fixed { exclusive: true }, id: def_id(13) },
    Starter { key: "motor1", name: "Motor, single-phase", aliases: &["motor"], prefix: "M", kind: Kind::Fixed { exclusive: true }, id: def_id(14) },
    Starter {
        key: "trafo",
        name: "Transformer",
        aliases: &["control transformer"],
        prefix: "T",
        kind: Kind::Fixed { exclusive: true },
        id: def_id(15),
    },
    Starter { key: "terminal", name: "Terminal", aliases: &["terminal", "clamp"], prefix: "X", kind: Kind::Terminal, id: def_id(16) },
    // Brief 028 (design 028 §2): a protective-earth terminal; it joins a strip like `terminal`.
    Starter {
        key: "terminalpe",
        name: "PE terminal",
        aliases: &["pe terminal", "earth terminal"],
        prefix: "X",
        kind: Kind::Terminal,
        id: def_id(21),
    },
    Starter { key: "di", name: "PLC digital input", aliases: &["plc", "input", "di"], prefix: "A", kind: Kind::Plc { letter: 'I' }, id: def_id(17) },
    Starter {
        key: "do",
        name: "PLC digital output",
        aliases: &["plc", "output", "do"],
        prefix: "A",
        kind: Kind::Plc { letter: 'Q' },
        id: def_id(18),
    },
    Starter { key: "junction", name: "Junction", aliases: &["junction", "dot", "tee"], prefix: "", kind: Kind::Tagless, id: def_id(19) },
    Starter {
        key: "continuation",
        name: "Continuation",
        aliases: &["continuation", "arrow", "potential", "link"],
        prefix: "",
        kind: Kind::Tagless,
        id: def_id(20),
    },
];

/// The starter symbol with key `key` (case-insensitive).
pub fn by_key(key: &str) -> Option<&'static Starter> {
    let k = key.trim();
    STARTERS.iter().find(|s| s.key.eq_ignore_ascii_case(k))
}

/// The starter symbol an embedded definition was made from, by its origin.
pub fn of_definition(d: &Definition) -> Option<&'static Starter> {
    let o = d.origin.as_ref()?;
    if o.library != STARTER_LIBRARY {
        return None;
    }
    STARTERS.iter().find(|s| s.id == o.definition)
}

/// The project's embedded copy of `s`, found by origin (library and definition).
pub fn embedded(doc: &Document, s: &Starter) -> Option<DefinitionId> {
    doc.definitions.iter().find(|(_, d)| d.origin.as_ref().is_some_and(|o| o.library == STARTER_LIBRARY && o.definition == s.id)).map(|(id, _)| *id)
}

/// Most symbols one search lists.
pub const MAX_SEARCH: usize = 500;

/// Most characters of a search query that are used.
pub const MAX_QUERY: usize = 60;

/// The symbols matching `query` (design 018 §5.1): a substring of the key, name or an alias, or the
/// tag prefix with or without `-`. An empty query lists every symbol, in table order.
pub fn search(query: &str) -> Vec<&'static Starter> {
    let q: String = query.trim().chars().take(MAX_QUERY).collect::<String>().to_lowercase();
    if q.is_empty() {
        return STARTERS.iter().collect();
    }
    let bare = q.trim_start_matches('-');
    STARTERS
        .iter()
        .filter(|s| {
            s.key == q
                || s.name.to_lowercase().contains(&q)
                || s.aliases.iter().any(|a| a.contains(q.as_str()))
                || (!bare.is_empty() && s.prefix.eq_ignore_ascii_case(bare))
        })
        .collect()
}

// ---------------------------------------------------------------- geometry

fn p(x: f64, y: f64) -> Pt {
    Pt::new(x, y)
}

fn line(a: (f64, f64), b: (f64, f64)) -> Primitive {
    Primitive::Line { a: p(a.0, a.1), b: p(b.0, b.1) }
}

fn poly(v: &[(f64, f64)], closed: bool) -> Primitive {
    Primitive::Polyline { vertices: v.iter().map(|(x, y)| p(*x, *y)).collect(), closed }
}

fn rect(a: (f64, f64), b: (f64, f64)) -> Primitive {
    poly(&[(a.0, a.1), (b.0, a.1), (b.0, b.1), (a.0, b.1)], true)
}

fn circle(c: (f64, f64), radius: f64) -> Primitive {
    Primitive::Circle { center: p(c.0, c.1), radius }
}

fn text(at: (f64, f64), height: f64, value: &str) -> Primitive {
    Primitive::Text { at: p(at.0, at.1), height, rotation: 0.0, value: value.into(), halign: Default::default(), valign: Default::default() }
}

fn shifted(prims: Vec<Primitive>, dx: f64) -> Vec<Primitive> {
    let m = |q: &Pt| p(q.x + dx, q.y);
    prims
        .into_iter()
        .map(|g| match g {
            Primitive::Line { a, b } => Primitive::Line { a: m(&a), b: m(&b) },
            Primitive::Polyline { vertices, closed } => Primitive::Polyline { vertices: vertices.iter().map(m).collect(), closed },
            Primitive::Circle { center, radius } => Primitive::Circle { center: m(&center), radius },
            other => other,
        })
        .collect()
}

/// A mechanical link drawn dashed (definitions carry no linetype): 1.5 mm dashes, 1 mm gaps.
fn dashed(x0: f64, x1: f64, y: f64) -> Vec<Primitive> {
    let mut out = Vec::new();
    let mut x = x0;
    // At most a few dozen dashes for the widths used here.
    while x < x1 && out.len() < 64 {
        out.push(line((x, y), ((x + 1.5).min(x1), y)));
        x += 2.5;
    }
    out
}

fn contact_no() -> Vec<Primitive> {
    vec![line((0.0, 10.0), (0.0, 4.0)), line((0.0, -10.0), (0.0, -4.0)), line((0.0, -4.0), (-4.0, 5.0))]
}

fn contact_nc() -> Vec<Primitive> {
    vec![line((0.0, 10.0), (0.0, 4.0)), line((0.0, -10.0), (0.0, -4.0)), line((0.0, 4.0), (3.0, 4.0)), line((0.0, -4.0), (4.0, 5.5))]
}

fn mcb() -> Vec<Primitive> {
    let mut g = contact_no();
    g.push(line((-1.5, 2.5), (1.5, 5.5)));
    g.push(line((-1.5, 5.5), (1.5, 2.5)));
    g
}

fn motor(phase: &str, leads: &[((f64, f64), (f64, f64))]) -> Vec<Primitive> {
    let mut g = vec![circle((0.0, 0.0), 10.0), text((-2.0, -0.5), 5.0, "M"), text((-2.5, -6.0), 3.0, phase)];
    g.extend(leads.iter().map(|(a, b)| line(*a, *b)));
    g.push(poly(&[(20.0, 15.0), (20.0, 0.0), (10.0, 0.0)], false));
    g
}

fn graphics(key: &str) -> Vec<Primitive> {
    match key {
        "coil" => vec![rect((-6.0, -3.0), (6.0, 3.0)), line((0.0, 3.0), (0.0, 10.0)), line((0.0, -3.0), (0.0, -10.0))],
        "no" => contact_no(),
        "nc" => contact_nc(),
        "power3" => {
            let mut g = Vec::new();
            for x in [-10.0, 0.0, 10.0] {
                g.extend(shifted(contact_no(), x));
            }
            g.extend(dashed(-12.0, 8.0, 0.5));
            g
        }
        "mcb1" => mcb(),
        "mcb2" => {
            let mut g = mcb();
            g.extend(shifted(mcb(), 10.0));
            g.extend(dashed(-2.0, 8.0, 0.5));
            g
        }
        "fuse" => vec![rect((-2.0, -5.0), (2.0, 5.0)), line((0.0, 10.0), (0.0, -10.0))],
        "psu" => {
            let mut g = vec![
                rect((-15.0, -10.0), (15.0, 10.0)),
                line((-15.0, -10.0), (15.0, 10.0)),
                text((-13.0, 4.0), 3.5, "~"),
                text((8.0, -8.0), 3.5, "="),
            ];
            for x in [-10.0, 0.0, 10.0] {
                g.push(line((x, 15.0), (x, 10.0)));
            }
            for x in [-5.0, 5.0] {
                g.push(line((x, -15.0), (x, -10.0)));
            }
            g
        }
        "pbno" => {
            let mut g = contact_no();
            g.extend(dashed(-8.0, -2.0, 0.5));
            g.push(poly(&[(-10.0, 3.0), (-8.0, 3.0), (-8.0, -2.0), (-10.0, -2.0)], false));
            g
        }
        "pbnc" => {
            let mut g = contact_nc();
            g.extend(dashed(-6.0, 1.9, 0.5));
            g.push(poly(&[(-8.0, 3.0), (-6.0, 3.0), (-6.0, -2.0), (-8.0, -2.0)], false));
            g
        }
        "sel" => {
            let mut g = contact_no();
            g.extend(dashed(-8.0, -2.0, 0.5));
            g.push(poly(&[(-10.0, 3.0), (-8.0, 3.0), (-8.0, -2.0), (-6.0, -2.0)], false));
            g
        }
        "lamp" => vec![
            circle((0.0, 0.0), 4.0),
            line((-2.83, -2.83), (2.83, 2.83)),
            line((-2.83, 2.83), (2.83, -2.83)),
            line((0.0, 4.0), (0.0, 10.0)),
            line((0.0, -4.0), (0.0, -10.0)),
        ],
        "motor3" => motor("3~", &[((-10.0, 15.0), (-5.0, 8.66)), ((0.0, 15.0), (0.0, 10.0)), ((10.0, 15.0), (5.0, 8.66))]),
        "motor1" => motor("1~", &[((-5.0, 15.0), (-5.0, 8.66)), ((5.0, 15.0), (5.0, 8.66))]),
        "trafo" => {
            // Where x = ±5 meets each circle of radius 6 around (0, ±4.5).
            let y = 4.5 + (36.0_f64 - 25.0).sqrt();
            vec![
                circle((0.0, 4.5), 6.0),
                circle((0.0, -4.5), 6.0),
                line((-5.0, 15.0), (-5.0, y)),
                line((5.0, 15.0), (5.0, y)),
                line((-5.0, -15.0), (-5.0, -y)),
                line((5.0, -15.0), (5.0, -y)),
            ]
        }
        "terminal" => vec![circle((0.0, 0.0), 1.5)],
        // Our own earth mark to the right of the ring (design 028 §2).
        "terminalpe" => vec![circle((0.0, 0.0), 1.5), line((2.5, -1.2), (2.5, 1.2)), line((3.2, -0.8), (3.2, 0.8)), line((3.9, -0.4), (3.9, 0.4))],
        // A filled dot (`ecad.junction-size`): concentric circles, so it reads as filled at any
        // lineweight setting (design 022 §3.1).
        "junction" => vec![circle((0.0, 0.0), 0.5), circle((0.0, 0.0), 0.33), circle((0.0, 0.0), 0.17)],
        // An arrow pointing away from its wire (design 022 §3.2).
        "continuation" => vec![line((0.0, 0.0), (0.0, -5.0)), poly(&[(-1.5, -3.5), (0.0, -5.0), (1.5, -3.5)], false)],
        "di" => vec![rect((-7.5, -2.5), (7.5, 2.5)), text((-2.5, -1.25), 2.5, "DI"), line((0.0, 2.5), (0.0, 10.0))],
        "do" => vec![rect((-7.5, -2.5), (7.5, 2.5)), text((-2.5, -1.25), 2.5, "DO"), line((0.0, -2.5), (0.0, -10.0))],
        _ => Vec::new(),
    }
}

fn terminals(key: &str) -> Vec<TerminalDef> {
    let t = |k: &str, x: f64, y: f64| TerminalDef { key: k.into(), label: Some(k.into()), at: p(x, y) };
    match key {
        "coil" => vec![t("A1", 0.0, 10.0), t("A2", 0.0, -10.0)],
        "no" | "pbno" | "sel" => vec![t("13", 0.0, 10.0), t("14", 0.0, -10.0)],
        "nc" | "pbnc" => vec![t("11", 0.0, 10.0), t("12", 0.0, -10.0)],
        "power3" => vec![t("1", -10.0, 10.0), t("2", -10.0, -10.0), t("3", 0.0, 10.0), t("4", 0.0, -10.0), t("5", 10.0, 10.0), t("6", 10.0, -10.0)],
        "mcb1" | "fuse" => vec![t("1", 0.0, 10.0), t("2", 0.0, -10.0)],
        "mcb2" => vec![t("1", 0.0, 10.0), t("2", 0.0, -10.0), t("3", 10.0, 10.0), t("4", 10.0, -10.0)],
        "psu" => {
            vec![t("L", -10.0, 15.0), t("N", 0.0, 15.0), t("PE", 10.0, 15.0), t("+", -5.0, -15.0), t("-", 5.0, -15.0)]
        }
        "lamp" => vec![t("X1", 0.0, 10.0), t("X2", 0.0, -10.0)],
        "motor3" => vec![t("U1", -10.0, 15.0), t("V1", 0.0, 15.0), t("W1", 10.0, 15.0), t("PE", 20.0, 15.0)],
        "motor1" => vec![t("U1", -5.0, 15.0), t("U2", 5.0, 15.0), t("PE", 20.0, 15.0)],
        "trafo" => vec![t("1U1", -5.0, 15.0), t("1U2", 5.0, 15.0), t("2U1", -5.0, -15.0), t("2U2", 5.0, -15.0)],
        "terminal" | "terminalpe" => vec![t("1", 0.0, 0.0)],
        // No label: a junction or continuation shows no connection point name.
        "junction" | "continuation" => vec![TerminalDef { key: "1".into(), label: None, at: p(0.0, 0.0) }],
        "di" => vec![t("I0.0", 0.0, 10.0)],
        "do" => vec![t("Q0.0", 0.0, -10.0)],
        _ => Vec::new(),
    }
}

/// The local bounding box of a definition's graphics and terminals (text counted at its insertion
/// point and height). `(0,0)`–`(0,0)` for an empty definition.
pub fn bounds(d: &Definition) -> (Pt, Pt) {
    let mut lo = p(f64::INFINITY, f64::INFINITY);
    let mut hi = p(f64::NEG_INFINITY, f64::NEG_INFINITY);
    let mut add = |q: Pt| {
        if q.is_finite() {
            lo = p(lo.x.min(q.x), lo.y.min(q.y));
            hi = p(hi.x.max(q.x), hi.y.max(q.y));
        }
    };
    for g in &d.graphics {
        match g {
            Primitive::Line { a, b } => {
                add(*a);
                add(*b);
            }
            Primitive::Polyline { vertices, .. } => vertices.iter().for_each(|v| add(*v)),
            Primitive::Circle { center, radius } | Primitive::Arc { center, radius, .. } => {
                add(p(center.x - radius, center.y - radius));
                add(p(center.x + radius, center.y + radius));
            }
            Primitive::Text { at, height, value, .. } => {
                add(*at);
                add(p(at.x + height * 0.8 * value.chars().count() as f64, at.y + height));
            }
            Primitive::Point { at } => add(*at),
        }
    }
    for t in &d.terminals {
        add(t.at);
    }
    if !(lo.is_finite() && hi.is_finite()) {
        return (p(0.0, 0.0), p(0.0, 0.0));
    }
    (lo, hi)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_starter_is_valid_and_on_the_grid() {
        let mut keys = std::collections::BTreeSet::new();
        let mut ids = std::collections::BTreeSet::new();
        for s in STARTERS {
            assert!(keys.insert(s.key) && ids.insert(s.id), "{} repeated", s.key);
            let d = s.definition().unwrap();
            assert!(!d.graphics.is_empty() && !d.terminals.is_empty(), "{}", s.key);
            for t in &d.terminals {
                let on = |v: f64| (v / SYMBOL_GRID_MM - (v / SYMBOL_GRID_MM).round()).abs() < 1e-9;
                assert!(on(t.at.x) && on(t.at.y), "{} terminal {} off the grid", s.key, t.key);
            }
            assert_eq!(of_definition(d).map(|x| x.key), Some(s.key));
            // The origin's hash is the content's.
            let o = d.origin.clone().unwrap();
            let mut bare = d.clone();
            bare.origin = None;
            assert_eq!(ecad_file::definition_hash(&bare).unwrap(), o.content_hash);
        }
        // 18 symbols (brief 018), the junction and continuation (brief 022), the PE terminal (028).
        assert_eq!(STARTERS.len(), 21);
    }

    #[test]
    fn search_table() {
        let keys = |q: &str| search(q).into_iter().map(|s| s.key).collect::<Vec<_>>();
        assert_eq!(keys("-Q"), ["mcb1", "mcb2"]);
        assert_eq!(keys("q"), ["mcb1", "mcb2"]);
        assert_eq!(keys("contact"), ["coil", "no", "nc", "power3"]);
        assert_eq!(keys("fuse"), ["fuse"]);
        assert!(keys("xyz").is_empty());
        assert_eq!(keys("").len(), 21);
        assert!(keys(&"z".repeat(10_000)).is_empty());
    }
}
