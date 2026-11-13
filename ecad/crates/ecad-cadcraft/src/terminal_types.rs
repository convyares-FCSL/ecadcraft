//! Multi-level terminals (brief 036, design 036 §2–§5, §7.1, §12): the terminal type catalogue,
//! the level of a terminal, the shown label, positions of a strip, offers by type, and the
//! geometry of the level starters.
//!
//! # Semantics
//! - A **position** is the strip terminals that share one position label (`Terminal.label`); each
//!   is one **level**, read from its key: `.n` (one point), `.ni`/`.no` (two-point levels: in and
//!   out). A key that does not start with `.` is level 1 of a one-level position, exactly as before
//!   this brief ([`ecad_model::links::level_of_key`]).
//! - A position's **type** is read from the starter of the symbols that draw it; the first drawn
//!   level (lowest terminal ID) decides when they differ (`position-mixed`).
//! - The **shown label** of level n of position p is the strip property `term_label.<p>.<n>`, else
//!   `p` for a one-level type, else `p` + separator + level name (`1.L`). Separator and level names
//!   are registry entries ([`crate::defaults::terminals`]), resolved strip → project → built-in.
//! - Types, codes, level counts, bridges and geometry are what a part *is*: catalogue code here,
//!   never defaults.
//!
//! Nothing here changes a project.

use std::collections::{BTreeMap, BTreeSet};

use ecad_model::links::{level_of_key, natural_key};
use ecad_model::{ComponentId, Document, Primitive, Pt, Subject, TerminalDef, TerminalId};

/// The strip property prefix of a level's own label (FP4): `term_label.<p>.<n>`.
pub const LABEL_PREFIX: &str = "term_label.";
/// The strip property prefix of a fuse position's rating (FP5): `rating.<p>`.
pub const RATING_PREFIX: &str = "rating.";
/// Most characters of a position label (`termlabel {position}`).
pub const MAX_POSITION_CHARS: usize = 16;
/// Most characters of a level's own label (`termlabel {text}`).
pub const MAX_LABEL_CHARS: usize = 24;
/// Most characters of an offer read.
pub const MAX_OFFER_CHARS: usize = 64;

/// A terminal type (design 036 §2.2).
#[derive(Debug)]
pub struct TermType {
    /// The whole-terminal starter's key (`termlnpe`); for one-level types the starter itself.
    pub key: &'static str,
    /// The registry code, lower case (`lnpe`): `terminal.<code>.levels`, `part_number.<code>`.
    pub code: &'static str,
    /// The offer code as written (`LNPE`); empty for feed-through.
    pub offer: &'static str,
    /// Its name and default BOM description.
    pub name: &'static str,
    /// The words of the plan header (`three-level`).
    pub word: &'static str,
    /// Built-in level names, one per level, top deck first.
    pub levels: &'static [&'static str],
    /// Levels joined inside the block, by key.
    pub bridges: &'static [&'static [&'static str]],
    /// Two-point levels that break the net (disconnect, fuse).
    pub breaks: bool,
    /// The level that carries the PE mark.
    pub pe_level: Option<u32>,
}

impl PartialEq for TermType {
    fn eq(&self, other: &Self) -> bool {
        self.key == other.key
    }
}

impl TermType {
    /// How many levels.
    pub fn count(&self) -> u32 {
        u32::try_from(self.levels.len()).unwrap_or(1)
    }

    /// Whether it has more than one level.
    pub fn multi(&self) -> bool {
        self.levels.len() > 1
    }

    /// The definition keys of level `n` (`.n`, or `.ni` and `.no`).
    pub fn level_keys(&self, n: u32) -> Vec<String> {
        if self.breaks { vec![format!(".{n}i"), format!(".{n}o")] } else { vec![format!(".{n}")] }
    }

    /// The starter key of level `n` (`termlnpe.2`), or of the whole terminal (`None`).
    pub fn starter_key(&self, n: Option<u32>) -> String {
        match n {
            Some(n) if self.multi() => format!("{}.{n}", self.key),
            _ => self.key.to_string(),
        }
    }

