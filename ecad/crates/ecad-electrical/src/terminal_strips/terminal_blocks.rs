//! Multi-level terminals (brief 036, design 036 §2–§5, §7.1, §12): the terminal type catalogue,
//! the level of a terminal, the shown label, positions of a strip, offers by type, and the
//! geometry of the level starters.
//!
//! # Semantics
//! - A **position** is the strip terminals that share one position label (`ConnectionPoint.label`); each
//!   is one **level**, read from its key: `.n` (one point), `.ni`/`.no` (two-point levels: in and
//!   out). A key that does not start with `.` is level 1 of a one-level position, exactly as before
//!   this brief ([`ecad_model::connectivity::jumper_bars::level_of_key`]).
//! - A position's **type** is read from the starter of the symbols that draw it; the first drawn
//!   level (lowest terminal ID) decides when they differ (`position-mixed`).
//! - The **shown label** of level n of position p is the strip property `term_label.<p>.<n>`, else
//!   `p` for a one-level type, else `p` + separator + level name (`1.L`). Separator and level names
//!   are registry entries ([`crate::defaults::connection_points`]), resolved strip → project → built-in.
//! - Types, codes, level counts, bridges and geometry are what a part *is*: catalogue code here,
//!   never defaults.
//!
//! Nothing here changes a project.

use std::collections::{BTreeMap, BTreeSet};

use ecad_model::connectivity::jumper_bars::{level_of_key, natural_key};
use ecad_model::{ConnectionPointDef, ConnectionPointId, DeviceId, Document, LevelIo, PointKey, Primitive, Pt, Subject};

use crate::CmdError;
use crate::symbols::StarterKey;

/// The strip property prefix of a level's own label (FP4): `term_label.<p>.<n>`.
pub const LABEL_PREFIX: &str = "term_label.";
/// The `setprop` key prefix of a fuse position's rating (FP5): `rating.<p>`. The rating is the
/// strip's typed field ([`ecad_model::objects::devices::TerminalStrip::fuses`], design 072 §2.5), not a
/// property.
pub(crate) const RATING_PREFIX: &str = ecad_model::objects::devices::RATING_OF_PREFIX;
/// Every strip property keyed by a position label (review M1): what renumbering and `terminalblocklabel`
/// move with a position, in one list, so a prefix added later is never left behind. `acc.after.`
/// and `acc.on.` are brief 030's placements, `term_label.` this brief's; `jumper_bar.` values name
/// positions inside and are rewritten, not moved ([`crate::terminal_strips::accessories::renumber_ops`]). A fuse
/// position's rating is the strip's field and moves with [`moved_fuses`].
pub const POSITION_KEYED: &[&str] =
    &[crate::terminal_strips::accessories::AFTER_PREFIX, crate::terminal_strips::accessories::ON_PREFIX, LABEL_PREFIX];

/// Most characters of a position label (`terminalblocklabel {position}`).
pub const MAX_POSITION_CHARS: usize = 16;
/// Most characters of a level's own label (`terminalblocklabel {text}`).
pub const MAX_LABEL_CHARS: usize = 24;
/// Most characters of an offer read.
pub(crate) const MAX_OFFER_CHARS: usize = 64;

/// A terminal type (design 036 §2.2).
#[derive(Debug)]
pub struct TermType {
    /// The whole-terminal starter (`termlnpe`); for one-level types the starter itself.
    pub key: StarterKey,
    /// The starter of each level, level 1 first (`termlnpe.1`…); empty for a one-level type.
    pub level_starters: &'static [StarterKey],
    /// The registry code, lower case (`lnpe`): `terminal_block.<code>.levels`, `part_number.<code>`.
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
    pub bridges: &'static [&'static [PointKey]],
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
    pub(crate) fn level_keys(&self, n: u32) -> Vec<PointKey> {
        let level = u8::try_from(n).unwrap_or(u8::MAX);
        if self.breaks {
            vec![PointKey::Level { level, io: Some(LevelIo::In) }, PointKey::Level { level, io: Some(LevelIo::Out) }]
        } else {
            vec![PointKey::Level { level, io: None }]
        }
    }

    /// The starter of level `n` (`termlnpe.2`), or of the whole terminal (`None`).
    pub fn starter_key(&self, n: Option<u32>) -> StarterKey {
        n.filter(|_| self.multi()).and_then(|n| self.level_starters.get(usize::try_from(n).ok()?.checked_sub(1)?)).copied().unwrap_or(self.key)
    }

    /// Whether it is 028's feed-through or PE terminal (one level, old keys).
    pub fn legacy(&self) -> bool {
        self.key == StarterKey::TerminalBlock || self.key == crate::terminal_strips::PE_KEY
    }
}

