//! The commands of brief 024 (design 024 §3.5, §6.4): `locationbox` (writes), and `bom`,
//! `wirelist`, `cablelist`, `printbook` and `reports`, which read the project and record nothing.
//!
//! Each command is a thin shim (brief 070): its params struct reads the JSON, the rules in
//! [`ecad_electrical::cmd::output`] decide, and the shim commits the ops and answers in JSON.

use cadcraft_engine::{CommandSpec, Session};
use ecad_electrical::CmdError;
use ecad_electrical::cmd::output::{self as rules, Paging};
use ecad_model::{FolderId, ItemId, PageId};
use schemars::JsonSchema;
use serde::Deserialize;
use serde_json::{Value, json};

use crate::cmd::input::{Command, Typed};
use crate::cmd::{err, on_any_page, on_project};
use crate::outputs::{self, Group, Table};
use crate::printbook::{self, Options, Scope};
use crate::project::Project;
use ecad_electrical::cmd::input::{Bounded, Point};

/// The commands of this module with a typed input.
pub const TYPED: &[Typed] = &[
    Typed::of::<LocationboxParams>("locationbox"),
    Typed::of::<FunctionboxParams>("functionbox"),
    Typed::of::<BomParams>("bom"),
    Typed::of::<WirelistParams>("wirelist"),
    Typed::of::<CablelistParams>("cablelist"),
    Typed::of::<PrintbookParams>("printbook"),
    Typed::of::<ReportsParams>("reports"),
];

/// The commands this module adds.
pub fn specs() -> Vec<CommandSpec> {
    vec![
        CommandSpec::new("locationbox", "Location Box", crate::cmd::input::typed::<LocationboxParams>)
            .params(
                "{location: \"+CP1\", function?: \"=F1\", corners?: [[x,y],[x,y]] | points?: [[x,y], …] | item?: id (a closed polyline), page?: id, proposal?: \"propose\" | \"move\" | \"keep\"} (exactly one of corners, points, item; default the active page; brief 052: it proposes its values for the devices inside)",
            )
            .interactive(crate::interact::locationbox)
            .noundo(),
        CommandSpec::new("functionbox", "Function Box", crate::cmd::input::typed::<FunctionboxParams>)
            .params(
                "{function: \"=F1\", location?: \"+CP1\", corners?: [[x,y],[x,y]] | points?: [[x,y], …] | item?: id, page?: id, proposal?: \"propose\" | \"move\" | \"keep\"} (brief 052: drawn dash-dot; it proposes its values for the devices inside)",
            )
            .interactive(crate::interact::functionbox)
            .noundo(),
        CommandSpec::new("bom", "Bill of Materials", crate::cmd::input::typed::<BomParams>)
            .params(
                "{group?: \"location\"|\"part\", columns?: [\"article\"], lines?: \"flat\"|\"nested\", path?: csv file, excel_safe?: bool (true), \
limit?: n (500, max 5000), after?: n} (columns and lines override the project's report settings for this call; nothing written)",
            )
            .noundo(),
        CommandSpec::new("wirelist", "Wire Report", crate::cmd::input::typed::<WirelistParams>)
            .params("{path?: csv file, excel_safe?: bool (true), limit?: n (500, max 5000), after?: n}")
            .noundo(),
        CommandSpec::new("cablelist", "Cable Report", crate::cmd::input::typed::<CablelistParams>)
            .params("{path?: csv file, excel_safe?: bool (true), limit?: n (500, max 5000), after?: n, include_not_drawn?: bool (true; brief 059)}")
            .noundo(),
        CommandSpec::new("printbook", "Print Book", crate::cmd::input::typed::<PrintbookParams>)
            .params("{scope?: \"all\" | {folder: id} | {pages: \"100, 105-110\"}, path?: pdf file, colour?: \"mono\"|\"colour\", lineweights?: bool}")
            .noundo(),
        CommandSpec::new("reports", "Reports", crate::cmd::input::typed::<ReportsParams>).params("{} (opens the Reports window)").noundo(),
    ]
}

pub use ecad_electrical::cmd::output::{box_words, rectangle};

