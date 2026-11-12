//! Device cross-references on a general reference index (brief 025, design 025).
//!
//! # Semantics
//! - A **reference** is a text that shows another drawn item's place. A **source** shows a
//!   **target**'s place; both are representations. Sources and targets of one kind belong to one
//!   **group** ([`Group`]): a cable's reference ends, or one device's symbols. A short rule table
//!   ([`RefIndex`]'s edges, design 025 §3.2) decides which members of a group point at which:
//!   every cable reference end at every other end of its cable; every **child** (a symbol with
//!   contact poles) of a device at its **head** (its exclusive symbol: coil, MCB, motor…) and the
//!   head at every child. A device without a head has no references.
//! - Everything here is derived from the document and never stored: the index ([`RefIndex`]), the
//!   text under a contact (its head's place, [`contact_text`]) and the **contact mirror table**
//!   under a head ([`table`]): one row per pole, NO or NC, power or auxiliary, its labels and its
//!   place, with the poles the part offers that nobody drew (**spare**) and those drawn beyond the
//!   offer (**over**).
//! - What a part offers is the component property [`CONTACTS_KEY`] (FP1, grammar in
//!   [`parse_offer`]); where its table goes is [`MIRROR_KEY`] (FP2, [`parse_mirror`]), else the
//!   project's `contact_mirror` (FP3), else `auto`.
//! - Cable references keep their own derivation ([`crate::references`]); this module only scopes
//!   them per commit ([`Watch`]), so every cable text is exactly as before.
//!
//! # Update map (design 025 §6)
//! [`RefIndex`] is built whole on open, reload and branch switch, and kept per commit, undo, redo
//! and merge from the ops ([`RefIndex::groups_named`], [`RefIndex::refresh`]): only the groups an
//! op names are made again (O(group)). The texts and tables to redraw are worked out by
//! [`Watch`], which derives the touched devices' texts before and after a commit:
//!
//! | Op | Devices whose texts are compared |
//! | --- | --- |
//! | `ModifyItem` / `RemoveItem` of an item of a component's representation | that component |
//! | the same of a conductor route | the components at the conductor's ends (a table under `auto` follows the rail its coil's wire meets) |
//! | `LinkRepresentation` / `UnlinkRepresentation` / `ModifyRepresentation` | the component it shows, or the components at its conductor's ends |
//! | `AttachEnd`, `CreateConductor`, `DeleteConductor` | the components at the ends before and after |
//! | `AddTerminal` / `RemoveTerminal` | that component (its labels) |
//! | `CreateComponent` / `DeleteComponent` | that component |
//! | `SetProperty` component `contacts` or `mirror` | that component |
//! | `SetProperty` page number or sheet | the devices with a member on that page |
//! | page order (folder property, page and folder ops) | the devices with a member on a page whose book position changed |
//! | `EmbedDefinition` / `RemoveDefinition` | the components using it |
//! | `SetProjectSettings` changing `reference_style`, `sheet`, `length_unit` or `contact_mirror` | none derived: every page with a device member is redrawn or marked stale (the one wide case) |
//! | every other op | none |
//!
//! Cable references: `CreateCable`, `DeleteCable`, `AddCableMember` and `RemoveCableMember` now
//! scope to their cable ([`crate::references::cables_touched`]); everything else is as before.
//!
//! This module records nothing and has no UI: commands commit through their JSON forms.

use std::collections::{BTreeMap, BTreeSet};

use cadcraft_engine::doc::{HAlign, VAlign};
use ecad_model::{
    CableId, ComponentId, ContactMirror, Document, Form, LengthUnit, ObjectRef, Op, PageId, Primitive, Pt, Representation, RepresentationId, Subject,
};
use serde_json::{Value, json};

use crate::symbols::{Kind, Starter};

/// The component property that says what contacts the part offers (FP1): `3P+1NO+1NC`.
pub const CONTACTS_KEY: &str = "contacts";
/// The component property that says where the device's contact table goes (FP2).
pub const MIRROR_KEY: &str = "mirror";
/// Most poles an offer may list.
pub const MAX_POLES: usize = 32;
/// Most characters of one explicit label in an offer.
pub const MAX_LABEL_CHARS: usize = 16;
/// Most millimetres a moved table may be from its coil, in each direction.
pub const MAX_OFFSET_MM: f64 = 2_000.0;
/// `ecad.xref`: ACI 7 (ByLayer on `ECAD-TAGS`), 2.5 mm.
pub const XREF_TEXT_MM: f64 = 2.5;
/// `ecad.xref-spare`: ACI 8.
pub const SPARE_ACI: u8 = 8;
/// `ecad.orphan`: ACI 1, for rows drawn beyond the offer.
pub const OVER_ACI: u8 = crate::references::ORPHAN_ACI;
/// `ecad.mirror-row`: the table's row pitch, millimetres.
pub const ROW_PITCH_MM: f64 = 4.0;
/// The gap before the first auxiliary row after power rows, millimetres.
pub const GROUP_GAP_MM: f64 = 2.0;
/// What a spare row shows for its place.
pub const SPARE_PLACE: &str = "—";

/// Refusal of an edit of the text under a contact (design 025 §4.1).
pub const TEXT_DERIVED: &str = "The reference text is derived; move the coil or the contact instead.";
/// Refusal of an edit of a contact table (design 025 §4.2).
pub const TABLE_DERIVED: &str = "The contact table is derived; move it with Move Mirror Table, or set it in Properties.";

/// The refusal of an edit of a drawn cross-reference: the contact text's or the table's.
pub fn refusal(doc: &Document, mapped: Option<crate::project::Mapped>) -> &'static str {
    match mapped {
        Some(crate::project::Mapped::Xref(r)) if class_in(doc, r).is_some_and(|(_, c)| c == Class::Head) => TABLE_DERIVED,
        _ => TEXT_DERIVED,
    }
}

// ---------------------------------------------------------------- offer grammar (FP1)

/// Normally open or normally closed.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum PoleKind {
    No,
    Nc,
}

impl PoleKind {
    /// `NO` or `NC`.
    pub fn word(self) -> &'static str {
        match self {
            PoleKind::No => "NO",
            PoleKind::Nc => "NC",
        }
    }
}

/// One pole a part offers, with its labels.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OfferedPole {
    pub labels: [String; 2],
    pub kind: PoleKind,
    /// A power (main) pole rather than an auxiliary contact.
    pub power: bool,
}

/// The refusal of an unreadable offer.
pub fn not_a_contact_list(text: &str) -> String {
    let shown: String = text.trim().chars().take(40).collect();
    format!("\"{shown}\" is not a contact list. Write it like 3P+1NO+1NC, or 13/14 NO, 21/22 NC.")
}

/// Read an offer (design 025 FP1): entries separated by `+` or `,`, case-insensitive, spaces
/// ignored. `3P` (power NO poles `1/2`, `3/4`…), `1PNC` (power NC poles, continuing the power
/// numbering), `1NO` / `2NC` (auxiliary contacts, IEC sequence digit in written order), or one
/// contact with explicit labels: `13/14 NO`, `21/22 NC`, `L1/T1 P`, `1/2 PNC`.
///
/// # Errors
/// Unreadable text, no poles, or more than [`MAX_POLES`].
pub fn parse_offer(text: &str) -> Result<Vec<OfferedPole>, String> {
    let bad = || not_a_contact_list(text);
    let mut out: Vec<OfferedPole> = Vec::new();
    let (mut power_n, mut seq) = (0u32, 0u32);
    for entry in text.split(['+', ',']) {
        let e: String = entry.chars().filter(|c| !c.is_whitespace()).collect::<String>().to_ascii_uppercase();
        if e.is_empty() {
            return Err(bad());
        }
        let (body, kind, power) = if let Some(b) = e.strip_suffix("PNC") {
            (b, PoleKind::Nc, true)
        } else if let Some(b) = e.strip_suffix("NO") {
            (b, PoleKind::No, false)
        } else if let Some(b) = e.strip_suffix("NC") {
            (b, PoleKind::Nc, false)
        } else if let Some(b) = e.strip_suffix('P') {
            (b, PoleKind::No, true)
        } else {
            return Err(bad());
        };
        if let Some((a, b)) = body.split_once('/') {
            let ok = |l: &str| !l.is_empty() && l.chars().count() <= MAX_LABEL_CHARS && l.chars().all(|c| c.is_ascii_alphanumeric() || c == '.');
            if !(ok(a) && ok(b)) {
                return Err(bad());
            }
            out.push(OfferedPole { labels: [a.to_string(), b.to_string()], kind, power });
        } else {
            let n: usize = body.parse().map_err(|_| bad())?;
            if n == 0 {
                return Err(bad());
            }
            if out.len() + n > MAX_POLES {
                return Err(format!("A part offers at most {MAX_POLES} contacts."));
            }
            for _ in 0..n {
                // A counted pole skips labels already offered (an explicit pair written before it).
                let labels = loop {
                    let labels = if power {
                        power_n += 1;
                        [(2 * power_n - 1).to_string(), (2 * power_n).to_string()]
                    } else {
                        seq += 1;
                        let (a, b) = if kind == PoleKind::No { (3, 4) } else { (1, 2) };
                        [format!("{seq}{a}"), format!("{seq}{b}")]
                    };
                    if !out.iter().any(|o| o.labels.iter().any(|l| labels.contains(l))) {
                        break labels;
                    }
                };
                out.push(OfferedPole { labels, kind, power });
            }
        }
        if out.len() > MAX_POLES {
            return Err(format!("A part offers at most {MAX_POLES} contacts."));
        }
    }
    if out.is_empty() {
        return Err(bad());
    }
    Ok(out)
}