const NONE: &[&[PointKey]] = &[];

/// Level `n` of a strip terminal as a point key (`.n`).
const fn level(n: u8) -> PointKey {
    PointKey::Level { level: n, io: None }
}

/// The catalogue, in offer order (design 036 §2.2). Adding a type is a row here and its starters.
pub(crate) static TYPES: &[TermType] = &[
    TermType {
        key: StarterKey::TerminalBlock,
        level_starters: &[],
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
        key: StarterKey::TerminalBlockPe,
        level_starters: &[],
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
        key: StarterKey::Term2,
        level_starters: &[StarterKey::Term2L1, StarterKey::Term2L2],
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
        key: StarterKey::Term2b,
        level_starters: &[StarterKey::Term2bL1, StarterKey::Term2bL2],
        code: "x2b",
        offer: "x2B",
        name: "Two-level terminal, bridged",
        word: "two-level bridged",
        levels: &["1", "2"],
        bridges: &[&[level(1), level(2)]],
        breaks: false,
        pe_level: None,
    },
    TermType {
        key: StarterKey::Term3,
        level_starters: &[StarterKey::Term3L1, StarterKey::Term3L2, StarterKey::Term3L3],
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
        key: StarterKey::Termlnpe,
        level_starters: &[StarterKey::TermlnpeL1, StarterKey::TermlnpeL2, StarterKey::TermlnpeL3],
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
        key: StarterKey::Termsensor,
        level_starters: &[StarterKey::TermsensorL1, StarterKey::TermsensorL2, StarterKey::TermsensorL3],
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
        key: StarterKey::Termdisc,
        level_starters: &[],
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
        key: StarterKey::Terminalblockfuse,
        level_starters: &[],
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
pub(crate) const TYPE_COUNT: usize = 9;

/// The feed-through type.
pub fn feedthrough() -> &'static TermType {
    &TYPES[0]
}

/// The type with registry code `code` (case-insensitive).
pub fn by_code(code: &str) -> Option<&'static TermType> {
    TYPES.iter().find(|t| t.code.eq_ignore_ascii_case(code))
}

/// The type whose offer code is `code` (case-insensitive; empty: feed-through).
pub(crate) fn by_offer(code: &str) -> Option<&'static TermType> {
    TYPES.iter().find(|t| t.offer.eq_ignore_ascii_case(code))
}

/// The type a Place Terminals prompt or JSON names: its offer code, its registry code, its key, or
/// `Feedthrough`.
pub fn by_word(word: &str) -> Option<&'static TermType> {
    let w = word.trim();
    if ["feedthrough", "feed-through", "ft", ""].iter().any(|x| x.eq_ignore_ascii_case(w)) {
        return Some(feedthrough());
    }
    TYPES.iter().find(|t| t.offer.eq_ignore_ascii_case(w) || t.code.eq_ignore_ascii_case(w) || ecad_model::Word::word(t.key).eq_ignore_ascii_case(w))
}

/// The index of `t` in [`TYPES`].
pub fn index_of(t: &TermType) -> usize {
    TYPES.iter().position(|x| std::ptr::eq(x, t)).unwrap_or(0)
}

/// The type and level a starter draws: `termlnpe.2` → (L/N/PE, level 2); `termlnpe` → (L/N/PE,
/// `None`: the whole terminal); `connection_point`, `terminal_block_pe`, `termdisc`, `terminalblockfuse` → level 1.
pub fn of_starter(key: StarterKey) -> Option<(&'static TermType, Option<u32>)> {
    TYPES.iter().find_map(|t| {
        if t.key == key {
            return Some((t, if t.multi() { None } else { Some(1) }));
        }
        let i = t.level_starters.iter().position(|k| *k == key)?;
        Some((t, Some(u32::try_from(i).ok()? + 1)))
    })
}

/// Whether `key` is one of this brief's new starters (a level, a whole terminal, disconnect, fuse).
pub fn is_level_starter(key: crate::symbols::StarterKey) -> bool {
    of_starter(key).is_some_and(|(t, _)| !t.legacy())
}

