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
//!   under a head (`table`): one row per pole, NO or NC, power or auxiliary, its labels and its
//!   place, with the poles the part offers that nobody drew (**spare**) and those drawn beyond the
//!   offer (**over**).
//! - What a part offers is its typed contacts ([`crate::traits::Contacts`], design 072 §2.2; typed
//!   through `setprop`'s [`CONTACTS_KEY`] in the grammar of [`parse_set`]); where its table goes is
//!   a switching device's `contact_table` (typed through [`MIRROR_KEY`], [`parse_mirror`]), else
//!   the project's `contact_mirror` (FP3), else `auto`.
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
//! | `CreateWire`, `DeleteWire` (the commit's wire rewrite included) | the components at the ends |
//! | `AddTerminal` / `RemoveTerminal` | that component (its labels) |
//! | `CreateDevice` / `DeleteDevice` | that component |
//! | `ModifyDevice` changing its contacts or contact table | that component |
//! | `SetProperty` page number or sheet | the devices with a member on that page |
//! | page order (folder property, page and folder ops) | the devices with a member on a page whose book position changed |
//! | `EmbedDefinition` / `RemoveDefinition` | the components using it |
//! | `SetProjectSettings` changing `reference_style`, `sheet`, `length_unit` or `contact_mirror` | none derived: every page with a device member is redrawn or marked stale (the one wide case) |
//! | every other op | none |
//!
//! Cable references: `CreateCable`, `DeleteCable` and `ModifyCable` now scope to their cable ([`crate::references::cables_touched`]); everything else is as before.
//!
//! This module records nothing and has no UI: commands commit through their JSON forms.

use std::collections::{BTreeMap, BTreeSet};

use crate::CmdError;
use crate::traits::DeviceKind;
use ecad_model::device::{ContactEntry, ContactLabel, ContactPoles, ContactSet, ContactTable};
use ecad_model::{
    CableId, ContactMirror, Device as StoredDevice, DeviceId, DeviceType, Document, Form, LengthUnit, ObjectRef, Op, PageId, Pt, Representation,
    RepresentationId, Subject,
};
use serde_json::{Value, json};

use crate::symbols::{Kind, Starter, StarterKey as K};
use crate::text_style::fonts::TextMetrics;

/// The text style of a contact's or a connector member's reference (brief 066d).
const REF: crate::text_style::TextKind = crate::text_style::TextKind::Reference;

/// The height of reference text in drawing units: the `Reference` text style's.
fn ref_height(doc: &Document) -> f64 {
    crate::text_style::text_style(doc, REF).height
}

/// The `setprop` key of the contacts a part offers (FP1): `3P+1NO+1NC`. A typed field, never in a
/// property bag.
pub const CONTACTS_KEY: &str = "contacts";
/// The `setprop` key of where a switching device's contact table goes (FP2). A typed field.
pub const MIRROR_KEY: &str = "mirror";
pub use ecad_model::device::PoleKind;
pub use ecad_model::device::contacts::{MAX_LABEL_CHARS, MAX_OFFSET_MM, MAX_POLES};
/// `ecad.xref`: ACI 7 (ByLayer on `ECAD-TAGS`). Its text is reference text: the `Reference` text
/// style (brief 066d), never a size of its own.
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

// ---------------------------------------------------------------- offer grammar (FP1)

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

/// Read a typed offer (design 025 FP1): entries separated by `+` or `,`, case-insensitive, spaces
/// ignored. `3P` (power NO poles), `1PNC` (power NC poles), `1NO` / `2NC` (auxiliary contacts), or
/// one contact with explicit labels: `13/14 NO`, `21/22 NC`, `L1/T1 P`, `1/2 PNC`.
///
/// # Errors
/// Unreadable text, no poles, or more than [`MAX_POLES`].
pub fn parse_set(text: &str) -> Result<ContactSet, CmdError> {
    let bad = || CmdError::refused(not_a_contact_list(text));
    let too_many = || CmdError::refused(format!("A part offers at most {MAX_POLES} contacts."));
    let mut out: Vec<ContactEntry> = Vec::new();
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
        let poles = if let Some((a, b)) = body.split_once('/') {
            let label = |l: &str| {
                let ok = !l.is_empty() && l.chars().all(|c| c.is_ascii_alphanumeric() || c == '.');
                if ok { ContactLabel::new(l).ok() } else { None }
            };
            ContactPoles::Labels([label(a).ok_or_else(bad)?, label(b).ok_or_else(bad)?])
        } else {
            let n: usize = body.parse().map_err(|_| bad())?;
            if n == 0 {
                return Err(bad());
            }
            ContactPoles::Count(u8::try_from(n).ok().and_then(std::num::NonZeroU8::new).ok_or_else(too_many)?)
        };
        out.push(ContactEntry { poles, kind, power });
        if out.iter().map(ContactEntry::len).sum::<usize>() > MAX_POLES {
            return Err(too_many());
        }
    }
    if out.is_empty() {
        return Err(bad());
    }
    Ok(ContactSet::new(out))
}