/// What component `c` offers, when it says so readably (an unreadable stored value, from a file,
/// counts as no offer).
pub fn offer_of(doc: &Document, c: ComponentId) -> Option<(String, Vec<OfferedPole>)> {
    let text = doc.components.get(&c)?.properties.get(CONTACTS_KEY)?;
    parse_offer(text).ok().map(|p| (text.clone(), p))
}

/// The stored form of an offer typed by someone: trimmed, checked; empty clears it.
///
/// # Errors
/// As [`parse_offer`].
pub fn offer_value(text: Option<&str>) -> Result<Option<String>, String> {
    let Some(t) = text.map(str::trim).filter(|t| !t.is_empty()) else { return Ok(None) };
    if t.chars().count() > crate::devices::MAX_PROPERTY_CHARS {
        return Err(not_a_contact_list(t));
    }
    parse_offer(t)?;
    Ok(Some(t.to_string()))
}

// ---------------------------------------------------------------- placement (FP2, FP3)

/// Where a device's table goes.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Placement {
    Auto,
    Below,
    Right,
    Off,
    /// Millimetres from the head symbol's insertion point (Move Mirror Table).
    At(Pt),
}

impl Placement {
    /// Its stored word: `auto`, `below`, `right`, `off`, `at 12,-30`.
    pub fn word(self) -> String {
        match self {
            Placement::Auto => "auto".into(),
            Placement::Below => "below".into(),
            Placement::Right => "right".into(),
            Placement::Off => "off".into(),
            Placement::At(p) => format!("at {},{}", trim_num(p.x), trim_num(p.y)),
        }
    }

    fn of_default(m: ContactMirror) -> Placement {
        match m {
            ContactMirror::Auto => Placement::Auto,
            ContactMirror::Below => Placement::Below,
            ContactMirror::Right => Placement::Right,
            ContactMirror::Off => Placement::Off,
        }
    }
}

/// A number without trailing zeros (`12`, `-3.5`), rounded to a hundredth of a millimetre.
fn trim_num(v: f64) -> String {
    let r = (v * 100.0).round() / 100.0;
    let r = if r == 0.0 { 0.0 } else { r };
    format!("{r}")
}

/// The refusal of an unreadable `mirror` value.
pub fn not_a_placement(text: &str) -> String {
    let shown: String = text.trim().chars().take(40).collect();
    format!("\"{shown}\" is not a table placement. Use auto, below, right, off, or at dx,dy in millimetres.")
}

/// Read a `mirror` value (design 025 FP2).
///
/// # Errors
/// Not one of the words, or an offset that is not two finite numbers within [`MAX_OFFSET_MM`].
pub fn parse_mirror(text: &str) -> Result<Placement, String> {
    let t = text.trim();
    if let Some(m) = ContactMirror::parse(t) {
        return Ok(Placement::of_default(m));
    }
    let lower = t.to_ascii_lowercase();
    let rest = lower.strip_prefix("at").ok_or_else(|| not_a_placement(t))?.trim();
    let (a, b) = rest.split_once(',').ok_or_else(|| not_a_placement(t))?;
    let (x, y) = (a.trim().parse::<f64>().map_err(|_| not_a_placement(t))?, b.trim().parse::<f64>().map_err(|_| not_a_placement(t))?);
    if !(x.is_finite() && y.is_finite() && x.abs() <= MAX_OFFSET_MM && y.abs() <= MAX_OFFSET_MM) {
        return Err(not_a_placement(t));
    }
    Ok(Placement::At(Pt::new(x, y)))
}

/// The stored form of a typed `mirror` value; empty (or `auto`'s project default) clears it.
///
/// # Errors
/// As [`parse_mirror`].
pub fn mirror_value(text: Option<&str>) -> Result<Option<String>, String> {
    let Some(t) = text.map(str::trim).filter(|t| !t.is_empty()) else { return Ok(None) };
    if t.eq_ignore_ascii_case("default") || t.to_ascii_lowercase().starts_with("project default") {
        return Ok(None);
    }
    Ok(Some(parse_mirror(t)?.word()))
}

/// Where `c`'s table goes and whether the device says so itself (else the project default). An
/// unreadable stored value counts as absent.
pub fn placement_of(doc: &Document, c: ComponentId) -> (Placement, bool) {
    match doc.components.get(&c).and_then(|x| x.properties.get(MIRROR_KEY)).and_then(|t| parse_mirror(t).ok()) {
        Some(p) => (p, true),
        None => (Placement::of_default(doc.settings.contact_mirror), false),
    }
}

// ---------------------------------------------------------------- classes (design 025 §3)

/// The objects whose representations may point at each other.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Group {
    Cable(CableId),
    Device(ComponentId),
}

/// What a member of a group is.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Class {
    /// A cable reference end.
    End,
    /// A device's head: its exclusive symbol.
    Head,
    /// A symbol with contact poles.
    Child,
    /// Anything else (no edges).
    Other,
}

/// The starter symbol a representation shows: its definition's, else (a drawn form) its role key.
pub fn starter_of(doc: &Document, r: &Representation) -> Option<&'static Starter> {
    if let Some(s) = crate::devices::rep_starter(doc, r) {
        return Some(s);
    }
    (r.form == Form::Drawn).then(|| crate::symbols::by_key(&r.role)).flatten()
}

/// Whether a starter symbol is a head.
pub fn is_head(s: &Starter) -> bool {
    matches!(s.kind, Kind::Fixed { exclusive: true })
}

/// The poles a starter symbol carries (code, not format: design 025 §2).
pub fn poles_of(s: &Starter) -> Vec<(PoleKind, bool)> {
    match (s.key, s.kind) {
        ("power3", _) => vec![(PoleKind::No, true); 3],
        (_, Kind::Contact { make }) => vec![(if make { PoleKind::No } else { PoleKind::Nc }, false)],
        _ => Vec::new(),
    }
}

/// The class of a component representation by its symbol alone (a second head is made `Other`
/// by [`device`]).
fn raw_class(doc: &Document, r: &Representation) -> Class {
    match starter_of(doc, r) {
        Some(s) if is_head(s) => Class::Head,
        Some(s) if !poles_of(s).is_empty() => Class::Child,
        _ => Class::Other,
    }
}

// ---------------------------------------------------------------- a device's structure

/// One drawn pole.
#[derive(Debug, Clone, PartialEq)]
pub struct DrawnPole {
    pub rep: RepresentationId,
    pub labels: [String; 2],
    pub kind: PoleKind,
    pub power: bool,
    pub page: PageId,
    /// Its symbol's anchor, when it can be located.
    pub at: Option<Pt>,
}

/// A device as the reference rule sees it.
#[derive(Debug, Clone, PartialEq)]
pub struct Device {
    pub component: ComponentId,
    /// Its head (the lowest-ID head symbol).
    pub head: Option<RepresentationId>,
    /// Its children, in ID order.
    pub children: Vec<RepresentationId>,
    /// Its drawn poles, children in ID order, each child's poles in its terminal order.
    pub poles: Vec<DrawnPole>,
}

/// The anchor of a representation (design 025 §3.1): a symbol's insertion point, else the
/// middle of its connection points.
pub fn anchor(doc: &Document, r: &Representation) -> Option<Pt> {
    if r.form == Form::Symbol {
        let (_, pl) = crate::devices::rep_definition(doc, r)?;
        return (pl.is_valid() && pl.origin.is_finite()).then_some(pl.origin);
    }
    let pts: Vec<Pt> = r.connection_points.values().copied().filter(|p| p.is_finite()).collect();
    if pts.is_empty() {
        return None;
    }
    let (lo, hi) = bounds(&pts)?;
    Some(Pt::new((lo.x + hi.x) / 2.0, (lo.y + hi.y) / 2.0))
}

fn bounds(pts: &[Pt]) -> Option<(Pt, Pt)> {
    let first = *pts.first()?;
    let (mut lo, mut hi) = (first, first);
    for p in pts {
        lo = Pt::new(lo.x.min(p.x), lo.y.min(p.y));
        hi = Pt::new(hi.x.max(p.x), hi.y.max(p.y));
    }
    (lo.is_finite() && hi.is_finite()).then_some((lo, hi))
}

/// The page positions of a representation's connection points, in its definition's terminal
/// order.
pub fn terminal_points(doc: &Document, c: ComponentId, r: &Representation) -> Vec<(ecad_model::TerminalId, Pt)> {
    let shown = crate::devices::shown_terminals(doc, c, r);
    match (r.form, crate::devices::rep_definition(doc, r)) {
        (Form::Symbol, Some((_, pl))) if pl.is_valid() => shown.into_iter().map(|(t, p)| (t, pl.apply(p))).filter(|(_, p)| p.is_finite()).collect(),
        (Form::Symbol, _) => Vec::new(),
        (Form::Drawn, _) => shown,
    }
}

/// Device `c` as the reference rule sees it: O(its representations).
pub fn device(doc: &Document, c: ComponentId) -> Device {
    let mut head = None;
    let mut children = Vec::new();
    let mut poles = Vec::new();
    for (rid, r) in doc.representations_of(Subject::Component(c)) {
        match raw_class(doc, r) {
            Class::Head if head.is_none() => head = Some(rid),
            Class::Child => {
                children.push(rid);
                let Some(s) = starter_of(doc, r) else { continue };
                let kinds = poles_of(s);
                let pts = crate::devices::shown_terminals(doc, c, r);
                let at = anchor(doc, r);
                for (i, (kind, power)) in kinds.iter().enumerate() {
                    let label = |j: usize| pts.get(2 * i + j).map_or_else(|| "?".to_string(), |(t, _)| crate::devices::label_of(doc, *t));
                    poles.push(DrawnPole { rep: rid, labels: [label(0), label(1)], kind: *kind, power: *power, page: r.page, at });
                }
            }
            _ => {}
        }
    }
    Device { component: c, head, children, poles }
}

