//! The cable planning model (design 059 §2; design 065 §3, cut 2): the fields a cable stores, their
//! bounds, and the reads over them (planned ends, glands, shield, part values, drawn pages and
//! ends, off-plan ends and value checks). Pure rules over the document; the commands that write
//! them are `ecad_cadcraft::cmd::cable`.

use ecad_model::keys::PropsExt;
use std::collections::{BTreeMap, BTreeSet};

use ecad_model::{CableId, Document, ItemKind, PageId, Primitive, StyleRef, Subject};

use crate::checks::{Check, Fix, Kind, Object};
use crate::designations::{self, Aspect};
use crate::slash::MAX_CABLE_MEMBERS;
use crate::styles::{self, cable};

/// The pages a cable core is drawn on: its wire's pages, or its line's page while it has no wire.
pub fn member_pages(doc: &Document, m: &ecad_model::Core) -> BTreeSet<PageId> {
    match m.position {
        ecad_model::CorePosition::Wire(w) => crate::wires::wire_pages(doc, w),
        ecad_model::CorePosition::Route { route, .. } => doc.routes.get(&route).map(|r| r.page).into_iter().collect(),
        ecad_model::CorePosition::Unplaced => BTreeSet::new(),
    }
}

/// A cable end: `From` is the cabinet end, `To` the field end (057's sides).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Side {
    From,
    To,
}

impl Side {
    /// Both ends, cabinet first.
    pub const BOTH: [Side; 2] = [Side::From, Side::To];

    /// `from`, `to`: the JSON word and the stored key's part.
    pub fn word(self) -> &'static str {
        match self {
            Side::From => "from",
            Side::To => "to",
        }
    }

    /// `cabinet end`, `field end`, in words.
    pub fn words(self) -> &'static str {
        match self {
            Side::From => "cabinet end",
            Side::To => "field end",
        }
    }
}

/// A cable field the manager edits; [`CableField::key`] is its stored property.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum CableField {
    Tag,
    CableType,
    Type,
    Cores,
    Length,
    Gland(Side),
    Shield,
    Maker,
    PartNumber,
    Description,
    EndLocation(Side),
    EndFunction(Side),
}

impl CableField {
    /// Every field, in the Details pane's order.
    pub const ALL: [CableField; 15] = [
        CableField::Tag,
        CableField::CableType,
        CableField::Type,
        CableField::Cores,
        CableField::Shield,
        CableField::Maker,
        CableField::PartNumber,
        CableField::Description,
        CableField::Length,
        CableField::EndFunction(Side::From),
        CableField::EndLocation(Side::From),
        CableField::Gland(Side::From),
        CableField::EndFunction(Side::To),
        CableField::EndLocation(Side::To),
        CableField::Gland(Side::To),
    ];

    /// The stored property key.
    pub fn key(self) -> &'static str {
        match self {
            CableField::Tag => ecad_model::tag::TAG_KEY,
            CableField::CableType => cable::CABLE_TYPE,
            CableField::Type => "type",
            CableField::Cores => crate::strips::CORES_KEY,
            CableField::Length => "length",
            CableField::Gland(Side::From) => "gland.from",
            CableField::Gland(Side::To) => "gland.to",
            CableField::Shield => "shield",
            CableField::Maker => "maker",
            CableField::PartNumber => "part_number",
            CableField::Description => "description",
            CableField::EndLocation(Side::From) => "end.from.location",
            CableField::EndLocation(Side::To) => "end.to.location",
            CableField::EndFunction(Side::From) => "end.from.function",
            CableField::EndFunction(Side::To) => "end.to.function",
        }
    }

    /// The field a stored key names.
    pub fn parse(key: &str) -> Option<CableField> {
        CableField::ALL.into_iter().find(|f| f.key() == key)
    }

    /// The field's name in words (`Length`, `Gland, field end`).
    pub fn label(self) -> String {
        match self {
            CableField::Tag => "Tag".into(),
            CableField::CableType => "Cable type".into(),
            CableField::Type => "Type".into(),
            CableField::Cores => "Cores".into(),
            CableField::Length => "Length".into(),
            CableField::Shield => "Shield".into(),
            CableField::Maker => "Maker".into(),
            CableField::PartNumber => "Part number".into(),
            CableField::Description => "Description".into(),
            CableField::Gland(s) => format!("Gland, {}", s.words()),
            CableField::EndLocation(s) => format!("Planned location, {}", s.words()),
            CableField::EndFunction(s) => format!("Planned function, {}", s.words()),
        }
    }
}