    /// Whether it is 028's feed-through or PE terminal (one level, old keys).
    pub fn legacy(&self) -> bool {
        self.key == "terminal" || self.key == crate::strips::PE_KEY
    }
}

const NONE: &[&[&str]] = &[];

/// The catalogue, in offer order (design 036 §2.2). Adding a type is a row here and its starters.
pub static TYPES: &[TermType] = &[
    TermType {
        key: "terminal",
        code: "ft",
        offer: "",
        name: "Feed-through terminal",
        word: "feed-through",
        levels: &["1"],
        bridges: NONE,
        breaks: false,
        pe_level: None,
    },
    TermType {
        key: "terminalpe",
        code: "pe",
        offer: "PE",
        name: "PE terminal",
        word: "PE",
        levels: &["1"],
        bridges: NONE,
        breaks: false,
        pe_level: Some(1),
    },
    TermType {
        key: "term2",
        code: "x2",
        offer: "x2",
        name: "Two-level terminal",
        word: "two-level",
        levels: &["1", "2"],
        bridges: NONE,
        breaks: false,
        pe_level: None,
    },
    TermType {
        key: "term2b",
        code: "x2b",
        offer: "x2B",
        name: "Two-level terminal, bridged",
        word: "two-level bridged",
        levels: &["1", "2"],
        bridges: &[&[".1", ".2"]],
        breaks: false,
        pe_level: None,
    },
    TermType {
        key: "term3",
        code: "x3",
        offer: "x3",
        name: "Three-level terminal",
        word: "three-level",
        levels: &["1", "2", "3"],
        bridges: NONE,
        breaks: false,
        pe_level: None,
    },
    TermType {
        key: "termlnpe",
        code: "lnpe",
        offer: "LNPE",
        name: "L/N/PE terminal",
        word: "L/N/PE",
        levels: &["L", "N", "PE"],
        bridges: NONE,
        breaks: false,
        pe_level: Some(3),
    },
    TermType {
        key: "termsensor",
        code: "sen",
        offer: "SEN",
        name: "Sensor terminal",
        word: "sensor",
        levels: &["S", "+", "-"],
        bridges: NONE,
        breaks: false,
        pe_level: None,
    },
    TermType {
        key: "termdisc",
        code: "dis",
        offer: "DIS",
        name: "Disconnect terminal",
        word: "disconnect",
        levels: &["1"],
        bridges: NONE,
        breaks: true,
        pe_level: None,
    },
    TermType {
        key: "termfuse",
        code: "fuse",
        offer: "FUSE",
        name: "Fuse terminal",
        word: "fuse",
        levels: &["1"],
        bridges: NONE,
        breaks: true,
        pe_level: None,
    },
];

/// How many types.
pub const TYPE_COUNT: usize = 9;

/// The feed-through type.
pub fn feedthrough() -> &'static TermType {
    &TYPES[0]
}

/// The type with registry code `code` (case-insensitive).
pub fn by_code(code: &str) -> Option<&'static TermType> {
    TYPES.iter().find(|t| t.code.eq_ignore_ascii_case(code))
}

/// The type whose offer code is `code` (case-insensitive; empty: feed-through).
pub fn by_offer(code: &str) -> Option<&'static TermType> {
    TYPES.iter().find(|t| t.offer.eq_ignore_ascii_case(code))
}

/// The type a Place Terminals prompt or JSON names: its offer code, its registry code, its key, or
/// `Feedthrough`.
pub fn by_word(word: &str) -> Option<&'static TermType> {
    let w = word.trim();
    if ["feedthrough", "feed-through", "ft", ""].iter().any(|x| x.eq_ignore_ascii_case(w)) {
        return Some(feedthrough());
    }
    TYPES.iter().find(|t| t.offer.eq_ignore_ascii_case(w) || t.code.eq_ignore_ascii_case(w) || t.key.eq_ignore_ascii_case(w))
}

/// The index of `t` in [`TYPES`].
pub fn index_of(t: &TermType) -> usize {
    TYPES.iter().position(|x| std::ptr::eq(x, t)).unwrap_or(0)
}

