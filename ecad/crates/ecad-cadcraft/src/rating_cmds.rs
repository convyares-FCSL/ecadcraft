//! Brief 057's commands (design 057 §8): `rating` and `ratingtable` (write) and `ratings` (read).
//!
//! # Semantics
//! - `rating {wire | cable | many, …}` sets a subject's rating properties in one transaction of
//!   `SetProperty` ops; `null` clears a key, an absent key is untouched. Every value is checked
//!   ([`crate::rating::parse`]); a refusal names the key and its range, and nothing is written.
//! - `ratingtable {name, source?, csv}` creates or replaces a rating table (one `SetStyle`);
//!   `{import: path}` reads a CSV from disk, `{export: path}` writes one (command line and CLI only;
//!   MCP refuses both and passes `csv`). An import with any bad row is refused whole.
//! - `ratings {wire | cable | {}}` reads the results the panel shows. It records nothing.
//!
//! Programmatic calls never prompt.

use cadcraft_engine::{CommandSpec, Session};
use ecad_model::style::{StyleLayer, StyleRecord, StyleRef, is_name};
use ecad_model::{CableId, ConductorId, Document, Op};
use serde_json::{Value, json};

use crate::commands::{err, with_any_page, with_project};
use crate::rating::{self, Basis, RatedSubject, Rating, dataset, parse};
use crate::styles::{self, cable, ratingtable};

/// Most subjects one `rating {many}` call sets.
pub const MAX_MANY: usize = 500;
/// Most results one `ratings {}` call returns.
pub const PAGE: usize = 200;

/// The commands of this module, registered through [`crate::lanes`].
pub fn specs() -> Vec<CommandSpec> {
    vec![
        CommandSpec::new("rating", "Rating", run_rating)
            .params(
                "{wire: conductor id or wire number | cable: tag or id | many: [{wire|cable, …}], basis?: \"datasheet\"|\"table\"|\"manual\", \
manual?, bundling?: \"0.8\"|\"3 bunched\", load?, protected_by?: \"-Q01\"|\"-F01:3\", method?, ambient?, grouped?, voltage?, system?: \
\"ac1\"|\"ac3\"|\"dc\", vd_limit?, cable_type?: id or name} | {location: \"+CAB1\", temperature: °C | null} (a location's panel \
temperature, on its location boxes; null clears; one transaction)",
            )
            .noundo(),
        CommandSpec::new("ratingtable", "Rating Table", run_ratingtable)
            .params("{name, source?, csv, layer?: \"project\"|\"standard\"} (creates or replaces; one transaction) | {import: path, name?, source?} | {export: path, name?}")
            .noundo(),
        CommandSpec::new("ratings", "Ratings", run_ratings).params("{wire?: id or number, cable?: tag or id, from?: n} (read only)").noundo(),
    ]
}

// ---------------------------------------------------------------------------------------------
// Subjects

/// The conductors `text` names: a conductor ID, or every panel wire carrying that wire number.
fn wires(doc: &Document, text: &str) -> Result<Vec<ConductorId>, String> {
    let t = text.trim();
    if let Some(c) = ConductorId::parse(t).filter(|c| doc.conductors.contains_key(c)) {
        return Ok(vec![c]);
    }
    let conn = doc.connectivity();
    let mut out: Vec<ConductorId> = conn.nets_numbered(t).filter_map(|(n, _)| conn.net(n)).flat_map(|n| n.conductors.iter().copied()).collect();
    out.retain(|c| rating::is_panel_wire(doc, *c));
    out.sort();
    out.dedup();
    if out.is_empty() {
        let shown: String = t.chars().take(32).collect();
        return Err(format!("No panel wire {shown:?} (give a wire number or a conductor ID; a cable's cores are rated with the cable)."));
    }
    Ok(out)
}

