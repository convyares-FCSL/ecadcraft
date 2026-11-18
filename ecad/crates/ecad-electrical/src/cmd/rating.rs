//! The rules of the rating commands (brief 057, design 057 §8): which wires or cable `rating` and
//! `ratings` name, the values `rating` sets, a location's panel temperature, and the record
//! `ratingtable` writes. The engine crate's shims read the files, commit the ops and answer in
//! JSON.

use ecad_model::style::{StyleLayer, StyleRecord, StyleRef, is_name};
use ecad_model::units::Amps;
use ecad_model::{CableId, DeviceId, Document, IdAllocator, ObjectRef, Op, Wire, WireId, Word};

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
        "protected_by" => match rating::trace::named(doc, v) {
            Some((c, fuse)) => {
                let tag = rating::trace::device_words(doc, c, fuse.as_deref());
                if rating::trace::amps_of(doc, c, fuse.as_deref()).is_none() {
                    return Err(CmdError::refused(format!("{tag}'s rating has no current; write it as 16 A first.")));
                }
                Ok(tag)
            }
            None => Err(CmdError::refused(format!("There is no device or fuse {shown:?}; give a tag such as -Q01, or -F01:3 for a fuse of a bank."))),
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
        "cable_type" => styles::find(doc, cable::WORD, v).map(|id| id.text()),
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
                    set_wire(&mut after, key, value.as_deref());
                }
                if after != *w {
                    ops.push(Op::ModifyWire { id: *c, before: w.clone(), after });
                }
            }
            RatedSubject::Cable(k) => {
                let props = doc.cables.get(k).map(|x| &x.properties);
                for (_, key, after) in &sets {
                    let before = props.and_then(|p| p.get(*key)).cloned();
                    if before != *after {
                        ops.push(Op::SetProperty { object: s.object(), key: (*key).to_string(), before, after: after.clone() });
                    }
                }
            }
        }
    }
    Ok((ops, subjects))
}

/// Set a wire's input `key` (a JSON key of [`KEYS`] for wires) to `value`, already checked by
/// [`check_value`] (`None` clears it).
pub fn set_wire(w: &mut Wire, key: &str, value: Option<&str>) {
    match key {
        "basis" => w.rating.basis = value.and_then(Basis::parse),
        "manual" => w.rating.manual = value.and_then(parse::current).map(Amps),
        "bundling" => w.rating.bundling = value.and_then(parse::bundling),
        "load" => w.load = value.and_then(parse::current).map(Amps),
        "protected_by" => {
            match value {
                Some(v) => w.display.insert(rating::PROTECTED_BY.into(), v.to_string()),
                None => w.display.remove(rating::PROTECTED_BY),
            };
        }
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
/// code or an entry key) take `rating.temperature` (`None` clears it).
///
/// # Errors
/// No location box of that location.
pub fn plan_temperature(doc: &Document, typed: &str, after: Option<&str>) -> Result<PanelTemperature, CmdError> {
    // Brief 052: `location` is a code (or an entry key); boxes store the entry's key.
    let entry = crate::designations::find(doc, crate::designations::Aspect::Location, typed);
    let loc = entry.as_ref().map_or_else(|| typed.to_string(), |e| e.code.clone());
    let boxes: Vec<DeviceId> = match &entry {
        None => Vec::new(),
        Some(e) => doc
            .devices
            .iter()
            .filter(|(_, x)| {
                crate::locations::is_box_component(x)
                    && crate::designations::device_key(x, crate::designations::Aspect::Location).is_some_and(|l| l == e.key)
            })
            .map(|(c, _)| *c)
            .collect(),
    };
    if boxes.is_empty() {
        return Err(CmdError::refused(format!("No location box is {loc}; draw one with Location Box first.")));
    }
    let after = after.map(str::to_string);
    let mut ops = Vec::new();
    for c in &boxes {
        let before = doc.devices.get(c).and_then(|x| x.properties.get(rating::TEMPERATURE)).cloned();
        if before != after {
            ops.push(Op::SetProperty { object: ObjectRef::Device(*c), key: rating::TEMPERATURE.into(), before, after: after.clone() });
        }
    }
    let words = after.as_deref().map_or_else(|| "project default".to_string(), |t| format!("{t} \u{b0}C"));
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
        Some(n) => styles::find(doc, ratingtable::WORD, n)?,
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
    let before = styles::record(doc, w.layer, cat.word, &id).cloned();
    let mut rec = before.clone().unwrap_or(StyleRecord { name: w.name.clone(), fields: Default::default(), body: None });
    if rec.name.is_empty() && existing.is_none() {
        rec.name = w.name.clone();
    }
    rec.fields.insert(ratingtable::ROWS.into(), w.csv.clone());
    if let Some(src) = &w.source {
        rec.fields.insert(ratingtable::SOURCE.into(), src.clone());
    }
    styles::check_record(doc, cat, &id, w.layer, &rec)?;
    let rows = data.rows.len();
    let label = format!("Rating table {}: {rows} row{}", w.name, if rows == 1 { "" } else { "s" });
    let ops = if before.as_ref() == Some(&rec) {
        Vec::new()
    } else {
        vec![Op::SetStyle { layer: w.layer, catalogue: cat.word.into(), id: id.clone(), before, after: Some(rec) }]
    };
    Ok(TablePlan { ops, id, rows, odd: data.odd(), label })
}
