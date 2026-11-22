//! The rules of the rating commands (brief 057, design 057 §8): which wires or cable `rating` and
//! `ratings` name, the values `rating` sets, a location's panel temperature, and the record
//! `ratingtable` writes. The engine crate's shims read the files, commit the ops and answer in
//! JSON.

use crate::rating::Rating;
use ecad_model::style::{StyleLayer, StyleRecord, StyleRef, is_name};
use ecad_model::units::{Amps, Celsius, Percent, Volts};
use ecad_model::wire::{InstallMethod, SupplySystem};
use ecad_model::{CableId, Document, IdAllocator, Op, Wire, WireId, Word};
use schemars::JsonSchema;
use serde::Deserialize;
use serde_json::{Value, json};

use crate::cmd::input::{Bounded, TextOrNumber, nullable};
use crate::cmd::run::{Access, Cx, Generic, Got, Io, Outcome, Pages, Read};
use crate::error::CmdError;
use crate::rating::{self, Basis, RatedSubject, dataset, parse};
use crate::styles::{self, cable, ratingtable};

/// Most subjects one `rating {many}` call sets.
pub const MAX_MANY: usize = 500;
/// Most results one `ratings {}` call returns.
pub const PAGE: usize = 200;

// ---------------------------------------------------------------------------------------------
// Subjects

/// The conductors `text` names: a conductor ID, or every panel wire carrying that wire number.
///
/// # Errors
/// No panel wire by that number or ID.
pub fn wires(doc: &Document, text: &str) -> Result<Vec<WireId>, CmdError> {
    let t = text.trim();
    if let Some(c) = WireId::parse(t).filter(|c| doc.wires.contains_key(c)) {
        return Ok(vec![c]);
    }
    let conn = doc.connectivity();
    let mut out: Vec<WireId> = conn.nets_numbered(t).filter_map(|(n, _)| conn.net(n)).flat_map(|n| n.wires.iter().copied()).collect();
    out.retain(|c| rating::is_panel_wire(doc, *c));
    out.sort();
    out.dedup();
    if out.is_empty() {
        let shown: String = t.chars().take(32).collect();
        return Err(CmdError::refused(format!(
            "No panel wire {shown:?} (give a wire number or a conductor ID; a cable's cores are rated with the cable)."
        )));
    }
    Ok(out)
}

/// The cable `text` names: a tag or an ID.
///
/// # Errors
/// No cable by that tag or ID.
pub fn cable_named(doc: &Document, text: &str) -> Result<CableId, CmdError> {
    let t = text.trim();
    if let Some(k) = CableId::parse(t).filter(|k| doc.cables.contains_key(k)) {
        return Ok(k);
    }
    doc.cables_tagged(t).into_iter().next().ok_or_else(|| CmdError::refused(format!("No cable {:?}.", t.chars().take(32).collect::<String>())))
}

/// The subjects `wire` or `cable` names (exactly one of them).
///
/// # Errors
/// Both, neither, or one that names nothing.
pub fn subjects_of(doc: &Document, wire: Option<&str>, cable: Option<&str>) -> Result<Vec<RatedSubject>, CmdError> {
    match (wire, cable) {
        (Some(w), None) => Ok(wires(doc, w)?.into_iter().map(RatedSubject::Wire).collect()),
        (None, Some(c)) => Ok(vec![RatedSubject::Cable(cable_named(doc, c)?)]),
        (Some(_), Some(_)) => Err(CmdError::input("Give `wire` or `cable`, not both.")),
        (None, None) => Err(CmdError::input("Give `wire` (a wire number or conductor ID) or `cable` (a tag or ID).")),
    }
}

// ---------------------------------------------------------------------------------------------
// rating

/// The keys `rating` takes: (JSON key, property key, for wires, for cables).
pub const KEYS: &[(&str, &str, bool, bool)] = &[
    ("basis", rating::BASIS, true, true),
    ("manual", rating::MANUAL, true, true),
    ("bundling", rating::BUNDLING, true, false),
    ("load", rating::LOAD, true, true),
    ("protected_by", rating::PROTECTED_BY, true, true),
    ("method", rating::METHOD, false, true),
    ("ambient", rating::AMBIENT, false, true),
    ("grouped", rating::GROUPED, false, true),
    ("voltage", rating::VOLTAGE, false, true),
    ("system", rating::SYSTEM, false, true),
    ("vd_limit", rating::VD_LIMIT, false, true),
    ("cable_type", cable::CABLE_TYPE, false, true),
];

/// The refusal for a key `rating` does not take here.
pub fn not_a_key(key: &str) -> CmdError {
    CmdError::refused(format!("{key} is not a rating key; the keys are {}.", KEYS.iter().map(|k| k.0).collect::<Vec<_>>().join(", ")))
}