/// A cable's shield (design 059 §2.1): the model's, stored as a field (design 072 §4.5).
pub use ecad_model::style::cable::Shield;

/// Where a shown value comes from (design 059 §2.2): the hint `set here`, `from type`,
/// `from notation`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Source {
    Here,
    Type,
    Notation,
}

impl Source {
    pub fn hint(self) -> &'static str {
        match self {
            Source::Here => "set here",
            Source::Type => "from type",
            Source::Notation => "from notation",
        }
    }
}

/// Most characters of a value a refusal echoes (tester 059a, rule 7): longer text ends with `…`.
pub const MAX_ECHO: usize = 40;

/// Most characters of a whole refusal.
pub const MAX_WORDS: usize = 400;

/// `t` cut to [`MAX_ECHO`] characters, with `…` when cut, and no hidden characters.
pub fn clip(t: &str) -> String {
    ecad_model::text::clip(t, MAX_ECHO)
}

/// Shortest and longest cable length stored, in metres.
pub const MIN_LENGTH: f64 = 0.1;

pub const MAX_LENGTH: f64 = 10_000.0;

/// Longest free text stored (`type`, maker, part number, description).
pub const MAX_TEXT: usize = 64;

/// A length read from text: metres, within bounds.
pub fn length_of(text: &str) -> Option<f64> {
    let t = text.trim();
    let t = t.strip_suffix('m').map_or(t, str::trim_end);
    t.parse::<f64>().ok().filter(|v| v.is_finite() && (MIN_LENGTH..=MAX_LENGTH).contains(v))
}

fn text_ok(t: &str, max: usize) -> bool {
    ecad_model::text::check(t, max).is_ok()
}

/// A stored value of cable `k` that ECAD reads: its property for `field`, when within bounds.
/// Entry keys (planned ends) and the cable type are given as stored; tags as stored.
pub fn read(doc: &Document, k: CableId, field: CableField) -> Option<String> {
    let c = doc.cables.get(&k)?;
    if field == CableField::Shield {
        return c.shield.map(|s| ecad_model::words::Word::word(s).to_string());
    }
    let v = c.properties.get(field.key())?;
    valid(doc, field, v).then(|| v.trim().to_string())
}

/// Whether a stored value of `field` is within bounds.
fn valid(doc: &Document, field: CableField, v: &str) -> bool {
    match field {
        CableField::Length => length_of(v).is_some(),
        CableField::Gland(_) => cable::is_gland(v),
        CableField::Shield => <Shield as ecad_model::words::Word>::parse(v).is_some(),
        CableField::Maker | CableField::PartNumber | CableField::Description | CableField::Type => text_ok(v, MAX_TEXT),
        CableField::Cores => crate::strips::parse_cores(v).is_ok(),
        CableField::EndLocation(_) => designations::code_of(doc, Aspect::Location, v.trim()).is_some(),
        CableField::EndFunction(_) => designations::code_of(doc, Aspect::Function, v.trim()).is_some(),
        CableField::CableType => StyleRef::parse(v).is_some_and(|id| styles::exists(doc, ecad_model::StyleCatalogue::Cable, &id)),
        CableField::Tag => !v.trim().is_empty(),
    }
}

/// The fields of §2.1 a file holds out of bounds (a file made elsewhere): `cable-bad-value`.
pub fn bad_values(doc: &Document, k: CableId) -> Vec<(CableField, String)> {
    let Some(c) = doc.cables.get(&k) else { return Vec::new() };
    let checked = [
        CableField::Length,
        CableField::Gland(Side::From),
        CableField::Gland(Side::To),
        CableField::Maker,
        CableField::PartNumber,
        CableField::Description,
        CableField::EndLocation(Side::From),
        CableField::EndLocation(Side::To),
        CableField::EndFunction(Side::From),
        CableField::EndFunction(Side::To),
    ];
    checked
        .into_iter()
        .filter_map(|f| {
            let v = c.properties.get(f.key())?;
            (!valid(doc, f, v)).then(|| (f, v.chars().take(24).filter(|c| !c.is_control()).collect()))
        })
        .collect()
}

