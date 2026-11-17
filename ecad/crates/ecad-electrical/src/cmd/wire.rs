//! The wire commands' rules (brief 022, design 022 §10; brief 032 §2; brief 050b): nets and their
//! numbers, potentials and categories, the wiring settings, junctions, joins and the wire itself.
//! Each function reads the document and returns the ops to commit and what to report.

use ecad_model::units::Radians;
use std::collections::{BTreeMap, BTreeSet};

use ecad_model::connectivity::{CONTINUATION, JUNCTION, KIND, NAME};
use ecad_model::wiring::{self, Scheme};
use ecad_model::{
    ComponentId, ConductorId, Document, End, IdAllocator, Ledger, NetData, NetId, Noun, ObjectRef, Op, PageId, Placement, ProjectSettings, Pt, Style,
    StyleRef, Subject, TerminalId,
};

use crate::error::{CmdError, msg};
use crate::wires::{self, ON_WIRE};

/// `nets` lists at most this many by default.
pub const DEFAULT_LIMIT: usize = 200;
/// …and at most this many per call.
pub const MAX_LIMIT: usize = 2000;
/// Most rows in the potentials table.
pub const MAX_POTENTIALS: usize = 200;

/// Which net a command means: by one of its wires, one of its connection points, or its number
/// (the first such net, in listing order). The first given wins, in that order.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct NetChoice {
    pub conductor: Option<ConductorId>,
    pub terminal: Option<TerminalId>,
    pub number: Option<String>,
}

impl NetChoice {
    /// Whether any of the three is given.
    pub fn is_given(&self) -> bool {
        self.conductor.is_some() || self.terminal.is_some() || self.number.is_some()
    }
}

/// A wire category as a net shows it.
#[derive(Debug, Clone, PartialEq)]
pub struct CategoryName {
    pub id: String,
    pub name: String,
}

/// A net as `nets` and `net` show it (design 022 §10, design 050 §5.6).
#[derive(Debug, Clone, PartialEq)]
pub struct NetView {
    pub number: Option<String>,
    pub fixed: bool,
    pub potential: Option<String>,
    /// The category every wire of the net has; `None` when there is none or they differ.
    pub category: Option<CategoryName>,
    /// The categories, when the net's wires resolve to different ones (or some have none).
    pub categories: Option<Vec<CategoryName>>,
    pub conductors: Vec<ConductorId>,
    /// Its device connection points (not junctions or continuations), with their words.
    pub points: Vec<(TerminalId, String)>,
    /// The names of its continuations.
    pub continuations: BTreeSet<String>,
    /// Per page it is drawn on, in page order: the shown page number and the column of the first
    /// route start there.
    pub pages: Vec<(String, Option<String>)>,
}

/// The net `net` as shown.
pub fn net_view(doc: &Document, net: &NetData) -> NetView {
    let pot = wiring::net_potential(doc, net);
    let order: BTreeMap<PageId, usize> = doc.page_order.iter().enumerate().map(|(i, p)| (*p, i + 1)).collect();
    let mut pages: BTreeMap<usize, (String, Option<String>)> = BTreeMap::new();
    for c in &net.conductors {
        for (_, r) in doc.representations_of(Subject::Conductor(*c)) {
            let Some(n) = order.get(&r.page) else { continue };
            let col = ecad_model::derive::route_ends_on(doc, *c, r.page).and_then(|(a, _)| wiring::place(doc, r.page, a)).map(|p| p.column_label);
            let slot = pages.entry(*n).or_insert_with(|| (ecad_model::book::page_number(doc, r.page), None));
            if slot.1.is_none() {
                slot.1 = col;
            }
        }
    }
    let points = net
        .terminals
        .iter()
        .filter(|t| doc.terminals.get(t).is_none_or(|x| !matches!(wires::kind_of(doc, x.component), Some(JUNCTION | CONTINUATION))))
        .map(|t| (*t, wires::point_words(doc, *t)))
        .collect();
    let continuations = net
        .terminals
        .iter()
        .filter_map(|t| doc.terminals.get(t))
        .filter(|x| wires::kind_of(doc, x.component) == Some(CONTINUATION))
        .filter_map(|x| doc.components.get(&x.component).and_then(|c| c.properties.get(NAME)).cloned())
        .collect();
    let looks = crate::styles::wire::WireLooks::new(doc);
    let cats: BTreeSet<StyleRef> = net.conductors.iter().filter_map(|c| looks.category(doc, *c).0).collect();
    let named = |id: &StyleRef| CategoryName { id: id.text(), name: crate::styles::name(doc, "wire", id) };
    let uncategorised = net.conductors.iter().any(|c| looks.category(doc, *c).0.is_none());
    let (category, categories) = match (cats.len(), uncategorised) {
        (1, false) => (cats.first().map(named), None),
        (0, _) => (None, None),
        _ => (None, Some(cats.iter().map(named).collect())),
    };
    NetView {
        number: wiring::net_number(doc, net),
        fixed: wiring::net_fixed(net),
        potential: pot.name,
        category,
        categories,
        conductors: net.conductors.iter().copied().collect(),
        points,
        continuations,
        pages: pages.into_values().collect(),
    }
}