/// The cable `text` names: a tag or an ID.
fn cable_named(doc: &Document, text: &str) -> Result<CableId, String> {
    let t = text.trim();
    if let Some(k) = CableId::parse(t).filter(|k| doc.cables.contains_key(k)) {
        return Ok(k);
    }
    doc.cables_tagged(t).into_iter().next().ok_or_else(|| format!("No cable {:?}.", t.chars().take(32).collect::<String>()))
}

fn subjects_of(doc: &Document, item: &Value) -> Result<Vec<RatedSubject>, String> {
    match (item.get("wire").and_then(Value::as_str), item.get("cable").and_then(Value::as_str)) {
        (Some(w), None) => Ok(wires(doc, w)?.into_iter().map(RatedSubject::Conductor).collect()),
        (None, Some(c)) => Ok(vec![RatedSubject::Cable(cable_named(doc, c)?)]),
        (Some(_), Some(_)) => Err("Give `wire` or `cable`, not both.".into()),
        (None, None) => Err("Give `wire` (a wire number or conductor ID) or `cable` (a tag or ID).".into()),
    }
}

// ---------------------------------------------------------------------------------------------
// rating

/// The keys `rating` takes: (JSON key, property key, for wires, for cables).
const KEYS: &[(&str, &str, bool, bool)] = &[
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

/// A JSON value as the text it stands for (`35` → `"35"`).
fn as_text(v: &Value) -> Option<String> {
    match v {
        Value::String(s) => Some(s.trim().to_string()),
        Value::Number(n) => Some(n.to_string()),
        _ => None,
    }
}

/// Check `value` for `key` (design 057 §6.1) and give the text stored.
///
/// # Errors
/// The key and its range, in words.
pub fn check_value(doc: &Document, key: &str, value: &str) -> Result<String, String> {
    let v = value.trim();
    let shown: String = v.chars().take(32).collect();
    match key {
        "basis" => Basis::parse(v).map(|b| b.word().to_string()).ok_or_else(|| "basis is datasheet, table or manual.".into()),
        "manual" | "load" => {
            parse::current(v).map(|_| v.to_string()).ok_or_else(|| format!("{key} is a current above 0 and up to 6300 A, not {shown:?}."))
        }
        "bundling" => parse::bundling(v)
            .map(|_| v.to_string())
            .ok_or_else(|| "bundling is a factor 0.30 to 1.00, or a circuit count 1 to 20 (\"3 bunched\").".into()),
        "protected_by" => match rating::trace::named(doc, v) {
            Some((c, fuse)) => {
                let tag = rating::trace::device_words(doc, c, fuse.as_deref());
                if rating::trace::amps_of(doc, c, fuse.as_deref()).is_none() {
                    return Err(format!("{tag}'s rating has no current; write it as 16 A first."));
                }
                Ok(tag)
            }
            None => Err(format!("There is no device or fuse {shown:?}; give a tag such as -Q01, or -F01:3 for a fuse of a bank.")),
        },
        "method" => {
            dataset::method(v).map(str::to_string).ok_or_else(|| format!("method is an IEC 60364-5-52 name: {}.", dataset::METHODS.join(", ")))
        }
        "ambient" => parse::ambient(v).map(|_| v.to_string()).ok_or_else(|| "ambient is \u{2212}40 to 90 \u{b0}C.".into()),
        "grouped" => parse::count(v).map(|n| n.to_string()).ok_or_else(|| "grouped is a circuit count 1 to 20.".into()),
        "voltage" => parse::voltage(v).map(|_| v.to_string()).ok_or_else(|| "voltage is 1 to 1000 V.".into()),
        "system" => {
            ["ac1", "ac3", "dc"].into_iter().find(|w| w.eq_ignore_ascii_case(v)).map(str::to_string).ok_or_else(|| "system is ac1, ac3 or dc.".into())
        }
        "vd_limit" => parse::percent(v).map(|_| v.to_string()).ok_or_else(|| "vd_limit is 0.5 to 20 %.".into()),
        "cable_type" => styles::find(doc, cable::WORD, v).map(|id| id.text()),
        _ => Err(format!("{key} is not a rating key.")),
    }
}

/// The ops one item of `rating` asks for.
fn item_ops(doc: &Document, item: &Value) -> Result<(Vec<Op>, Vec<RatedSubject>), String> {
    let subjects = subjects_of(doc, item)?;
    let cable = matches!(subjects.first(), Some(RatedSubject::Cable(_)));
    let mut sets: Vec<(&'static str, Option<String>)> = Vec::new();
    for (json_key, prop, for_wire, for_cable) in KEYS {
        let Some(v) = item.get(*json_key) else { continue };
        if (cable && !for_cable) || (!cable && !for_wire) {
            return Err(format!("{json_key} is for {}.", if *for_wire { "panel wires" } else { "cables" }));
        }
        let after = match v {
            Value::Null => None,
            other => {
                let t = as_text(other).ok_or_else(|| format!("{json_key} is text or a number, or null to clear it."))?;
                Some(check_value(doc, json_key, &t)?)
            }
        };
        sets.push((prop, after));
    }
    if let Some(o) = item.as_object()
        && let Some(k) = o.keys().find(|k| !matches!(k.as_str(), "wire" | "cable") && !KEYS.iter().any(|(j, ..)| j == k))
    {
        return Err(format!("{k} is not a rating key; the keys are {}.", KEYS.iter().map(|k| k.0).collect::<Vec<_>>().join(", ")));
    }
    let mut ops = Vec::new();
    for s in &subjects {
        let props = match s {
            RatedSubject::Conductor(c) => doc.conductors.get(c).map(|x| &x.properties),
            RatedSubject::Cable(k) => doc.cables.get(k).map(|x| &x.properties),
        };
        for (key, after) in &sets {
            let before = props.and_then(|p| p.get(*key)).cloned();
            if before != *after {
                ops.push(Op::SetProperty { object: s.object(), key: (*key).to_string(), before, after: after.clone() });
            }
        }
    }
    Ok((ops, subjects))
}

/// `rating {location, temperature}` (design 057 FP6; tester F1): the location boxes of
/// `location` take `rating.temperature`, written here (034's `setprop` rule reads `rating.<n>` as a
/// fuse bank's and refuses it on a box).
fn run_location(s: &mut Session, params: &Value) -> cadcraft_engine::Result<Value> {
    let loc = params
        .get("location")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|l| !l.is_empty())
        .ok_or_else(|| err("`location` names a location, such as +CAB1."))?;
    if let Some(o) = params.as_object()
        && let Some(k) = o.keys().find(|k| !matches!(k.as_str(), "location" | "temperature"))
    {
        return Err(err(format!("{k} is not a location rating key; give `temperature`.")));
    }
    let after = match params.get("temperature") {
        Some(Value::Null) => None,
        Some(v) => {
            let t = as_text(v).ok_or_else(|| err("temperature is a number in \u{b0}C, or null to clear it."))?;
            if parse::ambient(&t).is_none() {
                return Err(err("temperature is \u{2212}40 to 90 \u{b0}C."));
            }
            Some(t)
        }
        None => return Err(err("Give `temperature` (\u{b0}C), or null to clear it.")),
    };
    let typed = loc.to_string();
    with_project(s, |p, pg, _| {
        let doc = p.ledger.document();
        // Brief 052: `location` is a code (or an entry key); boxes store the entry's key.
        let entry = crate::designations::find(doc, crate::designations::Aspect::Location, &typed);
        let loc = entry.as_ref().map_or_else(|| typed.clone(), |e| e.code.clone());
        let boxes: Vec<ecad_model::ComponentId> = match &entry {
            None => Vec::new(),
            Some(e) => doc
                .components
                .iter()
                .filter(|(_, x)| {
                    crate::locations::is_box_component(x) && x.properties.get(ecad_model::book::LOCATION_KEY).is_some_and(|l| l.trim() == e.key)
                })
                .map(|(c, _)| *c)
                .collect(),
        };
        if boxes.is_empty() {
            return Err(format!("No location box is {loc}; draw one with Location Box first."));
        }
        let mut ops = Vec::new();
        for c in &boxes {
            let before = doc.components.get(c).and_then(|x| x.properties.get(rating::TEMPERATURE)).cloned();
            if before != after {
                ops.push(Op::SetProperty {
                    object: ecad_model::ObjectRef::Component(*c),
                    key: rating::TEMPERATURE.into(),
                    before,
                    after: after.clone(),
                });
            }
        }
        let changed = !ops.is_empty();
        if changed {
            let words = after.as_deref().map_or_else(|| "project default".to_string(), |t| format!("{t} \u{b0}C"));
            p.apply_edit(pg, "rating", &format!("Panel temperature {loc}: {words}"), ops)?;
        }
        Ok(json!({"changed": changed, "location": loc, "boxes": boxes.len(), "temperature": after}))
    })
}

fn run_rating(s: &mut Session, params: &Value) -> cadcraft_engine::Result<Value> {
    if params.get("location").is_some() {
        return run_location(s, params);
    }
    with_project(s, |p, pg, _| {
        let doc = p.ledger.document();
        let items: Vec<&Value> = match params.get("many") {
            Some(Value::Array(a)) if a.len() <= MAX_MANY => a.iter().collect(),
            Some(Value::Array(_)) => return Err(format!("`many` takes at most {MAX_MANY} subjects.")),
            Some(_) => return Err("`many` is a list.".into()),
            None => vec![params],
        };
        let mut ops = Vec::new();
        let mut all = Vec::new();
        for item in items {
            let (o, s) = item_ops(doc, item)?;
            ops.extend(o);
            all.extend(s);
        }
        let changed = !ops.is_empty();
        if changed {
            let who: Vec<String> = all.iter().take(3).map(|s| s.words(doc)).collect();
            let more = if all.len() > 3 { format!(" and {} more", all.len() - 3) } else { String::new() };
            p.apply_edit(pg, "rating", &format!("Rating {}{more}", who.join(", ")), ops)?;
        }
        let doc = p.ledger.document();
        let results: Vec<Value> = all.iter().take(PAGE).map(|s| rating_json(doc, &rating::rating_of(doc, *s))).collect();
        Ok(json!({"changed": changed, "ratings": results}))
    })
}

// ---------------------------------------------------------------------------------------------
// ratingtable

fn run_ratingtable(s: &mut Session, params: &Value) -> cadcraft_engine::Result<Value> {
    let text = |k: &str| params.get(k).and_then(Value::as_str).map(str::trim);
    if let Some(path) = text("export") {
        let path = path.to_string();
        let name = text("name").map(str::to_string);
        return with_any_page(s, |p, _| {
            let doc = p.ledger.document();
            let id = match &name {
                Some(n) => styles::find(doc, ratingtable::WORD, n)?,
                None => ratingtable::chosen(doc).ok_or("No rating table chosen; give `name`.")?,
            };
            let t = ratingtable::read(doc, &id);
            std::fs::write(&path, dataset::to_csv(&t.data)).map_err(|e| format!("Could not write {path}: {e}"))?;
            Ok(json!({"exported": path, "rows": t.data.rows.len()}))
        });
    }
    let (csv, from_file) = match (text("import"), params.get("csv").and_then(Value::as_str)) {
        (Some(path), None) => {
            let meta = std::fs::metadata(path).map_err(|e| err(format!("Could not read {path}: {e}")))?;
            if meta.len() > dataset::MAX_BYTES as u64 {
                return Err(err(format!("The table is over {} KiB. Nothing was written.", dataset::MAX_BYTES / 1024)));
            }
            (std::fs::read_to_string(path).map_err(|e| err(format!("Could not read {path} as text: {e}")))?, Some(path.to_string()))
        }
        (None, Some(c)) => (c.to_string(), None),
        (Some(_), Some(_)) => return Err(err("Give `csv` or `import`, not both.")),
        (None, None) => return Err(err("Give `csv` (the rows), `import` (a CSV file) or `export` (a CSV file).")),
    };
    let data = dataset::parse_strict(&csv).map_err(err)?;
    let default_name = from_file
        .as_deref()
        .and_then(|f| std::path::Path::new(f).file_stem().and_then(|s| s.to_str()))
        .map(str::to_string)
        .unwrap_or_else(|| "Rating table".to_string());
    let name = text("name").map(str::to_string).unwrap_or(default_name);
    if !is_name(&name) {
        return Err(err("A rating table needs a name (1 to 64 characters, no control characters)."));
    }
    let source = text("source").map(str::to_string);
    if source.as_ref().is_some_and(|s| s.chars().count() > ratingtable::MAX_SOURCE_CHARS || s.chars().any(char::is_control)) {
        return Err(err(format!("source is at most {} characters, no control characters.", ratingtable::MAX_SOURCE_CHARS)));
    }
    let layer = match text("layer") {
        None | Some("project") => StyleLayer::Project,
        Some("standard") => StyleLayer::Standard,
        Some(_) => return Err(err("layer is project or standard.")),
    };
    with_project(s, |p, pg, _| {
        let doc = p.ledger.document();
        if layer == StyleLayer::Standard && doc.standard.is_none() {
            return Err(crate::style_cmds::STANDARD_FIRST.into());
        }
        let cat = &ratingtable::CATALOGUE;
        let existing = styles::rows(doc, cat).into_iter().find(|r| r.name.eq_ignore_ascii_case(&name)).map(|r| r.id);
        let id = existing.clone().unwrap_or_else(|| StyleRef::Own(p.ids.fresh::<u128>()));
        let before = styles::record(doc, layer, cat.word, &id).cloned();
        let mut rec = before.clone().unwrap_or(StyleRecord { name: name.clone(), fields: Default::default(), body: None });
        if rec.name.is_empty() && existing.is_none() {
            rec.name = name.clone();
        }
        rec.fields.insert(ratingtable::ROWS.into(), csv.clone());
        if let Some(src) = &source {
            rec.fields.insert(ratingtable::SOURCE.into(), src.clone());
        }
        styles::check_record(doc, cat, &id, layer, &rec)?;
        let changed = before.as_ref() != Some(&rec);
        if changed {
            let label = format!("Rating table {name}: {} row{}", data.rows.len(), if data.rows.len() == 1 { "" } else { "s" });
            p.apply_edit(
                pg,
                "ratingtable",
                &label,
                vec![Op::SetStyle { layer, catalogue: cat.word.into(), id: id.clone(), before, after: Some(rec) }],
            )?;
        }
        Ok(json!({"id": id.text(), "name": name, "rows": data.rows.len(), "changed": changed, "odd": data.odd()}))
    })
}

// ---------------------------------------------------------------------------------------------
// ratings

fn run_ratings(s: &mut Session, params: &Value) -> cadcraft_engine::Result<Value> {
    let from = params.get("from").and_then(Value::as_u64).unwrap_or(0) as usize;
    with_any_page(s, |p, _| {
        let doc = p.ledger.document();
        let subjects: Vec<RatedSubject> = if params.get("wire").is_some() || params.get("cable").is_some() {
            subjects_of(doc, params)?
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
        let next = (from + PAGE < total).then_some(from + PAGE);
        Ok(json!({"ratings": out, "total": total, "next": next}))
    })
}

/// One result as `ratings` returns it (design 057 §8.2): the object the panel shows.
pub fn rating_json(doc: &Document, r: &Rating) -> Value {
    let subject = match r.subject {
        RatedSubject::Conductor(c) => json!({"wire": r.subject.words(doc), "conductor": c.hex()}),
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