/// Check `value` for `key` (design 057 §6.1) and give the text stored.
///
/// # Errors
/// The key and its range, in words.
pub fn check_value(doc: &Document, key: &str, value: &str) -> Result<String, CmdError> {
    let v = value.trim();
    let shown: String = v.chars().take(32).collect();
    let no = |why: &'static str| CmdError::refused(why);
    match key {
        "basis" => Basis::parse(v).map(|b| b.word().to_string()).ok_or_else(|| no("basis is datasheet, table or manual.")),
        "manual" | "load" => parse::current(v)
            .map(|_| v.to_string())
            .ok_or_else(|| CmdError::refused(format!("{key} is a current above 0 and up to 6300 A, not {shown:?}."))),
        "bundling" => parse::bundling(v)
            .map(|_| v.to_string())
            .ok_or_else(|| no("bundling is a factor 0.30 to 1.00, or a circuit count 1 to 20 (\"3 bunched\").")),
        "protected_by" => match rating::trace::protected_by(doc, v).map(|p| (p.device, p.position.map(|n| n.to_string()))) {
            Some((c, fuse)) => {
                let tag = rating::trace::device_words(doc, c, fuse.as_deref());
                if rating::trace::amps_of(doc, c, fuse.as_deref()).is_none() {
                    return Err(CmdError::refused(format!("{tag} has no rated current; set its rating first (for example C16).")));
                }
                Ok(tag)
            }
            None => Err(CmdError::refused(format!(
                "There is no device or fuse {shown:?}; give a tag such as -Q01, or -F01:3 for a fuse of a bank (positions 1 to 255)."
            ))),
        },
        "method" => dataset::method(v)
            .map(str::to_string)
            .ok_or_else(|| CmdError::refused(format!("method is an IEC 60364-5-52 name: {}.", dataset::METHODS.join(", ")))),
        "ambient" => parse::ambient(v).map(|_| v.to_string()).ok_or_else(|| no("ambient is \u{2212}40 to 90 \u{b0}C.")),
        "grouped" => parse::count(v).map(|n| n.to_string()).ok_or_else(|| no("grouped is a circuit count 1 to 20.")),
        "voltage" => parse::voltage(v).map(|_| v.to_string()).ok_or_else(|| no("voltage is 1 to 1000 V.")),
        "system" => {
            ["ac1", "ac3", "dc"].into_iter().find(|w| w.eq_ignore_ascii_case(v)).map(str::to_string).ok_or_else(|| no("system is ac1, ac3 or dc."))
        }
        "vd_limit" => parse::percent(v).map(|_| v.to_string()).ok_or_else(|| no("vd_limit is 0.5 to 20 %.")),
        "cable_type" => styles::find(doc, ecad_model::StyleCatalogue::Cable, v).map(|id| id.text()),
        _ => Err(CmdError::refused(format!("{key} is not a rating key."))),
    }
}