/// A net's sort key in listings: its first conductor, else its first terminal.
fn sort_key(net: &NetData) -> (u8, u128) {
    match net.first_conductor() {
        Some(c) => (0, c.0),
        None => (1, net.terminals.first().map_or(0, |t| t.0)),
    }
}

/// The net `choice` names.
///
/// # Errors
/// No such wire or connection point, no net with that number, or nothing given.
pub fn chosen_net(doc: &Document, choice: &NetChoice) -> Result<NetId, CmdError> {
    let conn = doc.connectivity();
    if let Some(c) = choice.conductor {
        return conn.net_of_conductor(c).ok_or(CmdError::NotFound(Noun::Wire));
    }
    if let Some(t) = choice.terminal {
        return conn.net_of_terminal(t).ok_or(CmdError::NotFound(Noun::Terminal));
    }
    if let Some(n) = &choice.number {
        let n = n.trim();
        let mut nets: Vec<NetId> = conn.nets_numbered(n).map(|(id, _)| id).collect();
        nets.sort_by_key(|id| conn.net(*id).map(sort_key));
        return nets.first().copied().ok_or_else(|| CmdError::refused(format!("no net has number {n}")));
    }
    Err(CmdError::input("one of `conductor`, `terminal` or `number` is required"))
}

/// The net `choice` names, as shown (`net`).
///
/// # Errors
/// As [`chosen_net`].
pub fn net(doc: &Document, choice: &NetChoice) -> Result<NetView, CmdError> {
    let id = chosen_net(doc, choice)?;
    let net = doc.connectivity().net(id).ok_or(CmdError::NotFound(Noun::Net))?;
    Ok(net_view(doc, net))
}

/// What `nets` lists.
#[derive(Debug, Clone, Default)]
pub struct NetsQuery {
    /// Only nets drawn on this page.
    pub page: Option<PageId>,
    /// One net (or, by number, every net with that number).
    pub choice: NetChoice,
    /// Also nets without a wire.
    pub all: bool,
    /// At most this many (1 to [`MAX_LIMIT`]).
    pub limit: usize,
    /// The cursor `next` gave.
    pub after: Option<String>,
}

/// One page of `nets`.
#[derive(Debug, Clone)]
pub struct NetsPage {
    pub nets: Vec<NetView>,
    /// The cursor for the next page, when there is more.
    pub next: Option<String>,
}

/// The nets `q` asks for, in listing order.
///
/// # Errors
/// As [`chosen_net`]; a page that does not exist; an unreadable cursor.
pub fn nets(doc: &Document, q: &NetsQuery) -> Result<NetsPage, CmdError> {
    let conn = doc.connectivity();
    let mut ids: Vec<NetId> = if q.choice.is_given() {
        if let Some(n) = &q.choice.number { conn.nets_numbered(n.trim()).map(|(id, _)| id).collect() } else { vec![chosen_net(doc, &q.choice)?] }
    } else if let Some(pg) = q.page {
        if !doc.pages.contains_key(&pg) {
            return Err(CmdError::refused(format!("page {pg} does not exist")));
        }
        let mut set = BTreeSet::new();
        for (_, r) in doc.representations_on(pg) {
            if let Subject::Conductor(c) = r.subject
                && let Some(id) = conn.net_of_conductor(c)
            {
                set.insert(id);
            }
        }
        set.into_iter().collect()
    } else {
        conn.nets().map(|(id, _)| id).collect()
    };
    ids.retain(|id| conn.net(*id).is_some_and(|n| q.all || !n.conductors.is_empty()));
    let mut keyed: Vec<((u8, u128), NetId)> = ids.into_iter().filter_map(|id| conn.net(id).map(|n| (sort_key(n), id))).collect();
    keyed.sort();
    if let Some(a) = &q.after {
        let cursor = u128::from_str_radix(a.trim(), 16).map_err(|_| CmdError::input("`after` must be a cursor from `next`"))?;
        keyed.retain(|((_, k), _)| *k > cursor);
    }
    let more = keyed.len() > q.limit;
    let shown: Vec<_> = keyed.into_iter().take(q.limit).collect();
    let next = more.then(|| shown.last().map(|((_, k), _)| format!("{k:032x}"))).flatten();
    let nets = shown.iter().filter_map(|(_, id)| conn.net(*id)).map(|n| net_view(doc, n)).collect();
    Ok(NetsPage { nets, next })
}

