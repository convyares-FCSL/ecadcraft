//! The wire commands' rules (brief 022, design 022 §10; brief 032 §2; brief 050b): nets and their
//! numbers, potentials and categories, the wiring settings, joins and the wire itself.
//! Each function reads the document and returns the ops to commit and what to report.

use ecad_model::units::Radians;
use serde_json::{Value, json};
use std::collections::{BTreeMap, BTreeSet};

use ecad_model::text::Text;
use ecad_model::wiring::{self, Scheme};
use ecad_model::{
    Document, End, IdAllocator, Ledger, NetData, NetKey, Noun, ObjectRef, Op, PageId, Placement, ProjectSettings, Pt, RouteEnd, RouteId, Style,
    StyleRef, TerminalId, Wire, WireId,
};

use crate::error::{CmdError, msg};
use crate::wires;

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
    pub conductor: Option<WireId>,
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
    pub wires: Vec<WireId>,
    /// Its device connection points (not continuations), with their words.
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
    for c in &net.wires {
        for r in wires::routes_of_wire(doc, *c).into_iter().filter_map(|r| doc.routes.get(&r)) {
            let Some(n) = order.get(&r.page) else { continue };
            let col = r.points.first().and_then(|a| wiring::place(doc, r.page, *a)).map(|p| p.column_label);
            let slot = pages.entry(*n).or_insert_with(|| (ecad_model::book::page_number(doc, r.page), None));
            if slot.1.is_none() {
                slot.1 = col;
            }
        }
    }
    let points = net
        .terminals
        .iter()
        .filter(|t| doc.terminals.get(t).is_none_or(|x| x.owner.aid().is_none()))
        .map(|t| (*t, wires::point_words(doc, *t)))
        .collect();
    let continuations = net
        .terminals
        .iter()
        .filter_map(|t| doc.terminals.get(t))
        .filter_map(|x| x.owner.aid())
        .filter_map(|a| crate::continuations::continuation(doc, a).map(|(n, _)| n.unwrap_or_default().to_string()))
        .collect();
    let looks = crate::styles::wire::WireLooks::new(doc);
    let cats: BTreeSet<StyleRef> = net.wires.iter().filter_map(|c| looks.category(doc, *c).0).collect();
    let named = |id: &StyleRef| CategoryName { id: id.text(), name: crate::styles::name(doc, ecad_model::StyleCatalogue::Wire, id) };
    let uncategorised = net.wires.iter().any(|c| looks.category(doc, *c).0.is_none());
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
        wires: net.wires.iter().copied().collect(),
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
pub fn chosen_net(doc: &Document, choice: &NetChoice) -> Result<NetKey, CmdError> {
    let conn = doc.connectivity();
    if let Some(c) = choice.conductor {
        return conn.net_of_conductor(c).ok_or(CmdError::NotFound(Noun::Wire));
    }
    if let Some(t) = choice.terminal {
        return conn.net_of_terminal(t).ok_or(CmdError::NotFound(Noun::Terminal));
    }
    if let Some(n) = &choice.number {
        let n = n.trim();
        let mut nets: Vec<NetKey> = conn.nets_numbered(n).map(|(id, _)| id).collect();
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
    let mut ids: Vec<NetKey> = if q.choice.is_given() {
        if let Some(n) = &q.choice.number { conn.nets_numbered(n.trim()).map(|(id, _)| id).collect() } else { vec![chosen_net(doc, &q.choice)?] }
    } else if let Some(pg) = q.page {
        if !doc.pages.contains_key(&pg) {
            return Err(CmdError::refused(format!("page {pg} does not exist")));
        }
        let mut set = BTreeSet::new();
        for r in doc.routes_on(pg) {
            if let Some(id) = wires::wire_of_route(doc, r).and_then(|c| conn.net_of_conductor(c)) {
                set.insert(id);
            }
        }
        set.into_iter().collect()
    } else {
        conn.nets().map(|(id, _)| id).collect()
    };
    ids.retain(|id| conn.net(*id).is_some_and(|n| q.all || !n.wires.is_empty()));
    let mut keyed: Vec<((u8, u128), NetKey)> = ids.into_iter().filter_map(|id| conn.net(id).map(|n| (sort_key(n), id))).collect();
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
        wires::routes_of_wire(doc, c).into_iter().filter_map(|r| doc.routes.get(&r)).find_map(|r| {
            let a = *r.points.first()?;
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

/// The free route end of a net to continue from: (route, end, page, point, direction away).
fn open_end(doc: &Document, net: &NetData) -> Option<(RouteId, End, PageId, Pt, Pt)> {
    for c in &net.wires {
        for r in wires::routes_of_wire(doc, *c) {
            let Some(route) = doc.routes.get(&r) else { continue };
            let v = &route.points;
            for (end, e) in End::BOTH.into_iter().zip(route.ends) {
                if e != RouteEnd::Free {
                    continue;
                }
                let (at, prev) = if end == End::A { (v.first(), v.get(1)) } else { (v.last(), v.iter().rev().nth(1)) };
                let (Some(at), Some(prev)) = (at, prev) else { continue };
                let d = Pt::new(at.x - prev.x, at.y - prev.y);
                return Some((r, end, route.page, *at, d));
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
    Merged { ops: Vec<Op>, number: String, continuations: Vec<ecad_model::AidId> },
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
    if net.wires.is_empty() {
        return Err(CmdError::refused("That net has no wire to number."));
    }
    let conductors = net.wires.len();
    let Some(v) = value else {
        let number = wiring::net_number(doc, &net).ok_or(CmdError::refused("That net has no number to release."))?;
        let ops = wiring::write_ops(doc, &net, &number, false);
        return Ok(Numbered::Released { ops, number, conductors });
    };
    let v = v.to_string();
    let conn = doc.connectivity();
    let exempt = doc.settings.wiring.potentials.iter().any(|x| !x.numbered && x.prefix == v);
    let others: Vec<NetKey> = if exempt { Vec::new() } else { conn.nets_numbered(&v).map(|(n, _)| n).filter(|n| *n != id).collect() };
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
            let starter = wires::continuation_starter();
            let mut ops = wiring::write_ops(doc, &net, &v, true);
            let mut continuations = Vec::new();
            for n in [&net, &other] {
                let (_, _, page, at, d) = open_end(doc, n).ok_or_else(|| {
                    CmdError::refused(format!(
                        "The nets can't be merged: the net at {} has no open end to continue from; draw a continuation there.",
                        where_words(doc, n)
                    ))
                })?;
                let aid = wires::continuation_aid(&name, None)?;
                let placement = Placement { origin: at, rotation: arrow_rotation(d), mirror: false, scale: 1.0 };
                // The plan lands the free route end at the point.
                let t = wires::plan_tagless(doc, ids, page, style.clone(), starter, placement, aid)?;
                continuations.push(t.aid);
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
pub fn potential(doc: &Document, c: WireId, value: Option<&str>) -> Result<(Vec<Op>, usize), CmdError> {
    let conn = doc.connectivity();
    let net = conn.net_of_conductor(c).and_then(|id| conn.net(id)).ok_or(CmdError::NotFound(Noun::Wire))?;
    let value = value.map(Text::new).transpose().map_err(|e| CmdError::refused(format!("Potential: {e}")))?;
    let mut ops = Vec::new();
    for w in &net.wires {
        let Some(before) = doc.wires.get(w) else { continue };
        if before.potential != value {
            let after = Wire { potential: value.clone(), ..before.clone() };
            ops.push(Op::ModifyWire { id: *w, before: before.clone(), after });
        }
    }
    Ok((ops, net.wires.len()))
}

/// The number wire `c` shows ([`wiring::wire_number`]: its net's, or its own numbering per
/// conductor).
pub fn number_of(doc: &Document, c: WireId) -> Option<String> {
    wiring::wire_number(doc, c)
}

/// The ops giving `conductors` category `value` (`None`: clear their own), with the potential
/// rule of design 050 §5.4: a conductor on a net with no potential also gets the category's
/// potential; a net that has another potential keeps it. Returns the ops and what to say.
///
/// # Errors
/// No such wire.
pub fn category_ops(doc: &Document, conductors: &[WireId], value: Option<&StyleRef>) -> Result<(Vec<Op>, Option<String>), CmdError> {
    let pot = value
        .and_then(|id| crate::styles::field_of(doc, ecad_model::StyleCatalogue::Wire, id, "potential", None))
        .and_then(|r| crate::styles::wire::filled(&r));
    let conn = doc.connectivity();
    let mut ops = Vec::new();
    let mut kept: Option<String> = None;
    for c in conductors {
        let cond = doc.wires.get(c).ok_or(CmdError::NotFound(Noun::Wire))?;
        let mut after = cond.clone();
        after.wire_type = value.cloned();
        if let Some(p) = &pot {
            let net_pot = conn.net_of_conductor(*c).and_then(|id| conn.net(id)).and_then(|n| wiring::net_potential(doc, n).name);
            match net_pot {
                None => after.potential = Text::new(p).ok(),
                Some(n) if !n.eq_ignore_ascii_case(p) => kept = Some(n),
                Some(_) => {}
            }
        }
        if after != *cond {
            ops.push(Op::ModifyWire { id: *c, before: cond.clone(), after });
        }
    }
    Ok((ops, kept.map(|n| format!("Category set; the net keeps potential {n}."))))
}

/// The wires `category` acts on.
#[derive(Debug, Clone, PartialEq)]
pub enum Wires {
    /// One wire.
    One(WireId),
    /// Every wire of the net this one belongs to.
    Net(WireId),
    /// These wires.
    Many(Vec<WireId>),
}

/// What `category` did.
#[derive(Debug, Clone)]
pub struct Categorised {
    pub ops: Vec<Op>,
    /// The history label.
    pub label: String,
    pub wires: usize,
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
    let conductors: Vec<WireId> = match wires {
        Wires::One(c) => vec![c],
        Wires::Net(c) => {
            let conn = doc.connectivity();
            conn.net_of_conductor(c).and_then(|id| conn.net(id)).map(|n| n.wires.iter().copied().collect()).ok_or(CmdError::NotFound(Noun::Wire))?
        }
        Wires::Many(v) => v,
    };
    let id = match value {
        Some(v) => Some(crate::styles::find(doc, ecad_model::StyleCatalogue::Wire, v)?),
        None => None,
    };
    let (ops, message) = category_ops(doc, &conductors, id.as_ref())?;
    let label = match &id {
        Some(i) => format!("Category {}", crate::styles::name(doc, ecad_model::StyleCatalogue::Wire, i)),
        None => "Clear category".into(),
    };
    Ok(Categorised { ops, label, wires: conductors.len(), category: id, message })
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
    /// Design 072 F40: what one wire number counts, `net` or `conductor`.
    pub number_by: Option<String>,
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
        w.colour_style = Some(<ecad_model::colour::ColourWords as ecad_model::words::Word>::parse(c).ok_or(CmdError::input(
            "`colour_style` must be \"code2\" (BN, BU, GN/YE), \"code3\" (BRN, BLU, GRN/YEL) or \"name\" (Brown, Blue, Green/Yellow)",
        ))?);
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
    if let Some(b) = &change.number_by {
        w.number_by = <wiring::NumberBy as ecad_model::Word>::parse(b)
            .ok_or(CmdError::input("`number_by` must be \"net\" (one number per net) or \"conductor\" (one number per wire)"))?;
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
    let nets: BTreeSet<NetKey> = ops
        .iter()
        .filter_map(|op| match op {
            Op::ModifyWire { id, .. } => doc.connectivity().net_of_conductor(*id),
            _ => None,
        })
        .collect();
    (ops, nets.len())
}

/// What `joinwires` did.
#[derive(Debug, Clone)]
pub enum Joined {
    /// Two free route ends became one route.
    Merged { ops: Vec<Op>, survivor: RouteId, removed: RouteId, at: Pt },
    /// Free route ends landed on route `route` (a T): on its middle, or on its free end where three
    /// or more ends meet.
    Tee { ops: Vec<Op>, route: RouteId, landed: usize, at: Pt },
}

/// `joinwires` (design 032 §2.5, design 072 §4.3): two free route ends that meet at `at` become
/// one route; free ends on a route's middle, or three or more ends, land on one route there (a
/// T). `quiet` ends are never joined.
///
/// # Errors
/// Nothing to join there; a join that is too long or not sensible (§2.2).
pub fn join_wires(doc: &Document, page: PageId, at: Pt, quiet: &[Pt], max_points: usize) -> Result<Joined, CmdError> {
    let loose = wires::joinable_ends_at(doc, page, at, quiet);
    let through = wires::interior_route_at(doc, page, at);
    if let ([a, b], None) = (loose.as_slice(), through)
        && let Some(j) = wires::plan_end_join(doc, *a, *b, max_points)?
    {
        let snapped = [a, b].into_iter().find(|(r, _)| *r == j.survivor).and_then(|(r, e)| wires::end_point(doc, *r, *e)).unwrap_or(at);
        return Ok(Joined::Merged { ops: j.ops, survivor: j.survivor, removed: j.removed, at: snapped });
    }
    // The route the ends land on: the one under the point, else the first free end's.
    let (route, place, landing): (RouteId, Pt, Vec<(RouteId, End)>) = match (through, loose.split_first()) {
        (Some(on), Some(_)) => (on.route, on.at, loose.iter().copied().filter(|(r, _)| *r != on.route).collect()),
        (None, Some(((r, e), rest))) => (*r, wires::end_point(doc, *r, *e).unwrap_or(at), rest.iter().copied().filter(|(x, _)| x != r).collect()),
        (_, None) => return Err(CmdError::refused(msg::NO_ENDS_TO_JOIN)),
    };
    if landing.is_empty() {
        return Err(CmdError::refused(msg::NO_ENDS_TO_JOIN));
    }
    for (r, _) in &landing {
        wires::join_sense(doc, route, *r)?;
    }
    let mut edits = wires::RouteEdits::new(doc);
    for (r, e) in &landing {
        edits.set_end(*r, *e, RouteEnd::On { route, at: place });
    }
    Ok(Joined::Tee { ops: edits.ops(), route, landed: landing.len(), at: place })
}

/// `wire` (design 032 §2.3): the plan for drawing `input`, with `category` (an ID or a name), when
/// given, on the wires the drawn route carries.
///
/// # Semantics
/// With a category, the commit's wire rewrite (`ecad_model::wire_order::rewrite`) is run here on
/// the preview and its ops are part of the plan, so the category can be set on the wires it makes;
/// the commit's own rewrite then finds nothing left to do.
///
/// # Errors
/// The route does not work (too many points…); no such category; the model refused the preview.
pub fn wire(ledger: &Ledger, ids: &mut IdAllocator, input: wires::WireInput<'_>, category: Option<&str>) -> Result<wires::WirePlan, CmdError> {
    let doc = ledger.document();
    let mut plan = wires::plan_wire(doc, ids, input)?;
    if let Some(c) = category {
        let id = crate::styles::find(doc, ecad_model::StyleCatalogue::Wire, c)?;
        let drawn = ledger.preview(&plan.ops)?;
        let rewritten = ecad_model::wire_order::rewrite(&drawn, &plan.ops);
        let after = if rewritten.is_empty() { drawn } else { ledger.preview(&[plan.ops.clone(), rewritten.clone()].concat())? };
        plan.ops.extend(rewritten);
        let carried = wires::wires_of_route(&after, plan.route);
        let (more, _) = category_ops(&after, &carried, Some(&id))?;
        plan.ops.extend(more);
    }
    Ok(plan)
}

/// The `setprop` rules of brief 022 for a conductor or a continuation: `Ok(Some(value))` to write
/// (checked), `Err` to refuse; `Ok(None)` when the key is not one of brief 022's.
///
/// # Errors
/// A wire number or potential (they belong to the whole net), a tool-made device's kind, or a
/// caption on a device (a continuation's is `setprop {aids}`, design 072 §6.2).
pub fn setprop_rule(doc: &Document, object: ObjectRef, key: &str, _value: Option<&str>) -> Result<Option<Option<String>>, CmdError> {
    match object {
        // Brief 049 (design 049 §3.1): a caption only on a continuation (`setprop {aid}`).
        ObjectRef::Device(_) if key == crate::continuations::CAPTION_KEY => {
            Err(CmdError::refused("Only continuation arrows have a caption; that is a device."))
        }
        ObjectRef::Device(c) if crate::bom::is_item(doc, c) && key == crate::bom::KIND_KEY => {
            Err(CmdError::refused("It keeps its kind; erase it and place another."))
        }
        _ => Ok(None),
    }
}

pub fn net_json(n: &NetView) -> Value {
    let cat = |c: &CategoryName| json!({"id": c.id, "name": c.name});
    let mut out = json!({
        "number": n.number,
        "fixed": n.fixed,
        "potential": n.potential,
        "category": n.category.as_ref().map(cat),
        "conductors": n.wires.iter().map(|c| c.hex()).collect::<Vec<_>>(),
        "points": n.points.iter().map(|(t, label)| json!({"terminal": t.hex(), "label": label})).collect::<Vec<_>>(),
        "continuations": n.continuations,
        "pages": n.pages.iter().map(|(p, c)| json!({"page": p, "column": c})).collect::<Vec<_>>(),
    });
    if let Some(list) = &n.categories {
        out["categories"] = json!(list.iter().map(cat).collect::<Vec<_>>());
    }
    out
}

/// The project's wiring settings as `wiresettings` shows them (the label rule as resolved).
pub fn settings_json(doc: &Document) -> Value {
    let w = &doc.settings.wiring;
    json!({
        "scheme": w.scheme.name(),
        "colour_style": ecad_model::words::Word::word(w.colour_style()),
        "label_height": crate::cmd::sheet::wire_label_height(doc),
        "label_rule": wires::label_rule_word(doc),
        "label_min_length": wires::label_min_length(doc),
        "digits": w.digits(),
        "number_by": ecad_model::Word::word(w.number_by),
        "unclassified_prefix": w.unclassified_prefix,
        "potentials": w.potentials.iter().map(|p| json!({
            "name": p.name, "prefix": p.prefix, "numbered": p.numbered,
        })).collect::<Vec<_>>(),
    })
}

/// Whether `wire` joins ends that meet other wires when `join` is not given (design 032 §4.5):
/// the registered default `wire.join`.
pub fn join_by_default(doc: &Document) -> bool {
    crate::defaults::resolve(doc, &crate::defaults::wires::WIRE_JOIN, crate::defaults::Scope::project()).flag()
}