/// The type and level a starter draws: `termlnpe.2` → (L/N/PE, level 2); `termlnpe` → (L/N/PE,
/// `None`: the whole terminal); `terminal`, `terminalpe`, `termdisc`, `termfuse` → level 1.
pub fn of_starter(key: &str) -> Option<(&'static TermType, Option<u32>)> {
    if let Some(t) = TYPES.iter().find(|t| t.key == key) {
        return Some((t, if t.multi() { None } else { Some(1) }));
    }
    let (base, n) = key.rsplit_once('.')?;
    let t = TYPES.iter().find(|t| t.key == base && t.multi())?;
    let n: u32 = n.parse().ok()?;
    (1..=t.count()).contains(&n).then_some((t, Some(n)))
}

/// Whether `key` is one of this brief's new starters (a level, a whole terminal, disconnect, fuse).
pub fn is_level_starter(key: &str) -> bool {
    of_starter(key).is_some_and(|(t, _)| !t.legacy())
}

/// Every new starter key, in catalogue order: per multi-level type its levels then its whole
/// symbol; then disconnect and fuse.
pub fn starter_keys() -> Vec<String> {
    let mut out = Vec::new();
    for t in TYPES.iter().filter(|t| !t.legacy()) {
        if t.multi() {
            out.extend((1..=t.count()).map(|n| t.starter_key(Some(n))));
        }
        out.push(t.key.to_string());
    }
    out
}

// ---------------------------------------------------------------- terminals and positions

/// The type a terminal is drawn as: the starter of the first symbol that shows it, else (an old
/// strip's first symbol, matched by key) the strip's own definition.
pub fn type_of_terminal(doc: &Document, t: TerminalId) -> &'static TermType {
    let from_reps = doc.representations_showing(t).find_map(|(_, r)| crate::devices::rep_starter(doc, r));
    let starter = from_reps.or_else(|| {
        let c = doc.terminals.get(&t)?.component;
        let d = doc.components.get(&c)?.definition?;
        crate::symbols::of_definition(doc.definitions.get(&d)?)
    });
    starter.and_then(|s| of_starter(s.key)).map_or_else(feedthrough, |(t, _)| t)
}

/// A terminal's level and point (`.1i` → (1, `i`)); a hostile key is level 1.
pub fn level_of(doc: &Document, t: TerminalId) -> (u32, Option<char>) {
    level_of_key(doc.terminals.get(&t).and_then(|x| x.key.as_deref())).unwrap_or((1, None))
}

/// The registry value of `terminal.level.separator` for strip `c`: strip → project → built-in;
/// the word `none` is no separator.
pub fn separator(doc: &Document, c: ComponentId) -> String {
    let v = crate::defaults::resolve(doc, &crate::defaults::terminals::SEPARATOR, crate::defaults::Scope::component(c)).text();
    if v.eq_ignore_ascii_case(crate::defaults::terminals::NO_SEPARATOR) { String::new() } else { v }
}

/// The level names of type `t` on strip `c` (registry `terminal.<code>.levels`, else built-in).
pub fn level_names(doc: &Document, c: ComponentId, t: &TermType) -> Vec<String> {
    if let Some(e) = crate::defaults::terminals::levels_entry(t.code) {
        let v = crate::defaults::resolve(doc, e, crate::defaults::Scope::component(c)).text();
        let names: Vec<String> = v.split(',').map(|x| x.trim().to_string()).collect();
        if names.len() == t.levels.len() {
            return names;
        }
    }
    t.levels.iter().map(|x| x.to_string()).collect()
}

/// The name of level `n` of `t` on strip `c` (`N`).
pub fn level_name(doc: &Document, c: ComponentId, t: &TermType, n: u32) -> String {
    let i = usize::try_from(n.saturating_sub(1)).unwrap_or(0);
    level_names(doc, c, t).get(i).cloned().unwrap_or_else(|| n.to_string())
}

/// The key of a level's own label (FP4).
pub fn label_key(position: &str, n: u32) -> String {
    format!("{LABEL_PREFIX}{position}.{n}")
}

/// The key of a position's rating (FP5).
pub fn rating_key(position: &str) -> String {
    format!("{RATING_PREFIX}{position}")
}