/// A net in words for refusals: its first device point and page (`-K01:A1, /2.1`).
fn where_words(doc: &Document, net: &NetData) -> String {
    let point = wires::net_points(doc, net).into_iter().next();
    let place = net.first_conductor().and_then(|c| {
        doc.representations_of(Subject::Conductor(c)).find_map(|(_, r)| {
            let (a, _) = ecad_model::derive::route_ends_on(doc, c, r.page)?;
            Some(wiring::place(doc, r.page, a)?.text(doc.settings.reference_style()))
        })
    });
    [point, place].into_iter().flatten().collect::<Vec<_>>().join(", ")
}

/// The next free number in the scope of `value` (its prefix), with its padding; this net's own
/// next free number when `value` has no sequence.
fn next_like(doc: &Document, value: &str, net: &NetData) -> String {
    match wiring::sequence_of(value) {
        Some((prefix, _)) => {
            let digits = value.len().saturating_sub(prefix.len());
            let n = doc.connectivity().next_in(prefix);
            format!("{prefix}{n:0digits$}")
        }
        None => {
            let pot = wiring::net_potential(doc, net);
            wiring::Allocator::default().next(doc, net, &pot)
        }
    }
}

/// Why two nets cannot be merged; its text completes "The nets can't be merged: …".
#[derive(Debug)]
enum Unmergeable {
    /// They carry these two different potentials.
    Potentials(String, String),
    /// Both have fixed numbers of their own.
    BothFixed,
}

impl std::fmt::Display for Unmergeable {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Unmergeable::Potentials(x, y) => write!(f, "they carry different potentials ({x} and {y}), so joining them would short them"),
            Unmergeable::BothFixed => f.write_str("both nets have fixed numbers of their own"),
        }
    }
}

/// Whether merging nets `a` and `b` is electrically sensible: `Err` with the reason when not.
fn merge_sense(doc: &Document, a: &NetData, b: &NetData) -> Result<(), Unmergeable> {
    let (pa, pb) = (wiring::net_potential(doc, a), wiring::net_potential(doc, b));
    if let (Some(x), Some(y)) = (&pa.name, &pb.name)
        && !x.eq_ignore_ascii_case(y)
    {
        return Err(Unmergeable::Potentials(x.clone(), y.clone()));
    }
    let fixed: BTreeSet<&String> = a.fixed.keys().chain(b.fixed.keys()).collect();
    if fixed.len() > 1 {
        return Err(Unmergeable::BothFixed);
    }
    Ok(())
}

/// The loose end of a net to continue from: (conductor, end, page, point, direction away).
fn open_end(doc: &Document, net: &NetData) -> Option<(ConductorId, End, PageId, Pt, Pt)> {
    for c in &net.conductors {
        let cond = doc.conductors.get(c)?;
        for (_, r) in doc.representations_of(Subject::Conductor(*c)) {
            let Some((_, _, v)) = wires::route_vertices(doc, *c, r.page) else { continue };
            for end in End::BOTH {
                if cond.end(end).is_some() {
                    continue;
                }
                let (at, prev) = if end == End::A { (v.first(), v.get(1)) } else { (v.last(), v.iter().rev().nth(1)) };
                let (Some(at), Some(prev)) = (at, prev) else { continue };
                let d = Pt::new(at.x - prev.x, at.y - prev.y);
                return Some((*c, end, r.page, *at, d));
            }
        }
    }
    None
}

/// The rotation that points a continuation's arrow along `d` (snapped to 90°).
fn arrow_rotation(d: Pt) -> Radians {
    // The arrow runs to local (0, −5): rotation θ maps it to (sin θ, −cos θ).
    let theta = d.x.atan2(-d.y);
    let q = std::f64::consts::FRAC_PI_2;
    Radians((theta / q).round() * q)
}

/// The number the net `choice` names has now, to fix it as it is (the Properties panel's Fixed
/// box).
///
/// # Errors
/// As [`chosen_net`]; a net with no number yet.
pub fn current_number(doc: &Document, choice: &NetChoice) -> Result<String, CmdError> {
    let id = chosen_net(doc, choice)?;
    doc.connectivity().net(id).and_then(|n| wiring::net_number(doc, n)).ok_or(CmdError::refused(msg::NO_NUMBER_TO_FIX))
}

