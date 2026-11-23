//! The report lists: `bom`, `wirelist` and `cablelist`
//! read the kept report models and answer a page of rows, or write the CSV. Their parameters, the
//! paging, each row's ids and each column's JSON key sit here; the commands are generic
//! ([`crate::commands::run`]). Excludes the Print Book, which is the engine's.

use schemars::JsonSchema;
use serde::Deserialize;
use serde_json::{Value, json};

use crate::commands::run::{Access, Cx, Generic, Io, Outcome, Pages};
use crate::error::CmdError;
use crate::reports::kept::{CableColumn, ReportModels};
use crate::reports::{self, Group, Table};

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

/// `bom`'s `columns` for this call: whether the article column is
/// shown (`None`: as the project's report settings say).
///
/// # Errors
/// A column that is not optional.
pub(crate) fn bom_columns(columns: Option<&[String]>) -> Result<Option<bool>, CmdError> {
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

/// `limit`, `after`, `excel_safe` and `path` of a report command, read.
pub(crate) struct Paging {
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
/// and what the row counts.
pub(crate) fn bom_ids_json(o: &ReportModels, group: Group, nested: bool) -> Vec<Value> {
    o.bom_with(group, nested)
        .iter()
        .map(|r| {
            let mut v = json!({"devices": r.devices.iter().map(|c| c.hex()).collect::<Vec<_>>()});
            // An accessory row names its item and its strip.
            if let (Some(a), Some(c)) = (r.accessory, r.devices.first()) {
                v["accessory"] = json!(a);
                v["strip"] = json!(c.hex());
            }
            // What the row counts.
            let entries: Vec<Value> = r
                .entries
                .iter()
                .filter_map(|(c, n)| o.entry(*c, *n))
                .map(|e| {
                    // An item is a loose part, named by its own id.
                    let mut x = match e.owner {
                        reports::Owner::Device(c) => json!({"component": c.hex(), "line": e.line}),
                        reports::Owner::Loose(l) => json!({"item": l.hex()}),
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
pub(crate) fn bom_keys(nested: bool, article: bool) -> Vec<&'static str> {
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
pub(crate) const WIRE_KEYS: &[&str] =
    &["wire_number", "from", "from_location", "to", "to_location", "cross_section", "colour", "potential", "category", "cable", "page"];

/// The ids of each `wirelist` row.
pub(crate) fn wire_ids_json(o: &ReportModels) -> Vec<Value> {
    o.wires().iter().map(|r| json!({"conductor": r.conductor.hex()})).collect()
}

/// The ids of each `cablelist` row (the cables not drawn too when `include`).
///
/// `used` (a count), `spare` (the spare cores, or null when the offer
/// is unknown) and the devices at each end; these keys are the CSV columns.
pub(crate) fn cable_ids_json(o: &ReportModels, include: bool) -> Vec<Value> {
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

/// The JSON keys of `cablelist`'s columns: the nine fixed ones, then one per optional column it shows.
pub(crate) fn cable_keys(columns: &[CableColumn]) -> Vec<&'static str> {
    let mut keys = vec!["tag", "type", "cores", "from_location", "to_location", "used_text", "spare_text", "from", "to"];
    keys.extend(columns.iter().map(|c| c.key()));
    keys
}

// ---------------------------------------------------------------------------------------------
// The commands

/// A report's result: the page of rows asked for, or with `path` the CSV written.
fn report_result(t: &Table, keys: &[&str], ids: &[Value], pg: Paging, what: &str) -> Outcome {
    let flagged = t.flagged();
    if let Some(path) = &pg.path {
        let text = reports::csv(t, pg.excel_safe);
        let marked = if flagged == 1 { "1 cell marked ?".to_string() } else { format!("{flagged} cells marked ?") };
        let message = format!("Wrote {} {what} rows to {path} ({marked}).", t.rows.len());
        let answer = json!({ "path": path, "rows": t.rows.len(), "flagged": flagged, "message": message });
        let mut out = Outcome::answer(answer);
        out.writes.push(("path", text.into_bytes()));
        return out;
    }
    let all = reports::rows_json(t, keys, ids);
    let total = all.len();
    let rows: Vec<Value> = all.into_iter().skip(pg.after).take(pg.limit).collect();
    let mut v = json!({ "rows": rows, "total": total, "flagged": flagged });
    if pg.after.saturating_add(pg.limit) < total {
        v["next"] = json!(pg.after + pg.limit);
    }
    Outcome::answer(v)
}

/// The file a report writes.
const CSV: Io = Io { reads: &[], writes: &["path"] };

/// How `bom` lists a kit's lines.
#[derive(Deserialize, JsonSchema, Clone, Copy, PartialEq)]
#[serde(rename_all = "lowercase")]
enum Lines {
    /// One row per part.
    Flat,
    /// A kit's parts under its row.
    Nested,
}

/// `bom {group?, columns?, lines?, path?, excel_safe?, limit?, after?}` (read only): the bill of
/// materials, a page at a time, or written to a CSV file.
#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct BomParams {
    /// "location" (default) or "part".
    #[serde(default)]
    group: Option<String>,
    /// Optional columns for this call: ["article"].
    #[serde(default)]
    columns: Option<Vec<String>>,
    /// "flat" or "nested" for this call.
    #[serde(default)]
    lines: Option<Lines>,
    /// Write the CSV to this file instead.
    #[serde(default)]
    path: Option<String>,
    /// Guard cells a spreadsheet would read as a formula (default true).
    #[serde(default)]
    excel_safe: Option<bool>,
    /// At most this many rows (default 500, at most 5000).
    #[serde(default)]
    limit: Option<u64>,
    /// Skip this many rows (the `next` a previous call gave).
    #[serde(default)]
    after: Option<u64>,
}

impl Generic for BomParams {
    const ID: &'static str = "bom";
    const TITLE: &'static str = "Bill of Materials";
    const HELP: &'static str = "{group?: \"location\"|\"part\", columns?: [\"article\"], lines?: \"flat\"|\"nested\", path?: csv file, excel_safe?: bool (true), \
limit?: n (500, max 5000), after?: n} (columns and lines override the project's report settings for this call; nothing written)";
    const PAGES: Pages = Pages::Any;
    const ACCESS: Access = Access::Reads;
    const IO: Io = CSV;

    fn run(self, cx: &mut Cx<'_>) -> Result<Outcome, CmdError> {
        let group = bom_group(self.group.as_deref())?;
        // The report settings for this call only.
        let columns = bom_columns(self.columns.as_deref())?;
        let lines = self.lines.map(|l| l == Lines::Nested);
        let pg = Paging::of(self.limit, self.after, self.excel_safe, self.path);
        let o = cx.derived.reports(cx.doc);
        let view = reports::BomView { article: columns.unwrap_or(o.view.article), nested: lines.unwrap_or(o.view.nested) };
        let nested = view.nested && group == Group::Location;
        let t = o.bom_table_with(group, view, true);
        let ids = bom_ids_json(o, group, nested);
        let keys = bom_keys(nested, view.article);
        Ok(report_result(&t, &keys, &ids, pg, "BOM"))
    }
}

/// `wirelist {path?, excel_safe?, limit?, after?}` (read only): the wire report.
#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct WirelistParams {
    /// Write the CSV to this file instead.
    #[serde(default)]
    path: Option<String>,
    /// Guard cells a spreadsheet would read as a formula (default true).
    #[serde(default)]
    excel_safe: Option<bool>,
    /// At most this many rows (default 500, at most 5000).
    #[serde(default)]
    limit: Option<u64>,
    /// Skip this many rows (the `next` a previous call gave).
    #[serde(default)]
    after: Option<u64>,
}

impl Generic for WirelistParams {
    const ID: &'static str = "wirelist";
    const TITLE: &'static str = "Wire Report";
    const HELP: &'static str = "{path?: csv file, excel_safe?: bool (true), limit?: n (500, max 5000), after?: n}";
    const PAGES: Pages = Pages::Any;
    const ACCESS: Access = Access::Reads;
    const IO: Io = CSV;

    fn run(self, cx: &mut Cx<'_>) -> Result<Outcome, CmdError> {
        let pg = Paging::of(self.limit, self.after, self.excel_safe, self.path);
        let o = cx.derived.reports(cx.doc);
        let t = o.wire_table();
        Ok(report_result(&t, WIRE_KEYS, &wire_ids_json(o), pg, "wire"))
    }
}

/// `cablelist {path?, excel_safe?, limit?, after?, include_not_drawn?}` (read only): the cable
/// report.
#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct CablelistParams {
    /// Write the CSV to this file instead.
    #[serde(default)]
    path: Option<String>,
    /// Guard cells a spreadsheet would read as a formula (default true).
    #[serde(default)]
    excel_safe: Option<bool>,
    /// At most this many rows (default 500, at most 5000).
    #[serde(default)]
    limit: Option<u64>,
    /// Skip this many rows (the `next` a previous call gave).
    #[serde(default)]
    after: Option<u64>,
    /// Also the cables not drawn (default true).
    #[serde(default)]
    include_not_drawn: Option<bool>,
}

impl Generic for CablelistParams {
    const ID: &'static str = "cablelist";
    const TITLE: &'static str = "Cable Report";
    const HELP: &'static str =
        "{path?: csv file, excel_safe?: bool (true), limit?: n (500, max 5000), after?: n, include_not_drawn?: bool (true; brief 059)}";
    const PAGES: Pages = Pages::Any;
    const ACCESS: Access = Access::Reads;
    const IO: Io = CSV;

    fn run(self, cx: &mut Cx<'_>) -> Result<Outcome, CmdError> {
        let pg = Paging::of(self.limit, self.after, self.excel_safe, self.path);
        // The cables not drawn are listed unless this run says not.
        let include = self.include_not_drawn.unwrap_or(true);
        let o = cx.derived.reports(cx.doc);
        let t = o.cable_table_with(include);
        let ids = cable_ids_json(o, include);
        let keys = cable_keys(&o.cable_columns(include));
        Ok(report_result(&t, &keys, &ids, pg, "cable"))
    }
}