/// The class of member `rid` of its group, as the index keeps it.
pub fn class_in(doc: &Document, rid: RepresentationId) -> Option<(Group, Class)> {
    let r = doc.representations.get(&rid)?;
    match r.subject {
        Subject::Cable(k) if crate::references::is_reference(r) => Some((Group::Cable(k), Class::End)),
        Subject::Component(c) => match raw_class(doc, r) {
            Class::Head => {
                let first = doc.representations_of(Subject::Component(c)).find(|(_, x)| raw_class(doc, x) == Class::Head).map(|(id, _)| id);
                Some((Group::Device(c), if first == Some(rid) { Class::Head } else { Class::Other }))
            }
            Class::Child => Some((Group::Device(c), Class::Child)),
            _ => None,
        },
        _ => None,
    }
}

// ---------------------------------------------------------------- places

/// Page places worked out once per page (grid, shown number, whether it is in the book).
pub struct Places<'a> {
    doc: &'a Document,
    book: Option<ecad_model::book::Book>,
    pages: BTreeMap<PageId, (ecad_model::sheet::Grid, String, bool)>,
}

impl<'a> Places<'a> {
    pub fn new(doc: &'a Document) -> Self {
        Places { doc, book: None, pages: BTreeMap::new() }
    }

    fn page(&mut self, page: PageId) -> &(ecad_model::sheet::Grid, String, bool) {
        let doc = self.doc;
        let book = &mut self.book;
        self.pages.entry(page).or_insert_with(|| {
            // A stored number needs no book order (the usual case): the book is built only for a
            // page shown by its position.
            let number = match ecad_model::book::stored_number(doc, page) {
                Some(n) => n.to_string(),
                None => book.get_or_insert_with(|| ecad_model::book::Book::of(doc)).number(doc, page),
            };
            (ecad_model::sheet::Grid::of(doc, page), number, doc.page_order.contains(&page))
        })
    }

    /// `100-B-3` (or `/100.B`) for `at` on `page`; `?` (`/?`) when it cannot be located.
    pub fn text(&mut self, page: PageId, at: Option<Pt>) -> String {
        let style = self.doc.settings.reference_style;
        let unlocated = ecad_model::derive::unlocated(style).to_string();
        let Some(at) = at else { return unlocated };
        let (grid, number, located) = self.page(page);
        if !*located {
            return unlocated;
        }
        ecad_model::derive::place_on(grid, number, at).map_or(unlocated, |p| p.text(style))
    }

    /// The page's shown number.
    pub fn number(&mut self, page: PageId) -> String {
        self.page(page).1.clone()
    }
}

// ---------------------------------------------------------------- rows (design 025 §4.2)

/// Whether a row's pole is drawn, offered and not drawn, or drawn beyond the offer.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum State {
    Drawn,
    Spare,
    Over,
}

impl State {
    pub fn word(self) -> &'static str {
        match self {
            State::Drawn => "drawn",
            State::Spare => "spare",
            State::Over => "over",
        }
    }
}

/// One row of a contact table.
#[derive(Debug, Clone, PartialEq)]
pub struct Row {
    pub labels: [String; 2],
    pub kind: PoleKind,
    pub power: bool,
    pub state: State,
    /// The contact that draws it.
    pub rep: Option<RepresentationId>,
    pub page: Option<PageId>,
    pub at: Option<Pt>,
    /// Its place, or [`SPARE_PLACE`].
    pub place: String,
}

impl Row {
    /// `13/14`.
    pub fn label_pair(&self) -> String {
        format!("{}/{}", self.labels[0], self.labels[1])
    }

    /// `power` or `aux`.
    pub fn group_word(&self) -> &'static str {
        if self.power { "power" } else { "aux" }
    }
}

/// Natural order of labels: `2` before `13`, `A2` before `A10`.
pub fn natural_key(s: &str) -> Vec<(u8, u128, String)> {
    // One ordering for labels (review of PR #57, 5): the model's.
    ecad_model::links::natural_key(s)
}

/// The rows of device `dev` (design 025 §4.2): offered poles matched to drawn poles by labels,
/// then by kind in order; unmatched offered poles are spare, unmatched drawn poles over. With no
/// offer every drawn pole is a plain row. Power rows first, each group in natural label order.
pub fn rows(places: &mut Places<'_>, dev: &Device, offer: Option<&[OfferedPole]>) -> Vec<Row> {
    let drawn_row = |places: &mut Places<'_>, p: &DrawnPole, state: State| Row {
        labels: p.labels.clone(),
        kind: p.kind,
        power: p.power,
        state,
        rep: Some(p.rep),
        page: Some(p.page),
        at: p.at,
        place: places.text(p.page, p.at),
    };
    let mut out: Vec<Row> = Vec::new();
    match offer {
        None => out.extend(dev.poles.iter().map(|p| drawn_row(places, p, State::Drawn))),
        Some(offer) => {
            let same = |o: &OfferedPole, p: &DrawnPole| o.kind == p.kind && o.power == p.power;
            let mut taken = vec![false; dev.poles.len()];
            let mut matched = vec![false; offer.len()];
            for (i, o) in offer.iter().enumerate() {
                if let Some(j) = dev.poles.iter().enumerate().position(|(j, p)| !taken[j] && same(o, p) && p.labels == o.labels) {
                    taken[j] = true;
                    matched[i] = true;
                }
            }
            for (i, o) in offer.iter().enumerate() {
                if matched[i] {
                    continue;
                }
                if let Some(j) = dev.poles.iter().enumerate().position(|(j, p)| !taken[j] && same(o, p)) {
                    taken[j] = true;
                    matched[i] = true;
                }
            }
            for (j, p) in dev.poles.iter().enumerate() {
                out.push(drawn_row(places, p, if taken[j] { State::Drawn } else { State::Over }));
            }
            for (i, o) in offer.iter().enumerate().filter(|(i, _)| !matched[*i]) {
                let _ = i;
                out.push(Row {
                    labels: o.labels.clone(),
                    kind: o.kind,
                    power: o.power,
                    state: State::Spare,
                    rep: None,
                    page: None,
                    at: None,
                    place: SPARE_PLACE.into(),
                });
            }
        }
    }
    out.sort_by(|a, b| {
        (!a.power, natural_key(&a.labels[0]), natural_key(&a.labels[1]), a.state, a.rep).cmp(&(
            !b.power,
            natural_key(&b.labels[0]),
            natural_key(&b.labels[1]),
            b.state,
            b.rep,
        ))
    });
    out
}

/// Per kind (power NO, power NC, auxiliary NO, auxiliary NC): drawn and offered.
pub fn counts(dev: &Device, offer: &[OfferedPole]) -> BTreeMap<(bool, PoleKind), (usize, usize)> {
    let mut m: BTreeMap<(bool, PoleKind), (usize, usize)> = BTreeMap::new();
    for p in &dev.poles {
        m.entry((p.power, p.kind)).or_default().0 += 1;
    }
    for o in offer {
        m.entry((o.power, o.kind)).or_default().1 += 1;
    }
    m
}

/// `auxiliary NO`, `power NC`…
pub fn kind_words(power: bool, kind: PoleKind) -> String {
    format!("{} {}", if power { "power" } else { "auxiliary" }, kind.word())
}

/// The labels the next contact of kind `make` joining `c` takes from its offer (design 025 §4.3,
/// owner answer Q4): the first offered auxiliary pole of that kind whose labels are free. `None`
/// when `c` has no readable offer or no such pole is free.
pub fn offer_labels(doc: &Document, c: ComponentId, make: bool, used: &BTreeSet<String>) -> Option<Vec<String>> {
    let (_, offer) = offer_of(doc, c)?;
    let kind = if make { PoleKind::No } else { PoleKind::Nc };
    offer.iter().find(|o| !o.power && o.kind == kind && !used.contains(&o.labels[0]) && !used.contains(&o.labels[1])).map(|o| o.labels.to_vec())
}

/// The echo after a join that went beyond the offer (design 025 §4.3), if it did.
pub fn over_line(doc: &Document, c: ComponentId, labels: &[String], make: bool) -> Option<String> {
    let (text, offer) = offer_of(doc, c)?;
    let dev = device(doc, c);
    let kind = if make { PoleKind::No } else { PoleKind::Nc };
    let (drawn, offered) = counts(&dev, &offer).get(&(false, kind)).copied().unwrap_or_default();
    (drawn > offered).then(|| {
        format!(
            "Added {} contact {} to {}; its part offers {offered} {} ({text}), now {drawn} drawn (see Checks).",
            kind.word(),
            labels.join("/"),
            crate::devices::tag_words(doc, c),
            kind_words(false, kind),
        )
    })
}

// ---------------------------------------------------------------- drawing (design 025 §4)

/// A drawn part of a reference: a line or a text, in drawing units.
#[derive(Debug, Clone, PartialEq)]
pub enum Shape {
    Line(Pt, Pt),
    Text { at: Pt, value: String, height: f64, halign: HAlign, valign: VAlign },
}

/// One part of a contact's text or a head's table.
#[derive(Debug, Clone, PartialEq)]
pub struct Part {
    /// The contact (its text) or the head (its table).
    pub rep: RepresentationId,
    /// Its index among that representation's parts (handles are kept by it).
    pub index: u32,
    pub shape: Shape,
    /// A colour of its own (spare, over), else ByLayer.
    pub aci: Option<u8>,
    /// The table row it belongs to.
    pub row: Option<usize>,
    /// The table's top bar (drawn at the wire lineweight).
    pub bar: bool,
}