/// What to do when another net uses the number `wirenumber` was given.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Conflict {
    /// The other net takes this one's number.
    Swap,
    /// This net takes the next free number instead.
    Another,
    /// The nets are joined with a continuation pair named for the number.
    Merge,
}

/// What `wirenumber` did.
#[derive(Debug, Clone)]
pub enum Numbered {
    /// The fixed number released (`ops` empty: it was not fixed).
    Released { ops: Vec<Op>, number: String, conductors: usize },
    /// The number fixed (`ops` empty: it already was).
    Fixed { ops: Vec<Op>, number: String, conductors: usize },
    /// The number was taken, and the net took the next free one instead.
    Another { ops: Vec<Op>, number: String, conductors: usize },
    /// The number was taken, and the other net took this one's.
    Swapped { ops: Vec<Op>, number: String, conductors: usize, swapped: String },
    /// The number was taken, and the nets were joined with a continuation pair.
    Merged { ops: Vec<Op>, number: String, continuations: Vec<ComponentId> },
}

impl Numbered {
    /// The ops, and the history label for them.
    pub fn ops(&self) -> (&[Op], &'static str) {
        match self {
            Numbered::Released { ops, .. } => (ops, "Release wire number"),
            Numbered::Fixed { ops, .. } | Numbered::Another { ops, .. } => (ops, "Fix wire number"),
            Numbered::Swapped { ops, .. } => (ops, "Swap wire numbers"),
            Numbered::Merged { ops, .. } => (ops, "Merge nets"),
        }
    }
}

/// `wirenumber`: fix the number `value` on the net `choice` names (`None`: release its fixed
/// number). A number another net uses needs `conflict`: `swap`, `another` or `merge`.
///
/// # Errors
/// As [`chosen_net`]; a net with no wire; a taken number without `conflict`; a merge that is not sensible or has nowhere to continue from.
pub fn wirenumber(
    doc: &Document, ids: &mut IdAllocator, style: Style, choice: &NetChoice, value: Option<&str>, conflict: Option<Conflict>,
) -> Result<Numbered, CmdError> {
    let id = chosen_net(doc, choice)?;
    let net = doc.connectivity().net(id).ok_or(CmdError::NotFound(Noun::Net))?.clone();
    if net.conductors.is_empty() {
        return Err(CmdError::refused("That net has no wire to number."));
    }
    let conductors = net.conductors.len();
    let Some(v) = value else {
        let number = wiring::net_number(doc, &net).ok_or(CmdError::refused("That net has no number to release."))?;
        let ops = wiring::write_ops(doc, &net, &number, false);
        return Ok(Numbered::Released { ops, number, conductors });
    };
    let v = v.to_string();
    let conn = doc.connectivity();
    let exempt = doc.settings.wiring.potentials.iter().any(|x| !x.numbered && x.prefix == v);
    let others: Vec<NetId> = if exempt { Vec::new() } else { conn.nets_numbered(&v).map(|(n, _)| n).filter(|n| *n != id).collect() };
    let Some(other_id) = others.first().copied() else {
        let ops = wiring::write_ops(doc, &net, &v, true);
        return Ok(Numbered::Fixed { ops, number: v, conductors });
    };
    let other = conn.net(other_id).ok_or(CmdError::NotFound(Noun::Net))?.clone();
    let next = next_like(doc, &v, &net);
    let sense = merge_sense(doc, &net, &other);
    match conflict {
        None => {
            let merge = match &sense {
                Ok(()) => "\"merge\" (join the two nets with a continuation pair named for it)".to_string(),
                Err(why) => format!("not \"merge\": {why}"),
            };
            let swap = match wiring::net_number(doc, &net) {
                Some(mine) => format!("\"swap\" (that net takes {mine})"),
                None => format!("\"swap\" (that net takes {next})"),
            };
            Err(CmdError::refused(format!(
                "{v} is used by another net ({}). Choose: {swap}, \"another\" ({next}, the next free), or {merge}; give it as `conflict`.",
                where_words(doc, &other)
            )))
        }
        Some(Conflict::Another) => Ok(Numbered::Another { ops: wiring::write_ops(doc, &net, &next, true), number: next, conductors }),
        Some(Conflict::Swap) => {
            if others.len() > 1 {
                return Err(CmdError::refused(format!("{v} is used by {} other nets; choose another number.", others.len())));
            }
            let theirs = wiring::net_number(doc, &net).unwrap_or_else(|| next.clone());
            let mut ops = wiring::write_ops(doc, &net, &v, true);
            ops.extend(wiring::write_ops(doc, &other, &theirs, wiring::net_fixed(&other)));
            Ok(Numbered::Swapped { ops, number: v, conductors, swapped: theirs })
        }
        Some(Conflict::Merge) => {
            sense.map_err(|why| CmdError::refused(format!("The nets can't be merged: {why}.")))?;
            let name = wires::valid_name(&v)
                .map_err(|_| CmdError::refused(format!("The nets can't be merged: {v} is not a continuation name ({}).", wires::NAME_RULE)))?;
            let starter = wires::continuation_starter().ok_or(CmdError::refused("the continuation symbol is missing"))?;
            let mut ops = wiring::write_ops(doc, &net, &v, true);
            let mut continuations = Vec::new();
            for n in [&net, &other] {
                let (_, _, page, at, d) = open_end(doc, n).ok_or_else(|| {
                    CmdError::refused(format!(
                        "The nets can't be merged: the net at {} has no open end to continue from; draw a continuation there.",
                        where_words(doc, n)
                    ))
                })?;
                let mut props = ecad_model::Properties::new();
                props.insert(KIND.into(), CONTINUATION.into());
                props.insert(NAME.into(), name.clone());
                let placement = Placement { origin: at, rotation: arrow_rotation(d), mirror: false, scale: 1.0 };
                // The plan attaches the loose end at the point.
                let t = wires::plan_tagless(doc, ids, page, style.clone(), starter, placement, props)?;
                continuations.push(t.component);
                ops.extend(t.ops);
            }
            Ok(Numbered::Merged { ops, number: v, continuations })
        }
    }
}