/// One subject of `rating`: a wire or a cable, and the keys set (JSON key, value; `None` clears).
#[derive(Debug, Clone, Default, PartialEq)]
pub struct RatingItem {
    pub wire: Option<String>,
    pub cable: Option<String>,
    /// In [`KEYS`] order.
    pub sets: Vec<(&'static str, Option<String>)>,
}

/// The ops one item of `rating` asks for, and the subjects it names.
///
/// # Errors
/// The subject does not read, a key is not for that kind of subject, or a value breaks its range.
pub fn item_ops(doc: &Document, item: &RatingItem) -> Result<(Vec<Op>, Vec<RatedSubject>), CmdError> {
    let subjects = subjects_of(doc, item.wire.as_deref(), item.cable.as_deref())?;
    let cable = matches!(subjects.first(), Some(RatedSubject::Cable(_)));
    let mut sets: Vec<(&'static str, &'static str, Option<String>)> = Vec::new();
    for (json_key, value) in &item.sets {
        let Some((key, prop, for_wire, for_cable)) = KEYS.iter().find(|k| k.0 == *json_key) else { return Err(not_a_key(json_key)) };
        if (cable && !for_cable) || (!cable && !for_wire) {
            return Err(CmdError::refused(format!("{json_key} is for {}.", if *for_wire { "panel wires" } else { "cables" })));
        }
        let after = match value {
            None => None,
            Some(t) => Some(check_value(doc, json_key, t)?),
        };
        sets.push((key, prop, after));
    }
    let mut ops = Vec::new();
    for s in &subjects {
        match s {
            RatedSubject::Wire(c) => {
                let Some(w) = doc.wires.get(c) else { continue };
                let mut after = w.clone();
                for (key, _, value) in &sets {
                    set_wire(doc, &mut after, key, value.as_deref());
                }
                if after != *w {
                    ops.push(Op::ModifyWire { id: *c, before: w.clone(), after });
                }
            }
            RatedSubject::Cable(k) => {
                let Some(cab) = doc.cables.get(k) else { continue };
                let mut after = cab.clone();
                for (key, _, value) in &sets {
                    set_cable(doc, &mut after, key, value.as_deref());
                }
                ops.extend(Op::modify_cable(*k, cab, after));
            }
        }
    }
    Ok((ops, subjects))
}

/// Set a wire's input `key` (a JSON key of [`KEYS`] for wires) to `value`, already checked by
/// [`check_value`] (`None` clears it).
pub fn set_wire(doc: &Document, w: &mut Wire, key: &str, value: Option<&str>) {
    match key {
        "basis" => w.rating.basis = value.and_then(Basis::parse),
        "manual" => w.rating.manual = value.and_then(parse::current).map(Amps),
        "bundling" => w.rating.bundling = value.and_then(parse::bundling),
        "load" => w.load = value.and_then(parse::current).map(Amps),
        "protected_by" => w.protected_by = value.and_then(|v| rating::trace::protected_by(doc, v)),
        _ => {}
    }
}

/// Set a cable's input `key` (a JSON key of [`KEYS`] for cables) to `value`, already checked by
/// [`check_value`] (`None` clears it): a field (the cable type is [`ecad_model::Cable::cable_type`]).
pub fn set_cable(doc: &Document, c: &mut ecad_model::Cable, key: &str, value: Option<&str>) {
    let r = &mut c.rating;
    match key {
        "basis" => r.basis = value.and_then(Basis::parse),
        "manual" => r.manual = value.and_then(parse::current).map(Amps),
        "method" => r.method = value.and_then(InstallMethod::parse),
        "ambient" => r.ambient = value.and_then(parse::ambient).map(Celsius),
        "grouped" => r.grouped = value.and_then(parse::count).and_then(|n| u8::try_from(n).ok()),
        "voltage" => r.voltage = value.and_then(parse::voltage).map(Volts),
        "system" => r.system = value.and_then(SupplySystem::parse),
        "vd_limit" => r.vd_limit = value.and_then(parse::percent).map(Percent),
        "load" => c.load = value.and_then(parse::current).map(Amps),
        "protected_by" => c.protected_by = value.and_then(|v| rating::trace::protected_by(doc, v)),
        "cable_type" => c.cable_type = value.and_then(ecad_model::StyleRef::parse),
        _ => {}
    }
}

/// What `rating` records: the ops of every item, the subjects named, and the label.
#[derive(Debug, Clone, PartialEq)]
pub struct Rated {
    pub ops: Vec<Op>,
    pub subjects: Vec<RatedSubject>,
    pub label: String,
}

/// `rating {wire | cable | many, …}`: the ops of every item, in one transaction.
///
/// # Errors
/// The first item's refusal ([`item_ops`]); nothing is written.
pub fn plan_rating(doc: &Document, items: &[RatingItem]) -> Result<Rated, CmdError> {
    let mut ops = Vec::new();
    let mut all = Vec::new();
    for item in items {
        let (o, s) = item_ops(doc, item)?;
        ops.extend(o);
        all.extend(s);
    }
    let who: Vec<String> = all.iter().take(3).map(|s| s.words(doc)).collect();
    let more = if all.len() > 3 { format!(" and {} more", all.len() - 3) } else { String::new() };
    Ok(Rated { ops, label: format!("Rating {}{more}", who.join(", ")), subjects: all })
}

/// What `rating {location, temperature}` records.
#[derive(Debug, Clone, PartialEq)]
pub struct PanelTemperature {
    /// Empty when every box already has that value.
    pub ops: Vec<Op>,
    /// The location's code.
    pub location: String,
    /// The location boxes of the location.
    pub boxes: usize,
    pub label: String,
}

/// A panel temperature as typed (°C), read.
///
/// # Errors
/// Outside −40 to 90 °C.
pub fn panel_temperature(text: &str) -> Result<ecad_model::units::Celsius, CmdError> {
    parse::ambient(text.trim()).map(ecad_model::units::Celsius).ok_or(CmdError::refused("temperature is \u{2212}40 to 90 \u{b0}C."))
}

/// Check a panel temperature (°C) and give the text stored.
///
/// # Errors
/// Outside −40 to 90 °C.
pub fn check_temperature(text: &str) -> Result<String, CmdError> {
    let t = text.trim();
    match parse::ambient(t) {
        Some(_) => Ok(t.to_string()),
        None => Err(CmdError::refused("temperature is \u{2212}40 to 90 \u{b0}C.")),
    }
}

/// `rating {location, temperature}` (design 057 FP6; tester F1): the location boxes of `typed` (a
/// code or an entry key) take panel temperature `after` (`None` clears it), one `ModifyAid` per
/// box that changes (design 072 §6.2).
///
/// # Errors
/// No location box of that location.
pub fn plan_temperature(doc: &Document, typed: &str, after: Option<&str>) -> Result<PanelTemperature, CmdError> {
    // Brief 052: `location` is a code (or an entry key); boxes store the entry's key.
    let entry = crate::designations::find(doc, crate::designations::Aspect::Location, typed);
    let loc = entry.as_ref().map_or_else(|| typed.to_string(), |e| e.code.clone());
    let boxes: Vec<ecad_model::AidId> = match &entry {
        None => Vec::new(),
        Some(e) => doc
            .aids
            .iter()
            .filter(|(_, x)| {
                x.as_box().and_then(|b| crate::designations::box_key(b, crate::designations::Aspect::Location)).is_some_and(|l| l == e.key)
            })
            .map(|(a, _)| *a)
            .collect(),
    };
    if boxes.is_empty() {
        return Err(CmdError::refused(format!("No location box is {loc}; draw one with Location Box first.")));
    }
    let temp = after.map(panel_temperature).transpose()?;
    let mut ops = Vec::new();
    for a in &boxes {
        if let Some(op) = crate::cmd::continuation::box_op(doc, *a, |b| b.panel_temperature = temp) {
            ops.push(op);
        }
    }
    let words = after.map_or_else(|| "project default".to_string(), |t| format!("{t} \u{b0}C"));
    Ok(PanelTemperature { ops, label: format!("Panel temperature {loc}: {words}"), location: loc, boxes: boxes.len() })
}

// ---------------------------------------------------------------------------------------------
// ratingtable

/// The table `ratingtable {export, name?}` writes: the one named, else the one chosen.
///
/// # Errors
/// No such table, or none chosen and no name.
pub fn export_table(doc: &Document, name: Option<&str>) -> Result<ratingtable::Table, CmdError> {
    let id = match name {
        Some(n) => styles::find(doc, ecad_model::StyleCatalogue::RatingTable, n)?,
        None => ratingtable::chosen(doc).ok_or(CmdError::refused("No rating table chosen; give `name`."))?,
    };
    Ok(ratingtable::read(doc, &id))
}

/// The refusal for a CSV file over [`dataset::MAX_BYTES`].
pub fn too_big() -> CmdError {
    CmdError::refused(format!("The table is over {} KiB. Nothing was written.", dataset::MAX_BYTES / 1024))
}

/// The layer `ratingtable` writes to: `project` (the default) or `standard`.
///
/// # Errors
/// Another word.
pub fn table_layer(word: Option<&str>) -> Result<StyleLayer, CmdError> {
    match word.map(str::trim) {
        None | Some("project") => Ok(StyleLayer::Project),
        Some("standard") => Ok(StyleLayer::Standard),
        Some(_) => Err(CmdError::refused("layer is project or standard.")),
    }
}

/// A `ratingtable` write: the rows (CSV), the table's name and source, and its layer.
#[derive(Debug, Clone, PartialEq)]
pub struct TableWrite {
    pub csv: String,
    pub name: String,
    pub source: Option<String>,
    pub layer: StyleLayer,
}

/// What `ratingtable` records.
#[derive(Debug, Clone, PartialEq)]
pub struct TablePlan {
    /// Empty when the table is already so.
    pub ops: Vec<Op>,
    pub id: StyleRef,
    pub rows: usize,
    /// What reads oddly in the rows, in words.
    pub odd: Option<String>,
    pub label: String,
}

/// Check a `ratingtable` write before the project is taken: the rows, the name and the source.
///
/// # Errors
/// A bad row (the whole import is refused), a name or source that does not read.
pub fn check_table(w: &TableWrite) -> Result<dataset::Dataset, CmdError> {
    let data = dataset::parse_strict(&w.csv)?;
    if !is_name(&w.name) {
        return Err(CmdError::refused("A rating table needs a name (1 to 64 characters, no control characters)."));
    }
    if w.source.as_ref().is_some_and(|s| s.chars().count() > ratingtable::MAX_SOURCE_CHARS || s.chars().any(char::is_control)) {
        return Err(CmdError::refused(format!("source is at most {} characters, no control characters.", ratingtable::MAX_SOURCE_CHARS)));
    }
    Ok(data)
}

/// `ratingtable {name, source?, csv, layer?}`: create or replace the table named `w.name` (one
/// `SetStyle`).
///
/// # Errors
/// [`check_table`]'s refusals, a standard layer without a standard, or a record the catalogue
/// refuses.
pub fn plan_table(doc: &Document, ids: &mut IdAllocator, w: &TableWrite) -> Result<TablePlan, CmdError> {
    let data = check_table(w)?;
    if w.layer == StyleLayer::Standard && doc.standard.is_none() {
        return Err(CmdError::refused(crate::cmd::style::STANDARD_FIRST));
    }
    let cat = &ratingtable::CATALOGUE;
    let existing = styles::rows(doc, cat).into_iter().find(|r| r.name.eq_ignore_ascii_case(&w.name)).map(|r| r.id);
    let id = existing.clone().unwrap_or_else(|| StyleRef::Own(ids.fresh::<u128>()));
    let before = styles::record(doc, w.layer, cat.catalogue, &id).cloned();
    let mut rec = before.clone().unwrap_or_else(|| StyleRecord::new(w.name.clone(), ecad_model::style::StyleBody::empty(cat.catalogue)));
    if rec.name.is_none() && existing.is_none() {
        rec.name = ecad_model::style::StyleName::new(&w.name).ok();
    }
    let mut set = vec![(ratingtable::ROWS.to_string(), Some(w.csv.clone()))];
    if let Some(src) = &w.source {
        set.push((ratingtable::SOURCE.to_string(), Some(src.clone())));
    }
    let rec = styles::with_fields(cat.catalogue, Some(&rec), None, &set)?;
    styles::check_record(doc, cat, &id, w.layer, &rec)?;
    let rows = data.rows.len();
    let label = format!("Rating table {}: {rows} row{}", w.name, if rows == 1 { "" } else { "s" });
    let ops = if before.as_ref() == Some(&rec) {
        Vec::new()
    } else {
        vec![Op::SetStyle { layer: w.layer, catalogue: cat.catalogue, id: id.clone(), before, after: Some(rec) }]
    };
    Ok(TablePlan { ops, id, rows, odd: data.odd(), label })
}

/// One result as `ratings` returns it (design 057 §8.2): the object the panel shows.
pub fn rating_json(doc: &Document, r: &Rating) -> Value {
    let subject = match r.subject {
        RatedSubject::Wire(c) => json!({"wire": r.subject.words(doc), "conductor": c.hex()}),
        RatedSubject::Cable(k) => json!({"cable": r.subject.words(doc), "id": k.hex()}),
    };
    let factors: Vec<Value> = r.factors.iter().map(|f| json!({"name": f.name, "factor": f.factor, "input": f.input, "from": f.from})).collect();
    let device = r.device.as_ref().map(|d| json!({"tag": d.tag, "in": d.amps, "how": d.how.word()}));
    let load = r.load.map(|l| json!({"amps": l.amps, "from": if l.from == rating::load::LoadFrom::Typed { "typed" } else { "power" }}));
    let vd = r.voltdrop.as_ref().map(|v| {
        json!({"volts": v.volts, "percent": v.percent, "shown": format!("{} %", rating::fmt_percent(v.percent)), "amps": v.amps, "why": v.why, "limit": v.limit})
    });
    let kinds: Vec<&str> = rating::rows(doc, r).iter().map(|c| c.kind.name()).collect();
    json!({
        "subject": subject,
        "basis": r.basis.word(),
        "rated": r.rated.map(|(a, at)| json!({"amps": a, "at": at})),
        "table_value": r.table_value.map(|(a, size, m)| json!({"amps": a, "size": size, "method": m})),
        "table": r.table.as_ref().map(|(n, src)| json!({"name": n, "source": src})),
        "factors": factors,
        "iz": r.iz,
        "iz_shown": r.iz.map(|x| format!("{} A", rating::fmt_amps(x, r.device.as_ref().and_then(|d| d.amps)))),
        "summary": r.summary(),
        "device": device,
        "load": load,
        "voltdrop": vd,
        "size": r.size,
        "checks": kinds,
    })
}

// ---------------------------------------------------------------------------------------------
// The commands (generic: crate::cmd::run)

/// A rating key as typed: absent (unchanged), null (cleared) or a value.
type KeyValue = Option<Option<TextOrNumber>>;

/// The keys given, each with its value as text (`None`: cleared).
fn sets_of(given: [(&'static str, &KeyValue); 12]) -> Vec<(&'static str, Option<String>)> {
    given.into_iter().filter_map(|(k, v)| v.as_ref().map(|v| (k, v.as_ref().map(|t| t.text().trim().to_string())))).collect()
}

/// The twelve rating keys of a params struct, with their values.
macro_rules! keys_of {
    ($p:expr) => {
        [
            ("basis", &$p.basis),
            ("manual", &$p.manual),
            ("bundling", &$p.bundling),
            ("load", &$p.load),
            ("protected_by", &$p.protected_by),
            ("method", &$p.method),
            ("ambient", &$p.ambient),
            ("grouped", &$p.grouped),
            ("voltage", &$p.voltage),
            ("system", &$p.system),
            ("vd_limit", &$p.vd_limit),
            ("cable_type", &$p.cable_type),
        ]
    };
}

/// One subject of `rating {many}`: a wire or a cable, and the keys to set (null clears a key).
#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct RatingItemParams {
    /// A panel wire: its conductor id or wire number.
    #[serde(default)]
    wire: Option<String>,
    /// A cable: its tag or id.
    #[serde(default)]
    cable: Option<String>,
    /// How the rating is found: datasheet, table or manual.
    #[serde(default, deserialize_with = "nullable")]
    basis: KeyValue,
    /// The rated current (A) when the basis is manual.
    #[serde(default, deserialize_with = "nullable")]
    manual: KeyValue,
    /// Panel wires: a factor 0.30 to 1.00, or a circuit count ("3 bunched").
    #[serde(default, deserialize_with = "nullable")]
    bundling: KeyValue,
    /// The load current (A).
    #[serde(default, deserialize_with = "nullable")]
    load: KeyValue,
    /// The protecting device: a tag such as -Q01, or -F01:3 for a fuse of a bank.
    #[serde(default, deserialize_with = "nullable")]
    protected_by: KeyValue,
    /// Cables: the IEC 60364-5-52 installation method.
    #[serde(default, deserialize_with = "nullable")]
    method: KeyValue,
    /// Cables: the ambient temperature (°C).
    #[serde(default, deserialize_with = "nullable")]
    ambient: KeyValue,
    /// Cables: the circuits grouped, 1 to 20.
    #[serde(default, deserialize_with = "nullable")]
    grouped: KeyValue,
    /// Cables: the voltage (V), for the voltage drop.
    #[serde(default, deserialize_with = "nullable")]
    voltage: KeyValue,
    /// Cables: ac1, ac3 or dc.
    #[serde(default, deserialize_with = "nullable")]
    system: KeyValue,
    /// Cables: the voltage drop limit (%).
    #[serde(default, deserialize_with = "nullable")]
    vd_limit: KeyValue,
    /// Cables: the cable type, an id or a name.
    #[serde(default, deserialize_with = "nullable")]
    cable_type: KeyValue,
}

/// `rating {wire | cable | many, …}` sets a subject's rating properties in one transaction (null
/// clears a key, an absent key is untouched); `{location, temperature}` sets a location's panel
/// temperature on its location boxes.
#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct RatingParams {
    /// A panel wire: its conductor id or wire number.
    #[serde(default)]
    wire: Option<String>,
    /// A cable: its tag or id.
    #[serde(default)]
    cable: Option<String>,
    /// Several subjects at once (at most 500), each `{wire | cable, …}`.
    #[serde(default)]
    many: Option<Bounded<RatingItemParams, 0, MAX_MANY>>,
    /// How the rating is found: datasheet, table or manual.
    #[serde(default, deserialize_with = "nullable")]
    basis: KeyValue,
    /// The rated current (A) when the basis is manual.
    #[serde(default, deserialize_with = "nullable")]
    manual: KeyValue,
    /// Panel wires: a factor 0.30 to 1.00, or a circuit count ("3 bunched").
    #[serde(default, deserialize_with = "nullable")]
    bundling: KeyValue,
    /// The load current (A).
    #[serde(default, deserialize_with = "nullable")]
    load: KeyValue,
    /// The protecting device: a tag such as -Q01, or -F01:3 for a fuse of a bank.
    #[serde(default, deserialize_with = "nullable")]
    protected_by: KeyValue,
    /// Cables: the IEC 60364-5-52 installation method.
    #[serde(default, deserialize_with = "nullable")]
    method: KeyValue,
    /// Cables: the ambient temperature (°C).
    #[serde(default, deserialize_with = "nullable")]
    ambient: KeyValue,
    /// Cables: the circuits grouped, 1 to 20.
    #[serde(default, deserialize_with = "nullable")]
    grouped: KeyValue,
    /// Cables: the voltage (V), for the voltage drop.
    #[serde(default, deserialize_with = "nullable")]
    voltage: KeyValue,
    /// Cables: ac1, ac3 or dc.
    #[serde(default, deserialize_with = "nullable")]
    system: KeyValue,
    /// Cables: the voltage drop limit (%).
    #[serde(default, deserialize_with = "nullable")]
    vd_limit: KeyValue,
    /// Cables: the cable type, an id or a name.
    #[serde(default, deserialize_with = "nullable")]
    cable_type: KeyValue,
    /// A location (such as +CAB1) whose panel temperature is set, with `temperature`.
    #[serde(default)]
    location: Option<String>,
    /// With `location`: the panel temperature (°C), or null to clear it.
    #[serde(default, deserialize_with = "nullable")]
    temperature: KeyValue,
}

/// `ratingtable {name, source?, csv, layer?}` creates or replaces a rating table (one
/// `SetStyle`); `{import: path, name?, source?}` reads a CSV file, `{export: path, name?}` writes
/// one.
#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct RatingtableParams {
    /// The table's name (default: the imported file's name, or "Rating table"); with `export`, the
    /// table to write (default: the one chosen).
    #[serde(default)]
    name: Option<String>,
    /// Where the values come from (at most 200 characters).
    #[serde(default)]
    source: Option<String>,
    /// The rows, as CSV.
    #[serde(default)]
    csv: Option<String>,
    /// project (the default) or standard.
    #[serde(default)]
    layer: Option<String>,
    /// Read the rows from this CSV file.
    #[serde(default)]
    import: Option<String>,
    /// Write the table to this CSV file.
    #[serde(default)]
    export: Option<String>,
}

