//! The output commands' rules (brief 024, design 024 §3.5, §6.4; briefs 047, 052d): location and
//! function boxes and the proposal they make for the devices inside, and the report parameters
//! (`bom`'s grouping and columns, the paging), and the JSON answers: a box's, and each report's
//! row ids and column keys, read from the kept outputs. Writing a report's CSV file is the engine
//! crate's.

use std::collections::BTreeSet;

use ecad_model::{Document, IdAllocator, ItemId, Op, PageId, Pt};
use serde_json::{Value, json};

use crate::error::CmdError;
use crate::locations;
use crate::locations::designations::{Aspect, Pending};
use crate::reports::kept::ReportModels;
use crate::reports::{self, Group, Table};
use crate::symbols::proposals::{self, Mode};

/// A rectangle's four corners from two opposite ones.
pub fn rectangle(a: Pt, b: Pt) -> Vec<Pt> {
    vec![
        Pt::new(a.x.min(b.x), a.y.min(b.y)),
        Pt::new(a.x.max(b.x), a.y.min(b.y)),
        Pt::new(a.x.max(b.x), a.y.max(b.y)),
        Pt::new(a.x.min(b.x), a.y.max(b.y)),
    ]
}

/// The echo of a new box: `Location box +CP1: 4 devices inside, 3 now in +CP1 (1 keeps its own +CP2).`
pub fn box_words(location: &str, inside: usize, keeps: usize) -> String {
    let devices = if inside == 1 { "1 device".to_string() } else { format!("{inside} devices") };
    // Brief 052: a box proposes; it never changes a device's stored location by itself.
    if keeps == 0 {
        format!("Location box {location}: {devices} inside.")
    } else {
        format!("Location box {location}: {devices} inside ({keeps} with another location).")
    }
}

/// A box's outline as given.
#[derive(Debug, Clone, PartialEq)]
pub enum Shape {
    /// Two opposite corners of a rectangle.
    Corners(Pt, Pt),
    /// A closed outline.
    Points(Vec<Pt>),
    /// A closed polyline of the page.
    Item(ItemId),
}

impl Shape {
    /// The one outline given.
    ///
    /// # Errors
    /// None or more than one given.
    pub fn one_of(corners: Option<(Pt, Pt)>, points: Option<Vec<Pt>>, item: Option<ItemId>) -> Result<Shape, CmdError> {
        match (corners, points, item) {
            (Some((a, b)), None, None) => Ok(Shape::Corners(a, b)),
            (None, Some(v), None) => Ok(Shape::Points(v)),
            (None, None, Some(i)) => Ok(Shape::Item(i)),
            _ => Err(CmdError::input("give exactly one of `corners` (two corners), `points` (a closed outline) or `item` (a closed polyline)")),
        }
    }
}

/// What a box command records and says.
#[derive(Debug)]
pub struct BoxPlan {
    /// The box's ops, then the proposal's.
    pub ops: Vec<Op>,
    /// The box.
    pub planned: locations::Planned,
    /// What it proposes for the device symbols inside.
    pub proposed: proposals::Planned,
    /// "Function box +F1 =CP1" and so on: the transaction's label.
    pub label: String,
    /// How many devices are inside.
    pub inside: usize,
    /// How many of them keep another value.
    pub keeps: usize,
    /// The echo, in words.
    pub message: String,
}