/// Whether cable `k` is drawn: a core on a line, or a representation (FD1). Its stored unplaced
/// cores (design 072 §4.5) do not draw it.
pub fn is_drawn(doc: &Document, k: CableId) -> bool {
    doc.cables.get(&k).is_some_and(|c| c.cores.iter().any(|m| m.position != ecad_model::CorePosition::Unplaced))
        || doc.representations_of(Subject::Cable(k)).next().is_some()
}

/// The cables not drawn yet, in tag order. O(cables): the pickers build it once when opened.
pub fn not_drawn(doc: &Document) -> Vec<CableId> {
    let mut v: Vec<(String, CableId)> =
        doc.cables.keys().filter(|k| !is_drawn(doc, **k)).map(|k| (crate::cables::cable_title(doc, *k), *k)).collect();
    v.sort_by(|a, b| crate::locations::natural_cmp(&a.0, &b.0));
    v.into_iter().map(|(_, k)| k).collect()
}

/// The cables whose planned ends store entry `key` (a rename shows the new code there).
/// O(cables), on a rename of the project's list only.
pub fn planning(doc: &Document, key: &str) -> Vec<CableId> {
    let fields = [Side::From, Side::To].into_iter().flat_map(|s| [CableField::EndLocation(s), CableField::EndFunction(s)]);
    let fields: Vec<&str> = fields.map(CableField::key).collect();
    doc.cables.iter().filter(|(_, c)| fields.iter().any(|f| c.properties.get(*f).is_some_and(|v| v.trim() == key))).map(|(k, _)| *k).collect()
}

/// Cable `k`'s planned code of `aspect` at `side` (`+CAB1`), when it names an entry.
pub fn planned(doc: &Document, k: CableId, side: Side, aspect: Aspect) -> Option<String> {
    let field = match aspect {
        Aspect::Location => CableField::EndLocation(side),
        Aspect::Function => CableField::EndFunction(side),
    };
    let key = doc.cables.get(&k)?.properties.get(field.key())?;
    designations::code_of(doc, aspect, key.trim())
}

/// Cable `k`'s plan at `side` in words (`=PUMP +CAB1`), when it has one.
pub fn plan_words(doc: &Document, k: CableId, side: Side) -> Option<String> {
    let parts: Vec<String> = [Aspect::Function, Aspect::Location].into_iter().filter_map(|a| planned(doc, k, side, a)).collect();
    (!parts.is_empty()).then(|| parts.join(" "))
}

/// Cable `k`'s cable type, when it names one that exists.
fn type_of(doc: &Document, k: CableId) -> Option<StyleRef> {
    cable::of(doc, k)
}

/// A text value of cable `k`: its own, else its cable type's field `type_key`.
fn own_or_type(doc: &Document, k: CableId, field: CableField, type_key: &str) -> Option<(String, Source)> {
    if let Some(v) = read(doc, k, field) {
        return Some((v, Source::Here));
    }
    let id = type_of(doc, k)?;
    cable::type_text(doc, &id, type_key).map(|v| (v, Source::Type))
}

/// Cable `k`'s gland at `side`: its own, else its cable type's (Q5, option B).
pub fn gland(doc: &Document, k: CableId, side: Side) -> Option<(String, Source)> {
    own_or_type(doc, k, CableField::Gland(side), cable::TYPE_GLAND).filter(|(g, _)| cable::is_gland(g))
}

/// Cable `k`'s shield: its own field, else its cable type's.
pub fn shield(doc: &Document, k: CableId) -> Option<(Shield, Source)> {
    if let Some(s) = doc.cables.get(&k)?.shield {
        return Some((s, Source::Here));
    }
    let id = type_of(doc, k)?;
    cable::type_text(doc, &id, cable::TYPE_SHIELD).and_then(|w| <Shield as ecad_model::words::Word>::parse(&w)).map(|s| (s, Source::Type))
}

/// Cable `k`'s maker, part number or description: its own, else its cable type's (Q8).
pub fn part_value(doc: &Document, k: CableId, field: CableField) -> Option<(String, Source)> {
    let type_key = match field {
        CableField::Maker => "maker",
        CableField::PartNumber => "part_number",
        CableField::Description => "description",
        _ => return read(doc, k, field).map(|v| (v, Source::Here)),
    };
    own_or_type(doc, k, field, type_key)
}