/// `ratings {wire? | cable?, from?}` (read only): the results the panel shows, a page at a time.
#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct RatingsParams {
    /// Only this panel wire: its conductor id or wire number.
    #[serde(default)]
    wire: Option<String>,
    /// Only this cable: its tag or id.
    #[serde(default)]
    cable: Option<String>,
    /// Skip this many (the `next` of the last call).
    #[serde(default)]
    from: Option<u64>,
}

impl RatingItemParams {
    fn item(&self) -> RatingItem {
        RatingItem { wire: self.wire.clone(), cable: self.cable.clone(), sets: sets_of(keys_of!(self)) }
    }
}

impl RatingParams {
    fn single(&self) -> RatingItem {
        RatingItem { wire: self.wire.clone(), cable: self.cable.clone(), sets: sets_of(keys_of!(self)) }
    }

    /// The parameters given besides `location` and `temperature`, in name order.
    fn others(&self) -> Vec<&'static str> {
        let mut given: Vec<&'static str> = [("cable", self.cable.is_some()), ("many", self.many.is_some()), ("wire", self.wire.is_some())]
            .into_iter()
            .filter(|g| g.1)
            .map(|g| g.0)
            .collect();
        given.extend(self.single().sets.iter().map(|(k, _)| *k));
        given.sort_unstable();
        given
    }
}