fn unit(doc: &Document) -> f64 {
    match doc.settings.length_unit {
        LengthUnit::Millimetre => 1.0,
        LengthUnit::Inch => 1.0 / 25.4,
    }
}

/// The text under contact `rep` of device `dev` (design 025 §4.1): its head's place, where it is
/// drawn, and how it is aligned. `None` when the device has no head or the contact cannot be
/// drawn.
pub fn contact_text(doc: &Document, places: &mut Places<'_>, dev: &Device, rep: RepresentationId) -> Option<Part> {
    let head = dev.head?;
    if !dev.children.contains(&rep) {
        return None;
    }
    let h = doc.representations.get(&head)?;
    let value = places.text(h.page, anchor(doc, h));
    let r = doc.representations.get(&rep)?;
    let k = unit(doc);
    let pts = terminal_points(doc, dev.component, r);
    let vertical = match crate::devices::rep_definition(doc, r) {
        Some((_, pl)) if r.form == Form::Symbol => crate::devices::vertical(&pl),
        _ => {
            let xs: Vec<f64> = pts.iter().map(|(_, p)| p.x).collect();
            xs.windows(2).all(|w| (w[0] - w[1]).abs() < 1e-6)
        }
    };
    let (at, halign) = if vertical {
        let x = pts.iter().map(|(_, p)| p.x).fold(f64::NEG_INFINITY, f64::max);
        let y = pts.iter().map(|(_, p)| p.y).fold(f64::INFINITY, f64::min);
        (Pt::new(x + 1.0 * k, y - 1.0 * k), HAlign::Left)
    } else {
        let (lo, hi) = match (r.form, crate::devices::rep_definition(doc, r)) {
            (Form::Symbol, Some((def, pl))) => crate::devices::placed_box(def, &pl)?,
            _ => bounds(&pts.iter().map(|(_, p)| *p).collect::<Vec<_>>())?,
        };
        (Pt::new((lo.x + hi.x) / 2.0, lo.y - 1.5 * k), HAlign::Center)
    };
    at.is_finite().then_some(Part {
        rep,
        index: 0,
        shape: Shape::Text { at, value, height: XREF_TEXT_MM * k, halign, valign: VAlign::Top },
        aci: None,
        row: None,
        bar: false,
    })
}

/// A head's table: where it is, its rows and its parts.
#[derive(Debug, Clone, PartialEq)]
pub struct Table {
    pub head: RepresentationId,
    pub page: PageId,
    /// The top centre of its bar.
    pub top: Pt,
    pub placement: Placement,
    pub rows: Vec<Row>,
    pub parts: Vec<Part>,
    /// Which step of design 033 §4.1 placed it.
    pub shown: TableShown,
    /// The y of its box's bottom (drawing units).
    pub bottom: f64,
}

/// Where a contact table went (design 033 §4.1): where its placement puts it, right of its head
/// (no room below), or lifted above the title block.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TableShown {
    AsPlaced,
    Right,
    Lifted,
}

impl TableShown {
    /// The `contacts` reply's word.
    pub fn word(self) -> &'static str {
        match self {
            TableShown::AsPlaced => "as placed",
            TableShown::Right => "right",
            TableShown::Lifted => "lifted",
        }
    }
}

/// Room kept between a contact table and the title block's top line, in millimetres.
pub const TITLE_CLEARANCE_MM: f64 = 2.0;

/// The box of a table at `top` with `rows` (design 033 §4.1): from −10 mm to 12 mm plus the widest
/// place text across, 3 mm plus the rows and the power/auxiliary gap down. (lo, hi), drawing units.
pub fn table_box(doc: &Document, top: Pt, rows: &[Row]) -> (Pt, Pt) {
    let k = unit(doc);
    let widest = rows.iter().map(|r| r.place.chars().count()).max().unwrap_or(0);
    // The stroke font is 0.8 × the height per character (`frame::fit`).
    let width = 12.0 + 0.8 * XREF_TEXT_MM * widest as f64;
    let gap = rows.windows(2).any(|w| w.first().is_some_and(|a| a.power) && w.get(1).is_some_and(|b| !b.power));
    let height = 3.0 + ROW_PITCH_MM * rows.len() as f64 + if gap { GROUP_GAP_MM } else { 0.0 };
    (Pt::new(top.x - 10.0 * k, top.y - height * k), Pt::new(top.x + width * k, top.y))
}

/// The page's free area for tables (design 033 §4.1): the frame's drawing area above the title
/// block, [`TITLE_CLEARANCE_MM`] clear of its top line. (lo, hi), drawing units.
pub fn free_area(doc: &Document, page: PageId) -> (Pt, Pt) {
    let g = ecad_model::sheet::Grid::of(doc, page);
    let k = unit(doc);
    let lo = Pt::new(g.frame_left(), g.frame_bottom() + (ecad_model::sheet::TITLE_BLOCK_MM + TITLE_CLEARANCE_MM) * k);
    let hi = Pt::new(g.frame_left() + g.area_width(), g.frame_top());
    (lo, hi)
}

/// Whether the box `b` lies in the free area `free`.
fn inside(b: (Pt, Pt), free: (Pt, Pt)) -> bool {
    b.0.x >= free.0.x - 1e-9 && b.0.y >= free.0.y - 1e-9 && b.1.x <= free.1.x + 1e-9 && b.1.y <= free.1.y + 1e-9
}

/// Whether the box `b` enters the title block of `page` (the user-placed check of design 033 §4.1).
pub fn enters_title_block(doc: &Document, page: PageId, b: (Pt, Pt)) -> bool {
    let g = ecad_model::sheet::Grid::of(doc, page);
    let line = g.frame_bottom() + ecad_model::sheet::TITLE_BLOCK_MM * unit(doc);
    let (left, right) = (g.frame_left(), g.frame_left() + g.area_width());
    b.0.y < line && b.1.y > g.frame_bottom() && b.1.x > left && b.0.x < right
}

/// Where an automatic table goes (design 033 §4.1), from where `placement` puts it (`top`): there
/// when its box lies in the free area; else right of the head (`auto`, `below`) when that fits;
/// else lifted onto the free area's bottom and pulled left inside the frame. A table taller than
/// the free area starts at its top. A table the user placed (`at`) is never moved.
pub fn settle(doc: &Document, c: ComponentId, head: RepresentationId, placement: Placement, top: Pt, rows: &[Row]) -> (Pt, TableShown) {
    if matches!(placement, Placement::At(_) | Placement::Off) {
        return (top, TableShown::AsPlaced);
    }
    let Some(page) = doc.representations.get(&head).map(|r| r.page) else { return (top, TableShown::AsPlaced) };
    let free = free_area(doc, page);
    if inside(table_box(doc, top, rows), free) {
        return (top, TableShown::AsPlaced);
    }
    if matches!(placement, Placement::Auto | Placement::Below)
        && let Some(right) = table_top(doc, c, head, Placement::Right)
        && inside(table_box(doc, right, rows), free)
    {
        return (right, TableShown::Right);
    }
    let (lo, hi) = table_box(doc, top, rows);
    let mut at = top;
    if lo.y < free.0.y {
        at.y += free.0.y - lo.y;
    }
    if at.y > free.1.y {
        at.y = free.1.y;
    }
    if hi.x > free.1.x {
        at.x -= hi.x - free.1.x;
    }
    if at.x - (top.x - lo.x) < free.0.x {
        at.x = free.0.x + (top.x - lo.x);
    }
    if at == top || !at.is_finite() {
        return (top, TableShown::AsPlaced);
    }
    (at, TableShown::Lifted)
}

/// The straight vertical run of a conductor route leaving `from` (one of its ends): the y of the
/// run's far end, when it runs down.
fn run_down(path: &[Pt], from: Pt) -> Option<f64> {
    let (first, last) = (*path.first()?, *path.last()?);
    let pts: Vec<Pt> = if first.dist(from) <= last.dist(from) { path.to_vec() } else { path.iter().rev().copied().collect() };
    let start = *pts.first()?;
    let mut end = start;
    for p in pts.iter().skip(1) {
        if (p.x - start.x).abs() > 1e-6 {
            break;
        }
        end = *p;
    }
    (end.y < start.y - 1e-6).then_some(end.y)
}

fn path_points(p: &Primitive) -> Option<Vec<Pt>> {
    match p {
        Primitive::Line { a, b } => Some(vec![*a, *b]),
        Primitive::Polyline { vertices, closed: false } => Some(vertices.clone()),
        _ => None,
    }
}

/// Where the table of head `head` of device `c` goes, for `placement`; `None` for `off` or a
/// head that cannot be drawn.
pub fn table_top(doc: &Document, c: ComponentId, head: RepresentationId, placement: Placement) -> Option<Pt> {
    let r = doc.representations.get(&head)?;
    let k = unit(doc);
    let origin = anchor(doc, r)?;
    let pts = terminal_points(doc, c, r);
    let lowest = pts.iter().min_by(|a, b| a.1.y.total_cmp(&b.1.y).then(a.0.cmp(&b.0))).copied();
    let below = || lowest.map_or(Pt::new(origin.x, origin.y - 4.0 * k), |(_, p)| Pt::new(origin.x, p.y - 4.0 * k));
    let top = match placement {
        Placement::Off => return None,
        Placement::At(d) => Pt::new(origin.x + d.x * k, origin.y + d.y * k),
        Placement::Below => below(),
        Placement::Right => {
            let hi = match crate::devices::rep_definition(doc, r) {
                Some((def, pl)) if r.form == Form::Symbol => crate::devices::placed_box(def, &pl).map(|(_, hi)| hi),
                _ => bounds(&pts.iter().map(|(_, p)| *p).collect::<Vec<_>>()).map(|(_, hi)| hi),
            };
            let hi = hi?;
            Pt::new(hi.x + 14.0 * k, hi.y)
        }
        Placement::Auto => {
            let run = lowest.and_then(|(t, p)| {
                doc.ends_at(t).into_iter().find_map(|(w, _)| {
                    let (_, prim) = ecad_model::derive::route_on_page(doc, w, r.page)?;
                    run_down(&path_points(&prim)?, p)
                })
            });
            match run {
                Some(y) => Pt::new(origin.x, y - 4.0 * k),
                None => below(),
            }
        }
    };
    top.is_finite().then_some(top)
}

