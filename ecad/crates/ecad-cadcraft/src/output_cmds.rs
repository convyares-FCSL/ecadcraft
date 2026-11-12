//! The commands of brief 024 (design 024 §3.5, §6.4): `locationbox` (writes), and `bom`,
//! `wirelist`, `cablelist`, `printbook` and `reports`, which read the project and record nothing.

use cadcraft_engine::{CommandSpec, Session};
use ecad_model::{FolderId, ItemId, PageId, Pt};
use serde_json::{Value, json};

use crate::commands::{err, opt_id, opt_typed, point, with_any_page, with_project};
use crate::outputs::{self, Group, Table};
use crate::printbook::{self, Options, Scope};
use crate::project::Project;

/// The commands this module adds.
pub fn specs() -> Vec<CommandSpec> {
    vec![
        CommandSpec::new("locationbox", "Location Box", run_locationbox)
            .params(
                "{location: \"+CP1\", corners?: [[x,y],[x,y]] | points?: [[x,y], …] | item?: id (a closed polyline), page?: id} (exactly one of corners, points, item; default the active page)",
            )
            .interactive(crate::interact::locationbox)
            .noundo(),
        CommandSpec::new("bom", "Bill of Materials", run_bom)
            .params("{group?: \"location\"|\"part\", path?: csv file, excel_safe?: bool (true), limit?: n (500, max 5000), after?: n}")
            .noundo(),
        CommandSpec::new("wirelist", "Wire Report", run_wirelist)
            .params("{path?: csv file, excel_safe?: bool (true), limit?: n (500, max 5000), after?: n}")
            .noundo(),
        CommandSpec::new("cablelist", "Cable Report", run_cablelist)
            .params("{path?: csv file, excel_safe?: bool (true), limit?: n (500, max 5000), after?: n}")
            .noundo(),
        CommandSpec::new("printbook", "Print Book", run_printbook)
            .params("{scope?: \"all\" | {folder: id} | {pages: \"100, 105-110\"}, path?: pdf file, colour?: \"mono\"|\"screen\", lineweights?: bool}")
            .noundo(),
        CommandSpec::new("reports", "Reports", run_reports).params("{} (opens the Reports window)").noundo(),
    ]
}

fn points(v: &Value, key: &str) -> Result<Vec<Pt>, String> {
    let a = v.as_array().ok_or_else(|| format!("`{key}` must be a list of [x, y]"))?;
    if a.len() > crate::locations::MAX_VERTICES {
        return Err(format!("at most {} points in `{key}`", crate::locations::MAX_VERTICES));
    }
    a.iter().map(|p| point(p).ok_or_else(|| format!("each entry of `{key}` must be a finite [x, y]"))).collect()
}

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
    if keeps == 0 {
        format!("Location box {location}: {devices} inside, now in {location}.")
    } else {
        format!("Location box {location}: {devices} inside, {} now in {location} ({keeps} keep their own location).", inside - keeps)
    }
}

fn run_locationbox(s: &mut Session, params: &Value) -> cadcraft_engine::Result<Value> {
    let location = params.get("location").and_then(Value::as_str).ok_or_else(|| err("`location` is required, such as \"+CP1\""))?.to_string();
    let corners = params.get("corners").filter(|v| !v.is_null()).map(|v| points(v, "corners")).transpose().map_err(err)?;
    let pts = params.get("points").filter(|v| !v.is_null()).map(|v| points(v, "points")).transpose().map_err(err)?;
    let item: Option<ItemId> = opt_id(params, "item").map_err(err)?;
    let page: Option<PageId> = opt_id(params, "page").map_err(err)?;
    let given = [corners.is_some(), pts.is_some(), item.is_some()].iter().filter(|x| **x).count();
    if given != 1 {
        return Err(err("give exactly one of `corners` (two corners), `points` (a closed outline) or `item` (a closed polyline)"));
    }
    with_project(s, |p, pg, active| {
        let page = page.unwrap_or(active);
        let doc = p.ledger.document();
        let planned = match (corners, pts, item) {
            (Some(c), _, _) => match c.as_slice() {
                [a, b] => crate::locations::plan_box(doc, &mut p.ids, page, &location, rectangle(*a, *b))?,
                _ => return Err("`corners` must hold two points".into()),
            },
            (_, Some(v), _) => crate::locations::plan_box(doc, &mut p.ids, page, &location, v)?,
            (_, _, Some(i)) => crate::locations::plan_from_item(doc, &mut p.ids, i, &location)?,
            _ => return Err("nothing to draw".into()),
        };
        let shown = crate::locations::location_value(doc, &location)?;
        p.apply_edit(pg, "locationbox", &format!("Location box {shown}"), planned.ops)?;
        let doc = p.ledger.document();
        let (inside, keeps) = doc
            .representations
            .get(&planned.rep)
            .and_then(|r| crate::locations::box_of_rep(doc, planned.rep, r))
            .map_or((0, 0), |b| crate::locations::inside_counts(doc, &b));
        let message = box_words(&shown, inside, keeps);
        pg.echo(message.clone());
        Ok(json!({
            "component": planned.component.hex(), "representation": planned.rep.hex(), "item": planned.item.hex(),
            "inside": inside, "keeps_own": keeps, "message": message,
        }))
    })
}