impl RatingParams {
    /// A location's panel temperature as given: the location and the value (`None`: cleared).
    fn location_temperature(&self, loc: &str) -> Result<(String, Option<String>), CmdError> {
        let typed = loc.trim().to_string();
        if typed.is_empty() {
            return Err(CmdError::refused("`location` names a location, such as +CAB1."));
        }
        if let Some(k) = self.others().first() {
            return Err(CmdError::refused(format!("{k} is not a location rating key; give `temperature`.")));
        }
        let after = match &self.temperature {
            Some(None) => None,
            Some(Some(t)) => Some(check_temperature(&t.text())?),
            None => return Err(CmdError::refused("Give `temperature` (\u{b0}C), or null to clear it.")),
        };
        Ok((typed, after))
    }
}

impl Generic for RatingParams {
    const ID: &'static str = "rating";
    const TITLE: &'static str = "Rating";
    const HELP: &'static str = "{wire: conductor id or wire number | cable: tag or id | many: [{wire|cable, …}], basis?: \"datasheet\"|\"table\"|\"manual\", \
manual?, bundling?: \"0.8\"|\"3 bunched\", load?, protected_by?: \"-Q01\"|\"-F01:3\", method?, ambient?, grouped?, voltage?, system?: \
\"ac1\"|\"ac3\"|\"dc\", vd_limit?, cable_type?: id or name} | {location: \"+CAB1\", temperature: °C | null} (a location's panel \
temperature, on its location boxes; null clears; one transaction)";

    fn check(&self) -> Result<(), CmdError> {
        match &self.location {
            Some(loc) => self.location_temperature(loc).map(|_| ()),
            None if self.temperature.is_some() => Err(not_a_key("temperature")),
            None => Ok(()),
        }
    }

    fn run(self, cx: &mut Cx<'_>) -> Result<Outcome, CmdError> {
        self.check()?;
        if let Some(loc) = &self.location {
            let (typed, after) = self.location_temperature(loc)?;
            let planned = plan_temperature(cx.doc, &typed, after.as_deref())?;
            let changed = !planned.ops.is_empty();
            let answer = json!({"changed": changed, "location": planned.location, "boxes": planned.boxes, "temperature": after});
            return Ok(Outcome::edit_if_any(planned.label, planned.ops, answer));
        }
        let items: Vec<RatingItem> = match &self.many {
            Some(many) => many.0.iter().map(RatingItemParams::item).collect(),
            None => vec![self.single()],
        };
        let planned = plan_rating(cx.doc, &items)?;
        let changed = !planned.ops.is_empty();
        let subjects = planned.subjects;
        Ok(Outcome::edit_then(planned.label, planned.ops, move |doc| {
            let results: Vec<Value> = subjects.iter().take(PAGE).map(|s| rating_json(doc, &rating::rating_of(doc, *s))).collect();
            json!({"changed": changed, "ratings": results})
        }))
    }
}