/// `potential`: the ops giving the whole net of wire `c` the potential `value` (`None`: clear it),
/// and how many wires the net has.
///
/// # Errors
/// No such wire.
pub fn potential(doc: &Document, c: ConductorId, value: Option<&str>) -> Result<(Vec<Op>, usize), CmdError> {
    let conn = doc.connectivity();
    let net = conn.net_of_conductor(c).and_then(|id| conn.net(id)).ok_or(CmdError::NotFound(Noun::Wire))?;
    let mut ops = Vec::new();
    for w in &net.conductors {
        let before = doc.conductors.get(w).and_then(|x| x.properties.get(wiring::POTENTIAL_KEY)).cloned();
        if before.as_deref() != value {
            ops.push(Op::SetProperty {
                object: ObjectRef::Conductor(*w),
                key: wiring::POTENTIAL_KEY.into(),
                before,
                after: value.map(str::to_string),
            });
        }
    }
    Ok((ops, net.conductors.len()))
}

/// The number of the net wire `c` belongs to.
pub fn number_of(doc: &Document, c: ConductorId) -> Option<String> {
    let conn = doc.connectivity();
    conn.net_of_conductor(c).and_then(|id| conn.net(id)).and_then(|n| wiring::net_number(doc, n))
}

/// The ops giving `conductors` category `value` (`None`: clear their own), with the potential
/// rule of design 050 §5.4: a conductor on a net with no potential also gets the category's
/// potential; a net that has another potential keeps it. Returns the ops and what to say.
///
/// # Errors
/// No such wire.
pub fn category_ops(doc: &Document, conductors: &[ConductorId], value: Option<&StyleRef>) -> Result<(Vec<Op>, Option<String>), CmdError> {
    let key = wiring::WIRE_CATEGORY;
    let pot = value.and_then(|id| crate::styles::field_of(doc, "wire", id, "potential", None)).and_then(|r| crate::styles::wire::filled(&r));
    let conn = doc.connectivity();
    let mut ops = Vec::new();
    let mut kept: Option<String> = None;
    for c in conductors {
        let cond = doc.conductors.get(c).ok_or(CmdError::NotFound(Noun::Wire))?;
        let before = cond.properties.get(key).cloned();
        let after = value.map(StyleRef::text);
        if before != after {
            ops.push(Op::SetProperty { object: ObjectRef::Conductor(*c), key: key.into(), before, after });
        }
        if let Some(p) = &pot {
            let net_pot = conn.net_of_conductor(*c).and_then(|id| conn.net(id)).and_then(|n| wiring::net_potential(doc, n).name);
            match net_pot {
                None => {
                    let before = cond.properties.get(wiring::POTENTIAL_KEY).cloned();
                    if before.as_deref() != Some(p.as_str()) {
                        ops.push(Op::SetProperty {
                            object: ObjectRef::Conductor(*c),
                            key: wiring::POTENTIAL_KEY.into(),
                            before,
                            after: Some(p.clone()),
                        });
                    }
                }
                Some(n) if !n.eq_ignore_ascii_case(p) => kept = Some(n),
                Some(_) => {}
            }
        }
    }
    Ok((ops, kept.map(|n| format!("Category set; the net keeps potential {n}."))))
}