/// `limit`, `after`, `excel_safe` and `path` of a report command.
struct Paging {
    limit: usize,
    after: usize,
    excel_safe: bool,
    path: Option<String>,
}

fn paging(params: &Value) -> Result<Paging, String> {
    let limit =
        opt_typed(params, "limit", "a whole number", Value::as_u64)?.map_or(outputs::DEFAULT_LIMIT, |n| n as usize).clamp(1, outputs::MAX_LIMIT);
    let after = opt_typed(params, "after", "a whole number", Value::as_u64)?.unwrap_or(0) as usize;
    let excel_safe = opt_typed(params, "excel_safe", "true or false", Value::as_bool)?.unwrap_or(true);
    let path = opt_typed(params, "path", "a file path", |v| v.as_str().map(str::to_string))?;
    Ok(Paging { limit, after, excel_safe, path })
}

/// A report's result: the page of rows asked for, or with `path` the CSV written.
fn report_result(t: &Table, keys: &[&str], ids: &[Value], pg: &Paging, what: &str) -> Result<Value, String> {
    let flagged = t.flagged();
    if let Some(path) = &pg.path {
        let text = outputs::csv(t, pg.excel_safe);
        printbook::write_file(path, text.as_bytes())?;
        let marked = if flagged == 1 { "1 cell marked ?".to_string() } else { format!("{flagged} cells marked ?") };
        let message = format!("Wrote {} {what} rows to {path} ({marked}).", t.rows.len());
        return Ok(json!({ "path": path, "rows": t.rows.len(), "flagged": flagged, "message": message }));
    }
    let all = outputs::rows_json(t, keys, ids);
    let total = all.len();
    let rows: Vec<Value> = all.into_iter().skip(pg.after).take(pg.limit).collect();
    let mut v = json!({ "rows": rows, "total": total, "flagged": flagged });
    if pg.after + pg.limit < total {
        v["next"] = json!(pg.after + pg.limit);
    }
    Ok(v)
}

fn run_bom(s: &mut Session, params: &Value) -> cadcraft_engine::Result<Value> {
    let group = match params.get("group").and_then(Value::as_str) {
        None => Group::Location,
        Some(t) => Group::parse(t).ok_or_else(|| err("`group` is \"location\" or \"part\""))?,
    };
    let pg = paging(params).map_err(err)?;
    with_any_page(s, |p, _| {
        let o = p.outputs();
        let t = o.bom_table(group);
        let ids: Vec<Value> = o.bom(group).iter().map(|r| json!({"devices": r.devices.iter().map(|c| c.hex()).collect::<Vec<_>>()})).collect();
        report_result(&t, &["location", "qty", "tags", "description", "maker", "part_number"], &ids, &pg, "BOM")
    })
}

fn run_wirelist(s: &mut Session, params: &Value) -> cadcraft_engine::Result<Value> {
    let pg = paging(params).map_err(err)?;
    with_any_page(s, |p, _| {
        let o = p.outputs();
        let t = o.wire_table();
        let ids: Vec<Value> = o.wires().iter().map(|r| json!({"conductor": r.conductor.hex()})).collect();
        report_result(
            &t,
            &["wire_number", "from", "from_location", "to", "to_location", "cross_section", "colour", "potential", "cable", "page"],
            &ids,
            &pg,
            "wire",
        )
    })
}