/// The poles `set` offers, numbered (design 025 FP1): power NO poles `1/2`, `3/4`…, power NC poles
/// continuing the power numbering, auxiliary contacts with the IEC sequence digit in written order
/// (`13/14` NO, `21/22` NC). A counted pole skips labels already offered.
pub fn poles_offered(set: &ContactSet) -> Vec<OfferedPole> {
    let mut out: Vec<OfferedPole> = Vec::new();
    let (mut power_n, mut seq) = (0u32, 0u32);
    for e in set.entries() {
        let (kind, power) = (e.kind, e.power);
        match &e.poles {
            ContactPoles::Labels([a, b]) => out.push(OfferedPole { labels: [a.to_string(), b.to_string()], kind, power }),
            ContactPoles::Count(n) => {
                for _ in 0..n.get() {
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
        }
    }
    out
}

/// Read an offer and number its poles ([`parse_set`], [`poles_offered`]).
///
/// # Errors
/// As [`parse_set`].
pub fn parse_offer(text: &str) -> Result<Vec<OfferedPole>, CmdError> {
    parse_set(text).map(|s| poles_offered(&s))
}

/// The contacts component `c` offers: its typed set ([`crate::traits::Contacts`]); `None` when its
/// group has no contacts.
pub fn contacts_of(doc: &Document, c: DeviceId) -> Option<&ContactSet> {
    doc.devices.get(&c)?.device_type.as_contacts().map(|x| x.contacts())
}

/// What component `c` offers, as written and numbered, when it offers any.
pub fn offer_of(doc: &Document, c: DeviceId) -> Option<(String, Vec<OfferedPole>)> {
    contacts_of(doc, c).filter(|s| !s.is_empty()).map(|s| (s.to_string(), poles_offered(s)))
}

/// Device `d` (named `who` in refusals) with its contacts set to the typed `text` (`None` or blank
/// clears them).
///
/// # Errors
/// A device whose group has no contacts; as [`parse_set`].
pub fn with_contacts(d: &StoredDevice, who: &str, text: Option<&str>) -> Result<StoredDevice, CmdError> {
    let set = match text.map(str::trim).filter(|t| !t.is_empty()) {
        None => ContactSet::default(),
        Some(t) => parse_set(t)?,
    };
    let mut out = d.clone();
    match out.device_type.as_contacts_mut() {
        Some(x) => *x.contacts_mut() = set,
        None if set.is_empty() => {}
        None => {
            return Err(CmdError::refused(format!("{who} is a {}: it has no contacts.", d.device_type.group().label().to_lowercase())));
        }
    }
    Ok(out)
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

    /// The placement a stored [`ContactTable`] says.
    pub fn of_table(t: ContactTable) -> Placement {
        match t {
            ContactTable::Auto => Placement::Auto,
            ContactTable::Below => Placement::Below,
            ContactTable::Right => Placement::Right,
            ContactTable::Off => Placement::Off,
            ContactTable::At { dx, dy } => Placement::At(Pt::new(dx, dy)),
        }
    }

    /// Its stored form.
    pub fn table(self) -> ContactTable {
        match self {
            Placement::Auto => ContactTable::Auto,
            Placement::Below => ContactTable::Below,
            Placement::Right => ContactTable::Right,
            Placement::Off => ContactTable::Off,
            Placement::At(p) => ContactTable::At { dx: p.x, dy: p.y },
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
pub fn parse_mirror(text: &str) -> Result<Placement, CmdError> {
    let t = text.trim();
    if let Some(m) = ContactMirror::parse(t) {
        return Ok(Placement::of_default(m));
    }
    let lower = t.to_ascii_lowercase();
    let rest = lower.strip_prefix("at").ok_or_else(|| CmdError::refused(not_a_placement(t)))?.trim();
    let (a, b) = rest.split_once(',').ok_or_else(|| CmdError::refused(not_a_placement(t)))?;
    let (x, y) = (
        a.trim().parse::<f64>().map_err(|_| CmdError::refused(not_a_placement(t)))?,
        b.trim().parse::<f64>().map_err(|_| CmdError::refused(not_a_placement(t)))?,
    );
    if !(x.is_finite() && y.is_finite() && x.abs() <= MAX_OFFSET_MM && y.abs() <= MAX_OFFSET_MM) {
        return Err(CmdError::refused(not_a_placement(t)));
    }
    Ok(Placement::At(Pt::new(x, y)))
}

/// The table placement a typed `mirror` value says; `None` for empty, `default` or `project
/// default` (the project's default).
///
/// # Errors
/// As [`parse_mirror`].
pub fn table_of_typed(text: Option<&str>) -> Result<Option<ContactTable>, CmdError> {
    let Some(t) = text.map(str::trim).filter(|t| !t.is_empty()) else { return Ok(None) };
    if t.eq_ignore_ascii_case("default") || t.to_ascii_lowercase().starts_with("project default") {
        return Ok(None);
    }
    Ok(Some(parse_mirror(t)?.table()))
}

/// Device `d` (named `who` in refusals) with its contact table placed at `table` (`None`: the
/// project's default).
///
/// # Errors
/// Not a switching device (only a switching device has a contact table of its own).
pub fn with_table(d: &StoredDevice, who: &str, table: Option<ContactTable>) -> Result<StoredDevice, CmdError> {
    let mut out = d.clone();
    match &mut out.device_type {
        DeviceType::Switching(s) => s.contact_table = table,
        _ if table.is_none() => {}
        _ => return Err(CmdError::refused(format!("{who} has no contact table: only a switching device has one."))),
    }
    Ok(out)
}

/// Where `c`'s table goes and whether the device says so itself (else the project default).
pub fn placement_of(doc: &Document, c: DeviceId) -> (Placement, bool) {
    match doc.devices.get(&c).and_then(|x| x.device_type.as_contacts()).and_then(|x| x.contact_table()) {
        Some(t) => (Placement::of_table(t), true),
        None => (Placement::of_default(doc.settings.contact_mirror), false),
    }
}

// ---------------------------------------------------------------- classes (design 025 §3)

/// The objects whose representations may point at each other.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Group {
    Cable(CableId),
    Device(DeviceId),
    /// Brief 049 (FP4): the continuation arrows of one name, keyed by the lowest component ID
    /// among them. Its edges are implicit: every member shows every other.
    Continuation(DeviceId),
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
    /// Brief 044 (design 044 §7, design 052 §3.4): a connector half's whole-half symbol. A
    /// pin-only half has none.
    Half(crate::symbols::Side),
    /// Brief 044: any other connector symbol, by the side it draws.
    Pin(crate::symbols::Side),
    /// Brief 049: a continuation arrow's symbol.
    Arrow,
    /// Anything else (no edges).
    Other,
}

/// The starter symbol a representation shows: its definition's (a drawn form has none).
pub fn starter_of(doc: &Document, r: &Representation) -> Option<&'static Starter> {
    crate::devices::rep_starter(doc, r)
}

/// Whether a starter symbol is a head.
pub fn is_head(s: &Starter) -> bool {
    matches!(s.kind, Kind::Fixed { exclusive: true })
}

/// The poles a starter symbol carries (code, not format: design 025 §2).
pub fn poles_of(s: &Starter) -> Vec<(PoleKind, bool)> {
    match (s.key, s.kind) {
        (K::Power3, _) => vec![(PoleKind::No, true); 3],
        // Pack re-run, bug 11: the 4-pole set is a contactor's power contacts too.
        (K::Power4, _) => vec![(PoleKind::No, true); 4],
        (_, Kind::Contact { make }) => vec![(if make { PoleKind::No } else { PoleKind::Nc }, false)],
        _ => Vec::new(),
    }
}

/// The class of a component representation by its symbol alone (a second head is made `Other`
/// by [`device`]): a head by its type ([`crate::reps::rep_type`]), a child when its symbol carries
/// poles.
fn raw_class(doc: &Document, r: &Representation) -> Class {
    if crate::reps::rep_type(doc, r) == ecad_model::PartRole::Head {
        return Class::Head;
    }
    match starter_of(doc, r) {
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
    pub component: DeviceId,
    /// Its head ([`crate::reps::head`]: the first head symbol in book order, then top-left; never
    /// by ID or placement order, design 052 §3.4).
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
pub fn terminal_points(doc: &Document, c: DeviceId, r: &Representation) -> Vec<(ecad_model::TerminalId, Pt)> {
    let shown = crate::devices::shown_terminals(doc, c, r);
    match (r.form, crate::devices::rep_definition(doc, r)) {
        (Form::Symbol, Some((_, pl))) if pl.is_valid() => shown.into_iter().map(|(t, p)| (t, pl.apply(p))).filter(|(_, p)| p.is_finite()).collect(),
        (Form::Symbol, _) => Vec::new(),
        (Form::Drawn, _) => shown,
    }
}

/// Device `c` as the reference rule sees it: O(its representations).
pub fn device(doc: &Document, c: DeviceId) -> Device {
    // Brief 054 (design 054 §3.4): the first head on a page with checks and references on.
    let head =
        crate::reps::heads(doc, c).into_iter().find(|h| doc.representations.get(h).is_some_and(|r| ecad_model::sheet::page_checks(doc, r.page)));
    let mut children = Vec::new();
    let mut poles = Vec::new();
    for (rid, r) in doc.representations_of(Subject::Device(c)) {
        // Brief 054 (design 054 §3.4): a symbol on a Plain page has no references.
        if !ecad_model::sheet::page_checks(doc, r.page) {
            continue;
        }
        if raw_class(doc, r) != Class::Child {
            continue;
        }
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
    Device { component: c, head, children, poles }
}

/// The class of member `rid` of its group, as the index keeps it.
pub fn class_in(doc: &Document, rid: RepresentationId) -> Option<(Group, Class)> {
    let r = doc.representations.get(&rid)?;
    // Brief 049: a continuation arrow's symbol.
    if let Some(g) = crate::continuations::group_of_rep(doc, r) {
        return Some((g, Class::Arrow));
    }
    // Brief 044: a connector's members.
    if let Subject::Device(c) = r.subject
        && crate::connectors::is_connector(doc, c)
    {
        return crate::connectors::ref_classes(doc, c).into_iter().find(|(x, _)| *x == rid).map(|(_, k)| (Group::Device(c), k));
    }
    // Brief 054 (design 054 §3.4): a symbol on a Plain page is in no device's group.
    if matches!(r.subject, Subject::Device(_)) && !ecad_model::sheet::page_checks(doc, r.page) {
        return None;
    }
    match r.subject {
        Subject::Cable(k) if crate::references::is_reference(r) => Some((Group::Cable(k), Class::End)),
        Subject::Device(c) => match raw_class(doc, r) {
            Class::Head => Some((Group::Device(c), if crate::reps::head(doc, c) == Some(rid) { Class::Head } else { Class::Other })),
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
        let style = self.doc.settings.reference_style();
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

    /// Brief 049 (design 049 §2.2): where `at` on `page` sorts, by book position, then column,
    /// then row, and its text; `None` (shown `?`, sorted last) when it cannot be located. The book
    /// is built at most once per `Places`.
    pub fn sorted(&mut self, page: PageId, at: Option<Pt>) -> Option<((usize, u32, u32), String)> {
        let at = at?;
        let style = self.doc.settings.reference_style();
        let doc = self.doc;
        let position = self.book.get_or_insert_with(|| ecad_model::book::Book::of(doc)).position(page)?;
        let (grid, number, located) = self.page(page);
        if !*located {
            return None;
        }
        let p = ecad_model::derive::place_on(grid, number, at)?;
        Some(((position, p.column, p.row), p.text(style)))
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
    format!("{} {}", if power { "power" } else { "auxiliary" }, kind.upper())
}

/// The labels the next contact of kind `make` joining `c` takes from its offer (design 025 §4.3,
/// owner answer Q4): the first offered auxiliary pole of that kind whose labels are free. `None`
/// when `c` has no readable offer or no such pole is free.
pub fn offer_labels(doc: &Document, c: DeviceId, make: bool, used: &BTreeSet<String>) -> Option<Vec<String>> {
    let (_, offer) = offer_of(doc, c)?;
    let kind = if make { PoleKind::No } else { PoleKind::Nc };
    offer.iter().find(|o| !o.power && o.kind == kind && !used.contains(&o.labels[0]) && !used.contains(&o.labels[1])).map(|o| o.labels.to_vec())
}

/// The echo after a join that went beyond the offer (design 025 §4.3), if it did.
pub fn over_line(doc: &Document, c: DeviceId, labels: &[String], make: bool) -> Option<String> {
    let (text, offer) = offer_of(doc, c)?;
    let dev = device(doc, c);
    let kind = if make { PoleKind::No } else { PoleKind::Nc };
    let (drawn, offered) = counts(&dev, &offer).get(&(false, kind)).copied().unwrap_or_default();
    (drawn > offered).then(|| {
        format!(
            "Added {} contact {} to {}; its part offers {offered} {} ({text}), now {drawn} drawn (see Checks).",
            kind.upper(),
            labels.join("/"),
            crate::devices::tag_words(doc, c),
            kind_words(false, kind),
        )
    })
}

// ---------------------------------------------------------------- drawing (design 025 §4)

/// Where a reference text sits across its point (the drawing maps it to CADCraft's alignment,
/// `draw::xref`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum HAlign {
    #[default]
    Left,
    Center,
    Right,
}

/// Where a reference text sits up and down its point (as [`HAlign`]).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum VAlign {
    #[default]
    Baseline,
    Bottom,
    Middle,
    Top,
}

/// A drawn part of a reference: a line or a text, in drawing units.
#[derive(Debug, Clone, PartialEq)]
pub enum Shape {
    Line(Pt, Pt),
    /// `rotation` in radians (brief 049, FP3): 0 for every text but a continuation's reference.
    /// `kind` is the text style it is drawn in (brief 066d): its height is that style's, and the
    /// engine applies the style's font, weight and colour.
    Text {
        kind: crate::text_style::TextKind,
        at: Pt,
        value: String,
        height: f64,
        halign: HAlign,
        valign: VAlign,
        rotation: f64,
    },
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
        shape: Shape::Text { kind: REF, at, value, height: ref_height(doc), halign, valign: VAlign::Top, rotation: 0.0 },
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

/// The box of a table at `top` with `rows` (design 033 §4.1): from −10 mm to 12 mm plus the widest
/// place text across, 3 mm plus the rows and the power/auxiliary gap down. (lo, hi), drawing units.
pub fn table_box(doc: &Document, top: Pt, rows: &[Row], m: &dyn TextMetrics) -> (Pt, Pt) {
    let k = unit(doc);
    // The widest place text in the font and size it is drawn with (drawing units).
    let widest = rows.iter().map(|r| crate::text_style::fonts::text_width(doc, crate::text_style::TextKind::Table, &r.place, m)).fold(0.0, f64::max);
    let gap = rows.windows(2).any(|w| w.first().is_some_and(|a| a.power) && w.get(1).is_some_and(|b| !b.power));
    let height = 3.0 + ROW_PITCH_MM * rows.len() as f64 + if gap { GROUP_GAP_MM } else { 0.0 };
    (Pt::new(top.x - 10.0 * k, top.y - height * k), Pt::new(top.x + 12.0 * k + widest, top.y))
}

/// The page's free area for tables (design 033 §4.1): the frame's drawing area above the title
/// block, the registry's `print.contact_table_clearance` clear of its top line. (lo, hi),
/// drawing units.
pub fn free_area(doc: &Document, page: PageId) -> (Pt, Pt) {
    let g = ecad_model::sheet::Grid::of(doc, page);
    let k = unit(doc);
    let gap = crate::defaults::resolve(doc, &crate::defaults::print::CONTACT_TABLE_CLEARANCE, crate::defaults::Scope::project()).length(doc);
    let lo = Pt::new(g.frame_left(), g.frame_bottom() + ecad_model::sheet::keep_out(doc, page) * k + gap);
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
    let line = g.frame_bottom() + ecad_model::sheet::keep_out(doc, page) * unit(doc);
    let (left, right) = (g.frame_left(), g.frame_left() + g.area_width());
    b.0.y < line && b.1.y > g.frame_bottom() && b.1.x > left && b.0.x < right
}

/// Where an automatic table goes (design 033 §4.1), from where `placement` puts it (`top`): there
/// when its box lies in the free area; else right of the head (`auto`, `below`) when that fits;
/// else lifted onto the free area's bottom and pulled left inside the frame. A table taller than
/// the free area starts at its top. A table the user placed (`at`) is never moved.
pub fn settle(
    doc: &Document, c: DeviceId, head: RepresentationId, placement: Placement, top: Pt, rows: &[Row], m: &dyn TextMetrics,
) -> (Pt, TableShown) {
    if matches!(placement, Placement::At(_) | Placement::Off) {
        return (top, TableShown::AsPlaced);
    }
    let Some(page) = doc.representations.get(&head).map(|r| r.page) else { return (top, TableShown::AsPlaced) };
    let free = free_area(doc, page);
    if inside(table_box(doc, top, rows, m), free) {
        return (top, TableShown::AsPlaced);
    }
    if matches!(placement, Placement::Auto | Placement::Below)
        && let Some(right) = table_top(doc, c, head, Placement::Right)
        && inside(table_box(doc, right, rows, m), free)
    {
        return (right, TableShown::Right);
    }
    let (lo, hi) = table_box(doc, top, rows, m);
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

/// Where the table of head `head` of device `c` goes, for `placement`; `None` for `off` or a
/// head that cannot be drawn.
pub fn table_top(doc: &Document, c: DeviceId, head: RepresentationId, placement: Placement) -> Option<Pt> {
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
                doc.routes_at(t).filter_map(|x| doc.routes.get(&x)).filter(|x| x.page == r.page).find_map(|x| run_down(&x.points, p))
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
pub fn table_measured(doc: &Document, places: &mut Places<'_>, dev: &Device, m: &dyn TextMetrics) -> Option<Table> {
    let head = dev.head?;
    let offer = offer_of(doc, dev.component);
    if dev.children.is_empty() && offer.is_none() {
        return None;
    }
    let (placement, _) = placement_of(doc, dev.component);
    let top = table_top(doc, dev.component, head, placement)?;
    let page = doc.representations.get(&head)?.page;
    let rows = rows(places, dev, offer.as_ref().map(|(_, o)| o.as_slice()));
    let (top, shown) = settle(doc, dev.component, head, placement, top, &rows, m);
    let parts = table_parts(doc, head, top, &rows);
    let bottom = table_box(doc, top, &rows, m).0.y;
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
    // Design 033 §1.2: contact-table rows are table text.
    let h = crate::text_style::text_style(doc, crate::text_style::TextKind::Table).height;
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
        let text = |x: f64, value: &str, halign: HAlign| Shape::Text {
            kind: crate::text_style::TextKind::Table,
            at: at(x, y),
            value: value.to_string(),
            height: h,
            halign,
            valign: VAlign::Middle,
            rotation: 0.0,
        };
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

/// Most places one connector reference lists before `+n` (design 044 §7; hover lists all).
pub const MAX_CONNECTOR_PLACES: usize = 3;

/// The reference texts of connector `c` (design 044 §7): under each member with targets, their
/// places (`/102.C4`), at most [`MAX_CONNECTOR_PLACES`] then `+n`. O(the connector's symbols).
pub fn connector_parts(doc: &Document, places: &mut Places<'_>, c: DeviceId) -> Vec<(PageId, Part)> {
    let list = crate::connectors::ref_classes(doc, c);
    let edges = crate::connectors::ref_edges(doc, c, &list);
    let mut targets: BTreeMap<RepresentationId, Vec<RepresentationId>> = BTreeMap::new();
    for (s, t) in edges {
        targets.entry(s).or_default().push(t);
    }
    let k = unit(doc);
    let mut out = Vec::new();
    for (rid, ts) in targets {
        let Some(r) = doc.representations.get(&rid) else { continue };
        let mut texts: Vec<String> = ts.iter().filter_map(|t| doc.representations.get(t)).map(|t| places.text(t.page, anchor(doc, t))).collect();
        texts.sort();
        texts.dedup();
        let more = texts.len().saturating_sub(MAX_CONNECTOR_PLACES);
        texts.truncate(MAX_CONNECTOR_PLACES);
        let mut value = texts.join(" ");
        if more > 0 {
            value.push_str(&format!(" +{more}"));
        }
        let Some((lo, hi)) = (match (r.form, crate::devices::rep_definition(doc, r)) {
            (Form::Symbol, Some((def, pl))) => crate::devices::placed_box(def, &pl),
            _ => None,
        }) else {
            continue;
        };
        let at = Pt::new((lo.x + hi.x) / 2.0, lo.y - 1.5 * k);
        if !at.is_finite() {
            continue;
        }
        out.push((
            r.page,
            Part {
                rep: rid,
                index: 0,
                shape: Shape::Text { kind: REF, at, value, height: ref_height(doc), halign: HAlign::Center, valign: VAlign::Top, rotation: 0.0 },
                aci: None,
                row: None,
                bar: false,
            },
        ));
    }
    out
}

/// Every reference part drawn on `page`: the texts under its contacts and the tables under its
/// heads. O(the page's component representations and their groups).
pub fn parts_on_page_measured(doc: &Document, page: PageId, m: &dyn TextMetrics) -> Vec<Part> {
    let mut by_component: BTreeMap<DeviceId, Vec<RepresentationId>> = BTreeMap::new();
    for (rid, r) in doc.representations_on(page) {
        if let Subject::Device(c) = r.subject {
            by_component.entry(c).or_default().push(rid);
        }
    }
    let mut places = Places::new(doc);
    let mut out = Vec::new();
    let mut tables: Vec<Table> = Vec::new();
    for (c, here) in by_component {
        // Brief 044 (design 044 §7): a connector's half and pin references.
        if crate::connectors::is_connector(doc, c) {
            out.extend(connector_parts(doc, &mut places, c).into_iter().filter(|(_, p)| here.contains(&p.rep)).map(|(_, p)| p));
            continue;
        }
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
            && let Some(t) = table_measured(doc, &mut places, &dev, m)
        {
            tables.push(t);
        }
    }
    // Pack re-run, bug 4: tables of heads drawn close together clear each other. This depends
    // only on the page's tables as each device derives it, and a commit that changes one of them
    // redraws the page ([`Watch`]), so the update map is unchanged.
    spread(doc, &mut tables, m);
    out.extend(tables.into_iter().flat_map(|t| t.parts));
    // Brief 049 (design 049 §4): continuation arrows' other ends, brackets and captions.
    out.extend(crate::continuations::parts_on_page_measured(doc, &mut places, page, m));
    out
}

/// The most steps one table takes to clear the tables before it (hostile pages stay bounded).
const MAX_SPREAD_STEPS: usize = 64;
/// The gap kept between two tables spread apart, millimetres.
const SPREAD_GAP_MM: f64 = 2.0;

/// Whether boxes `a` and `b` overlap (touching is not overlapping).
fn boxes_overlap(a: (Pt, Pt), b: (Pt, Pt)) -> bool {
    a.0.x < b.1.x - 1e-9 && b.0.x < a.1.x - 1e-9 && a.0.y < b.1.y - 1e-9 && b.0.y < a.1.y - 1e-9
}

/// Spread one page's `tables` so that no two overlap (pack re-run, bug 4). A table the user placed
/// (`at`) never moves. The others are taken left to right (then top down): each keeps its place
/// when it clears the tables before it, else moves right past them; when that leaves the free
/// area, it goes under them instead, if that stays in the free area, else stays. Deterministic,
/// and each table moves only for tables before it.
pub fn spread(doc: &Document, tables: &mut [Table], m: &dyn TextMetrics) {
    let Some(page) = tables.first().map(|t| t.page) else { return };
    let free = free_area(doc, page);
    let gap = SPREAD_GAP_MM * unit(doc);
    let fixed = |t: &Table| matches!(t.placement, Placement::At(_) | Placement::Off);
    let mut taken: Vec<(Pt, Pt)> = tables.iter().filter(|t| fixed(t)).map(|t| table_box(doc, t.top, &t.rows, m)).collect();
    let mut order: Vec<usize> = (0..tables.len()).filter(|i| tables.get(*i).is_some_and(|t| !fixed(t))).collect();
    order.sort_by(|a, b| match (tables.get(*a), tables.get(*b)) {
        (Some(x), Some(y)) => x.top.x.total_cmp(&y.top.x).then(y.top.y.total_cmp(&x.top.y)).then(x.head.cmp(&y.head)),
        _ => a.cmp(b),
    });
    for i in order {
        let Some(t) = tables.get_mut(i) else { continue };
        let first = table_box(doc, t.top, &t.rows, m);
        let left = t.top.x - first.0.x;
        let height = t.top.y - first.0.y;
        // Right past the tables it meets.
        let mut right = t.top;
        for _ in 0..MAX_SPREAD_STEPS {
            let b = table_box(doc, right, &t.rows, m);
            let Some(edge) = taken.iter().filter(|o| boxes_overlap(b, **o)).map(|o| o.1.x).reduce(f64::max) else { break };
            right.x = edge + gap + left;
        }
        let mut top = right;
        if !inside(table_box(doc, right, &t.rows, m), free) && inside(first, free) {
            // Under the tables it meets, from its own place; else where it was (never off the
            // free area it started in).
            let mut down = t.top;
            for _ in 0..MAX_SPREAD_STEPS {
                let b = table_box(doc, down, &t.rows, m);
                let Some(edge) = taken.iter().filter(|o| boxes_overlap(b, **o)).map(|o| o.0.y).reduce(f64::min) else { break };
                down.y = edge - gap;
            }
            top = if inside(table_box(doc, down, &t.rows, m), free) { down } else { t.top };
        }
        let b = table_box(doc, top, &t.rows, m);
        if top != t.top && top.is_finite() && taken.iter().all(|o| !boxes_overlap(b, *o)) {
            t.top = top;
            t.parts = table_parts(doc, t.head, top, &t.rows);
            t.bottom = top.y - height;
        }
        taken.push(table_box(doc, t.top, &t.rows, m));
    }
}

/// Device `dev`'s table where its page draws it: `table`, then moved by [`spread`] with the other
/// tables on its page (review of #92, 2). O(devices on that page); for replies, not per commit.
pub fn drawn_table(doc: &Document, places: &mut Places<'_>, dev: &Device, m: &dyn TextMetrics) -> Option<Table> {
    let own = table_measured(doc, places, dev, m)?;
    let mut others: BTreeSet<DeviceId> = BTreeSet::new();
    for (_, r) in doc.representations_on(own.page) {
        if let Subject::Device(c) = r.subject
            && c != dev.component
        {
            others.insert(c);
        }
    }
    let mut tables: Vec<Table> = others
        .into_iter()
        .filter(|c| !crate::connectors::is_connector(doc, *c))
        .map(|c| device(doc, c))
        .filter(|d| d.head.is_some_and(|h| doc.representations.get(&h).is_some_and(|r| r.page == own.page)))
        .filter_map(|d| table_measured(doc, places, &d, m))
        .collect();
    let head = own.head;
    tables.push(own);
    spread(doc, &mut tables, m);
    tables.into_iter().find(|t| t.head == head)
}

/// What device `c` shows on each page (its contacts' texts and its table), for comparing before
/// and after a commit: page → parts.
fn shown(doc: &Document, places: &mut Places<'_>, c: DeviceId, m: &dyn TextMetrics) -> BTreeMap<PageId, Vec<Part>> {
    let mut out: BTreeMap<PageId, Vec<Part>> = BTreeMap::new();
    // Brief 044: a connector's references.
    if crate::connectors::is_connector(doc, c) {
        for (page, p) in connector_parts(doc, places, c) {
            out.entry(page).or_default().push(p);
        }
        return out;
    }
    let dev = device(doc, c);
    if dev.head.is_none() {
        return out;
    }
    for rid in &dev.children {
        if let (Some(p), Some(r)) = (contact_text(doc, places, &dev, *rid), doc.representations.get(rid)) {
            out.entry(r.page).or_default().push(p);
        }
    }
    if let Some(t) = table_measured(doc, places, &dev, m) {
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
                Subject::Device(c) => match raw_class(doc, r) {
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
        // Brief 044: connectors' members and edges (design 044 §7).
        for c in doc.devices.keys() {
            if crate::connectors::is_connector(doc, *c) {
                idx.insert_connector(doc, *c);
            }
        }
        // Brief 049 (FP4): one group per continuation name, one pass over the name index.
        let names: BTreeSet<&str> = doc.continuation_names().map(|(k, _)| k).collect();
        for k in names {
            if let Some((g, list)) = crate::continuations::group_named(doc, k) {
                idx.insert_group(g, list.into_iter().map(|r| (r, Class::Arrow)).collect());
            }
        }
        idx
    }

    /// Add connector `c`'s group (design 044 §7): its halves and pins, and their edges.
    fn insert_connector(&mut self, doc: &Document, c: DeviceId) {
        let list = crate::connectors::ref_classes(doc, c);
        if list.is_empty() {
            return;
        }
        let edges = crate::connectors::ref_edges(doc, c, &list);
        let g = Group::Device(c);
        for (r, k) in &list {
            self.member.insert(*r, (g, *k));
        }
        self.members.insert(g, list.iter().map(|(r, _)| *r).collect());
        for (s, t) in edges {
            self.targets.entry(s).or_default().insert(t);
            self.sources.entry(t).or_default().insert(s);
        }
    }

    /// Take group `g` out: its members, classes and edges. O(group).
    fn remove(&mut self, g: Group) {
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
    }

    /// Put group `g` back from `doc` (after [`RefIndex::remove`]): its members, classes and edges.
    /// O(group).
    fn make(&mut self, doc: &Document, g: Group) {
        // Brief 049: a continuation group is made from its name; its key may have changed (the
        // lowest arrow deleted or renamed), so it goes in under the key `doc` gives it.
        if let Group::Continuation(c) = g {
            let Some(name) = crate::continuations::name_of(doc, c) else { return };
            if let Some((key, list)) = crate::continuations::group_named(doc, &name) {
                self.remove(key);
                self.insert_group(key, list.into_iter().map(|r| (r, Class::Arrow)).collect());
            }
            return;
        }
        if let Group::Device(c) = g
            && crate::connectors::is_connector(doc, c)
        {
            self.insert_connector(doc, c);
            return;
        }
        let list: Vec<(RepresentationId, Class)> = match g {
            Group::Cable(k) => doc
                .representations_of(Subject::Cable(k))
                .filter(|(_, r)| crate::references::is_reference(r))
                .map(|(id, _)| (id, Class::End))
                .collect(),
            Group::Device(c) => {
                let mut head_seen = false;
                doc.representations_of(Subject::Device(c))
                    // Brief 054 (design 054 §3.4): no references to or from a Plain page.
                    .filter(|(_, r)| ecad_model::sheet::page_checks(doc, r.page))
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
            Group::Continuation(_) => Vec::new(),
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
            // Brief 049 (FP4): implicit, never stored ([`RefIndex::targets`]).
            Group::Continuation(_) => {}
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

    /// Make the groups `groups` again from `doc` (after a commit): all of them out first, then
    /// back, so a continuation group whose key changed is never removed after it was remade.
    pub fn refresh(&mut self, doc: &Document, groups: &BTreeSet<Group>) {
        for g in groups {
            self.remove(*g);
        }
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
            Subject::Device(c) => {
                out.insert(Group::Device(c));
                // Brief 049: an arrow's name group.
                out.extend(crate::continuations::name_of(doc, c).and_then(|n| crate::continuations::group_key(doc, &n)));
            }
            Subject::Wire(_) | Subject::Route(_) => {}
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
                Op::CreateDevice { id, .. } | Op::DeleteDevice { id, .. } | Op::SetDeviceDefinition { id, .. } => {
                    out.insert(Group::Device(*id));
                }
                Op::CreateCable { id, .. } | Op::DeleteCable { id, .. } | Op::ModifyCable { id, .. } => {
                    out.insert(Group::Cable(*id));
                }
                Op::EmbedDefinition { id, .. } | Op::RemoveDefinition { id, .. } => {
                    out.extend(doc.components_using(*id).into_iter().map(Group::Device));
                }
                _ => {}
            }
        }
        // Brief 049 (design 049 §8.1): the name groups the ops name, as `doc` keys them.
        for n in crate::continuations::names_named(doc, ops) {
            out.extend(crate::continuations::group_key(doc, &n));
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

    /// The other members of `r`'s continuation group (brief 049: its implicit edges, both ways).
    fn implicit(&self, r: RepresentationId) -> impl Iterator<Item = RepresentationId> + '_ {
        let group = match self.member.get(&r) {
            Some((g @ Group::Continuation(_), _)) => self.members.get(g),
            _ => None,
        };
        group.into_iter().flatten().copied().filter(move |x| *x != r)
    }

    /// What source `s` shows.
    pub fn targets(&self, s: RepresentationId) -> impl Iterator<Item = RepresentationId> + '_ {
        self.targets.get(&s).into_iter().flatten().copied().chain(self.implicit(s))
    }

    /// Who shows target `t`.
    pub fn sources(&self, t: RepresentationId) -> impl Iterator<Item = RepresentationId> + '_ {
        self.sources.get(&t).into_iter().flatten().copied().chain(self.implicit(t))
    }

    /// How many members group `g` has (O(1)).
    pub fn group_len(&self, g: Group) -> usize {
        self.members.get(&g).map_or(0, BTreeSet::len)
    }

    /// Every group.
    pub fn groups(&self) -> impl Iterator<Item = Group> + '_ {
        self.members.keys().copied()
    }

    /// How many members, groups and edges (a continuation group's m(m−1) implicit edges counted,
    /// not stored).
    pub fn sizes(&self) -> (usize, usize, usize) {
        let implicit: usize = self
            .members
            .iter()
            .filter(|(g, _)| matches!(g, Group::Continuation(_)))
            .map(|(_, m)| m.len().saturating_mul(m.len().saturating_sub(1)))
            .fold(0, usize::saturating_add);
        (self.member.len(), self.members.len(), self.targets.values().map(BTreeSet::len).sum::<usize>().saturating_add(implicit))
    }

    /// The pages that hold a continuation arrow (brief 049: the wide case's pages).
    pub fn arrow_pages(&self, doc: &Document) -> BTreeSet<PageId> {
        self.members
            .iter()
            .filter(|(g, _)| matches!(g, Group::Continuation(_)))
            .flat_map(|(_, m)| m.iter())
            .filter_map(|r| doc.representations.get(r).map(|x| x.page))
            .collect()
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
    pub fn devices_on(&self, doc: &Document, page: PageId) -> BTreeSet<DeviceId> {
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
    pub fn verify(&self, doc: &Document) -> Result<(), CmdError> {
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
        Err(CmdError::refused(format!("The reference index differs from a rebuild: {first}.")))
    }
}

// ---------------------------------------------------------------- per commit (design 025 §6)

/// What a commit's ops could change in the device references: the devices to compare, or every
/// page with a device member (`wide`).
#[derive(Debug, Clone, Default)]
struct Scope {
    devices: BTreeSet<DeviceId>,
    wide: bool,
    /// Brief 049 (design 049 §8.1): the continuation names whose arrows' texts and caption runs
    /// are compared, or every page with an arrow (`arrows_wide`).
    names: BTreeSet<String>,
    arrows_wide: bool,
}

impl Scope {
    /// Add what `ops` name, looked up in `doc` (called on the state before and after).
    fn collect(&mut self, doc: &Document, idx: &RefIndex, ops: &[Op]) {
        let ends = |s: &mut Scope, w: ecad_model::WireId| {
            if let Some(c) = doc.wires.get(&w) {
                for t in &c.ends {
                    if let Some(x) = doc.terminals.get(t) {
                        s.devices.insert(x.component);
                    }
                }
            }
        };
        let rep = |s: &mut Scope, r: &Representation| match r.subject {
            Subject::Device(c) => {
                s.devices.insert(c);
            }
            Subject::Wire(w) => ends(s, w),
            Subject::Cable(_) | Subject::Route(_) => {}
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
                Op::CreateWire { wire, .. } | Op::DeleteWire { wire, .. } => {
                    for t in &wire.ends {
                        if let Some(x) = doc.terminals.get(t) {
                            self.devices.insert(x.component);
                        }
                    }
                }
                Op::AddTerminal { terminal, .. } | Op::RemoveTerminal { terminal, .. } => {
                    self.devices.insert(terminal.component);
                }
                Op::CreateDevice { id, .. } | Op::DeleteDevice { id, .. } | Op::SetDeviceDefinition { id, .. } => {
                    self.devices.insert(*id);
                }
                Op::ModifyDevice { id, before, after } if contacts_changed(before, after) => {
                    self.devices.insert(*id);
                }
                Op::ModifyPage { id: p, before, after } => {
                    let ch = before.change(after);
                    if ch.number || ch.sheet || ch.page_type {
                        self.devices.extend(idx.devices_on(doc, *p));
                    }
                    if ch.folder {
                        self.moved(doc, idx, ops, &mut moved);
                    }
                }
                // Brief 054 (design 054 §10.2): a page type's sheet changes its pages' grids.
                Op::SetStyle { catalogue, .. } if *catalogue == ecad_model::StyleCatalogue::Page => {
                    if let Some((pages, true)) = crate::styles::page::touched_by(doc, op) {
                        for p in pages {
                            self.devices.extend(idx.devices_on(doc, p));
                        }
                    }
                }
                Op::AddPage { .. } | Op::RemovePage { .. } | Op::AddFolder { .. } | Op::RemoveFolder { .. } | Op::ModifyFolder { .. } => {
                    self.moved(doc, idx, ops, &mut moved);
                }
                Op::EmbedDefinition { id, .. } | Op::RemoveDefinition { id, .. } => {
                    self.devices.extend(doc.components_using(*id));
                }
                // Brief 033 (design 033 §9.3): the table text's size or the font sizes every table.
                Op::SetProperty { object: ObjectRef::Project | ObjectRef::Standard, key, .. } if table_text_key(key) => {
                    self.wide = true;
                }
                // Brief 050a: a change of the text style the table text uses (its height, weight).
                Op::SetStyle { catalogue, id, .. }
                    if *catalogue == ecad_model::StyleCatalogue::Text
                        && *id == crate::text_style::style_of(doc, crate::text_style::TextKind::Table, crate::defaults::Scope::project()) =>
                {
                    self.wide = true;
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
            // Brief 055 (FP4; review of #110, 1): a frame decides its pages' grids and the free
            // area above the block, so its pages' devices, tables and arrows follow it.
            match crate::title_block::frame_reach(doc, op) {
                crate::title_block::Reach::Pages(ps) => {
                    for p in ps {
                        self.devices.extend(idx.devices_on(doc, p));
                        self.names.extend(crate::continuations::names_on(doc, idx, p));
                    }
                }
                crate::title_block::Reach::Wide => {
                    self.wide = true;
                    self.arrows_wide = true;
                }
                crate::title_block::Reach::NotFrame | crate::title_block::Reach::Unchanged => {}
            }
            // Brief 049: what continuation arrows the op could change.
            match op {
                Op::ModifyPage { id: p, before, after } => {
                    let ch = before.change(after);
                    if ch.number || ch.sheet || ch.page_type {
                        self.names.extend(crate::continuations::names_on(doc, idx, *p));
                    }
                }
                Op::SetStyle { catalogue, .. } if *catalogue == ecad_model::StyleCatalogue::Page => {
                    if let Some((pages, true)) = crate::styles::page::touched_by(doc, op) {
                        for p in pages {
                            self.names.extend(crate::continuations::names_on(doc, idx, p));
                        }
                    }
                }
                Op::SetProjectSettings { before, after }
                    if before.reference_style != after.reference_style || before.sheet != after.sheet || before.length_unit != after.length_unit =>
                {
                    self.arrows_wide = true;
                }
                Op::SetProperty { object: ObjectRef::Project | ObjectRef::Standard, key, .. } if crate::continuations::is_wide_key(key) => {
                    self.arrows_wide = true;
                }
                Op::SetStyle { catalogue, id, .. } if crate::continuations::is_wide_style(doc, *catalogue, id) => {
                    self.arrows_wide = true;
                }
                _ => {}
            }
        }
        self.names.extend(crate::continuations::names_named(doc, ops));
    }

    fn moved(&mut self, doc: &Document, idx: &RefIndex, ops: &[Op], done: &mut bool) {
        if std::mem::replace(done, true) {
            return;
        }
        for p in ecad_model::book::positions_changed(doc, ops) {
            self.devices.extend(idx.devices_on(doc, p));
            self.names.extend(crate::continuations::names_on(doc, idx, p));
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
    before: BTreeMap<DeviceId, BTreeMap<PageId, Vec<Part>>>,
    /// The pages with a device member before (only in the wide case).
    wide_pages: BTreeSet<PageId>,
    /// Brief 049: what the named continuations showed before, by page, and the pages with an
    /// arrow before (the wide case).
    before_names: BTreeMap<PageId, Vec<Part>>,
    arrow_pages: BTreeSet<PageId>,
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
pub type Shown = BTreeMap<DeviceId, BTreeMap<PageId, Vec<Part>>>;

impl Watch {
    /// Before `ops` are committed on `doc`. `reuse` holds what the last commit derived, for
    /// the same `doc` (the caller checks the state is unchanged): a device found there is not
    /// derived again.
    pub fn take(doc: &Document, idx: &RefIndex, ops: &[Op], mut reuse: Shown, m: &dyn TextMetrics) -> Watch {
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
                        let fresh = shown(doc, &mut places, *c, m);
                        if kept != fresh {
                            problems.push(format!("the texts kept for {} were stale; derived again", c.hex()));
                        }
                        (*c, fresh)
                    }
                    Some(kept) => (*c, kept),
                    None => (*c, shown(doc, &mut places, *c, m)),
                })
                .collect()
        };
        let wide_pages = if scope.wide { idx.device_pages(doc) } else { BTreeSet::new() };
        let (before_names, arrow_pages) = if scope.arrows_wide {
            (BTreeMap::new(), idx.arrow_pages(doc))
        } else {
            (crate::continuations::shown_measured(doc, &mut places, &scope.names, m), BTreeSet::new())
        };
        Watch { cable_scope, cables, scope, before, wide_pages, before_names, arrow_pages, groups: RefIndex::groups_named(doc, ops), problems }
    }

    /// After the commit: `doc` is the state after it and `ops` its ops as recorded (with any pass
    /// that extended it). Brings `idx` up to date and says what changed.
    pub fn finish(mut self, doc: &Document, idx: &mut RefIndex, ops: &[Op], m: &dyn TextMetrics) -> Outcome {
        self.groups.extend(RefIndex::groups_named(doc, ops));
        idx.refresh(doc, &self.groups);
        // Brief 054 (design 054 §3.4): a page moved to or from a type with checks off changes the
        // groups of every device on it: the index is made again and every device page redrawn.
        if crate::page_types::checks_changed(doc, ops) {
            *idx = RefIndex::build(doc);
            self.scope.wide = true;
        }
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
                let new = shown(doc, &mut places, *c, m);
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
            // Brief 049 (design 049 §8.1): the pages whose continuation texts or caption runs
            // changed, or every page with an arrow before and after.
            if self.scope.arrows_wide {
                p.extend(std::mem::take(&mut self.arrow_pages));
                p.extend(idx.arrow_pages(doc));
            } else if !self.scope.names.is_empty() {
                let mut places = Places::new(doc);
                let after = crate::continuations::shown_measured(doc, &mut places, &self.scope.names, m);
                for page in self.before_names.keys().chain(after.keys()) {
                    if self.before_names.get(page) != after.get(page) {
                        p.insert(*page);
                    }
                }
            }
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
    /// `cable`, `contact` (a contact shows its coil), `mirror` (a table row shows a contact) or
    /// `continuation` (an arrow shows another arrow of its name, brief 049).
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
            Group::Cable(_) | Group::Continuation(_) => None,
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
                    // Brief 049 (design 049 §5.5): an arrow shows each other end's place.
                    (Group::Continuation(_), _) => out.push(r("continuation", None, places.text(tr.page, anchor(doc, tr)))),
                    // Brief 044 (tester D2): a connector half shows its mate half's place, a pin its
                    // mate pin's (labelled with the pin name).
                    (Group::Device(_), Some(Class::Half(_))) => out.push(r("half", None, places.text(tr.page, anchor(doc, tr)))),
                    (Group::Device(_), Some(Class::Pin(_))) => {
                        let name = sr.connection_points.keys().find_map(|t| doc.terminals.get(t).and_then(|x| x.label.clone()));
                        out.push(r("pin", name, places.text(tr.page, anchor(doc, tr))));
                    }
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
/// Continuation rows (brief 049) come after the others, one per (arrow, other end), made only for
/// the window asked for: with `component`, that arrow's; with `page`, those of the arrows on the
/// page (a row whose source is elsewhere is listed from its source). O(arrows listed + rows
/// returned), never O(m²) for a name of m arrows.
///
/// # Errors
/// Unknown component, cable or page.
pub fn xrefs_json(
    doc: &Document, idx: &RefIndex, component: Option<DeviceId>, cable: Option<CableId>, page: Option<PageId>, limit: usize, after: usize,
) -> Result<Value, CmdError> {
    let groups: BTreeSet<Group> = match (component, cable) {
        (Some(c), _) => {
            if !doc.devices.contains_key(&c) {
                return Err(CmdError::refused(crate::msg::NO_SUCH_COMPONENT));
            }
            // Brief 049 (design 049 §5.5): an arrow lists its other ends.
            match crate::continuations::name_of(doc, c).and_then(|n| crate::continuations::group_key(doc, &n)) {
                Some(g) => BTreeSet::from([g]),
                None => BTreeSet::from([Group::Device(c)]),
            }
        }
        (None, Some(k)) => {
            if !doc.cables.contains_key(&k) {
                return Err(CmdError::refused(crate::msg::NO_SUCH_CABLE));
            }
            BTreeSet::from([Group::Cable(k)])
        }
        (None, None) => match page {
            Some(p) => {
                if !doc.pages.contains_key(&p) {
                    return Err(CmdError::refused(crate::msg::NO_SUCH_PAGE));
                }
                doc.representations_on(p).filter_map(|(r, _)| idx.member(r).map(|(g, _)| g)).collect()
            }
            None => idx.groups().collect(),
        },
    };
    // Brief 049 (review of #85, 2): continuation rows are made per source and only for the
    // window asked for, never all m(m−1) of a name; the other groups' rows as before.
    let (cont, other): (BTreeSet<Group>, BTreeSet<Group>) = groups.into_iter().partition(|g| matches!(g, Group::Continuation(_)));
    let all: Vec<Reference> =
        references(doc, idx, &other).into_iter().filter(|r| page.is_none_or(|p| r.source_page == p || r.target_page == p)).collect();
    // The arrows whose rows are listed: the given arrow's, the arrows on `page`, or every arrow.
    let sources: Vec<(RepresentationId, usize)> = match (component, page) {
        (Some(c), _) => doc.representations_of(Subject::Device(c)).map(|(r, _)| r).collect::<Vec<_>>(),
        (None, Some(p)) => doc.representations_on(p).map(|(r, _)| r).collect(),
        (None, None) => cont.iter().flat_map(|g| idx.members(*g)).collect(),
    }
    .into_iter()
    .filter_map(|r| match idx.member(r) {
        Some((g @ Group::Continuation(_), _)) if cont.contains(&g) => Some((r, idx.group_len(g).saturating_sub(1))),
        _ => None,
    })
    .collect();
    let total = sources.iter().fold(all.len(), |n, (_, k)| n.saturating_add(*k));
    let row = |r: &Reference| {
        json!({
            "source": r.source.hex(), "source_page": crate::references::page_number(doc, r.source_page),
            "target": r.target.hex(), "target_page": crate::references::page_number(doc, r.target_page),
            "kind": r.kind, "labels": r.labels, "text": r.text,
        })
    };
    let end = after.saturating_add(limit);
    let mut list: Vec<Value> = all.iter().skip(after).take(limit).map(row).collect();
    // Walk the sources, making rows only for those that overlap [after, end).
    let mut at = all.len();
    let mut places = Places::new(doc);
    let mut ends: BTreeMap<String, Vec<crate::continuations::End>> = BTreeMap::new();
    for (s, k) in &sources {
        if at >= end {
            break;
        }
        let next = at.saturating_add(*k);
        if next > after
            && let Some(sr) = doc.representations.get(s)
            && let Subject::Device(c) = sr.subject
            && let Some(name) = crate::continuations::name_of(doc, c)
        {
            let sorted = ends.entry(name.clone()).or_insert_with(|| crate::continuations::ends_named(doc, &mut places, &name));
            let skip = after.saturating_sub(at);
            let take = end.saturating_sub(at.max(after));
            for e in sorted.iter().filter(|e| e.rep != *s).skip(skip).take(take) {
                list.push(row(&Reference {
                    source: *s,
                    source_page: sr.page,
                    target: e.rep,
                    target_page: e.page,
                    kind: "continuation",
                    labels: None,
                    text: e.text.clone(),
                }));
            }
        }
        at = next;
    }
    let mut v = json!({ "references": list });
    if end < total {
        v["next"] = json!(end);
    }
    Ok(v)
}

/// `contacts {component}`'s result (design 025 §5.4).
///
/// # Errors
/// Unknown component.
pub fn contacts_json_measured(doc: &Document, c: DeviceId, m: &dyn TextMetrics) -> Result<Value, CmdError> {
    if !doc.devices.contains_key(&c) {
        return Err(CmdError::refused(crate::msg::NO_SUCH_COMPONENT));
    }
    let dev = device(doc, c);
    let offer = offer_of(doc, c);
    let stored = offer.as_ref().map(|(t, _)| t.clone());
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
            "labels": r.label_pair(), "kind": r.kind.upper(), "group": r.group_word(), "state": r.state.word(),
            "representation": r.rep.map(|x| x.hex()), "place": (r.state != State::Spare).then(|| r.place.clone()),
        })).collect::<Vec<_>>(),
        "over": over,
        "table": table_json(doc, &mut places, &dev, m),
    }))
}

/// The `contacts` reply's `table` (design 033 §4.2): `{top, bottom, placement, shown}`, or null.
fn table_json(doc: &Document, places: &mut Places<'_>, dev: &Device, m: &dyn TextMetrics) -> Value {
    let Some(t) = drawn_table(doc, places, dev, m) else { return Value::Null };
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
pub fn contact_lines(doc: &Document, c: DeviceId, rep: RepresentationId) -> Vec<String> {
    let dev = device(doc, c);
    if !dev.children.contains(&rep) {
        return Vec::new();
    }
    let noun = |doc: &Document, h: RepresentationId| {
        doc.representations.get(&h).and_then(|r| starter_of(doc, r)).map_or("Head", |s| match s.key {
            K::Coil => "Coil",
            K::Mcb1 | K::Mcb2 => "MCB",
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
pub fn head_lines(doc: &Document, c: DeviceId, rep: RepresentationId) -> Vec<String> {
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
        lines.push(format!("  {} {} {} {}", r.label_pair(), r.kind.upper(), r.group_word(), r.place));
    }
    if rows.len() > 6 {
        lines.push(format!("  +{} more", rows.len() - 6));
    }
    lines
}

/// Hover lines for row `i` of head `head`'s table.
pub fn row_lines(doc: &Document, c: DeviceId, i: usize, m: &dyn TextMetrics) -> Option<Vec<String>> {
    let dev = device(doc, c);
    let mut places = Places::new(doc);
    let t = table_measured(doc, &mut places, &dev, m)?;
    let r = t.rows.get(i)?;
    let tag = crate::devices::tag_words(doc, c);
    let what = if r.power { "power" } else { "auxiliary" };
    Some(vec![match (r.state, r.page) {
        (State::Spare, _) | (_, None) => format!("{tag} {} {}, {what}, offered, not drawn", r.label_pair(), r.kind.upper()),
        (s, Some(p)) => {
            let over = if s == State::Over { " (beyond the part's offer)" } else { "" };
            format!("{tag} {} {}, {what}, {} {}{over}", r.label_pair(), r.kind.upper(), crate::references::page_words(doc, p), r.place)
        }
    }])
}

/// The Checks rows of device `c` (design 025 §4.3): one `too-many-contacts` warning per kind drawn
/// beyond the offer, ringed at the first over pole.
pub fn count_checks(doc: &Document, c: DeviceId) -> Vec<(String, Option<PageId>, Option<Pt>)> {
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

/// Whether a project key sizes every contact table's text, so every table is placed again (the
/// xref map's wide case, design 033 §9.3): the table text's style choice (brief 050a; its height
/// and weight are the style's) and the font.
pub fn table_text_key(key: &str) -> bool {
    use crate::defaults::text::FONT;
    [crate::defaults::styles::TABLE.key, FONT.key].contains(&key)
}

/// Whether a `ModifyDevice` from `before` to `after` changes what the device offers or where its
/// table goes.
fn contacts_changed(before: &StoredDevice, after: &StoredDevice) -> bool {
    let of = |d: &StoredDevice| d.device_type.as_contacts().map(|x| (x.contacts().clone(), x.contact_table()));
    of(before) != of(after)
}

/// The devices whose table the user placed (`at dx,dy`): the ones a sheet change can move onto
/// or off the title block. O(components), on a project sheet change only.
pub fn placed_tables(doc: &Document) -> Vec<DeviceId> {
    doc.devices
        .iter()
        .filter(|(_, c)| matches!(&c.device_type, DeviceType::Switching(s) if matches!(s.contact_table, Some(ContactTable::At { .. }))))
        .map(|(id, _)| *id)
        .collect()
}

/// The Checks row of a table the user placed over the title block (design 033 §4.1): its words,
/// page and the table's top; `None` for an automatic placement (never moved onto it) or no table.
pub fn title_check(doc: &Document, c: DeviceId, m: &dyn TextMetrics) -> Option<(String, PageId, Pt)> {
    let (placement, _) = placement_of(doc, c);
    if !matches!(placement, Placement::At(_)) {
        return None;
    }
    let dev = device(doc, c);
    let mut places = Places::new(doc);
    let t = table_measured(doc, &mut places, &dev, m)?;
    if !enters_title_block(doc, t.page, table_box(doc, t.top, &t.rows, m)) {
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
    fn the_table_text_keys_are_the_wide_case() {
        for k in ["text.table.style", "text.font"] {
            assert!(table_text_key(k), "{k}");
        }
        assert!(!table_text_key("text.tag.style"));
    }

    #[test]
    fn the_offer_grammar() {
        let labels = |t: &str| {
            parse_offer(t).map(|v| {
                v.iter().map(|o| format!("{}/{} {}{}", o.labels[0], o.labels[1], if o.power { "P" } else { "" }, o.kind.upper())).collect::<Vec<_>>()
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
        assert_eq!(parse_offer("3X").unwrap_err().to_string(), "\"3X\" is not a contact list. Write it like 3P+1NO+1NC, or 13/14 NO, 21/22 NC.");
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
        assert_eq!(table_of_typed(Some("Project default (auto)")).unwrap(), None);
        assert_eq!(table_of_typed(Some("OFF")).unwrap(), Some(ContactTable::Off));
    }

    #[test]
    fn natural_order() {
        let mut v = vec!["13", "2", "A10", "A2", "1"];
        v.sort_by_key(|s| natural_key(s));
        assert_eq!(v, ["1", "2", "13", "A2", "A10"]);
    }
}