/// A box (design 024 §7, design 052 §5.6, §5.8) on `page` carrying `function` and `location`
/// (at least one), drawn as `shape`, and the proposal it makes for the device symbols inside.
/// Unset devices take its values in the same transaction; devices with another value are listed as
/// conflicts (`mode`: propose, move or keep).
///
/// # Errors
/// No such page or item, an outline that is not a box, a value that is not a valid code, or
/// neither value given.
pub fn plan_box(
    doc: &ecad_model::Document, ids: &mut IdAllocator, page: PageId, function: Option<&str>, location: Option<&str>, shape: Shape, mode: &Mode,
) -> Result<BoxPlan, CmdError> {
    let mut values: Vec<(Aspect, &str)> = Vec::new();
    if let Some(f) = function {
        values.push((Aspect::Function, f));
    }
    if let Some(l) = location {
        values.push((Aspect::Location, l));
    }
    let planned = match shape {
        Shape::Corners(a, b) => locations::plan_box_with(doc, ids, page, &values, rectangle(a, b))?,
        Shape::Points(v) => locations::plan_box_with(doc, ids, page, &values, v)?,
        Shape::Item(i) => locations::plan_from_item_with(doc, ids, i, &values)?,
    };
    let b = locations::planned_box(doc, &planned).ok_or(CmdError::refused("the box could not be read back"))?;
    let shown = locations::label_of(&b).0;
    // Brief 052d: the box proposes its values for the device symbols inside it.
    let spots = proposals::spots_inside(doc, &b);
    let modes = [if b.function_key.is_some() { mode.clone() } else { Mode::Keep }, if b.location_key.is_some() { mode.clone() } else { Mode::Keep }];
    let mut pending = Pending::default();
    let proposed = proposals::plan(doc, &planned.ops, ids, &spots, &modes, std::slice::from_ref(&b), &mut pending)?;
    let inside: BTreeSet<_> = spots.iter().map(|s| s.device).collect();
    let mut ops = planned.ops.clone();
    ops.extend(proposed.ops.iter().cloned());
    let kind = if b.function_key.is_some() { "Function box" } else { "Location box" };
    let set: BTreeSet<_> = proposed.set.iter().map(|(c, _, _)| *c).collect();
    let keeps: BTreeSet<_> = proposed.conflicts.iter().map(|c| c.component).collect();
    let n = inside.len();
    let devices = if n == 1 { "1 device".to_string() } else { format!("{n} devices") };
    let mut message = format!("{kind} {shown}: {devices} inside");
    if !set.is_empty() || !keeps.is_empty() {
        let same = n.saturating_sub(set.len() + keeps.len());
        message.push_str(&format!("; {} set to {shown}, {same} already {shown}, {} kept", set.len(), keeps.len()));
    }
    message.push('.');
    if let Some(a) = &planned.added {
        message = format!("{message} {a}");
    }
    Ok(BoxPlan { ops, label: format!("{kind} {shown}"), planned, proposed, inside: n, keeps: keeps.len(), message })
}

/// `bom`'s `group`: "location" (the default) or "part".
///
/// # Errors
/// Any other word.
pub fn bom_group(word: Option<&str>) -> Result<Group, CmdError> {
    match word {
        None => Ok(Group::Location),
        Some(t) => Group::parse(t).ok_or(CmdError::refused("`group` is \"location\" or \"part\"")),
    }
}

/// `bom`'s `columns` for this call (brief 047, design 047 §7.4): whether the article column is
/// shown (`None`: as the project's report settings say).
///
/// # Errors
/// A column that is not optional.
pub fn bom_columns(columns: Option<&[String]>) -> Result<Option<bool>, CmdError> {
    let Some(cols) = columns else { return Ok(None) };
    let mut article = false;
    for c in cols {
        match c.as_str() {
            "article" => article = true,
            _ => return Err(CmdError::refused("`columns` lists optional columns: [\"article\"]")),
        }
    }
    Ok(Some(article))
}

/// The JSON a box answers (design 024 §7, design 052 §5.6): the box, the proposal it made for the
/// device symbols inside, and that proposal's conflicts.
pub fn box_json(doc: &Document, plan: &BoxPlan) -> Value {
    let (planned, proposed) = (&plan.planned, &plan.proposed);
    let mut v = json!({
        "aid": planned.aid.hex(), "representation": planned.rep.hex(), "item": planned.item.hex(),
        "inside": plan.inside, "keeps_own": plan.keeps, "message": plan.message,
    });
    if let (Some(o), Some(x)) = (v.as_object_mut(), proposals::json(doc, proposed).as_object()) {
        o.extend(x.clone());
    }
    let conflicts: Vec<Value> = proposed
        .conflicts
        .iter()
        .map(|c| json!({"component": c.component.hex(), "tag": crate::devices::tag_words(doc, c.component), "aspect": c.aspect.word(), "from": c.from, "proposed": c.to}))
        .collect();
    v["conflicts"] = json!(conflicts);
    v
}

/// `limit`, `after`, `excel_safe` and `path` of a report command, read.
pub struct Paging {
    pub limit: usize,
    pub after: usize,
    pub excel_safe: bool,
    pub path: Option<String>,
}