fn run_cablelist(s: &mut Session, params: &Value) -> cadcraft_engine::Result<Value> {
    let pg = paging(params).map_err(err)?;
    with_any_page(s, |p, _| {
        let o = p.outputs();
        let t = o.cable_table();
        // Brief 028 (design 028 §9.2): `used` (a count), `spare` (the spare cores, or null when the
        // offer is unknown) and the devices at each end; these keys are 027's CSV columns.
        let ids: Vec<Value> = o
            .cables()
            .iter()
            .map(|r| {
                json!({
                    "cable": r.cable.hex(), "used": r.count.used(),
                    "spare": r.count.offer.as_ref().map(|_| r.count.spare.clone()),
                    "over": r.count.over.len(),
                    "from_devices": r.from_devices, "to_devices": r.to_devices,
                })
            })
            .collect();
        let mut keys = vec!["tag", "type", "cores", "from_location", "to_location", "used_text", "spare_text", "from", "to"];
        for h in t.headers.iter().skip(9) {
            keys.push(match *h {
                "Maker" => "maker",
                "Length" => "length",
                _ => "designation",
            });
        }
        report_result(&t, &keys, &ids, &pg, "cable")
    })
}

/// The scope parameter of `printbook`.
pub fn scope_param(params: &Value) -> Result<Scope, String> {
    match params.get("scope") {
        None | Some(Value::Null) => Ok(Scope::All),
        Some(Value::String(t)) if t.trim() == "all" => Ok(Scope::All),
        Some(v @ Value::Object(_)) => {
            if let Some(f) = opt_id::<FolderId>(v, "folder")? {
                Ok(Scope::Folder(f))
            } else if let Some(t) = v.get("pages").and_then(Value::as_str) {
                Ok(Scope::Pages(t.to_string()))
            } else {
                Err("`scope` is \"all\", {folder: id} or {pages: \"100, 105-110\"}".into())
            }
        }
        Some(_) => Err("`scope` is \"all\", {folder: id} or {pages: \"100, 105-110\"}".into()),
    }
}

/// Print `scope` of `p` (design 024 §4): the printed result as JSON.
///
/// # Errors
/// No page matches, or a projection, plot or write failure (nothing written).
pub fn print_project(
    p: &mut Project, scope: &Scope, o: Options, path: Option<&str>, progress: &mut dyn FnMut(usize, usize) -> bool,
) -> Result<Value, String> {
    if crate::generated::has_bom_page(p.ledger.document()) {
        p.outputs();
    }
    let doc = p.ledger.document();
    let resolved = printbook::resolve(doc, scope)?;
    let done = printbook::print(doc, &resolved.pages, &p.views, p.tolerance, p.outputs.as_ref(), o, path, progress)?;
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
        "unknown": resolved.unknown, "notes": done.notes, "message": message,
    }))
}

fn run_printbook(s: &mut Session, params: &Value) -> cadcraft_engine::Result<Value> {
    let scope = scope_param(params).map_err(err)?;
    let mono = match params.get("colour").and_then(Value::as_str) {
        None | Some("mono") => true,
        Some("screen") => false,
        Some(_) => return Err(err("`colour` is \"mono\" (black on white) or \"screen\"")),
    };
    let lineweights = opt_typed(params, "lineweights", "true or false", Value::as_bool).map_err(err)?.unwrap_or(true);
    let path = opt_typed(params, "path", "a file path", |v| v.as_str().map(str::to_string)).map_err(err)?;
    let v = with_any_page(s, |p, _| print_project(p, &scope, Options { mono, lineweights }, path.as_deref(), &mut |_, _| true))?;
    if let Some(m) = v.get("message").and_then(Value::as_str) {
        s.echo(m.to_string());
    }
    for n in v.get("notes").and_then(Value::as_array).into_iter().flatten().filter_map(Value::as_str) {
        s.echo(n.to_string());
    }
    Ok(v)
}

fn run_reports(s: &mut Session, _params: &Value) -> cadcraft_engine::Result<Value> {
    with_any_page(s, |p, _| {
        p.open_reports = true;
        Ok(json!({"open": "reports"}))
    })
}