impl RatingtableParams {
    /// Refuse a call that gives neither rows nor a file, or both.
    fn source_given(&self) -> Result<(), CmdError> {
        if self.export.is_some() {
            return Ok(());
        }
        match (&self.import, &self.csv) {
            (Some(_), Some(_)) => Err(CmdError::refused("Give `csv` or `import`, not both.")),
            (None, None) => Err(CmdError::refused("Give `csv` (the rows), `import` (a CSV file) or `export` (a CSV file).")),
            _ => Ok(()),
        }
    }
}

impl Generic for RatingtableParams {
    const ID: &'static str = "ratingtable";
    const TITLE: &'static str = "Rating Table";
    const HELP: &'static str = "{name, source?, csv, layer?: \"project\"|\"standard\"} (creates or replaces; one transaction) | {import: path, name?, source?} | {export: path, name?}";
    const PAGES: Pages = Pages::ReadAnywhere;
    const IO: Io = Io { reads: &[("import", Read::Bytes(dataset::MAX_BYTES))], writes: &["export"] };

    fn check(&self) -> Result<(), CmdError> {
        self.source_given()
    }

    fn run(self, cx: &mut Cx<'_>) -> Result<Outcome, CmdError> {
        let trim = |v: &Option<String>| v.as_deref().map(|t| t.trim().to_string());
        let name = trim(&self.name);
        if self.export.is_some() {
            let t = export_table(cx.doc, name.as_deref())?;
            let mut out = Outcome::answer(json!({"exported": trim(&self.export), "rows": t.data.rows.len()}));
            out.writes.push(("export", dataset::to_csv(&t.data).into_bytes()));
            return Ok(out);
        }
        self.source_given()?;
        let (csv, from_file) = match (trim(&self.import), self.csv) {
            (Some(path), _) => {
                let bytes = match cx.read.get("import") {
                    Some(Ok(Got::Bytes(b))) => b,
                    Some(Err(e)) => return Err(CmdError::refused(format!("Could not read {path}: {e}"))),
                    Some(Ok(Got::Standard(_))) | None => return Err(CmdError::refused(format!("Could not read {path}"))),
                };
                if bytes.len() > dataset::MAX_BYTES {
                    return Err(too_big());
                }
                let text = String::from_utf8(bytes.clone())
                    .map_err(|_| CmdError::refused(format!("Could not read {path} as text: stream did not contain valid UTF-8")))?;
                (text, Some(path))
            }
            (None, c) => (c.unwrap_or_default(), None),
        };
        let default_name = from_file
            .as_deref()
            .and_then(|f| std::path::Path::new(f).file_stem().and_then(|s| s.to_str()))
            .map(str::to_string)
            .unwrap_or_else(|| "Rating table".to_string());
        let w = TableWrite { csv, name: name.unwrap_or(default_name), source: trim(&self.source), layer: StyleLayer::Project };
        // The rows, name and source are checked before the layer (as before).
        check_table(&w)?;
        let w = TableWrite { layer: table_layer(self.layer.as_deref())?, ..w };
        let plan = plan_table(cx.doc, cx.ids, &w)?;
        let changed = !plan.ops.is_empty();
        let answer = json!({"id": plan.id.text(), "name": w.name, "rows": plan.rows, "changed": changed, "odd": plan.odd});
        Ok(Outcome::edit_if_any(plan.label, plan.ops, answer))
    }
}