/// The wires `category` acts on.
#[derive(Debug, Clone, PartialEq)]
pub enum Wires {
    /// One wire.
    One(ConductorId),
    /// Every wire of the net this one belongs to.
    Net(ConductorId),
    /// These wires.
    Many(Vec<ConductorId>),
}

/// What `category` did.
#[derive(Debug, Clone)]
pub struct Categorised {
    pub ops: Vec<Op>,
    /// The history label.
    pub label: String,
    pub conductors: usize,
    /// The category set (`None`: cleared).
    pub category: Option<StyleRef>,
    /// What to tell the person, when a net keeps its own potential.
    pub message: Option<String>,
}

/// `category` (brief 050b, design 050 §6.9): the wires' own category `value` (an ID or a name;
/// `None` clears it).
///
/// # Errors
/// No such wire; no such category.
pub fn category(doc: &Document, wires: Wires, value: Option<&str>) -> Result<Categorised, CmdError> {
    let conductors: Vec<ConductorId> = match wires {
        Wires::One(c) => vec![c],
        Wires::Net(c) => {
            let conn = doc.connectivity();
            conn.net_of_conductor(c)
                .and_then(|id| conn.net(id))
                .map(|n| n.conductors.iter().copied().collect())
                .ok_or(CmdError::NotFound(Noun::Wire))?
        }
        Wires::Many(v) => v,
    };
    let id = match value {
        Some(v) => Some(crate::styles::find(doc, "wire", v)?),
        None => None,
    };
    let (ops, message) = category_ops(doc, &conductors, id.as_ref())?;
    let label = match &id {
        Some(i) => format!("Category {}", crate::styles::name(doc, "wire", i)),
        None => "Clear category".into(),
    };
    Ok(Categorised { ops, label, conductors: conductors.len(), category: id, message })
}

/// One row of the potentials table as given: words not given are empty.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct PotentialRow {
    pub name: String,
    pub prefix: String,
    /// `None`: numbered.
    pub numbered: Option<bool>,
    /// A colour or cross-section was given (they belong to wire categories now).
    pub colours: bool,
}

/// A change to the wiring settings: what is `None` stays.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct WireSettingsChange {
    pub scheme: Option<String>,
    pub digits: Option<u64>,
    pub colour_style: Option<String>,
    pub label_height: Option<f64>,
    pub label_rule: Option<String>,
    pub label_min_length: Option<f64>,
    pub unclassified_prefix: Option<String>,
    /// The whole table.
    pub potentials: Option<Vec<PotentialRow>>,
}

/// `wiresettings`: the project settings with `change` applied, checked.
///
/// # Errors
/// The first value out of range or unknown, in words.
pub fn wire_settings(before: &ProjectSettings, change: &WireSettingsChange) -> Result<ProjectSettings, CmdError> {
    let mut after = before.clone();
    let w = &mut after.wiring;
    if let Some(sc) = &change.scheme {
        w.scheme = Scheme::parse(sc).ok_or(CmdError::input("`scheme` must be \"potential\", \"page\" or \"page-column\""))?;
    }
    if let Some(d) = change.digits {
        let d = Some(d).filter(|d| (1..=u64::from(wiring::MAX_DIGITS)).contains(d)).ok_or(CmdError::input("`digits` must be 1 to 6"))?;
        w.digits = Some(d as u8);
    }
    if let Some(c) = &change.colour_style {
        w.colour_style = Some(
            wiring::ColourStyle::parse(c)
                .ok_or(CmdError::input("`colour_style` must be \"iec\" (BN, BU, GNYE) or \"three-letter\" (BRN, BLU, GRN/YEL)"))?,
        );
    }
    if let Some(hv) = change.label_height {
        w.label_height =
            Some(Some(hv).filter(|x| wiring::LABEL_HEIGHTS.contains(x)).ok_or(CmdError::input("`label_height` must be 0.5 to 10 (millimetres)"))?);
    }
    // Brief 032 §4: the label rule and the shortest labelled wire.
    if let Some(r) = &change.label_rule {
        let r = r.trim().to_ascii_lowercase();
        if r != wiring::LABEL_RULE_NET && r != wiring::LABEL_RULE_EACH {
            return Err(CmdError::refused("`label_rule` must be \"net\" (one label per net per page) or \"each\" (every wire)"));
        }
        w.label_rule = Some(r);
    }
    if let Some(lv) = change.label_min_length {
        w.label_min_length =
            Some(Some(lv).filter(|x| wiring::LABEL_MIN_LENGTHS.contains(x)).ok_or(CmdError::input("Shortest labelled wire is 0 to 100 mm."))?);
    }
    if let Some(u) = &change.unclassified_prefix {
        w.unclassified_prefix = wiring::valid_text("prefix", Some(u)).map_err(CmdError::refused)?.unwrap_or_default();
    }
    if let Some(table) = &change.potentials {
        if table.len() > MAX_POTENTIALS {
            return Err(CmdError::refused(format!("at most {MAX_POTENTIALS} potentials")));
        }
        let mut rows = Vec::new();
        let mut names = BTreeSet::new();
        for r in table {
            // Brief 050b (FP6): colours and cross-sections are wire categories' now.
            if r.colours {
                return Err(CmdError::refused(msg::CATEGORY_COLOURS));
            }
            let name = wiring::valid_text("potential", Some(r.name.trim()))
                .map_err(CmdError::refused)?
                .ok_or(CmdError::input("every potential needs a `name`"))?;
            if !names.insert(name.to_uppercase()) {
                return Err(CmdError::refused(format!("potential {name} is listed twice")));
            }
            rows.push(wiring::PotentialDef {
                prefix: wiring::valid_text("prefix", Some(r.prefix.trim())).map_err(CmdError::refused)?.unwrap_or_default(),
                numbered: r.numbered.unwrap_or(true),
                // Writing the table clears an older build's colours (Checks' row goes).
                legacy_colour: None,
                legacy_cross_section: None,
                name,
            });
        }
        w.potentials = rows;
    }
    Ok(after)
}