/// The derived label of level `n` of position `p` of type `t` on strip `c` (no override).
pub fn derived_label(doc: &Document, c: ComponentId, t: &TermType, p: &str, n: u32) -> String {
    if !t.multi() {
        return p.to_string();
    }
    format!("{p}{}{}", separator(doc, c), level_name(doc, c, t, n))
}

/// The shown label of a strip terminal (design 036 §4.1); `None` for a terminal with no label.
pub fn shown_label(doc: &Document, t: TerminalId) -> Option<String> {
    let term = doc.terminals.get(&t)?;
    let p = term.label.clone()?;
    let (n, _) = level_of(doc, t);
    let c = term.component;
    if let Some(v) = doc.components.get(&c).and_then(|x| x.properties.get(&label_key(&p, n))) {
        return Some(v.clone());
    }
    if !term.key.as_deref().is_some_and(|k| k.starts_with('.')) {
        return Some(p);
    }
    Some(derived_label(doc, c, type_of_terminal(doc, t), &p, n))
}

/// One position of a strip.
#[derive(Debug, Clone, PartialEq)]
pub struct Position {
    pub label: String,
    pub ty: &'static TermType,
    /// Drawn levels: level → its terminals (one, or in and out; more when drawn twice).
    pub levels: BTreeMap<u32, Vec<TerminalId>>,
    /// Another type's symbols draw some of its levels (`position-mixed`).
    pub mixed: Vec<TerminalId>,
}

impl Position {
    /// Whether level `n` is drawn.
    pub fn has(&self, n: u32) -> bool {
        self.levels.contains_key(&n)
    }

    /// Every terminal of the position.
    pub fn terminals(&self) -> impl Iterator<Item = TerminalId> + '_ {
        self.levels.values().flatten().copied()
    }
}

/// The positions of strip `c`, in plan order (natural order of their labels).
pub fn positions(doc: &Document, c: ComponentId) -> Vec<Position> {
    let mut by_label: BTreeMap<String, Vec<TerminalId>> = BTreeMap::new();
    for (t, x) in doc.terminals_of(c) {
        if let Some(l) = &x.label {
            by_label.entry(l.clone()).or_default().push(t);
        }
    }
    let mut out: Vec<Position> = by_label
        .into_iter()
        .map(|(label, ts)| {
            let first = ts.iter().min().copied();
            let ty = first.map_or_else(feedthrough, |t| type_of_terminal(doc, t));
            let mut levels: BTreeMap<u32, Vec<TerminalId>> = BTreeMap::new();
            let mut mixed = Vec::new();
            for t in ts {
                levels.entry(level_of(doc, t).0).or_default().push(t);
                if !std::ptr::eq(type_of_terminal(doc, t), ty) {
                    mixed.push(t);
                }
            }
            Position { label, ty, levels, mixed }
        })
        .collect();
    out.sort_by_cached_key(|p| natural_key(&p.label));
    out
}

/// The position groups the numbering planner numbers (design 036 §10.3, FP8c): terminals with one
/// label, split where a level is drawn twice (the second drawing is a group of its own).
pub fn position_groups(doc: &Document, c: ComponentId) -> Vec<Vec<TerminalId>> {
    let mut out = Vec::new();
    for p in positions(doc, c) {
        let mut main: Vec<TerminalId> = Vec::new();
        let ty = p.ty;
        for ts in p.levels.values() {
            let per = if ty.breaks { 2 } else { 1 };
            let (first, rest) = ts.split_at(per.min(ts.len()));
            main.extend(first);
            for chunk in rest.chunks(per) {
                out.push(chunk.to_vec());
            }
        }
        out.push(main);
    }
    out
}

/// The highest whole-number position label of strip `c`.
pub fn highest_number(doc: &Document, c: ComponentId) -> u64 {
    doc.terminals_of(c).filter_map(|(_, t)| t.label.as_deref()?.parse::<u64>().ok()).filter(|n| *n < 1_000_000).max().unwrap_or(0)
}