/// Every new starter key, in catalogue order: per multi-level type its levels then its whole
/// symbol; then disconnect and fuse.
#[cfg(test)]
pub(crate) fn starter_keys() -> Vec<StarterKey> {
    let mut out = Vec::new();
    for t in TYPES.iter().filter(|t| !t.legacy()) {
        if t.multi() {
            out.extend((1..=t.count()).map(|n| t.starter_key(Some(n))));
        }
        out.push(t.key);
    }
    out
}

// ---------------------------------------------------------------- terminals and positions

/// The type a terminal is drawn as: the starter of the first symbol that shows it, else (an old
/// strip's first symbol, matched by key) the strip's own definition.
pub(crate) fn type_of_connection_point(doc: &Document, t: ConnectionPointId) -> &'static TermType {
    let from_reps = doc.representations_showing(t).find_map(|(_, r)| crate::devices::rep_starter(doc, r));
    let starter = from_reps.or_else(|| {
        let c = doc.connection_points.get(&t)?.device()?;
        let d = doc.devices.get(&c)?.definition?;
        crate::symbols::of_definition(doc.definitions.get(&d)?)
    });
    starter.and_then(|s| of_starter(s.key)).map_or_else(feedthrough, |(t, _)| t)
}

/// A terminal's level and point (`.1i` → (1, `i`)); a hostile key is level 1.
pub fn level_of(doc: &Document, t: ConnectionPointId) -> (u32, Option<char>) {
    level_of_key(doc.connection_points.get(&t).and_then(|x| x.key.as_ref())).unwrap_or((1, None))
}

/// The registry value of `terminal_block.level.separator` for strip `c`: strip → project → built-in;
/// the word `none` is no separator.
pub fn separator(doc: &Document, c: DeviceId) -> String {
    let v = crate::defaults::resolve(doc, &crate::defaults::terminal_blocks::SEPARATOR, crate::defaults::Scope::component(c)).text();
    if v.eq_ignore_ascii_case(crate::defaults::terminal_blocks::NO_SEPARATOR) { String::new() } else { v }
}

/// The level names of type `t` on strip `c` (registry `terminal_block.<code>.levels`, else built-in).
pub fn level_names(doc: &Document, c: DeviceId, t: &TermType) -> Vec<String> {
    if let Some(e) = crate::defaults::terminal_blocks::levels_entry(t.code) {
        let v = crate::defaults::resolve(doc, e, crate::defaults::Scope::component(c)).text();
        let names: Vec<String> = v.split(',').map(|x| x.trim().to_string()).collect();
        if names.len() == t.levels.len() {
            return names;
        }
    }
    t.levels.iter().map(|x| x.to_string()).collect()
}

/// The name of level `n` of `t` on strip `c` (`N`).
pub fn level_name(doc: &Document, c: DeviceId, t: &TermType, n: u32) -> String {
    let i = usize::try_from(n.saturating_sub(1)).unwrap_or(0);
    level_names(doc, c, t).get(i).cloned().unwrap_or_else(|| n.to_string())
}

/// The key of a level's own label (FP4).
pub fn label_key(position: &str, n: u32) -> String {
    format!("{LABEL_PREFIX}{position}.{n}")
}

/// The key of a position's rating (FP5).
pub(crate) fn rating_key(position: &str) -> String {
    format!("{RATING_PREFIX}{position}")
}

/// Whether strip `d` has a fuse position rating on a label `map` (old → new) moves.
pub(crate) fn fuses_move(d: &ecad_model::Device, map: &BTreeMap<String, String>) -> bool {
    match &d.device_type {
        ecad_model::DeviceType::TerminalStrip(s) => s.fuses.keys().any(|l| map.contains_key(l.as_str())),
        _ => false,
    }
}

/// Strip `d` with its fuse position ratings relabelled by `map` (old → new), and a preview line
/// per move (`rating rating.2 → rating.7`). A rating whose new label is no valid position label
/// stays where it is, and says so.
pub(crate) fn moved_fuses(d: &ecad_model::Device, map: &BTreeMap<String, String>) -> (ecad_model::Device, Vec<String>) {
    let ecad_model::DeviceType::TerminalStrip(s) = &d.device_type else { return (d.clone(), Vec::new()) };
    let mut lines = Vec::new();
    let mut fuses = BTreeMap::new();
    for (l, f) in &s.fuses {
        let label = match map.get(l.as_str()) {
            Some(n) => match ecad_model::objects::devices::PositionLabel::new(n) {
                Ok(nl) => {
                    lines.push(format!("rating {} → {}", rating_key(l.as_str()), rating_key(nl.as_str())));
                    nl
                }
                Err(_) => {
                    let shown: String = n.chars().take(MAX_POSITION_CHARS).collect();
                    lines.push(format!("rating {} stays: {shown} is no position label", rating_key(l.as_str())));
                    l.clone()
                }
            },
            None => l.clone(),
        };
        fuses.insert(label, f.clone());
    }
    let strip = ecad_model::objects::devices::TerminalStrip { fuses };
    (ecad_model::Device { device_type: ecad_model::DeviceType::TerminalStrip(strip), ..d.clone() }, lines)
}