/// `renumberwires`: the ops renumbering the wires (`all`: also numbered ones; fixed numbers never
/// change), and how many nets they renumber.
pub fn renumber(doc: &Document, all: bool) -> (Vec<Op>, usize) {
    let ops = wiring::renumber(doc, all);
    let nets: BTreeSet<NetId> = ops
        .iter()
        .filter_map(|op| match op {
            Op::SetProperty { object: ObjectRef::Conductor(c), .. } => doc.connectivity().net_of_conductor(*c),
            _ => None,
        })
        .collect();
    (ops, nets.len())
}

/// A junction placed by `junction` or `joinwires`.
#[derive(Debug, Clone, PartialEq)]
pub struct Junctioned {
    pub junction: ComponentId,
    pub terminal: TerminalId,
    /// The new second half of the wire split at it (`None`: loose ends that only met).
    pub conductor: Option<ConductorId>,
    pub at: Pt,
    /// The loose ends attached (only when they only met).
    pub attached: Option<usize>,
}

/// The ops and result of a junction at `at` on `page` (design 022 §3.1): loose ends that only
/// meet get a junction with nothing to split; otherwise the wire under the point is split.
///
/// # Errors
/// No wire there; the junction symbol is missing.
pub fn junction_at(doc: &Document, ids: &mut IdAllocator, style: Style, page: PageId, at: Pt) -> Result<(Vec<Op>, Junctioned), CmdError> {
    // Loose ends that only meet (no wire runs through the point): a junction joins them, with
    // nothing to split (the Checks row's Add junction, design 022 §8).
    let loose = wires::loose_ends_at(doc, page, at, ON_WIRE);
    let through = wires::interior_wire_at(doc, page, at);
    if through.is_none() && loose.len() >= 2 {
        let starter = wires::junction_starter().ok_or(CmdError::refused("the junction symbol is missing"))?;
        let mut props = ecad_model::Properties::new();
        props.insert(KIND.into(), JUNCTION.into());
        let snapped = loose
            .first()
            .and_then(|(c, e)| {
                wires::route_vertices(doc, *c, page).and_then(|(_, _, v)| if *e == End::A { v.first().copied() } else { v.last().copied() })
            })
            .unwrap_or(at);
        let placement = Placement { origin: snapped, rotation: Radians(0.0), mirror: false, scale: 1.0 };
        let t = wires::plan_tagless(doc, ids, page, style, starter, placement, props)?;
        let done = Junctioned { junction: t.component, terminal: t.terminal, conductor: None, at: snapped, attached: Some(t.attached.len()) };
        return Ok((t.ops, done));
    }
    let on = wires::wire_at(doc, page, at, ON_WIRE, None).ok_or(CmdError::refused("There is no wire there: click on a wire."))?;
    let j = wires::plan_junction(doc, ids, page, style, on)?;
    let done = Junctioned { junction: j.tagless.component, terminal: j.tagless.terminal, conductor: Some(j.second), at: j.at, attached: None };
    Ok((j.tagless.ops, done))
}