/// The table under device `dev`'s head (design 025 §4.2), or `None`: no head, `off`, or neither
/// children nor an offer.
pub fn table(doc: &Document, places: &mut Places<'_>, dev: &Device) -> Option<Table> {
    let head = dev.head?;
    let offer = offer_of(doc, dev.component);
    if dev.children.is_empty() && offer.is_none() {
        return None;
    }
    let (placement, _) = placement_of(doc, dev.component);
    let top = table_top(doc, dev.component, head, placement)?;
    let page = doc.representations.get(&head)?.page;
    let rows = rows(places, dev, offer.as_ref().map(|(_, o)| o.as_slice()));
    let (top, shown) = settle(doc, dev.component, head, placement, top, &rows);
    let parts = table_parts(doc, head, top, &rows);
    let bottom = table_box(doc, top, &rows).0.y;
    Some(Table { head, page, top, placement, rows, parts, shown, bottom })
}

/// The lines and texts of a table at `top` (design 025 §4.2's geometry).
pub fn table_parts(doc: &Document, head: RepresentationId, top: Pt, rows: &[Row]) -> Vec<Part> {
    let k = unit(doc);
    let at = |x: f64, y: f64| Pt::new(top.x + x * k, top.y + y * k);
    let mut parts = Vec::new();
    let mut push = |shape: Shape, aci: Option<u8>, row: Option<usize>, bar: bool| {
        let index = u32::try_from(parts.len()).unwrap_or(u32::MAX);
        parts.push(Part { rep: head, index, shape, aci, row, bar });
    };
    push(Shape::Line(at(-10.0, 0.0), at(10.0, 0.0)), None, None, true);
    let h = XREF_TEXT_MM * k;
    let mut y = -3.0;
    let mut last_power = rows.first().map(|r| r.power);
    for (i, r) in rows.iter().enumerate() {
        if last_power == Some(true) && !r.power {
            y -= GROUP_GAP_MM;
        }
        last_power = Some(r.power);
        let aci = match r.state {
            State::Drawn => None,
            State::Spare => Some(SPARE_ACI),
            State::Over => Some(OVER_ACI),
        };
        let text =
            |x: f64, value: &str, halign: HAlign| Shape::Text { at: at(x, y), value: value.to_string(), height: h, halign, valign: VAlign::Middle };
        push(text(-5.0, &r.labels[0], HAlign::Right), aci, Some(i), false);
        let line = |a: (f64, f64), b: (f64, f64)| Shape::Line(at(a.0, y + a.1), at(b.0, y + b.1));
        push(line((-4.0, 0.0), (-1.5, 0.0)), aci, Some(i), false);
        match r.kind {
            PoleKind::No => push(line((-1.5, 0.0), (1.5, 1.5)), aci, Some(i), false),
            PoleKind::Nc => {
                push(line((-1.5, 0.0), (2.0, -1.2)), aci, Some(i), false);
                push(line((1.5, 0.0), (1.5, -1.2)), aci, Some(i), false);
            }
        }
        push(line((1.5, 0.0), (4.0, 0.0)), aci, Some(i), false);
        push(text(5.0, &r.labels[1], HAlign::Left), aci, Some(i), false);
        push(text(12.0, &r.place, HAlign::Left), aci, Some(i), false);
        y -= ROW_PITCH_MM;
    }
    parts
}

/// Every reference part drawn on `page`: the texts under its contacts and the tables under its
/// heads. O(the page's component representations and their groups).
pub fn parts_on_page(doc: &Document, page: PageId) -> Vec<Part> {
    let mut by_component: BTreeMap<ComponentId, Vec<RepresentationId>> = BTreeMap::new();
    for (rid, r) in doc.representations_on(page) {
        if let Subject::Component(c) = r.subject {
            by_component.entry(c).or_default().push(rid);
        }
    }
    let mut places = Places::new(doc);
    let mut out = Vec::new();
    for (c, here) in by_component {
        let dev = device(doc, c);
        if dev.head.is_none() {
            continue;
        }
        for rid in &here {
            if let Some(p) = contact_text(doc, &mut places, &dev, *rid) {
                out.push(p);
            }
        }
        if dev.head.is_some_and(|h| here.contains(&h))
            && let Some(t) = table(doc, &mut places, &dev)
        {
            out.extend(t.parts);
        }
    }
    out
}

/// What device `c` shows on each page (its contacts' texts and its table), for comparing before
/// and after a commit: page → parts.
fn shown(doc: &Document, places: &mut Places<'_>, c: ComponentId) -> BTreeMap<PageId, Vec<Part>> {
    let dev = device(doc, c);
    let mut out: BTreeMap<PageId, Vec<Part>> = BTreeMap::new();
    if dev.head.is_none() {
        return out;
    }
    for rid in &dev.children {
        if let (Some(p), Some(r)) = (contact_text(doc, places, &dev, *rid), doc.representations.get(rid)) {
            out.entry(r.page).or_default().push(p);
        }
    }
    if let Some(t) = table(doc, places, &dev) {
        out.entry(t.page).or_default().extend(t.parts);
    }
    out
}

// ---------------------------------------------------------------- the index (design 025 §3.3)

/// The general reference index: each member's group and class, each group's members, and its
/// edges both ways. Derived, never saved.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct RefIndex {
    member: BTreeMap<RepresentationId, (Group, Class)>,
    members: BTreeMap<Group, BTreeSet<RepresentationId>>,
    targets: BTreeMap<RepresentationId, BTreeSet<RepresentationId>>,
    sources: BTreeMap<RepresentationId, BTreeSet<RepresentationId>>,
}

impl RefIndex {
    /// The whole index of `doc` (open, reload, branch switch): O(representations).
    pub fn build(doc: &Document) -> Self {
        // One pass in ID order: each member classified once (the first head of a device is its
        // head, as `make` decides), then each group's edges.
        let mut groups: BTreeMap<Group, Vec<(RepresentationId, Class)>> = BTreeMap::new();
        for (rid, r) in &doc.representations {
            let (g, c) = match r.subject {
                Subject::Cable(k) if crate::references::is_reference(r) => (Group::Cable(k), Class::End),
                Subject::Component(c) => match raw_class(doc, r) {
                    Class::Other => continue,
                    class => (Group::Device(c), class),
                },
                _ => continue,
            };
            let list = groups.entry(g).or_default();
            let c = if c == Class::Head && list.iter().any(|(_, x)| *x == Class::Head) { Class::Other } else { c };
            list.push((*rid, c));
        }
        let mut idx = RefIndex::default();
        for (g, list) in groups {
            idx.insert_group(g, list);
        }
        idx
    }

    /// Make group `g` again from `doc`: its members, classes and edges. O(group).
    fn make(&mut self, doc: &Document, g: Group) {
        if let Some(old) = self.members.remove(&g) {
            for r in old {
                self.member.remove(&r);
                for t in self.targets.remove(&r).unwrap_or_default() {
                    if let Some(s) = self.sources.get_mut(&t) {
                        s.remove(&r);
                        if s.is_empty() {
                            self.sources.remove(&t);
                        }
                    }
                }
                for s in self.sources.remove(&r).unwrap_or_default() {
                    if let Some(t) = self.targets.get_mut(&s) {
                        t.remove(&r);
                        if t.is_empty() {
                            self.targets.remove(&s);
                        }
                    }
                }
            }
        }
        let list: Vec<(RepresentationId, Class)> = match g {
            Group::Cable(k) => doc
                .representations_of(Subject::Cable(k))
                .filter(|(_, r)| crate::references::is_reference(r))
                .map(|(id, _)| (id, Class::End))
                .collect(),
            Group::Device(c) => {
                let mut head_seen = false;
                doc.representations_of(Subject::Component(c))
                    .filter_map(|(id, r)| match raw_class(doc, r) {
                        Class::Head if !head_seen => {
                            head_seen = true;
                            Some((id, Class::Head))
                        }
                        Class::Head => Some((id, Class::Other)),
                        Class::Child => Some((id, Class::Child)),
                        _ => None,
                    })
                    .collect()
            }
        };
        self.insert_group(g, list);
    }

    /// Add group `g` with its members `list` (classes decided) and its edges.
    fn insert_group(&mut self, g: Group, list: Vec<(RepresentationId, Class)>) {
        if list.is_empty() {
            return;
        }
        let mut edges: Vec<(RepresentationId, RepresentationId)> = Vec::new();
        match g {
            Group::Cable(_) => {
                for (a, _) in &list {
                    for (b, _) in &list {
                        if a != b {
                            edges.push((*a, *b));
                        }
                    }
                }
            }
            Group::Device(_) => {
                if let Some((h, _)) = list.iter().find(|(_, c)| *c == Class::Head) {
                    for (ch, _) in list.iter().filter(|(_, c)| *c == Class::Child) {
                        edges.push((*ch, *h));
                        edges.push((*h, *ch));
                    }
                }
            }
        }
        for (r, c) in &list {
            self.member.insert(*r, (g, *c));
        }
        self.members.insert(g, list.iter().map(|(r, _)| *r).collect());
        for (s, t) in edges {
            self.targets.entry(s).or_default().insert(t);
            self.sources.entry(t).or_default().insert(s);
        }
    }