/// Positions chosen for levels within one transaction (design 036 §3.3).
#[derive(Debug, Default, Clone)]
pub struct Chooser {
    /// (position, level) taken by this transaction.
    taken: BTreeSet<(String, u32)>,
    /// Positions this transaction made, with their type.
    made: BTreeMap<String, &'static TermType>,
    next: u64,
}

impl Chooser {
    /// Start for strip `c` (`None`: a new strip).
    pub fn new(doc: &Document, c: Option<ComponentId>) -> Chooser {
        Chooser { next: c.map_or(0, |c| highest_number(doc, c)) + 1, ..Chooser::default() }
    }

    /// The position for a level `n` of type `t` (`None`: a whole terminal, always new): the named
    /// position, else the lowest of that type with level `n` free, else a new one.
    ///
    /// # Errors
    /// A named position of another type, or whose level is drawn, in words with the way out.
    pub fn choose(
        &mut self, doc: &Document, c: Option<ComponentId>, t: &'static TermType, n: Option<u32>, named: Option<&str>,
    ) -> Result<String, String> {
        let ps = c.map(|c| positions(doc, c)).unwrap_or_default();
        let tag = c.map(|c| crate::devices::tag_words(doc, c)).unwrap_or_default();
        let new = |me: &mut Chooser| {
            let mut label = me.next.to_string();
            while ps.iter().any(|p| p.label == label) || me.made.contains_key(&label) {
                me.next += 1;
                label = me.next.to_string();
            }
            me.next += 1;
            me.made.insert(label.clone(), t);
            label
        };
        let Some(n) = n else {
            if let Some(name) = named {
                let name = name.trim();
                if ps.iter().any(|p| p.label == name) || self.made.contains_key(name) {
                    return Err(format!("{tag}:{name} is already drawn. A whole terminal makes a new position; press Enter for the next one."));
                }
                self.made.insert(name.to_string(), t);
                return Ok(name.to_string());
            }
            return Ok(new(self));
        };
        let free = |me: &Chooser, label: &str, ty: &TermType, has: bool| std::ptr::eq(ty, t) && !has && !me.taken.contains(&(label.to_string(), n));
        let label = match named.map(str::trim).filter(|x| !x.is_empty()) {
            Some(name) => {
                if let Some(p) = ps.iter().find(|p| p.label == name) {
                    if !std::ptr::eq(p.ty, t) {
                        return Err(format!(
                            "{tag}:{name} is a {} and this is a {}. Pick another position, or Enter for the next free one.",
                            p.ty.name.to_lowercase(),
                            t.name.to_lowercase()
                        ));
                    }
                    if p.has(n) || self.taken.contains(&(name.to_string(), n)) {
                        let lvl = c.map_or_else(|| n.to_string(), |c| level_name(doc, c, t, n));
                        let where_ = p
                            .levels
                            .get(&n)
                            .and_then(|v| v.first())
                            .and_then(|x| doc.representations_showing(*x).next())
                            .map_or_else(String::new, |(_, r)| format!(" on {}", crate::references::page_words(doc, r.page)));
                        return Err(format!(
                            "{tag}:{name} is a {} and its level {lvl} is drawn{where_}. Pick another position, or Enter for the next free one.",
                            t.name.to_lowercase()
                        ));
                    }
                    name.to_string()
                } else if let Some(ty) = self.made.get(name) {
                    if !std::ptr::eq(*ty, t) || self.taken.contains(&(name.to_string(), n)) {
                        return Err(format!("{tag}:{name} is taken in this placement. Pick another position, or Enter for the next free one."));
                    }
                    name.to_string()
                } else {
                    if name.chars().count() > MAX_POSITION_CHARS || name.chars().any(|ch| ch.is_whitespace() || ch.is_control()) {
                        return Err(format!("A position label is 1 to {MAX_POSITION_CHARS} characters, without spaces."));
                    }
                    self.made.insert(name.to_string(), t);
                    if let Ok(x) = name.parse::<u64>() {
                        self.next = self.next.max(x.saturating_add(1));
                    }
                    name.to_string()
                }
            }
            None => {
                let existing = ps.iter().find(|p| free(self, &p.label, p.ty, p.has(n))).map(|p| p.label.clone());
                let made = self.made.iter().filter(|(l, ty)| free(self, l, ty, false)).map(|(l, _)| l.clone()).min_by_key(|l| natural_key(l));
                match (existing, made) {
                    (Some(a), Some(b)) => {
                        if natural_key(&a) <= natural_key(&b) {
                            a
                        } else {
                            b
                        }
                    }
                    (Some(a), None) => a,
                    (None, Some(b)) => b,
                    (None, None) => new(self),
                }
            }
        };
        self.taken.insert((label.clone(), n));
        Ok(label)
    }
}