/// What `joinwires` did.
#[derive(Debug, Clone)]
pub enum Joined {
    /// Two open ends became one wire.
    Merged { ops: Vec<Op>, survivor: ConductorId, removed: ConductorId, at: Pt },
    /// The ends got a junction.
    Junction { ops: Vec<Op>, junction: Junctioned },
}

/// `joinwires` (design 032 §2.5): two open ends that meet at `at` become one wire; an open end on
/// a wire's interior, or three or more ends, get a junction. `quiet` ends are never joined.
///
/// # Errors
/// Nothing to join there; a join that would close a loop or is too long.
pub fn join_wires(
    doc: &Document, ids: &mut IdAllocator, style: Style, page: PageId, at: Pt, quiet: &[Pt], max_points: usize,
) -> Result<Joined, CmdError> {
    let loose = wires::joinable_ends_at(doc, page, at, quiet);
    let through = wires::interior_wire_at(doc, page, at);
    if let ([a, b], None) = (loose.as_slice(), through)
        && let Some(j) = wires::plan_end_join(doc, page, *a, *b, max_points)?
    {
        let snapped = wires::route_vertices(doc, j.survivor, page)
            .and_then(|(_, _, v)| {
                if (a.0 == j.survivor && a.1 == End::A) || (b.0 == j.survivor && b.1 == End::A) { v.first().copied() } else { v.last().copied() }
            })
            .unwrap_or(at);
        return Ok(Joined::Merged { ops: j.ops, survivor: j.survivor, removed: j.removed, at: snapped });
    }
    if (through.is_none() && loose.len() < 2) || (through.is_some() && loose.is_empty()) {
        return Err(CmdError::refused(msg::NO_ENDS_TO_JOIN));
    }
    let (ops, junction) = junction_at(doc, ids, style, page, at)?;
    Ok(Joined::Junction { ops, junction })
}

/// `wire` (design 032 §2.3): the plan for drawing `input`, with the new wire's `category` (an ID
/// or a name) when given.
///
/// # Errors
/// The route does not work (too many points, a loop…); no such category; the model refused the
/// preview.
pub fn wire(ledger: &Ledger, ids: &mut IdAllocator, input: wires::WireInput<'_>, category: Option<&str>) -> Result<wires::WirePlan, CmdError> {
    let doc = ledger.document();
    let mut plan = wires::plan_wire(doc, ids, input)?;
    if let Some(c) = category {
        let id = crate::styles::find(doc, "wire", c)?;
        let after = ledger.preview(&plan.ops)?;
        let (more, _) = category_ops(&after, &[plan.conductor], Some(&id))?;
        plan.ops.extend(more);
    }
    Ok(plan)
}

/// The `setprop` rules of brief 022 for a conductor or a continuation: `Ok(Some(value))` to write
/// (checked), `Err` to refuse; `Ok(None)` when the key is not one of brief 022's.
///
/// # Errors
/// A wire number or potential (they belong to the whole net), a junction's or continuation's kind,
/// a continuation without a name, or a value that does not read.
pub fn setprop_rule(doc: &Document, object: ObjectRef, key: &str, value: Option<&str>) -> Result<Option<Option<String>>, CmdError> {
    match object {
        ObjectRef::Conductor(_) => match key {
            k if k == wiring::WIRE_NUMBER_KEY || k == ecad_model::connectivity::WIRE_NUMBER_FIXED => Err(CmdError::refused(msg::USE_WIRENUMBER)),
            k if k == wiring::POTENTIAL_KEY => Err(CmdError::refused(msg::USE_POTENTIAL)),
            k if k == wiring::CROSS_SECTION => Ok(Some(wiring::valid_text("cross-section", value).map_err(CmdError::refused)?)),
            k if k == wiring::COLOUR => Ok(Some(wiring::valid_text("colour", value).map_err(CmdError::refused)?)),
            _ => Ok(None),
        },
        // Brief 049 (design 049 §3.1): a caption only on a continuation, checked.
        ObjectRef::Component(c) if key == crate::continuations::CAPTION_KEY => Ok(Some(crate::continuations::caption_value(doc, c, value)?)),
        ObjectRef::Component(c) if wires::kind_of(doc, c).is_some() => match key {
            k if k == KIND => Err(CmdError::refused("A junction or continuation keeps its kind; erase it and place another.")),
            k if k == NAME && wires::kind_of(doc, c) == Some(CONTINUATION) => {
                let v = value.ok_or(CmdError::refused("A continuation needs a name."))?;
                Ok(Some(Some(wires::valid_name(v)?)))
            }
            _ => Ok(None),
        },
        _ => Ok(None),
    }
}