    /// Make the groups `groups` again from `doc` (after a commit).
    pub fn refresh(&mut self, doc: &Document, groups: &BTreeSet<Group>) {
        for g in groups {
            self.make(doc, *g);
        }
    }

    /// The groups whose members or classes `ops` could change, looked up in `doc` (called on the
    /// state before and after the ops).
    pub fn groups_named(doc: &Document, ops: &[Op]) -> BTreeSet<Group> {
        let mut out = BTreeSet::new();
        let of = |r: &Representation, out: &mut BTreeSet<Group>| match r.subject {
            Subject::Cable(k) => {
                out.insert(Group::Cable(k));
            }
            Subject::Component(c) => {
                out.insert(Group::Device(c));
            }
            Subject::Conductor(_) => {}
        };
        for op in ops {
            match op {
                Op::LinkRepresentation { rep, .. } | Op::UnlinkRepresentation { rep, .. } => of(rep, &mut out),
                Op::ModifyRepresentation { before, after, .. } => {
                    of(before, &mut out);
                    of(after, &mut out);
                }
                // A symbol item's definition says what the symbol is.
                Op::AddItem { id, .. } | Op::ModifyItem { id, .. } | Op::RemoveItem { id, .. } => {
                    if let Some((_, r)) = doc.representation_of_item(*id) {
                        of(r, &mut out);
                    }
                }
                Op::CreateComponent { id, .. } | Op::DeleteComponent { id, .. } => {
                    out.insert(Group::Device(*id));
                }
                Op::CreateCable { id, .. } | Op::DeleteCable { id, .. } => {
                    out.insert(Group::Cable(*id));
                }
                Op::EmbedDefinition { id, .. } | Op::RemoveDefinition { id, .. } => {
                    out.extend(doc.components_using(*id).into_iter().map(Group::Device));
                }
                _ => {}
            }
        }
        out
    }

    /// The group and class of member `r`.
    pub fn member(&self, r: RepresentationId) -> Option<(Group, Class)> {
        self.member.get(&r).copied()
    }

    /// The members of group `g`.
    pub fn members(&self, g: Group) -> impl Iterator<Item = RepresentationId> + '_ {
        self.members.get(&g).into_iter().flatten().copied()
    }

    /// What source `s` shows.
    pub fn targets(&self, s: RepresentationId) -> impl Iterator<Item = RepresentationId> + '_ {
        self.targets.get(&s).into_iter().flatten().copied()
    }

    /// Who shows target `t`.
    pub fn sources(&self, t: RepresentationId) -> impl Iterator<Item = RepresentationId> + '_ {
        self.sources.get(&t).into_iter().flatten().copied()
    }

    /// Every group.
    pub fn groups(&self) -> impl Iterator<Item = Group> + '_ {
        self.members.keys().copied()
    }

    /// How many members, groups and edges.
    pub fn sizes(&self) -> (usize, usize, usize) {
        (self.member.len(), self.members.len(), self.targets.values().map(BTreeSet::len).sum())
    }

    /// The pages that hold a member of a device group (the wide case's pages).
    pub fn device_pages(&self, doc: &Document) -> BTreeSet<PageId> {
        self.member
            .iter()
            .filter(|(_, (g, _))| matches!(g, Group::Device(_)))
            .filter_map(|(r, _)| doc.representations.get(r).map(|x| x.page))
            .collect()
    }

    /// The devices with a member on `page` (through the page index).
    pub fn devices_on(&self, doc: &Document, page: PageId) -> BTreeSet<ComponentId> {
        doc.representations_on(page)
            .filter_map(|(r, _)| match self.member.get(&r) {
                Some((Group::Device(c), _)) => Some(*c),
                _ => None,
            })
            .collect()
    }

    /// Compare with a rebuild from `doc` (verify mode, the debugger's Check index): `Err` names
    /// the first difference.
    ///
    /// # Errors
    /// The kept index differs from a rebuilt one.
    pub fn verify(&self, doc: &Document) -> Result<(), String> {
        let fresh = RefIndex::build(doc);
        if *self == fresh {
            return Ok(());
        }
        let first = self
            .member
            .iter()
            .find(|(r, v)| fresh.member.get(r) != Some(v))
            .map(|(r, v)| format!("member {} kept as {v:?}, rebuilt as {:?}", r.hex(), fresh.member.get(r)))
            .or_else(|| {
                fresh.member.iter().find(|(r, _)| !self.member.contains_key(r)).map(|(r, v)| format!("member {} missing (rebuilt as {v:?})", r.hex()))
            })
            .unwrap_or_else(|| "edges differ".into());
        Err(format!("The reference index differs from a rebuild: {first}."))
    }
}

// ---------------------------------------------------------------- per commit (design 025 §6)

/// What a commit's ops could change in the device references: the devices to compare, or every
/// page with a device member (`wide`).
#[derive(Debug, Clone, Default)]
struct Scope {
    devices: BTreeSet<ComponentId>,
    wide: bool,
}

impl Scope {
    /// Add what `ops` name, looked up in `doc` (called on the state before and after).
    fn collect(&mut self, doc: &Document, idx: &RefIndex, ops: &[Op]) {
        let ends = |s: &mut Scope, w: ecad_model::ConductorId| {
            if let Some(c) = doc.conductors.get(&w) {
                for t in c.ends.iter().flatten() {
                    if let Some(x) = doc.terminals.get(t) {
                        s.devices.insert(x.component);
                    }
                }
            }
        };
        let rep = |s: &mut Scope, r: &Representation| match r.subject {
            Subject::Component(c) => {
                s.devices.insert(c);
            }
            Subject::Conductor(w) => ends(s, w),
            Subject::Cable(_) => {}
        };
        let mut moved = false;
        for op in ops {
            match op {
                Op::ModifyItem { id, .. } | Op::RemoveItem { id, .. } | Op::AddItem { id, .. } => {
                    if let Some((_, r)) = doc.representation_of_item(*id) {
                        rep(self, r);
                    }
                }
                Op::LinkRepresentation { rep: r, .. } | Op::UnlinkRepresentation { rep: r, .. } => rep(self, r),
                Op::ModifyRepresentation { before, after, .. } => {
                    rep(self, before);
                    rep(self, after);
                }
                Op::AttachEnd { conductor, before, after, .. } => {
                    ends(self, *conductor);
                    for t in [before, after].into_iter().flatten() {
                        if let Some(x) = doc.terminals.get(t) {
                            self.devices.insert(x.component);
                        }
                    }
                }
                Op::CreateConductor { conductor, .. } | Op::DeleteConductor { conductor, .. } => {
                    for t in conductor.ends.iter().flatten() {
                        if let Some(x) = doc.terminals.get(t) {
                            self.devices.insert(x.component);
                        }
                    }
                }
                Op::AddTerminal { terminal, .. } | Op::RemoveTerminal { terminal, .. } => {
                    self.devices.insert(terminal.component);
                }
                Op::CreateComponent { id, .. } | Op::DeleteComponent { id, .. } => {
                    self.devices.insert(*id);
                }
                Op::SetProperty { object: ObjectRef::Component(c), key, .. } if key == CONTACTS_KEY || key == MIRROR_KEY => {
                    self.devices.insert(*c);
                }
                Op::SetProperty { object: ObjectRef::Page(p), key, .. }
                    if key == ecad_model::book::NUMBER_KEY || key == ecad_model::sheet::SHEET_KEY =>
                {
                    self.devices.extend(idx.devices_on(doc, *p));
                }
                Op::SetProperty { object: ObjectRef::Page(_), key, .. } if key == ecad_model::book::FOLDER_KEY => {
                    self.moved(doc, idx, ops, &mut moved);
                }
                Op::AddPage { .. } | Op::RemovePage { .. } | Op::AddFolder { .. } | Op::RemoveFolder { .. } | Op::ModifyFolder { .. } => {
                    self.moved(doc, idx, ops, &mut moved);
                }
                Op::EmbedDefinition { id, .. } | Op::RemoveDefinition { id, .. } => {
                    self.devices.extend(doc.components_using(*id));
                }
                Op::SetProjectSettings { before, after }
                    if before.reference_style != after.reference_style
                        || before.sheet != after.sheet
                        || before.length_unit != after.length_unit
                        || before.contact_mirror != after.contact_mirror =>
                {
                    self.wide = true;
                }
                _ => {}
            }
        }
    }

    fn moved(&mut self, doc: &Document, idx: &RefIndex, ops: &[Op], done: &mut bool) {
        if std::mem::replace(done, true) {
            return;
        }
        for p in ecad_model::book::positions_changed(doc, ops) {
            self.devices.extend(idx.devices_on(doc, p));
        }
    }
}

/// The references a commit could change, taken before it and finished after it: the cable
/// references ([`crate::references::Snapshot`], scoped per cable) and the touched devices' texts
/// and tables (design 025 §6).
#[derive(Debug, Clone)]
pub struct Watch {
    cable_scope: Option<BTreeSet<CableId>>,
    cables: crate::references::Snapshot,
    scope: Scope,
    before: BTreeMap<ComponentId, BTreeMap<PageId, Vec<Part>>>,
    /// The pages with a device member before (only in the wide case).
    wide_pages: BTreeSet<PageId>,
    groups: BTreeSet<Group>,
    /// What verify mode found wrong (bugs), for the caller to record.
    problems: Vec<String>,
}