/// The derived label of level `n` of position `p` of type `t` on strip `c` (no override).
pub fn derived_label(doc: &Document, c: DeviceId, t: &TermType, p: &str, n: u32) -> String {
    if !t.multi() {
        return p.to_string();
    }
    format!("{p}{}{}", separator(doc, c), level_name(doc, c, t, n))
}

/// The shown label of a strip terminal (design 036 §4.1); `None` for a terminal with no label.
pub fn shown_label(doc: &Document, t: ConnectionPointId) -> Option<String> {
    let term = doc.connection_points.get(&t)?;
    let p = term.label.clone()?;
    let (n, _) = level_of(doc, t);
    let c = term.device()?;
    if let Some(v) = doc.devices.get(&c).and_then(|x| x.properties.get(&label_key(&p, n))) {
        return Some(v.clone());
    }
    if !term.key.as_ref().is_some_and(|k| k.as_def().is_none()) {
        return Some(p);
    }
    // Brief 044 (FP6): only a strip's `.`-keys are levels; a connector pin (`.s`, `.p`) is named
    // by its half (`-X10.S:3`, design 044 §0).
    if crate::terminal_strips::strip_of(doc, c).is_none() {
        return Some(crate::connectors::pin_name(doc, t).unwrap_or(p));
    }
    Some(derived_label(doc, c, type_of_connection_point(doc, t), &p, n))
}

/// The ops removing strip `c`'s position-keyed properties that `dropped` terminals leave without
/// a terminal (review M1, no silent data loss): `term_label.<p>.<n>` when level `n` of `p` has no
/// terminal left, and the fuse rating of position `p` (the strip's field) when `p` has none; with
/// a note for each.
pub(crate) fn orphaned_key_ops(doc: &Document, c: DeviceId, dropped: &[ConnectionPointId]) -> (Vec<ecad_model::Op>, Vec<String>) {
    let (mut ops, mut notes) = (Vec::new(), Vec::new());
    let Some(props) = doc.devices.get(&c).map(|x| &x.properties) else { return (ops, notes) };
    if dropped.is_empty() || crate::terminal_strips::strip_of(doc, c).is_none() {
        return (ops, notes);
    }
    let gone: BTreeSet<ConnectionPointId> = dropped.iter().copied().collect();
    let left: Vec<(String, u32)> =
        doc.connection_points_of(c).filter(|(t, _)| !gone.contains(t)).filter_map(|(t, x)| Some((x.label.clone()?, level_of(doc, t).0))).collect();
    let tag = crate::devices::tag_words(doc, c);
    let remove = |key: String, ops: &mut Vec<ecad_model::Op>, notes: &mut Vec<String>| {
        if let Some(v) = props.get(&key)
            && !ops.iter().any(|o| matches!(o, ecad_model::Op::SetProperty { key: k, .. } if *k == key))
        {
            notes.push(format!("{tag}: {key} ({v}) removed with its terminal; U brings it back."));
            ops.push(ecad_model::Op::SetProperty { object: ecad_model::ObjectRef::Device(c), key, before: Some(v.clone()), after: None });
        }
    };
    let mut gone_positions: BTreeSet<String> = BTreeSet::new();
    for t in dropped {
        let Some(p) = doc.connection_points.get(t).and_then(|x| x.label.clone()) else { continue };
        let n = level_of(doc, *t).0;
        if !left.iter().any(|(l, m)| *l == p && *m == n) {
            remove(label_key(&p, n), &mut ops, &mut notes);
        }
        if !left.iter().any(|(l, _)| *l == p) {
            gone_positions.insert(p);
        }
    }
    // Design 072 §2.5: a fuse position's rating is the strip's field; one `ModifyDevice`, first.
    if let Some(d) = doc.devices.get(&c)
        && let ecad_model::DeviceType::TerminalStrip(s) = &d.device_type
    {
        let mut strip = s.clone();
        for (l, f) in &s.fuses {
            if gone_positions.contains(l.as_str()) {
                notes.push(format!("{tag}: {} ({f}) removed with its terminal; U brings it back.", rating_key(l.as_str())));
                strip.fuses.remove(l);
            }
        }
        if strip != *s {
            let after = ecad_model::Device { device_type: ecad_model::DeviceType::TerminalStrip(strip), ..d.clone() };
            ops.insert(0, ecad_model::Op::ModifyDevice { id: c, before: d.clone(), after });
        }
    }
    (ops, notes)
}