impl Paging {
    /// The paging a report call asks for: `limit` clamped to 1..=[`reports::MAX_LIMIT`]
    /// (default [`reports::DEFAULT_LIMIT`]), `after` 0 and `excel_safe` true by default.
    pub fn of(limit: Option<u64>, after: Option<u64>, excel_safe: Option<bool>, path: Option<String>) -> Paging {
        Paging {
            limit: limit.map_or(reports::DEFAULT_LIMIT, |n| usize::try_from(n).unwrap_or(usize::MAX)).clamp(1, reports::MAX_LIMIT),
            after: after.map_or(0, |n| usize::try_from(n).unwrap_or(usize::MAX)),
            excel_safe: excel_safe.unwrap_or(true),
            path,
        }
    }
}

/// The ids of each `bom` row (`group`, kits `nested`): its devices, an accessory's item and strip
/// (brief 030, design 030 §10) and what the row counts (brief 047).
pub fn bom_ids_json(o: &ReportModels, group: Group, nested: bool) -> Vec<Value> {
    o.bom_with(group, nested)
        .iter()
        .map(|r| {
            let mut v = json!({"devices": r.devices.iter().map(|c| c.hex()).collect::<Vec<_>>()});
            // Brief 030 (design 030 §10): an accessory row names its item and its strip.
            if let (Some(a), Some(c)) = (r.accessory, r.devices.first()) {
                v["accessory"] = json!(a);
                v["strip"] = json!(c.hex());
            }
            // Brief 047: what the row counts.
            let entries: Vec<Value> = r
                .entries
                .iter()
                .filter_map(|(c, n)| o.entry(*c, *n))
                .map(|e| {
                    // Design 072 §6.4: an item is a loose part, named by its own id.
                    let mut x = match e.owner {
                        crate::reports::Owner::Device(c) => json!({"component": c.hex(), "line": e.line}),
                        crate::reports::Owner::Loose(l) => json!({"item": l.hex()}),
                    };
                    if !e.subs.is_empty() || !e.missing.is_empty() {
                        x["sub"] = json!(e.subs);
                    }
                    x
                })
                .collect();
            v["entries"] = json!(entries);
            if nested && let Some(k) = &r.kit {
                v["kit"] = json!(k);
            }
            v
        })
        .collect()
}

/// The JSON keys of `bom`'s columns: with a `line` column when kits are `nested`, and the article
/// number when the `article` column is shown.
pub fn bom_keys(nested: bool, article: bool) -> Vec<&'static str> {
    let mut keys = vec!["location", "qty", "unit", "tags"];
    if nested {
        keys.push("line");
    }
    keys.extend(["description", "maker", "part_number"]);
    if article {
        keys.push("article_number");
    }
    keys
}

/// The JSON keys of `wirelist`'s columns.
pub const WIRE_KEYS: &[&str] =
    &["wire_number", "from", "from_location", "to", "to_location", "cross_section", "colour", "potential", "category", "cable", "page"];

/// The ids of each `wirelist` row.
pub fn wire_ids_json(o: &ReportModels) -> Vec<Value> {
    o.wires().iter().map(|r| json!({"conductor": r.conductor.hex()})).collect()
}

/// The ids of each `cablelist` row (the cables not drawn too when `include`).
///
/// Brief 028 (design 028 §9.2): `used` (a count), `spare` (the spare cores, or null when the offer
/// is unknown) and the devices at each end; these keys are 027's CSV columns.
pub fn cable_ids_json(o: &ReportModels, include: bool) -> Vec<Value> {
    o.cables()
        .iter()
        .filter(|r| include || r.drawn)
        .map(|r| {
            let mut v = json!({
                "cable": r.cable.hex(), "used": r.count.used(), "over": r.count.over.len(),
                "from_devices": r.from_devices, "to_devices": r.to_devices,
            });
            // `spare` only when the offer is known (absent keys are left out, not null).
            if r.count.offer.is_some() {
                v["spare"] = json!(r.count.spare);
            }
            v
        })
        .collect()
}

/// The JSON keys of `cablelist`'s columns in table `t`: the nine fixed ones, then one per optional
/// column it shows.
pub fn cable_keys(t: &Table) -> Vec<&'static str> {
    let mut keys = vec!["tag", "type", "cores", "from_location", "to_location", "used_text", "spare_text", "from", "to"];
    for h in t.headers.iter().skip(9) {
        keys.push(match *h {
            "Maker" => "maker",
            "Length" => "length",
            "Glands" => "glands",
            "Shield" => "shield",
            "Planned from" => "planned_from",
            "Planned to" => "planned_to",
            "Drawn" => "drawn",
            _ => "designation",
        });
    }
    keys
}