/// What a finished [`Watch`] found.
#[derive(Debug, Clone, Default)]
pub struct Outcome {
    /// Pages whose references changed; `None` for every page.
    pub pages: Option<BTreeSet<PageId>>,
    /// Design 010 §4.3's line about references left without another end.
    pub orphaned: Option<String>,
    /// The texts and tables of the devices it looked at, after the commit: the next commit's
    /// "before" when nothing has changed in between ([`Watch::take`]).
    pub shown: Shown,
    /// What verify mode found wrong and repaired (bugs): one line each, for the event log.
    pub problems: Vec<String>,
}

/// Texts and tables derived for some devices, by page.
pub type Shown = BTreeMap<ComponentId, BTreeMap<PageId, Vec<Part>>>;

impl Watch {
    /// Before `ops` are committed on `doc`. `reuse` holds what the last commit derived, for
    /// the same `doc` (the caller checks the state is unchanged): a device found there is not
    /// derived again.
    pub fn take(doc: &Document, idx: &RefIndex, ops: &[Op], mut reuse: Shown) -> Watch {
        let cable_scope = crate::references::cables_touched(doc, ops);
        let cables = crate::references::Snapshot::take(doc, cable_scope.as_ref());
        let mut scope = Scope::default();
        scope.collect(doc, idx, ops);
        let mut places = Places::new(doc);
        let mut problems = Vec::new();
        let before = if scope.wide {
            BTreeMap::new()
        } else {
            let verify = ecad_model::ledger::verify_mode();
            scope
                .devices
                .iter()
                .map(|c| match reuse.remove(c) {
                    // Verify mode checks a reuse against a fresh derivation; a stale one (a bug) is
                    // reported and the fresh one used.
                    Some(kept) if verify => {
                        let fresh = shown(doc, &mut places, *c);
                        if kept != fresh {
                            problems.push(format!("the texts kept for {} were stale; derived again", c.hex()));
                        }
                        (*c, fresh)
                    }
                    Some(kept) => (*c, kept),
                    None => (*c, shown(doc, &mut places, *c)),
                })
                .collect()
        };
        let wide_pages = if scope.wide { idx.device_pages(doc) } else { BTreeSet::new() };
        Watch { cable_scope, cables, scope, before, wide_pages, groups: RefIndex::groups_named(doc, ops), problems }
    }

    /// After the commit: `doc` is the state after it and `ops` its ops as recorded (with any pass
    /// that extended it). Brings `idx` up to date and says what changed.
    pub fn finish(mut self, doc: &Document, idx: &mut RefIndex, ops: &[Op]) -> Outcome {
        self.groups.extend(RefIndex::groups_named(doc, ops));
        idx.refresh(doc, &self.groups);
        if ecad_model::ledger::verify_mode()
            && let Err(e) = idx.verify(doc)
        {
            // A bug: reported for the event log, and the index rebuilt so it is never left stale.
            self.problems.push(format!("the reference index was out of step ({e}); rebuilt"));
            *idx = RefIndex::build(doc);
        }
        let after_cables = crate::references::Snapshot::take(doc, self.cable_scope.as_ref());
        let orphaned = self.cables.orphaned_line(doc, &after_cables);
        let mut pages = self.cables.pages_changed(&after_cables);
        self.scope.collect(doc, idx, ops);
        let mut shown_after = Shown::new();
        let device_pages: BTreeSet<PageId> = if self.scope.wide {
            let mut p = std::mem::take(&mut self.wide_pages);
            p.extend(idx.device_pages(doc));
            p
        } else {
            let mut places = Places::new(doc);
            let mut out = BTreeSet::new();
            for c in &self.scope.devices {
                let old = self.before.remove(c).unwrap_or_default();
                let new = shown(doc, &mut places, *c);
                for page in old.keys().chain(new.keys()) {
                    if old.get(page) != new.get(page) {
                        out.insert(*page);
                    }
                }
                shown_after.insert(*c, new);
            }
            out
        };
        if let Some(p) = &mut pages {
            p.extend(device_pages);
        }
        Outcome { pages, orphaned, shown: shown_after, problems: self.problems }
    }
}

// ---------------------------------------------------------------- reading (design 025 §5)

/// One reference as `xrefs` lists it.
#[derive(Debug, Clone, PartialEq)]
pub struct Reference {
    pub source: RepresentationId,
    pub source_page: PageId,
    pub target: RepresentationId,
    pub target_page: PageId,
    /// `cable`, `contact` (a contact shows its coil) or `mirror` (a table row shows a contact).
    pub kind: &'static str,
    /// The contact pole it is about (`13/14`), for device references: one per pole.
    pub labels: Option<String>,
    pub text: String,
}

/// The references of the groups `groups`, in group order, then source, then target.
pub fn references(doc: &Document, idx: &RefIndex, groups: &BTreeSet<Group>) -> Vec<Reference> {
    let mut out = Vec::new();
    let mut places = Places::new(doc);
    for g in groups {
        let mut cable_texts: BTreeMap<RepresentationId, String> = BTreeMap::new();
        if let Group::Cable(k) = g {
            for r in ecad_model::derive::cable_references_of(doc, &BTreeSet::from([*k])) {
                cable_texts.insert(r.reference, r.text);
            }
        }
        let dev = match g {
            Group::Device(c) => Some(device(doc, *c)),
            Group::Cable(_) => None,
        };
        for s in idx.members(*g) {
            for t in idx.targets(s) {
                let (Some(sr), Some(tr)) = (doc.representations.get(&s), doc.representations.get(&t)) else { continue };
                let r = |kind, labels, text| Reference { source: s, source_page: sr.page, target: t, target_page: tr.page, kind, labels, text };
                // A device reference is listed once per pole: a table row per pole, and a
                // contact's text for each of its poles.
                let poles = |rep: RepresentationId| -> Vec<String> {
                    dev.iter().flat_map(|d| d.poles.iter()).filter(|p| p.rep == rep).map(|p| format!("{}/{}", p.labels[0], p.labels[1])).collect()
                };
                match (g, idx.member(s).map(|(_, c)| c)) {
                    (Group::Cable(_), _) => out.push(r("cable", None, cable_texts.get(&s).cloned().unwrap_or_default())),
                    (Group::Device(_), Some(Class::Head)) => {
                        let place = places.text(tr.page, anchor(doc, tr));
                        out.extend(poles(t).into_iter().map(|l| r("mirror", Some(l), place.clone())));
                    }
                    (Group::Device(_), _) => {
                        let place = places.text(tr.page, anchor(doc, tr));
                        out.extend(poles(s).into_iter().map(|l| r("contact", Some(l), place.clone())));
                    }
                }
            }
        }
    }
    out
}

/// `xrefs {component?, cable?, page?, limit?, after?}`'s result (design 025 §5.4).
///
/// # Errors
/// Unknown component, cable or page.
pub fn xrefs_json(
    doc: &Document, idx: &RefIndex, component: Option<ComponentId>, cable: Option<CableId>, page: Option<PageId>, limit: usize, after: usize,
) -> Result<Value, String> {
    let groups: BTreeSet<Group> = match (component, cable) {
        (Some(c), _) => {
            if !doc.components.contains_key(&c) {
                return Err("no such component".into());
            }
            BTreeSet::from([Group::Device(c)])
        }
        (None, Some(k)) => {
            if !doc.cables.contains_key(&k) {
                return Err("no such cable".into());
            }
            BTreeSet::from([Group::Cable(k)])
        }
        (None, None) => match page {
            Some(p) => {
                if !doc.pages.contains_key(&p) {
                    return Err("no such page".into());
                }
                doc.representations_on(p).filter_map(|(r, _)| idx.member(r).map(|(g, _)| g)).collect()
            }
            None => idx.groups().collect(),
        },
    };
    let all: Vec<Reference> =
        references(doc, idx, &groups).into_iter().filter(|r| page.is_none_or(|p| r.source_page == p || r.target_page == p)).collect();
    let list: Vec<Value> = all
        .iter()
        .skip(after)
        .take(limit)
        .map(|r| {
            json!({
                "source": r.source.hex(), "source_page": crate::references::page_number(doc, r.source_page),
                "target": r.target.hex(), "target_page": crate::references::page_number(doc, r.target_page),
                "kind": r.kind, "labels": r.labels, "text": r.text,
            })
        })
        .collect();
    let mut v = json!({ "references": list });
    if after + limit < all.len() {
        v["next"] = json!(after + limit);
    }
    Ok(v)
}

/// `contacts {component}`'s result (design 025 §5.4).
///
/// # Errors
/// Unknown component.
pub fn contacts_json(doc: &Document, c: ComponentId) -> Result<Value, String> {
    if !doc.components.contains_key(&c) {
        return Err("no such component".into());
    }
    let dev = device(doc, c);
    let offer = offer_of(doc, c);
    let stored = doc.components.get(&c).and_then(|x| x.properties.get(CONTACTS_KEY)).cloned();
    let mut places = Places::new(doc);
    let rows = rows(&mut places, &dev, offer.as_ref().map(|(_, o)| o.as_slice()));
    let over = rows.iter().filter(|r| r.state == State::Over).count();
    let (placement, own) = placement_of(doc, c);
    let head = dev.head.and_then(|h| doc.representations.get(&h).map(|r| (h, r)));
    Ok(json!({
        "component": c.hex(),
        "tag": crate::devices::tag_of(doc, c),
        "offer": stored,
        "head": head.map(|(h, r)| json!({"representation": h.hex(), "page": crate::references::page_number(doc, r.page), "place": places.text(r.page, anchor(doc, r))})),
        "placement": placement.word(),
        "placement_from": if own { "device" } else { "project" },
        "rows": rows.iter().map(|r| json!({
            "labels": r.label_pair(), "kind": r.kind.word(), "group": r.group_word(), "state": r.state.word(),
            "representation": r.rep.map(|x| x.hex()), "place": (r.state != State::Spare).then(|| r.place.clone()),
        })).collect::<Vec<_>>(),
        "over": over,
        "table": table_json(doc, &mut places, &dev),
    }))
}