/// One position of a strip.
#[derive(Debug, Clone, PartialEq)]
pub struct Position {
    pub label: String,
    pub ty: &'static TermType,
    /// Drawn levels: level → its terminals (one, or in and out; more when drawn twice).
    pub levels: BTreeMap<u32, Vec<ConnectionPointId>>,
    /// Another type's symbols draw some of its levels (`position-mixed`).
    pub mixed: Vec<ConnectionPointId>,
}

impl Position {
    /// Whether level `n` is drawn.
    pub fn has(&self, n: u32) -> bool {
        self.levels.contains_key(&n)
    }

    /// Every terminal of the position.
    pub fn connection_points(&self) -> impl Iterator<Item = ConnectionPointId> + '_ {
        self.levels.values().flatten().copied()
    }
}

/// The positions of strip `c`, in plan order (natural order of their labels).
pub fn positions(doc: &Document, c: DeviceId) -> Vec<Position> {
    let mut by_label: BTreeMap<String, Vec<ConnectionPointId>> = BTreeMap::new();
    for (t, x) in doc.connection_points_of(c) {
        if let Some(l) = &x.label {
            by_label.entry(l.clone()).or_default().push(t);
        }
    }
    let mut out: Vec<Position> = by_label
        .into_iter()
        .map(|(label, ts)| {
            let first = ts.iter().min().copied();
            let ty = first.map_or_else(feedthrough, |t| type_of_connection_point(doc, t));
            let mut levels: BTreeMap<u32, Vec<ConnectionPointId>> = BTreeMap::new();
            let mut mixed = Vec::new();
            for t in ts {
                levels.entry(level_of(doc, t).0).or_default().push(t);
                if !std::ptr::eq(type_of_connection_point(doc, t), ty) {
                    mixed.push(t);
                }
            }
            Position { label, ty, levels, mixed }
        })
        .collect();
    out.sort_by_cached_key(|p| natural_key(&p.label));
    out
}

/// The labels of strip `c`'s fuse positions (the positions a `rating.<p>` names).
pub(crate) fn fuse_positions(doc: &Document, c: DeviceId) -> BTreeSet<String> {
    positions(doc, c).into_iter().filter(|p| p.ty.code == "fuse").map(|p| p.label).collect()
}