/// Cable `k`'s core offer as written (`4G`, `BN,BK,GY`) and where it comes from.
pub fn cores_shown(doc: &Document, k: CableId) -> Option<(String, Source)> {
    let (_, src, text) = cable::offer(doc, k)?;
    let props = &doc.cables.get(&k)?.properties;
    let own = props.get(CableField::Cores.key()).is_some_and(|v| crate::strips::parse_cores(v).is_ok());
    Some((
        text,
        match src {
            crate::strips::OfferSource::Cores if own => Source::Here,
            crate::strips::OfferSource::Cores => Source::Type,
            crate::strips::OfferSource::Type => Source::Notation,
        },
    ))
}

/// The pages cable `k` is drawn on, with the place of its slash (`2/D1`), in book order: its
/// representations' pages, else its members' route pages.
pub fn drawn_pages(doc: &Document, k: CableId) -> Vec<(PageId, String)> {
    let style = doc.settings.reference_style();
    let mut out: BTreeMap<PageId, String> = BTreeMap::new();
    for (_, r) in doc.representations_of(Subject::Cable(k)) {
        let at = r.items.first().and_then(|i| doc.items.get(i)).and_then(|i| match &i.kind {
            ItemKind::Primitive(Primitive::Line { a, .. }) => Some(*a),
            ItemKind::Primitive(Primitive::Point { at }) => Some(*at),
            ItemKind::Primitive(Primitive::Polyline { vertices, .. }) => vertices.first().copied(),
            _ => None,
        });
        let words =
            at.and_then(|a| ecad_model::wiring::place(doc, r.page, a)).map_or_else(|| crate::references::page_number(doc, r.page), |p| p.text(style));
        out.entry(r.page).or_insert(words);
    }
    if out.is_empty()
        && let Some(c) = doc.cables.get(&k)
    {
        for m in c.cores.iter().take(MAX_CABLE_MEMBERS) {
            for p in member_pages(doc, m) {
                out.entry(p).or_insert_with(|| crate::references::page_number(doc, p));
            }
        }
    }
    let mut v: Vec<(PageId, String)> = out.into_iter().collect();
    v.sort_by_key(|(p, _)| doc.page_order.iter().position(|x| x == p).unwrap_or(usize::MAX));
    v
}

/// One end of a drawn cable as drawn: the functions and locations (codes) of the devices at that
/// end of its cores. The cabinet end is where each core chain starts (its first wire's end A), the
/// field end where it finishes (its last wire's end B), as 057's core table reads them. Wire-only
/// items (`wires::kind_of`) do not count.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct DrawnEnd {
    pub locations: BTreeSet<String>,
    pub functions: BTreeSet<String>,
    /// A device there has no location (or no function): that aspect is not compared.
    pub no_location: bool,
    pub no_function: bool,
}

impl DrawnEnd {
    /// `=FAN +JB1`, as the end is drawn.
    pub fn words(&self) -> String {
        let f: Vec<&str> = self.functions.iter().map(String::as_str).collect();
        let l: Vec<&str> = self.locations.iter().map(String::as_str).collect();
        [f.join(", "), l.join(", ")].into_iter().filter(|x| !x.is_empty()).collect::<Vec<_>>().join(" ")
    }
}

/// Cable `k`'s two ends as drawn, cabinet then field (see [`DrawnEnd`]). Bounded by the cable's
/// member bound.
pub fn drawn_ends(doc: &Document, k: CableId) -> [DrawnEnd; 2] {
    let mut out = [DrawnEnd::default(), DrawnEnd::default()];
    for chain in crate::references::core_chains(doc, k).iter().take(MAX_CABLE_MEMBERS) {
        let ends = [chain.first().and_then(|w| doc.wires.get(w)).map(|c| c.ends[0]), chain.last().and_then(|w| doc.wires.get(w)).map(|c| c.ends[1])];
        for (slot, t) in out.iter_mut().zip(ends) {
            let Some(c) = t.and_then(|t| doc.terminals.get(&t)).map(|x| x.component) else { continue };
            if crate::wires::kind_of(doc, c).is_some() {
                continue;
            }
            match crate::locations::resolve(doc, c).value {
                Some(l) => {
                    slot.locations.insert(l);
                }
                None => slot.no_location = true,
            }
            match crate::locations::resolve_function(doc, c).value {
                Some(x) => {
                    slot.functions.insert(x);
                }
                None => slot.no_function = true,
            }
        }
    }
    out
}