// ---------------------------------------------------------------- offers (FP1, §7.1)

/// Counts per type, in [`TYPES`] order.
pub type Counts = [u32; TYPE_COUNT];

fn offer_refusal(text: &str) -> String {
    let shown: String = text.chars().take(24).collect();
    let more = if text.chars().count() > 24 { "\u{2026}" } else { "" };
    format!("\"{shown}{more}\" is not a terminal count. Write 12, 12+2PE or 12+4x3 (types: PE, x2, x2B, x3, LNPE, SEN, DIS, FUSE).")
}

/// Read an offer: `+`-separated `<count><code>` items, codes case-insensitive, no code =
/// feed-through, spaces ignored.
///
/// # Errors
/// Unreadable text, an unknown code, no positions, more than [`crate::strips::MAX_POSITIONS`].
pub fn parse_counts(text: &str) -> Result<Counts, String> {
    if text.chars().count() > MAX_OFFER_CHARS {
        return Err(offer_refusal(text));
    }
    let t: String = text.chars().filter(|c| !c.is_whitespace()).collect();
    let mut counts: Counts = [0; TYPE_COUNT];
    for item in t.split('+') {
        let digits: String = item.chars().take_while(char::is_ascii_digit).collect();
        if digits.is_empty() || digits.len() > 4 {
            return Err(offer_refusal(text));
        }
        let n: u32 = digits.parse().map_err(|_| offer_refusal(text))?;
        let code = item.get(digits.len()..).unwrap_or("");
        let ty = by_offer(code).ok_or_else(|| offer_refusal(text))?;
        let i = index_of(ty);
        if let Some(c) = counts.get_mut(i) {
            *c = c.saturating_add(n);
        }
    }
    let total: u32 = counts.iter().fold(0u32, |a, b| a.saturating_add(*b));
    if total == 0 {
        return Err("A strip offers at least 1 position. Write 12, 12+2PE or 12+4x3; empty clears it.".into());
    }
    if total > crate::strips::MAX_POSITIONS {
        return Err(format!("A strip offers at most {} positions, of every type together.", crate::strips::MAX_POSITIONS));
    }
    Ok(counts)
}

/// Counts as stored: `12+2PE+4x3`, in catalogue order; feed-through first and without a code.
pub fn counts_text(c: &Counts) -> String {
    let mut parts = Vec::new();
    for (t, n) in TYPES.iter().zip(c.iter()) {
        if *n > 0 || (t.offer.is_empty() && c.iter().skip(1).all(|x| *x == 0)) {
            parts.push(format!("{n}{}", t.offer));
        }
    }
    parts.join("+")
}

// ---------------------------------------------------------------- links (§5.1)

/// The text of a link item for position `p`, level `n` as every ECAD writer writes it: `p` for
/// level 1 of an undotted position, else `p.n` (a dotted position is always written with its
/// level, roadmap question 5).
pub fn link_end(p: &str, n: u32, point: Option<char>) -> String {
    match point {
        Some(c) => format!("{p}.{n}{c}"),
        None if n == 1 && !p.contains('.') => p.to_string(),
        None => format!("{p}.{n}"),
    }
}