/// The position groups the numbering planner numbers (design 036 §10.3, FP8c): terminals with one
/// label, split where a level is drawn twice (the second drawing is a group of its own).
pub(crate) fn position_groups(doc: &Document, c: DeviceId) -> Vec<Vec<ConnectionPointId>> {
    let mut out = Vec::new();
    for p in positions(doc, c) {
        let mut main: Vec<ConnectionPointId> = Vec::new();
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
pub fn highest_number(doc: &Document, c: DeviceId) -> u64 {
    doc.connection_points_of(c).filter_map(|(_, t)| t.label.as_deref()?.parse::<u64>().ok()).filter(|n| *n < 1_000_000).max().unwrap_or(0)
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
    pub fn new(doc: &Document, c: Option<DeviceId>) -> Chooser {
        Chooser { next: c.map_or(0, |c| highest_number(doc, c)) + 1, ..Chooser::default() }
    }

    /// The position for a level `n` of type `t` (`None`: a whole terminal, always new): the named
    /// position, else the lowest of that type with level `n` free, else a new one.
    ///
    /// # Errors
    /// A named position of another type, or whose level is drawn, in words with the way out.
    pub fn choose(
        &mut self, doc: &Document, c: Option<DeviceId>, t: &'static TermType, n: Option<u32>, named: Option<&str>,
    ) -> Result<String, CmdError> {
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
                    return Err(CmdError::refused(format!(
                        "{tag}:{name} is already drawn. A whole terminal makes a new position; press Enter for the next one."
                    )));
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
                        return Err(CmdError::refused(format!(
                            "{tag}:{name} is a {} and this is a {}. Pick another position, or Enter for the next free one.",
                            p.ty.name.to_lowercase(),
                            t.name.to_lowercase()
                        )));
                    }
                    if p.has(n) || self.taken.contains(&(name.to_string(), n)) {
                        let lvl = c.map_or_else(|| n.to_string(), |c| level_name(doc, c, t, n));
                        let where_ = p
                            .levels
                            .get(&n)
                            .and_then(|v| v.first())
                            .and_then(|x| doc.representations_showing(*x).next())
                            .map_or_else(String::new, |(_, r)| format!(" on {}", crate::references::cables::page_words(doc, r.page)));
                        return Err(CmdError::refused(format!(
                            "{tag}:{name} is a {} and its level {lvl} is drawn{where_}. Pick another position, or Enter for the next free one.",
                            t.name.to_lowercase()
                        )));
                    }
                    name.to_string()
                } else if let Some(ty) = self.made.get(name) {
                    if !std::ptr::eq(*ty, t) || self.taken.contains(&(name.to_string(), n)) {
                        return Err(CmdError::refused(format!(
                            "{tag}:{name} is taken in this placement. Pick another position, or Enter for the next free one."
                        )));
                    }
                    name.to_string()
                } else {
                    if name.chars().count() > MAX_POSITION_CHARS || name.chars().any(|ch| ch.is_whitespace() || ch.is_control()) {
                        return Err(CmdError::refused(format!("A position label is 1 to {MAX_POSITION_CHARS} characters, without spaces.")));
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

fn offer_refusal(text: &str) -> CmdError {
    let shown: String = text.chars().take(24).collect();
    let more = if text.chars().count() > 24 { "\u{2026}" } else { "" };
    CmdError::refused(format!(
        "\"{shown}{more}\" is not a terminal count. Write 12, 12+2PE or 12+4x3 (types: PE, x2, x2B, x3, LNPE, SEN, DIS, FUSE)."
    ))
}

/// Read an offer: `+`-separated `<count><code>` items, codes case-insensitive, no code =
/// feed-through, spaces ignored.
///
/// # Errors
/// Unreadable text, an unknown code, no positions, more than [`crate::terminal_strips::MAX_POSITIONS`].
pub(crate) fn parse_counts(text: &str) -> Result<Counts, CmdError> {
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
        return Err(CmdError::refused("A strip offers at least 1 position. Write 12, 12+2PE or 12+4x3; empty clears it."));
    }
    if total > crate::terminal_strips::MAX_POSITIONS {
        return Err(CmdError::refused(format!(
            "A strip offers at most {} positions, of every type together.",
            crate::terminal_strips::MAX_POSITIONS
        )));
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
pub(crate) fn jumper_bar_end(p: &str, n: u32, point: Option<char>) -> String {
    match point {
        Some(c) => format!("{p}.{n}{c}"),
        None if n == 1 && !p.contains('.') => p.to_string(),
        None => format!("{p}.{n}"),
    }
}

/// Canonical link text on level `n` and point `point` for `labels` (plan order) out of `order`:
/// runs become ranges, each end written with [`jumper_bar_end`] (review 036, 2: the point is kept).
pub(crate) fn jumper_bar_text(labels: &[&String], order: &[String], n: u32, point: Option<char>) -> String {
    let base = ecad_model::connectivity::jumper_bars::canonical(labels, order);
    if n == 1 && point.is_none() && !labels.iter().any(|l| l.contains('.')) {
        return base;
    }
    // Rebuild from the runs: canonical() never puts a level in, and its items are positions.
    let idx = |l: &String| order.iter().position(|o| o == l);
    let mut out: Vec<String> = Vec::new();
    let mut run: Vec<(usize, &String)> = Vec::new();
    let flush = |run: &mut Vec<(usize, &String)>, out: &mut Vec<String>| {
        match (run.first(), run.last()) {
            (Some((_, a)), Some((_, b))) if run.len() >= 2 => out.push(format!("{}-{}", jumper_bar_end(a, n, point), jumper_bar_end(b, n, point))),
            (Some((_, a)), _) => out.push(jumper_bar_end(a, n, point)),
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
                out.push(jumper_bar_end(l, n, point));
            }
        }
    }
    flush(&mut run, &mut out);
    out.join(", ")
}

/// A typed link value with level names (`1.N-3.N`) written with level numbers (`1.2-3.2`), by
/// the names of each position's type on strip `c` (§5.1: names are accepted when typed and stored
/// as numbers, so renaming a level never breaks a link). An end that already reads as a level, or
/// whose name no type of its position has, is left as typed. A name containing `-` (the sensor
/// terminal's `-`) is written by its number.
pub(crate) fn jumper_bar_names_to_numbers(doc: &Document, c: DeviceId, text: &str) -> String {
    if text.chars().count() > ecad_model::connectivity::jumper_bars::MAX_TEXT_CHARS {
        return text.to_string();
    }
    let ps = positions(doc, c);
    let end = |e: &str| -> String {
        let e = e.trim();
        let Some((pos, tail)) = e.rsplit_once('.') else { return e.to_string() };
        if ecad_model::connectivity::jumper_bars::split_level(e).is_ok_and(|(_, l)| l.is_some()) {
            return e.to_string();
        }
        let Some(p) = ps.iter().find(|p| p.label == pos) else { return e.to_string() };
        match level_names(doc, c, p.ty).iter().position(|n| n.eq_ignore_ascii_case(tail)) {
            Some(i) => format!("{pos}.{}", i + 1),
            None => e.to_string(),
        }
    };
    text.split(',')
        .map(|item| match item.split_once('-') {
            Some((a, b)) => format!("{}-{}", end(a), end(b)),
            None => end(item),
        })
        .collect::<Vec<_>>()
        .join(", ")
}

/// When link value `text` on strip `c` names an end like `2.1` while a position labelled `2.1`
/// exists (§5.1, Q5): `Link -X01 2.1 (position 2, level 1); position 2.1 is written 2.1.1.`
pub(crate) fn dotted_position_words(doc: &Document, c: DeviceId, text: &str) -> Option<String> {
    let p = ecad_model::connectivity::jumper_bars::parse(text).ok()?;
    let labels: BTreeSet<String> = doc.connection_points_of(c).filter_map(|(_, t)| t.label.clone()).collect();
    let mut words = Vec::new();
    for item in &p.0 {
        let ends: Vec<&String> = match item {
            ecad_model::connectivity::jumper_bars::Item::One(x) => vec![x],
            ecad_model::connectivity::jumper_bars::Item::Range(a, b) => vec![a, b],
        };
        for e in ends {
            let Ok((pos, Some((n, _)))) = ecad_model::connectivity::jumper_bars::split_level(e) else { continue };
            if labels.contains(e.as_str()) {
                words.push(format!("{e} (position {pos}, level {n}); position {e} is written {}", jumper_bar_end(e, 1, None)));
            }
        }
    }
    (!words.is_empty()).then(|| format!("Link {} {}.", crate::devices::tag_words(doc, c), words.join("; ")))
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
pub fn graphics(key: StarterKey) -> Option<Vec<Primitive>> {
    let (t, n) = of_starter(key).filter(|(t, _)| !t.legacy())?;
    Some(match (t.key, n) {
        (StarterKey::Terminalblockfuse, _) => crate::symbols::graphics_of(StarterKey::Fuseterminalblock),
        (StarterKey::Termdisc, _) => {
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
                let ys: Vec<f64> = group.iter().filter_map(|k| k.level()).map(|(n, _)| -5.0 * f64::from(n - 1)).collect();
                if let (Some(a), Some(b)) = (ys.iter().copied().reduce(f64::max), ys.iter().copied().reduce(f64::min)) {
                    g.push(line((1.0, a - 1.2), (1.0, b + 1.2)));
                }
            }
            g
        }
    })
}

/// The connection points of a new starter (`None` for any other key).
pub fn connection_points(key: StarterKey) -> Option<Vec<ConnectionPointDef>> {
    let (t, n) = of_starter(key).filter(|(t, _)| !t.legacy())?;
    let d = |k: PointKey, x: f64, y: f64| ConnectionPointDef { key: k, label: None, at: p(x, y), accepts: Default::default() };
    let level = |n: u32, io: Option<LevelIo>| PointKey::Level { level: u8::try_from(n).unwrap_or(u8::MAX), io };
    Some(match (t.breaks, n) {
        (true, _) => vec![d(level(1, Some(LevelIo::In)), 0.0, 10.0), d(level(1, Some(LevelIo::Out)), 0.0, -10.0)],
        (false, Some(_)) => t.level_keys(n.unwrap_or(1)).into_iter().take(1).map(|k| d(k, 0.0, 0.0)).collect(),
        (false, None) => (1..=t.count()).map(|i| d(level(i, None), 0.0, -5.0 * f64::from(i - 1))).collect(),
    })
}

/// The internal bridges of a new starter's definition (§6): its type's groups.
pub fn bridges(key: StarterKey) -> Vec<Vec<PointKey>> {
    of_starter(key).map(|(t, _)| t.bridges.iter().map(|g| g.to_vec()).collect()).unwrap_or_default()
}

/// Where each level of a symbol of strip `c` sits, keyed by the definition keys it shows (the
/// whole terminal: every level).
pub fn rep_type(doc: &Document, r: &ecad_model::Representation) -> Option<(&'static TermType, Option<u32>)> {
    let Subject::Device(_) = r.subject else { return None };
    of_starter(crate::devices::rep_starter(doc, r)?.key)
}

pub fn short(hex: &str) -> &str {
    hex.get(..8).unwrap_or(hex)
}

/// A terminal's name: its label, or `terminal <first 8 hex digits>`.
pub fn connection_point_name(doc: &Document, t: ConnectionPointId) -> String {
    // Brief 036 (design 036 §4.1): a strip level's shown label (`1.L`, or its own label).
    match crate::terminal_strips::terminal_blocks::shown_label(doc, t) {
        Some(l) => l,
        None => format!("connection point {}", short(&t.hex())),
    }
}

/// The stored label of a terminal: a strip terminal's **position** (brief 036), what links,
/// accessories and renumbering name. [`connection_point_name`] is what a person sees.
pub fn position_label(doc: &Document, t: ConnectionPointId) -> String {
    match doc.connection_points.get(&t).and_then(|x| x.label.clone()) {
        Some(l) => l,
        None => format!("connection point {}", short(&t.hex())),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn starters_and_codes() {
        assert_eq!(of_starter(StarterKey::TermlnpeL2).map(|(t, n)| (t.code, n)), Some(("lnpe", Some(2))));
        assert_eq!(of_starter(StarterKey::Termlnpe).map(|(t, n)| (t.code, n)), Some(("lnpe", None)));
        assert_eq!(of_starter(StarterKey::Coil), None);
        assert_eq!(of_starter(StarterKey::Terminalblockfuse).map(|(t, n)| (t.code, n)), Some(("fuse", Some(1))));
        for t in TYPES {
            assert_eq!(t.level_starters.len(), if t.multi() { t.levels.len() } else { 0 }, "{}", t.code);
            for (i, k) in t.level_starters.iter().enumerate() {
                assert_eq!(t.starter_key(Some(u32::try_from(i).unwrap() + 1)), *k);
            }
        }
        assert_eq!(starter_keys().len(), 20);
        assert!(is_level_starter(crate::symbols::StarterKey::Term2bL1) && !is_level_starter(crate::symbols::StarterKey::TerminalBlock));
        assert_eq!(by_word("x2b").map(|t| t.key), Some(StarterKey::Term2b));
        assert_eq!(by_word("Feedthrough").map(|t| t.key), Some(StarterKey::TerminalBlock));
    }

    #[test]
    fn offers_by_type() {
        let c = parse_counts("12+2PE+4x3").unwrap_or_default();
        assert_eq!(counts_text(&c), "12+2PE+4x3");
        assert_eq!(c.iter().sum::<u32>(), 18);
        assert_eq!(parse_counts("12 + 2pe").map(|c| counts_text(&c)), Ok("12+2PE".into()));
        assert_eq!(parse_counts("0+2PE").map(|c| counts_text(&c)), Ok("2PE".into()));
        assert!(parse_counts("6Q3").unwrap_err().to_string().contains("types: PE, x2"));
        assert!(parse_counts("999x3").is_ok());
        assert!(parse_counts("1000x2").is_err());
        assert!(parse_counts("990+10PE").is_err());
        assert!(parse_counts(&"9".repeat(10_000)).is_err());
        assert!(parse_counts("").is_err());
    }

    #[test]
    fn jumper_bar_ends() {
        assert_eq!(jumper_bar_end("4", 1, None), "4");
        assert_eq!(jumper_bar_end("4", 2, None), "4.2");
        assert_eq!(jumper_bar_end("2.1", 1, None), "2.1.1");
        let order: Vec<String> = ["1", "2", "3", "4"].iter().map(|s| s.to_string()).collect();
        let sel: Vec<&String> = order.iter().collect();
        assert_eq!(jumper_bar_text(&sel, &order, 2, None), "1.2-4.2");
        assert_eq!(jumper_bar_text(&sel, &order, 1, None), "1-4");
        assert_eq!(jumper_bar_text(&sel, &order, 1, Some('o')), "1.1o-4.1o", "review 036, 2: the point is kept");
    }
}