/// The outline and the proposal of a box command.
#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub(crate) struct LocationboxParams {
    /// The box's location: "+CP1".
    location: String,
    /// The box's function: "=F1".
    #[serde(default)]
    function: Option<String>,
    /// Two opposite corners [[x, y], [x, y]].
    #[serde(default)]
    corners: Option<Bounded<Point, 2, 2>>,
    /// A closed outline [[x, y], …].
    #[serde(default)]
    points: Option<Bounded<Point, 0, { crate::locations::MAX_VERTICES }>>,
    /// A closed polyline to take the outline from.
    #[serde(default)]
    item: Option<ItemId>,
    /// The page (default: the active page).
    #[serde(default)]
    page: Option<PageId>,
    /// What the box does to the devices inside: "propose" (default), "move" or "keep".
    #[serde(default)]
    proposal: Option<String>,
}

impl Command for LocationboxParams {
    fn run(self, s: &mut Session) -> cadcraft_engine::Result<Value> {
        let b = BoxInput { function: self.function, location: Some(self.location), corners: self.corners, points: self.points, item: self.item };
        b.run(s, self.page, self.proposal, "locationbox")
    }
}

/// `functionbox {function, location?, corners | points | item, page?, proposal?}` (brief 052d,
/// design 052 §5.8): a box carrying a function (and a location, when given), drawn dash-dot.
#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub(crate) struct FunctionboxParams {
    /// The box's function: "=F1".
    function: String,
    /// The box's location: "+CP1".
    #[serde(default)]
    location: Option<String>,
    /// Two opposite corners [[x, y], [x, y]].
    #[serde(default)]
    corners: Option<Bounded<Point, 2, 2>>,
    /// A closed outline [[x, y], …].
    #[serde(default)]
    points: Option<Bounded<Point, 0, { crate::locations::MAX_VERTICES }>>,
    /// A closed polyline to take the outline from.
    #[serde(default)]
    item: Option<ItemId>,
    /// The page (default: the active page).
    #[serde(default)]
    page: Option<PageId>,
    /// What the box does to the devices inside: "propose" (default), "move" or "keep".
    #[serde(default)]
    proposal: Option<String>,
}

impl Command for FunctionboxParams {
    fn run(self, s: &mut Session) -> cadcraft_engine::Result<Value> {
        let b = BoxInput { function: Some(self.function), location: self.location, corners: self.corners, points: self.points, item: self.item };
        b.run(s, self.page, self.proposal, "functionbox")
    }
}

/// What both box commands take.
struct BoxInput {
    function: Option<String>,
    location: Option<String>,
    corners: Option<Bounded<Point, 2, 2>>,
    points: Option<Bounded<Point, 0, { crate::locations::MAX_VERTICES }>>,
    item: Option<ItemId>,
}

impl BoxInput {
    /// A box (design 024 §7, design 052 §5.6, §5.8): its values, outline, and the proposal it
    /// makes for the device symbols inside.
    fn run(self, s: &mut Session, page: Option<PageId>, proposal: Option<String>, command: &str) -> cadcraft_engine::Result<Value> {
        let mode = ecad_electrical::cmd::sheet::proposal_mode(proposal.as_deref()).map_err(|e| err(e.to_string()))?;
        let corners = self.corners.and_then(|c| match c.0.as_slice() {
            [a, b] => Some((a.0, b.0)),
            _ => None,
        });
        let points = self.points.map(|v| v.0.into_iter().map(|p| p.0).collect());
        let shape = rules::Shape::one_of(corners, points, self.item).map_err(|e| err(e.to_string()))?;
        let (function, location) = (self.function, self.location);
        on_project(s, |p, pg, active| {
            let page = page.unwrap_or(active);
            let mut plan = rules::plan_box(p.ledger.document(), &mut p.ids, page, function.as_deref(), location.as_deref(), shape, &mode)?;
            p.apply_edit(pg, command, &plan.label, std::mem::take(&mut plan.ops))?;
            let proposed = &plan.proposed;
            if proposed.moved == 0 {
                p.conflicts = proposed.conflicts.clone();
            }
            let doc = p.ledger.document();
            pg.echo(plan.message.clone());
            for l in crate::proposals::conflict_words(doc, &proposed.conflicts) {
                pg.echo(l);
            }
            Ok(rules::box_json(doc, &plan))
        })
    }
}