/// A drawn end away from that same end's plan (design 059 §6, Q4; tester 059a).
#[derive(Debug, Clone, PartialEq)]
pub struct OffPlan {
    pub side: Side,
    pub drawn: DrawnEnd,
}

/// The ends of drawn cable `k` drawn away from that same end's plan: a planned location or
/// function that the devices at that end do not have. The two ends differing from each other is
/// normal and never raises; an end with no plan, or with no device drawn there, raises nothing.
pub fn off_plan(doc: &Document, k: CableId) -> Vec<OffPlan> {
    if doc.cables.get(&k).is_none_or(|c| c.cores.iter().all(|m| m.position == ecad_model::CorePosition::Unplaced)) {
        return Vec::new();
    }
    let ends = drawn_ends(doc, k);
    let mut out = Vec::new();
    for (side, end) in Side::BOTH.into_iter().zip(ends) {
        let differs = |aspect: Aspect, have: &BTreeSet<String>| {
            let Some(code) = planned(doc, k, side, aspect) else { return false };
            !have.is_empty() && !have.iter().any(|d| d.eq_ignore_ascii_case(&code))
        };
        if differs(Aspect::Location, &end.locations) || differs(Aspect::Function, &end.functions) {
            out.push(OffPlan { side, drawn: end });
        }
    }
    out
}

/// The value rows of cable `k` (design 059 §6): `cable-bad-value` always; `cable-end-off-plan` and
/// `no-gland` (059d) once drawn.
pub fn value_checks(doc: &Document, k: CableId, drawn: bool) -> Vec<Check> {
    let mut out = Vec::new();
    if !doc.cables.contains_key(&k) {
        return out;
    }
    let name = crate::cables::cable_title(doc, k);
    for (field, v) in bad_values(doc, k) {
        let what = match field {
            CableField::Length => format!("{name}'s length {v:?} is not a number of metres; it is ignored."),
            _ => format!("{name}'s {} {v:?} is not a value ECAD reads; it is ignored.", field.label().to_lowercase()),
        };
        out.push(Check {
            kind: Kind::CableBadValue,
            page: None,
            at: None,
            object: Object::Cable(k),
            which: what,
            fix: Some(Fix::ShowCableField(k, field)),
        });
    }
    if !drawn {
        return out;
    }
    let anchor = ecad_model::numbering::rep_anchor(doc, Subject::Cable(k));
    let (page, at) = anchor.map_or((None, None), |(p, a)| (Some(p), Some(a)));
    for o in off_plan(doc, k) {
        let (side, drawn_at) = (o.side, o.drawn.words());
        let plan = plan_words(doc, k, side).unwrap_or_default();
        out.push(Check {
            kind: Kind::CableEndOffPlan,
            page,
            at,
            object: Object::Cable(k),
            which: format!("{name}'s {} is planned {plan} but drawn at {drawn_at}.", side.words()),
            fix: Some(Fix::UseDrawing(k, side)),
        });
    }
    // 059d (design 059 §6): a drawn end with no gland size, its own or its cable type's.
    for side in Side::BOTH {
        if gland(doc, k, side).is_none() {
            out.push(Check {
                kind: Kind::NoGland,
                page,
                at,
                object: Object::Cable(k),
                which: format!("{name} has no gland size at the {}.", side.words()),
                fix: Some(Fix::ShowCableField(k, CableField::Gland(side))),
            });
        }
    }
    out
}

/// The notation a cable type gives `type` (`H07RN-F 4G1.5`): its name when it carries a core
/// notation, else its cores and cross-section (`4G1.5`).
pub fn type_notation(doc: &Document, id: &StyleRef) -> Option<String> {
    let name = styles::name(doc, ecad_model::StyleCatalogue::Cable, id);
    if crate::strips::offer_from_type(&name).is_some() {
        return Some(name);
    }
    let cores = cable::type_text(doc, id, cable::TYPE_CORES)?;
    let size = cable::type_text(doc, id, "size").unwrap_or_default();
    Some(format!("{cores}{size}"))
}

/// A cable after the word "Cable" in messages: its tag, else `8a3f19c0…`.
pub fn cable_title(doc: &Document, cable: CableId) -> String {
    match doc.cables.get(&cable).and_then(|c| c.properties.text(ecad_model::keys::TAG)) {
        Some(t) => t.to_string(),
        None => format!("{}…", cable.hex().get(..8).unwrap_or_default()),
    }
}