/// Canonical link text on level `n` for `labels` (plan order) out of `order`: runs become ranges,
/// each end written with [`link_end`].
pub fn link_text(labels: &[&String], order: &[String], n: u32) -> String {
    let base = ecad_model::links::canonical(labels, order);
    if n == 1 && !labels.iter().any(|l| l.contains('.')) {
        return base;
    }
    // Rebuild from the runs: canonical() never puts a level in, and its items are positions.
    let idx = |l: &String| order.iter().position(|o| o == l);
    let mut out: Vec<String> = Vec::new();
    let mut run: Vec<(usize, &String)> = Vec::new();
    let flush = |run: &mut Vec<(usize, &String)>, out: &mut Vec<String>| {
        match (run.first(), run.last()) {
            (Some((_, a)), Some((_, b))) if run.len() >= 2 => out.push(format!("{}-{}", link_end(a, n, None), link_end(b, n, None))),
            (Some((_, a)), _) => out.push(link_end(a, n, None)),
            _ => {}
        }
        run.clear();
    };
    for l in labels {
        match idx(l) {
            Some(i) => {
                if run.last().is_some_and(|(j, _)| j + 1 != i) {
                    flush(&mut run, &mut out);
                }
                run.push((i, l));
            }
            None => {
                flush(&mut run, &mut out);
                out.push(link_end(l, n, None));
            }
        }
    }
    flush(&mut run, &mut out);
    out.join(", ")
}

// ---------------------------------------------------------------- geometry (§3.1)

fn p(x: f64, y: f64) -> Pt {
    Pt::new(x, y)
}

fn line(a: (f64, f64), b: (f64, f64)) -> Primitive {
    Primitive::Line { a: p(a.0, a.1), b: p(b.0, b.1) }
}

fn rect(a: (f64, f64), b: (f64, f64)) -> Primitive {
    Primitive::Polyline { vertices: vec![p(a.0, a.1), p(b.0, a.1), p(b.0, b.1), p(a.0, b.1)], closed: true }
}

fn circle(c: (f64, f64), radius: f64) -> Primitive {
    Primitive::Circle { center: p(c.0, c.1), radius }
}

/// 028's earth mark beside a circle at height `y`.
fn pe_mark(y: f64) -> Vec<Primitive> {
    vec![line((2.5, y - 1.2), (2.5, y + 1.2)), line((3.2, y - 0.8), (3.2, y + 0.8)), line((3.9, y - 0.4), (3.9, y + 0.4))]
}

/// The level tick of level `n`: n short strokes 0.8 mm long above the circle, 0.6 mm apart.
fn ticks(n: u32) -> Vec<Primitive> {
    let n = n.min(9);
    let w = 0.6 * f64::from(n.saturating_sub(1));
    (0..n)
        .map(|i| {
            let x = -w / 2.0 + 0.6 * f64::from(i);
            line((x, 1.9), (x, 2.7))
        })
        .collect()
}

/// The graphics of a new starter (`None` for any other key).
pub fn graphics(key: &str) -> Option<Vec<Primitive>> {
    let (t, n) = of_starter(key).filter(|(t, _)| !t.legacy())?;
    Some(match (t.key, n) {
        ("termfuse", _) => crate::symbols::graphics_of("fuseterm"),
        ("termdisc", _) => {
            vec![rect((-3.5, -7.5), (3.5, 7.5)), line((0.0, 10.0), (0.0, 3.0)), line((0.0, 3.0), (2.5, -3.0)), line((0.0, -3.0), (0.0, -10.0))]
        }
        (_, Some(n)) => {
            let mut g = vec![circle((0.0, 0.0), 1.5)];
            g.extend(ticks(n));
            if t.pe_level == Some(n) {
                g.extend(pe_mark(0.0));
            }
            g
        }
        (_, None) => {
            let k = f64::from(t.count());
            let mut g = vec![rect((-2.5, 2.5), (2.5, 2.5 - 5.0 * k))];
            for i in 1..=t.count() {
                let y = -5.0 * f64::from(i - 1);
                g.push(circle((0.0, y), 1.5));
                if i > 1 {
                    g.push(line((-2.5, y + 2.5), (2.5, y + 2.5)));
                }
                if t.pe_level == Some(i) {
                    g.extend(pe_mark(y));
                }
            }
            for group in t.bridges {
                let ys: Vec<f64> = group.iter().filter_map(|k| k.strip_prefix('.')?.parse::<u32>().ok()).map(|n| -5.0 * f64::from(n - 1)).collect();
                if let (Some(a), Some(b)) = (ys.iter().copied().reduce(f64::max), ys.iter().copied().reduce(f64::min)) {
                    g.push(line((1.0, a - 1.2), (1.0, b + 1.2)));
                }
            }
            g
        }
    })
}