/// A report's result: the page of rows asked for, or with `path` the CSV written.
fn report_result(t: &Table, keys: &[&str], ids: &[Value], pg: &Paging, what: &str) -> Result<Value, CmdError> {
    let flagged = t.flagged();
    if let Some(path) = &pg.path {
        let text = outputs::csv(t, pg.excel_safe);
        printbook::write_file(path, text.as_bytes()).map_err(CmdError::refused)?;
        let marked = if flagged == 1 { "1 cell marked ?".to_string() } else { format!("{flagged} cells marked ?") };
        let message = format!("Wrote {} {what} rows to {path} ({marked}).", t.rows.len());
        return Ok(json!({ "path": path, "rows": t.rows.len(), "flagged": flagged, "message": message }));
    }
    let all = outputs::rows_json(t, keys, ids);
    let total = all.len();
    let rows: Vec<Value> = all.into_iter().skip(pg.after).take(pg.limit).collect();
    let mut v = json!({ "rows": rows, "total": total, "flagged": flagged });
    if pg.after.saturating_add(pg.limit) < total {
        v["next"] = json!(pg.after + pg.limit);
    }
    Ok(v)
}

/// How `bom` lists a kit's lines (brief 047).
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
pub(crate) struct BomParams {
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

impl Command for BomParams {
    fn run(self, s: &mut Session) -> cadcraft_engine::Result<Value> {
        let group = rules::bom_group(self.group.as_deref()).map_err(|e| err(e.to_string()))?;
        // Brief 047 (design 047 §7.4): the report settings for this call only.
        let columns = rules::bom_columns(self.columns.as_deref()).map_err(|e| err(e.to_string()))?;
        let lines = self.lines.map(|l| l == Lines::Nested);
        let pg = Paging::of(self.limit, self.after, self.excel_safe, self.path);
        on_any_page(s, |p, _| {
            let o = p.outputs();
            let view = outputs::BomView { article: columns.unwrap_or(o.view.article), nested: lines.unwrap_or(o.view.nested) };
            let nested = view.nested && group == Group::Location;
            let t = o.bom_table_with(group, view, true);
            let ids = rules::bom_ids_json(o, group, nested);
            let keys = rules::bom_keys(nested, view.article);
            report_result(&t, &keys, &ids, &pg, "BOM")
        })
    }
}

/// `wirelist {path?, excel_safe?, limit?, after?}` (read only): the wire report.
#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub(crate) struct WirelistParams {
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

impl Command for WirelistParams {
    fn run(self, s: &mut Session) -> cadcraft_engine::Result<Value> {
        let pg = Paging::of(self.limit, self.after, self.excel_safe, self.path);
        on_any_page(s, |p, _| {
            let o = p.outputs();
            let t = o.wire_table();
            report_result(&t, rules::WIRE_KEYS, &rules::wire_ids_json(o), &pg, "wire")
        })
    }
}

/// `cablelist {path?, excel_safe?, limit?, after?, include_not_drawn?}` (read only): the cable
/// report.
#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub(crate) struct CablelistParams {
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
    /// Also the cables not drawn (default true; brief 059).
    #[serde(default)]
    include_not_drawn: Option<bool>,
}

impl Command for CablelistParams {
    fn run(self, s: &mut Session) -> cadcraft_engine::Result<Value> {
        let pg = Paging::of(self.limit, self.after, self.excel_safe, self.path);
        // Brief 059 (design 059 §4.5, Q3): the cables not drawn are listed unless this run says not.
        let include = self.include_not_drawn.unwrap_or(true);
        on_any_page(s, |p, _| {
            let o = p.outputs();
            let t = o.cable_table_with(include);
            let ids = rules::cable_ids_json(o, include);
            let keys = rules::cable_keys(&t);
            report_result(&t, &keys, &ids, &pg, "cable")
        })
    }
}

/// What `printbook` prints: "all", a folder or pages.
#[derive(Deserialize, JsonSchema)]
#[serde(untagged)]
enum ScopeParam {
    /// "all".
    Word(String),
    /// {folder: id} or {pages: "100, 105-110"}.
    Pick {
        /// A folder's pages.
        #[serde(default)]
        folder: Option<FolderId>,
        /// Page numbers and ranges.
        #[serde(default)]
        pages: Option<String>,
    },
}

/// The scope `printbook` prints.
fn scope_of(v: Option<ScopeParam>) -> Result<Scope, CmdError> {
    const WORDS: &str = "`scope` is \"all\", {folder: id} or {pages: \"100, 105-110\"}";
    match v {
        None => Ok(Scope::All),
        Some(ScopeParam::Word(t)) if t.trim() == "all" => Ok(Scope::All),
        Some(ScopeParam::Pick { folder: Some(f), .. }) => Ok(Scope::Folder(f)),
        Some(ScopeParam::Pick { folder: None, pages: Some(t) }) => Ok(Scope::Pages(t)),
        Some(_) => Err(CmdError::refused(WORDS)),
    }
}