/// The `contacts` reply's `table` (design 033 §4.2): `{top, bottom, placement, shown}`, or null.
fn table_json(doc: &Document, places: &mut Places<'_>, dev: &Device) -> Value {
    let Some(t) = table(doc, places, dev) else { return Value::Null };
    let k = unit(doc);
    json!({
        "top": [t.top.x / k, t.top.y / k],
        "bottom": t.bottom / k,
        "placement": t.placement.word(),
        "shown": t.shown.word(),
    })
}

/// The target `refgoto` goes to from `source` (design 025 §5.3): a cable end's partner, a
/// contact's head, or a head's given target (`target`, else its first child). `None` when it shows
/// nothing.
pub fn goto_target(doc: &Document, idx: &RefIndex, source: RepresentationId, target: Option<RepresentationId>) -> Option<RepresentationId> {
    match idx.member(source) {
        Some((Group::Cable(_), _)) => {
            let refs = ecad_model::derive::cable_references_of(
                doc,
                &BTreeSet::from([match doc.representations.get(&source)?.subject {
                    Subject::Cable(k) => k,
                    _ => return None,
                }]),
            );
            refs.into_iter().find(|r| r.reference == source).and_then(|r| r.partner())
        }
        Some(_) => match target {
            Some(t) => idx.targets(source).find(|x| *x == t),
            None => idx.targets(source).next(),
        },
        None => None,
    }
}

/// Hover lines (design 025 §5.1) for contact `rep`: `Coil: 100-B-3  (page 100 "Supply")`.
pub fn contact_lines(doc: &Document, c: ComponentId, rep: RepresentationId) -> Vec<String> {
    let dev = device(doc, c);
    if !dev.children.contains(&rep) {
        return Vec::new();
    }
    let noun = |doc: &Document, h: RepresentationId| {
        doc.representations.get(&h).and_then(|r| starter_of(doc, r)).map_or("Head", |s| match s.key {
            "coil" => "Coil",
            "mcb1" | "mcb2" => "MCB",
            _ => "Head",
        })
    };
    match dev.head.and_then(|h| doc.representations.get(&h).map(|r| (h, r))) {
        Some((h, r)) => {
            let mut places = Places::new(doc);
            vec![format!("{}: {}  ({})", noun(doc, h), places.text(r.page, anchor(doc, r)), crate::references::page_words(doc, r.page))]
        }
        None => vec!["Coil: not drawn".into()],
    }
}

/// Hover lines for head `rep` of device `c`: `Contacts: 5 drawn, 1 spare (3P+1NO+1NC)` and up to
/// six rows.
pub fn head_lines(doc: &Document, c: ComponentId, rep: RepresentationId) -> Vec<String> {
    let dev = device(doc, c);
    if dev.head != Some(rep) {
        return Vec::new();
    }
    let offer = offer_of(doc, c);
    let mut places = Places::new(doc);
    let rows = rows(&mut places, &dev, offer.as_ref().map(|(_, o)| o.as_slice()));
    let n = |s: State| rows.iter().filter(|r| r.state == s).count();
    let mut head = format!("Contacts: {} drawn", n(State::Drawn) + n(State::Over));
    if n(State::Spare) > 0 {
        head.push_str(&format!(", {} spare", n(State::Spare)));
    }
    if n(State::Over) > 0 {
        head.push_str(&format!(", {} beyond the part", n(State::Over)));
    }
    if let Some((t, _)) = &offer {
        head.push_str(&format!(" ({t})"));
    }
    let mut lines = vec![head];
    for r in rows.iter().take(6) {
        lines.push(format!("  {} {} {} {}", r.label_pair(), r.kind.word(), r.group_word(), r.place));
    }
    if rows.len() > 6 {
        lines.push(format!("  +{} more", rows.len() - 6));
    }
    lines
}

/// Hover lines for row `i` of head `head`'s table.
pub fn row_lines(doc: &Document, c: ComponentId, i: usize) -> Option<Vec<String>> {
    let dev = device(doc, c);
    let mut places = Places::new(doc);
    let t = table(doc, &mut places, &dev)?;
    let r = t.rows.get(i)?;
    let tag = crate::devices::tag_words(doc, c);
    let what = if r.power { "power" } else { "auxiliary" };
    Some(vec![match (r.state, r.page) {
        (State::Spare, _) | (_, None) => format!("{tag} {} {}, {what}, offered, not drawn", r.label_pair(), r.kind.word()),
        (s, Some(p)) => {
            let over = if s == State::Over { " (beyond the part's offer)" } else { "" };
            format!("{tag} {} {}, {what}, {} {}{over}", r.label_pair(), r.kind.word(), crate::references::page_words(doc, p), r.place)
        }
    }])
}

/// The Checks rows of device `c` (design 025 §4.3): one `too-many-contacts` warning per kind drawn
/// beyond the offer, ringed at the first over pole.
pub fn count_checks(doc: &Document, c: ComponentId) -> Vec<(String, Option<PageId>, Option<Pt>)> {
    let Some((text, offer)) = offer_of(doc, c) else { return Vec::new() };
    let dev = device(doc, c);
    // A device with no head shows no references and gets no check (design 025 §12 Q2).
    if dev.head.is_none() || dev.poles.is_empty() {
        return Vec::new();
    }
    let mut places = Places::new(doc);
    let rows = rows(&mut places, &dev, Some(&offer));
    let tag = crate::devices::tag_words(doc, c);
    counts(&dev, &offer)
        .into_iter()
        .filter(|(_, (drawn, offered))| drawn > offered)
        .map(|((power, kind), (drawn, offered))| {
            let first = rows.iter().find(|r| r.state == State::Over && r.power == power && r.kind == kind);
            (
                format!("{tag}: {drawn} {} drawn, part offers {offered} ({text})", kind_words(power, kind)),
                first.and_then(|r| r.page),
                first.and_then(|r| r.at),
            )
        })
        .collect()
}

/// The Checks row of a table the user placed over the title block (design 033 §4.1): its words,
/// page and the table's top; `None` for an automatic placement (never moved onto it) or no table.
pub fn title_check(doc: &Document, c: ComponentId) -> Option<(String, PageId, Pt)> {
    let (placement, _) = placement_of(doc, c);
    if !matches!(placement, Placement::At(_)) {
        return None;
    }
    let dev = device(doc, c);
    let mut places = Places::new(doc);
    let t = table(doc, &mut places, &dev)?;
    if !enters_title_block(doc, t.page, table_box(doc, t.top, &t.rows)) {
        return None;
    }
    let which = format!(
        "The contact table of {} overlaps the title block on {}.",
        crate::devices::tag_words(doc, c),
        crate::references::page_number_words(doc, t.page)
    );
    Some((which, t.page, t.top))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_offer_grammar() {
        let labels = |t: &str| {
            parse_offer(t).map(|v| {
                v.iter().map(|o| format!("{}/{} {}{}", o.labels[0], o.labels[1], if o.power { "P" } else { "" }, o.kind.word())).collect::<Vec<_>>()
            })
        };
        assert_eq!(labels("3P+1NO+1NC").unwrap(), ["1/2 PNO", "3/4 PNO", "5/6 PNO", "13/14 NO", "21/22 NC"]);
        assert_eq!(labels("3p, 2no ,1nc").unwrap(), ["1/2 PNO", "3/4 PNO", "5/6 PNO", "13/14 NO", "23/24 NO", "31/32 NC"]);
        assert_eq!(labels("13/14 NO, 21/22 NC").unwrap(), ["13/14 NO", "21/22 NC"]);
        assert_eq!(labels("L1/T1 P").unwrap(), ["L1/T1 PNO"]);
        assert_eq!(labels("3P+1PNC").unwrap(), ["1/2 PNO", "3/4 PNO", "5/6 PNO", "7/8 PNC"]);
        for bad in ["3X", "", "+", "0NO", "NO", "/14 NO", "33NO+", "1NO++1NC"] {
            assert!(parse_offer(bad).is_err(), "{bad:?}");
        }
        assert_eq!(parse_offer("3X").unwrap_err(), "\"3X\" is not a contact list. Write it like 3P+1NO+1NC, or 13/14 NO, 21/22 NC.");
        assert!(parse_offer("32NO").is_ok());
        assert!(parse_offer("33NO").is_err());
        assert!(parse_offer("30NO+3NC").is_err());
        assert!(parse_offer(&"1NO+".repeat(5000)).is_err());
        assert!(parse_offer("99999999999999999999NO").is_err());
    }

    #[test]
    fn the_mirror_values() {
        assert_eq!(parse_mirror("Below").unwrap(), Placement::Below);
        assert_eq!(parse_mirror(" at 12, -30.5 ").unwrap(), Placement::At(Pt::new(12.0, -30.5)));
        assert_eq!(parse_mirror("at 12,-30.5").unwrap().word(), "at 12,-30.5");
        for bad in ["at NaN,1", "at 1", "at inf,2", "at 99999,0", "sideways", ""] {
            assert!(parse_mirror(bad).is_err(), "{bad:?}");
        }
        assert_eq!(mirror_value(Some("Project default (auto)")).unwrap(), None);
        assert_eq!(mirror_value(Some("OFF")).unwrap(), Some("off".into()));
    }

    #[test]
    fn natural_order() {
        let mut v = vec!["13", "2", "A10", "A2", "1"];
        v.sort_by_key(|s| natural_key(s));
        assert_eq!(v, ["1", "2", "13", "A2", "A10"]);
    }
}