/// The connection points of a new starter (`None` for any other key).
pub fn terminals(key: &str) -> Option<Vec<TerminalDef>> {
    let (t, n) = of_starter(key).filter(|(t, _)| !t.legacy())?;
    let d = |k: String, x: f64, y: f64| TerminalDef { key: k, label: None, at: p(x, y) };
    Some(match (t.breaks, n) {
        (true, _) => vec![d(".1i".into(), 0.0, 10.0), d(".1o".into(), 0.0, -10.0)],
        (false, Some(_)) => vec![d(t.level_keys(n.unwrap_or(1)).into_iter().next().unwrap_or_default(), 0.0, 0.0)],
        (false, None) => (1..=t.count()).map(|i| d(format!(".{i}"), 0.0, -5.0 * f64::from(i - 1))).collect(),
    })
}

/// The internal bridges of a new starter's definition (§6): its type's groups.
pub fn bridges(key: &str) -> Vec<Vec<String>> {
    of_starter(key).map(|(t, _)| t.bridges.iter().map(|g| g.iter().map(|k| k.to_string()).collect()).collect()).unwrap_or_default()
}

/// Where each level of a symbol of strip `c` sits, keyed by the definition keys it shows (the
/// whole terminal: every level).
pub fn rep_type(doc: &Document, r: &ecad_model::Representation) -> Option<(&'static TermType, Option<u32>)> {
    let Subject::Component(_) = r.subject else { return None };
    of_starter(crate::devices::rep_starter(doc, r)?.key)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn starters_and_codes() {
        assert_eq!(of_starter("termlnpe.2").map(|(t, n)| (t.code, n)), Some(("lnpe", Some(2))));
        assert_eq!(of_starter("termlnpe").map(|(t, n)| (t.code, n)), Some(("lnpe", None)));
        assert_eq!(of_starter("termlnpe.4"), None);
        assert_eq!(of_starter("termfuse").map(|(t, n)| (t.code, n)), Some(("fuse", Some(1))));
        assert_eq!(starter_keys().len(), 20);
        assert!(is_level_starter("term2b.1") && !is_level_starter("terminal"));
        assert_eq!(by_word("x2b").map(|t| t.key), Some("term2b"));
        assert_eq!(by_word("Feedthrough").map(|t| t.key), Some("terminal"));
    }

    #[test]
    fn offers_by_type() {
        let c = parse_counts("12+2PE+4x3").unwrap_or_default();
        assert_eq!(counts_text(&c), "12+2PE+4x3");
        assert_eq!(c.iter().sum::<u32>(), 18);
        assert_eq!(parse_counts("12 + 2pe").map(|c| counts_text(&c)), Ok("12+2PE".into()));
        assert_eq!(parse_counts("0+2PE").map(|c| counts_text(&c)), Ok("2PE".into()));
        assert!(parse_counts("6Q3").unwrap_err().contains("types: PE, x2"));
        assert!(parse_counts("999x3").is_ok());
        assert!(parse_counts("1000x2").is_err());
        assert!(parse_counts("990+10PE").is_err());
        assert!(parse_counts(&"9".repeat(10_000)).is_err());
        assert!(parse_counts("").is_err());
    }

    #[test]
    fn link_ends() {
        assert_eq!(link_end("4", 1, None), "4");
        assert_eq!(link_end("4", 2, None), "4.2");
        assert_eq!(link_end("2.1", 1, None), "2.1.1");
        let order: Vec<String> = ["1", "2", "3", "4"].iter().map(|s| s.to_string()).collect();
        let sel: Vec<&String> = order.iter().collect();
        assert_eq!(link_text(&sel, &order, 2), "1.2-4.2");
        assert_eq!(link_text(&sel, &order, 1), "1-4");
    }
}