/// Print `scope` of `p` (design 024 §4): the printed result as JSON.
///
/// # Errors
/// No page matches, or a projection, plot or write failure (nothing written).
pub fn print_project(
    p: &mut Project, scope: &Scope, o: Options, path: Option<&str>, progress: &mut dyn FnMut(usize, usize) -> bool,
) -> Result<Value, CmdError> {
    if crate::generated::has_bom_page(p.ledger.document()) {
        p.outputs();
    }
    // Brief 033: the book shows the pending marker as the screen does.
    p.refresh_pending();
    let o = Options { pending: p.pending, ..o };
    let doc = p.ledger.document();
    let resolved = printbook::resolve_with(doc, scope, p.kept_reports()).map_err(CmdError::refused)?;
    let done = printbook::print(doc, &resolved.pages, &p.views, p.tolerance, p.kept_reports(), o, path, progress).map_err(CmdError::refused)?;
    let files: Vec<Value> = done
        .files
        .iter()
        .map(|(path, sheets, bytes, data)| {
            let mut f = json!({ "path": path, "sheets": sheets, "bytes": bytes });
            if let Some(d) = data {
                f["data"] = json!(printbook::base64(d));
            }
            f
        })
        .collect();
    let sheets = if done.sheets == 1 { "1 sheet".to_string() } else { format!("{} sheets", done.sheets) };
    let message = match done.files.iter().filter_map(|(p, _, b, _)| p.as_ref().map(|p| (p, b))).collect::<Vec<_>>().as_slice() {
        [] => format!("Printed {sheets} ({} generated).", done.generated),
        [(path, bytes)] => format!("Printed {sheets} ({} generated) to {path} ({} KB).", done.generated, (**bytes).div_ceil(1024)),
        many => format!(
            "Printed {sheets} ({} generated) to {} files: {}.",
            done.generated,
            many.len(),
            many.iter().map(|(p, _)| p.as_str()).collect::<Vec<_>>().join(", ")
        ),
    };
    Ok(json!({
        "files": files, "sheets": done.sheets, "generated": done.generated, "flagged": done.flagged,
        "unknown": resolved.unknown, "notes": resolved.notes.iter().chain(done.notes.iter()).collect::<Vec<_>>(), "message": message,
    }))
}

/// `printbook {scope?, path?, colour?, lineweights?}`: the project's pages as a PDF.
#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub(crate) struct PrintbookParams {
    /// "all" (default), {folder: id} or {pages: "100, 105-110"}.
    #[serde(default)]
    scope: Option<ScopeParam>,
    /// The PDF file to write.
    #[serde(default)]
    path: Option<String>,
    /// "mono" (black, the colour code in the wire label) or "colour" (wire colours); default the
    /// project's `print.colour`.
    #[serde(default)]
    colour: Option<String>,
    /// Print lineweights (default true).
    #[serde(default)]
    lineweights: Option<bool>,
}

impl Command for PrintbookParams {
    fn run(self, s: &mut Session) -> cadcraft_engine::Result<Value> {
        let scope = scope_of(self.scope).map_err(|e| err(e.to_string()))?;
        let lineweights = self.lineweights.unwrap_or(true);
        let path = self.path;
        // Brief 040: no `colour` given prints as the project's `print.colour` says (mono by
        // default).
        let colour = self.colour;
        let v = on_any_page(s, |p, _| {
            let mono = crate::defaults::print::mono_word(p.ledger.document(), colour.as_deref())?;
            print_project(p, &scope, Options { mono, lineweights, pending: false }, path.as_deref(), &mut |_, _| true)
        })?;
        if let Some(m) = v.get("message").and_then(Value::as_str) {
            s.echo(m.to_string());
        }
        for n in v.get("notes").and_then(Value::as_array).into_iter().flatten().filter_map(Value::as_str) {
            s.echo(n.to_string());
        }
        Ok(v)
    }
}

/// `reports {}`: opens the Reports window.
#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
#[schemars(extend("properties" = {}))]
pub(crate) struct ReportsParams {}

impl Command for ReportsParams {
    fn run(self, s: &mut Session) -> cadcraft_engine::Result<Value> {
        on_any_page(s, |p, _| {
            p.open_reports = true;
            Ok(json!({"open": "reports"}))
        })
    }
}