impl Generic for RatingsParams {
    const ID: &'static str = "ratings";
    const TITLE: &'static str = "Ratings";
    const HELP: &'static str = "{wire?: id or number, cable?: tag or id, from?: n} (read only)";
    const PAGES: Pages = Pages::Any;
    const ACCESS: Access = Access::Reads;

    fn run(self, cx: &mut Cx<'_>) -> Result<Outcome, CmdError> {
        let from = usize::try_from(self.from.unwrap_or(0)).unwrap_or(usize::MAX);
        let doc = cx.doc;
        let subjects: Vec<RatedSubject> = if self.wire.is_some() || self.cable.is_some() {
            subjects_of(doc, self.wire.as_deref(), self.cable.as_deref())?
        } else {
            let ctx = rating::Context::new(doc);
            rating::subjects(doc, &ctx)
        };
        let total = subjects.len();
        let mut ctx = rating::Context::new(doc);
        let out: Vec<Value> = subjects
            .iter()
            .skip(from)
            .take(PAGE)
            .map(|s| {
                let found = rating::trace::trace_subject(doc, *s);
                rating_json(doc, &rating::evaluate(doc, &mut ctx, *s, &found))
            })
            .collect();
        let next = (from.saturating_add(PAGE) < total).then_some(from.saturating_add(PAGE));
        Ok(Outcome::answer(json!({"ratings": out, "total": total, "next": next})))
    }
}
