//! The commands of brief 048, the symbol builder (design 048 §9): `symbolnew`, `symboledit`,
//! `symbolimpact` (read only), `symbolupdate` and `makesymbol`, and `place` of a project symbol.
//!
//! Every write is one transaction, committed through [`Project::apply_edit`]; nothing here opens a
//! dialog. The builder (048b) saves through these JSON forms.
//!
//! # Semantics
//! - A project symbol is an embedded definition whose origin is [`PROJECT_SYMBOLS`] (FP3): its
//!   lineage is `origin.definition`, its version `origin.library_seq`. Every save that changes
//!   content embeds a new version; saving the newest version's content again is refused.
//! - An update rewrites placed copies by connection-point **key** (design §2.5): component,
//!   terminal and wire IDs are kept; a wired point that moves takes its wire end with it.
//! - Not here: the builder's drawing, drafts and the review drawer (048b); "update available" in
//!   Properties and Devices (048c).

use std::collections::{BTreeMap, BTreeSet};

use cadcraft_engine::doc::Handle;
use cadcraft_engine::{CommandSpec, Session};
use ecad_model::{
    Component, ComponentId, ConductorId, Definition, DefinitionId, Document, End, Form, Item, ItemId, ItemKind, Op, Origin, PageId, Placement,
    Primitive, Pt, Representation, RepresentationId, Subject, SymbolInstance, Terminal, TerminalDef, TerminalId, TextSlot,
};
use serde_json::{Value, json};

use crate::commands::{err, opt_id, plain_style, with_project};
use crate::defaults::symbols::Field;
use crate::project::{Mapped, Project};
use crate::symbols::{self, Join, PROJECT_SYMBOLS, Version};

/// Most shapes a symbol holds (design 048 §3.5).
pub const MAX_SHAPES: usize = 4000;
/// Most connection points a symbol holds.
pub const MAX_POINTS: usize = 512;
/// Most text slots a symbol holds.
pub const MAX_SLOTS: usize = 16;
/// Coordinates of a symbol stay within this many millimetres of its origin.
pub const MAX_EXTENT: f64 = 1000.0;
/// Most characters of a symbol's name.
pub const MAX_NAME: usize = 64;
/// Most copies one update or impact names.
pub const MAX_COPIES: usize = 100_000;
/// Most vertices of one polyline in a symbol.
const MAX_VERTICES: usize = 10_000;
/// Most characters of a point's key or label.
const MAX_POINT_TEXT: usize = 32;
/// Positions closer than this are one spot.
const TOL: f64 = 1e-6;

/// The point grid of the builder, the default of `symbol.point_grid` (design 048 §7.1, Q6).
pub const POINT_GRID_MM: f64 = 5.0;

/// The commands this module adds (registered in `lanes.rs`).
pub fn specs() -> Vec<CommandSpec> {
    vec![
        CommandSpec::new("symbolnew", "New Symbol", run_symbolnew)
            .params(
                "{name, key, graphics: [{kind: line|polyline|arc|circle|text|point, …} as in the file record], points: [{key, label?, at: [x,y]}], \
slots?: [{text: tag|description|rating|rating.<label>|part_number|maker|article_number|function|location, at: [x,y], h?: left|center|right, \
v?: baseline|bottom|middle|top, rotation?: radians}], defaults?: {prefix, description?, rating?, join?: device|part, aliases?, family?}, \
bridges?: [[key…]], from?: definition id} (design 048 §9.1: embeds version 1 of a project symbol; one transaction)",
            )
            .noundo(),
        CommandSpec::new("symboledit", "Edit Symbol", run_symboledit)
            .params(
                "{definition: id of the version edited, …content as symbolnew…, update: all|selected|none|new, copies?: [representation id], \
builtins?: bool} (design 048 §9.2: a new version, and the chosen placed copies updated, in one transaction)",
            )
            .noundo(),
        CommandSpec::new("symbolimpact", "Symbol Impact", run_symbolimpact).params(
            "{definition: id, …content as symbolnew…, builtins?: bool} (read only: every placed copy with what an update changes there and \
whether it clashes, design 048 §6.2)",
        ),
        CommandSpec::new("symbolupdate", "Update Symbol", run_symbolupdate)
            .params("{copies: [representation id] | lineage: id, to?: definition id (default: the newest)} (refused when a copy would clash)")
            .noundo(),
        CommandSpec::new("makesymbol", "Make Symbol", run_makesymbol)
            .params(
                "{handles?: [hex] (default: the selection) | component: id, name, key, points?: [{key, label?, at: [x,y] on the page}], \
slots?: [… at on the page], defaults?, replace?: bool (default true), graphics?: the drawing as edited in the builder, in the symbol's frame (points and slots then in that frame too), at?: [x,y] the page point of its origin} (design 048 §9.5: a symbol from plain geometry, or a drawn \
component promoted, keeping its IDs)",
            )
            .noundo(),
    ]
}

// ---------------------------------------------------------------- content (§3.5)

/// What a symbol holds, as given to a command (design 048 §9.1).
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Content {
    pub name: String,
    pub graphics: Vec<Primitive>,
    pub points: Vec<TerminalDef>,
    pub slots: Vec<TextSlot>,
    pub defaults: BTreeMap<String, String>,
    pub bridges: Vec<Vec<String>>,
}

impl Content {
    /// Its key (`defaults.key`), trimmed.
    pub fn key(&self) -> Option<&str> {
        self.defaults.get("key").map(|k| k.trim()).filter(|k| !k.is_empty())
    }

    /// The definition this content makes, with no origin.
    pub fn definition(&self) -> Definition {
        Definition {
            name: self.name.clone(),
            origin: None,
            graphics: self.graphics.clone(),
            terminals: self.points.clone(),
            bridges: self.bridges.clone(),
            texts: self.slots.clone(),
            defaults: self.defaults.clone(),
            derived_from: None,
        }
    }

    /// The content of a stored definition.
    pub fn of(d: &Definition) -> Self {
        Content {
            name: d.name.clone(),
            graphics: d.graphics.clone(),
            points: d.terminals.clone(),
            slots: d.texts.clone(),
            defaults: d.defaults.clone(),
            bridges: d.bridges.clone(),
        }
    }
}

fn finite_in(p: Pt) -> bool {
    p.is_finite() && p.x.abs() <= MAX_EXTENT && p.y.abs() <= MAX_EXTENT
}

fn pt_of(v: &Value) -> Option<Pt> {
    crate::commands::point(v)
}

fn text_ok(t: &str, max: usize) -> bool {
    !t.is_empty() && t.chars().count() <= max && !t.chars().any(char::is_control)
}

/// Read the content fields of `params`: name, key, graphics, points, slots, defaults, bridges.
/// Shapes and sizes are bounded before anything is collected; [`check`] applies the rules.
///
/// # Errors
/// A field of the wrong shape, or over a limit, in words.
pub fn parse_content(params: &Value) -> Result<Content, String> {
    let mut c = Content { name: params.get("name").and_then(Value::as_str).unwrap_or_default().trim().to_string(), ..Content::default() };
    if c.name.len() > MAX_NAME * 4 {
        return Err(format!("The name is too long ({MAX_NAME} characters at most)."));
    }
    if let Some(g) = params.get("graphics").filter(|v| !v.is_null()) {
        let a = g.as_array().ok_or("`graphics` is a list of shapes")?;
        if a.len() > MAX_SHAPES {
            return Err(format!("The symbol is too large ({MAX_SHAPES} shapes at most)."));
        }
        for v in a {
            let rec: ecad_file::v1::Primitive = serde_json::from_value(v.clone()).map_err(|e| format!("A shape could not be read: {e}"))?;
            let prim = ecad_model::Primitive::from(rec);
            if let Primitive::Polyline { vertices, .. } = &prim
                && vertices.len() > MAX_VERTICES
            {
                return Err(format!("A polyline has too many vertices ({MAX_VERTICES} at most)."));
            }
            c.graphics.push(prim);
        }
    }
    if let Some(ps) = params.get("points").filter(|v| !v.is_null()) {
        let a = ps.as_array().ok_or("`points` is a list of {key, label?, at}")?;
        if a.len() > MAX_POINTS {
            return Err(format!("The symbol has too many connection points ({MAX_POINTS} at most)."));
        }
        for v in a {
            let key = match v.get("key") {
                Some(Value::String(k)) => k.trim().to_string(),
                Some(Value::Number(n)) => n.to_string(),
                _ => return Err("Each point needs a `key`.".into()),
            };
            let label = v.get("label").and_then(Value::as_str).map(|l| l.trim().to_string()).filter(|l| !l.is_empty());
            let at = v.get("at").and_then(pt_of).ok_or_else(|| format!("Point {key} needs `at` [x, y]."))?;
            c.points.push(TerminalDef { key, label, at });
        }
    }
    if let Some(ss) = params.get("slots").filter(|v| !v.is_null()) {
        let a = ss.as_array().ok_or("`slots` is a list of {text, at, h?, v?, rotation?}")?;
        if a.len() > MAX_SLOTS {
            return Err(format!("The symbol has too many text slots ({MAX_SLOTS} at most)."));
        }
        for v in a {
            let text = v.get("text").and_then(Value::as_str).map(str::trim).ok_or("Each slot needs a `text`.")?.to_string();
            let at = v.get("at").and_then(pt_of).ok_or_else(|| format!("The {text} slot needs `at` [x, y]."))?;
            let h = match v.get("h") {
                None | Some(Value::Null) => Default::default(),
                Some(x) => serde_json::from_value(x.clone()).map_err(|_| "`h` is left, center or right".to_string())?,
            };
            let vv = match v.get("v") {
                None | Some(Value::Null) => Default::default(),
                Some(x) => serde_json::from_value(x.clone()).map_err(|_| "`v` is baseline, bottom, middle or top".to_string())?,
            };
            let rotation = match v.get("rotation") {
                None | Some(Value::Null) => 0.0,
                Some(x) => x.as_f64().filter(|r| r.is_finite()).ok_or("`rotation` is a finite number of radians")?,
            };
            c.slots.push(TextSlot { text, at, h, v: vv, rotation });
        }
    }
    if let Some(d) = params.get("defaults").filter(|v| !v.is_null()) {
        let o = d.as_object().ok_or("`defaults` is an object of words")?;
        if o.len() > ecad_model::DEFINITION_DEFAULT_KEYS.len() {
            return Err("`defaults` holds too many values.".into());
        }
        for (k, v) in o {
            let v = v.as_str().ok_or_else(|| format!("The default {k} is text."))?.trim().to_string();
            if !v.is_empty() {
                c.defaults.insert(k.trim().to_string(), v);
            }
        }
    }
    if let Some(k) = params.get("key").and_then(Value::as_str).map(str::trim).filter(|k| !k.is_empty()) {
        c.defaults.insert("key".into(), k.to_string());
    }
    if let Some(b) = params.get("bridges").filter(|v| !v.is_null()) {
        let a = b.as_array().ok_or("`bridges` is a list of key lists")?;
        if a.len() > MAX_POINTS {
            return Err("Too many bridges.".into());
        }
        for g in a {
            let keys = g.as_array().ok_or("A bridge is a list of point keys")?;
            if keys.len() > MAX_POINTS {
                return Err("A bridge names too many points.".into());
            }
            c.bridges.push(keys.iter().map(|k| k.as_str().unwrap_or_default().trim().to_string()).collect());
        }
    }
    Ok(c)
}

/// Whether `p` is on the point grid of `grid` millimetres.
fn on_grid(p: Pt, grid: f64) -> bool {
    let on = |x: f64| ((x / grid).round() * grid - x).abs() < 1e-6;
    on(p.x) && on(p.y)
}

/// What a check needs to know besides the content.
pub struct Rules<'a> {
    pub doc: &'a Document,
    /// The lineage being saved, whose own key may be kept.
    pub lineage: Option<DefinitionId>,
    /// Points must sit on the point grid (not for a promoted drawing, whose points stay exactly
    /// where its wires end).
    pub grid: bool,
}

/// The rules of design 048 §3.5, in the builder's order; every rule the content breaks, in words.
/// Empty when it may be saved.
pub fn problems(c: &Content, rules: &Rules<'_>) -> Vec<String> {
    let mut out = Vec::new();
    if c.name.is_empty() || c.name.chars().any(char::is_control) {
        out.push("A symbol needs a name.".to_string());
    } else if c.name.chars().count() > MAX_NAME {
        out.push(format!("The name is too long ({MAX_NAME} characters at most)."));
    }
    match c.key() {
        None => out.push("A symbol needs a key.".to_string()),
        Some(k) if !symbols::valid_key(k) => {
            out.push(format!("The key {k} is not a key: up to {} letters, digits, - and _.", symbols::MAX_KEY));
        }
        Some(k) if symbols::by_key(k).is_some() => out.push(format!("The key {k} is a built-in symbol's; choose another.")),
        Some(k) => {
            if let Some(v) = symbols::project_by_key(rules.doc, k).filter(|v| Some(v.lineage) != rules.lineage) {
                out.push(format!("The key {k} is used by {}; choose another.", v.def.name));
            }
        }
    }
    if let Some(k) = c.defaults.keys().find(|k| !ecad_model::DEFINITION_DEFAULT_KEYS.contains(&k.as_str())) {
        out.push(format!("A symbol has no default {k}."));
    }
    let join = c.defaults.get("join").map(|j| j.to_ascii_lowercase());
    if let Some(j) = &join
        && j != "device"
        && j != "part"
    {
        out.push("A symbol joins a device as `device` or `part`.".to_string());
    }
    let part = join.as_deref() == Some("part");
    match c.defaults.get("prefix") {
        Some(px) if crate::devices::valid_prefix(px).is_none() => out.push(format!("{px} is not a tag prefix (letters only, for example B).")),
        None if !part => out.push("A symbol of its own device needs a tag prefix (for example B).".to_string()),
        _ => {}
    }
    for (f, k) in [(Field::Description, "description"), (Field::Rating, "rating")] {
        if let Some(v) = c.defaults.get(k)
            && !f.accepts(v)
        {
            out.push(format!("The {k} is too long ({} characters at most).", f.max_chars()));
        }
    }
    if let Some(a) = c.defaults.get("aliases")
        && (a.split(',').count() > 8 || a.chars().count() > 200 || a.chars().any(char::is_control))
    {
        out.push("At most 8 aliases.".to_string());
    }
    if c.points.is_empty() {
        out.push("A symbol needs at least one connection point.".to_string());
    }
    if !part && !c.slots.iter().any(|s| s.text == "tag") {
        out.push("A symbol needs a tag slot.".to_string());
    }
    if !c.graphics.iter().all(prim_ok) {
        out.push(format!("A shape lies outside ±{MAX_EXTENT} mm or is not a number."));
    }
    let grid = crate::defaults::symbols::point_grid(rules.doc);
    let mut keys = BTreeSet::new();
    let mut spots: Vec<(Pt, &str)> = Vec::new();
    for p in &c.points {
        if !text_ok(&p.key, MAX_POINT_TEXT) || p.label.as_deref().is_some_and(|l| !text_ok(l, MAX_POINT_TEXT)) {
            out.push(format!("Point {} needs a short key and label ({MAX_POINT_TEXT} characters at most).", p.key));
            continue;
        }
        if !keys.insert(p.key.as_str()) {
            out.push(format!("Two points have the key {}.", p.key));
        }
        if !finite_in(p.at) {
            out.push(format!("Point {} lies outside ±{MAX_EXTENT} mm.", p.key));
            continue;
        }
        if rules.grid && !on_grid(p.at, grid) {
            out.push(format!("A point goes on the {} mm grid (point {}).", mm(grid), p.key));
        }
        if let Some((_, k)) = spots.iter().find(|(q, _)| q.near(p.at, TOL)) {
            out.push(format!("Point {k} is already here (point {}).", p.key));
        }
        spots.push((p.at, p.key.as_str()));
    }
    let mut texts = BTreeSet::new();
    for s in &c.slots {
        if !TextSlot::is_known_text(&s.text) {
            out.push(format!("A slot places a device text; {} is not one.", s.text));
        } else if !texts.insert(s.text.as_str()) {
            out.push(format!("Two slots place the {}.", s.text));
        }
        if !finite_in(s.at) || !s.rotation.is_finite() {
            out.push(format!("The {} slot lies outside ±{MAX_EXTENT} mm.", s.text));
        }
    }
    if let Some(k) = c.bridges.iter().flatten().find(|k| !keys.contains(k.as_str())) {
        out.push(format!("A bridge names point {k}, which the symbol does not have."));
    }
    out.dedup();
    out
}

fn prim_ok(p: &Primitive) -> bool {
    let pts: Vec<Pt> = match p {
        Primitive::Line { a, b } => vec![*a, *b],
        Primitive::Polyline { vertices, .. } => vertices.clone(),
        Primitive::Arc { center, radius, start, end } => {
            if !(radius.is_finite() && start.is_finite() && end.is_finite() && *radius <= MAX_EXTENT) {
                return false;
            }
            vec![*center]
        }
        Primitive::Circle { center, radius } => {
            if !(radius.is_finite() && *radius <= MAX_EXTENT) {
                return false;
            }
            vec![*center]
        }
        Primitive::Text { at, height, rotation, value, .. } => {
            if !(height.is_finite() && rotation.is_finite() && *height > 0.0 && *height <= 100.0 && value.chars().count() <= 200) {
                return false;
            }
            vec![*at]
        }
        Primitive::Point { at } => vec![*at],
    };
    pts.into_iter().all(finite_in)
}

/// The first problem as the refusal.
fn refuse_problems(c: &Content, rules: &Rules<'_>) -> Result<(), String> {
    match problems(c, rules).into_iter().next() {
        Some(p) => Err(p),
        None => Ok(()),
    }
}

/// The definition of version `seq` of `lineage` with `content`, its origin and content hash set.
///
/// # Errors
/// Its content hash could not be computed (not for checked content).
pub fn version_definition(c: &Content, lineage: DefinitionId, seq: u64, derived_from: Option<Origin>) -> Result<Definition, String> {
    let mut d = c.definition();
    d.derived_from = derived_from;
    let content_hash = ecad_file::definition_hash(&d).map_err(|e| format!("The symbol could not be prepared ({e})."))?;
    d.origin = Some(Origin { library: PROJECT_SYMBOLS, definition: lineage, library_seq: seq, content_hash });
    Ok(d)
}

// ---------------------------------------------------------------- the impact (§6.2)

/// What an update does to one placed copy (design 048 §6.1's row, as data).
#[derive(Debug, Clone, PartialEq)]
pub struct Row {
    pub rep: RepresentationId,
    pub item: ItemId,
    pub page: PageId,
    pub component: ComponentId,
    pub tag: String,
    /// The version it shows now.
    pub from: DefinitionId,
    pub changes: Vec<String>,
    /// Why the update cannot be made there, if it cannot.
    pub clash: Option<String>,
    /// A placed built-in (an edited starter's copies), unticked unless asked for.
    pub builtin: bool,
}

impl Row {
    /// The row as data; `book` is the project's page book, built once per listing (a page's
    /// number alone walks the book).
    pub fn json(&self, doc: &Document, book: &ecad_model::book::Book) -> Value {
        json!({
            "rep": self.rep.hex(), "item": self.item.hex(), "page": self.page.hex(),
            "page_number": book.number(doc, self.page), "component": self.component.hex(),
            "tag": self.tag, "from": self.from.hex(), "changes": self.changes, "clash": self.clash, "builtin": self.builtin,
        })
    }
}

/// The placed copies of the definitions in `defs`: (item, its representation), in page order then
/// ID. Through the model's index of symbol items by definition.
fn copies_of(doc: &Document, defs: &[DefinitionId]) -> Vec<(ItemId, RepresentationId, PageId)> {
    let mut out = Vec::new();
    for d in defs {
        for item in doc.items_using(*d) {
            let Some((rid, r)) = doc.representation_of_item(item) else { continue };
            if r.form == Form::Symbol && matches!(r.subject, Subject::Component(_)) {
                out.push((item, rid, r.page));
            }
        }
    }
    let order: BTreeMap<PageId, usize> = doc.page_order.iter().enumerate().map(|(i, p)| (*p, i)).collect();
    out.sort_by_key(|(i, r, p)| (order.get(p).copied().unwrap_or(usize::MAX), *r, *i));
    out.dedup();
    out
}

/// The terminal of component `c` a representation shows for definition key `key`.
fn shown_terminal(doc: &Document, c: ComponentId, r: &Representation, old: &Definition, key: &str) -> Option<TerminalId> {
    if r.connection_points.is_empty() {
        return doc.terminals_of(c).find(|(_, t)| t.key.as_deref() == Some(key)).map(|(t, _)| t);
    }
    let at = old.terminals.iter().find(|td| td.key == key)?.at;
    r.connection_points.iter().find(|(_, p)| p.near(at, 1e-9)).map(|(t, _)| *t)
}

fn mm(d: f64) -> String {
    let r = (d * 100.0).round() / 100.0;
    if r.fract() == 0.0 { format!("{r:.0}") } else { format!("{r}") }
}

/// A wire as people read it: its net's number (`24V-05`), else its tag, else its short ID.
fn wire_number(doc: &Document, c: ConductorId) -> String {
    let conn = doc.connectivity();
    conn.net_of_conductor(c)
        .and_then(|n| conn.net(n))
        .and_then(|n| ecad_model::wiring::net_number(doc, n))
        .unwrap_or_else(|| crate::slash::wire_name(doc, c))
}

fn wire_words(doc: &Document, t: TerminalId) -> String {
    let names: Vec<String> = doc.ends_at(t).iter().take(3).map(|(c, _)| wire_number(doc, *c)).collect();
    match names.len() {
        0 => String::new(),
        1 => format!("wire {}", names.join("")),
        _ => format!("wires {}", names.join(", ")),
    }
}

/// The row of one copy (`item` in `rid`) moving from `old` to `new` (design 048 §6.2).
fn row(doc: &Document, item: ItemId, rid: RepresentationId, old_id: DefinitionId, old: &Definition, new: &Definition) -> Option<Row> {
    let r = doc.representations.get(&rid)?;
    let Subject::Component(c) = r.subject else { return None };
    let placement = match &doc.items.get(&item)?.kind {
        ItemKind::Symbol(s) => s.placement,
        ItemKind::Primitive(_) => return None,
    };
    let mut changes = Vec::new();
    let mut clash = None;
    let new_keys: BTreeMap<&str, &TerminalDef> = new.terminals.iter().map(|t| (t.key.as_str(), t)).collect();
    let old_keys: BTreeMap<&str, &TerminalDef> = old.terminals.iter().map(|t| (t.key.as_str(), t)).collect();
    let mut labels_after: BTreeMap<String, usize> = BTreeMap::new();
    for (_, t) in doc.terminals_of(c) {
        if let Some(l) = &t.label {
            *labels_after.entry(l.clone()).or_default() += 1;
        }
    }
    let mut page_spots: Option<Vec<(TerminalId, Pt)>> = None;
    for (k, td) in &old_keys {
        let t = shown_terminal(doc, c, r, old, k);
        match new_keys.get(k) {
            None => match t {
                Some(t) if doc.has_ends(t) => {
                    clash.get_or_insert_with(|| format!("Point {k} is removed; {} is attached to it.", wire_words(doc, t)));
                }
                _ => changes.push(format!("Point {k} removed")),
            },
            Some(nd) => {
                if !nd.at.near(td.at, TOL) {
                    let d = nd.at.dist(td.at);
                    let wired = t.filter(|t| doc.has_ends(*t));
                    match wired {
                        Some(t) => {
                            changes.push(format!("{k} moves {} mm, {} follows", mm(d), wire_words(doc, t)));
                            // Rule 2: onto another point or wire end of the page.
                            let to = placement.apply(nd.at);
                            let spots = page_spots.get_or_insert_with(|| {
                                ecad_model::derive::terminal_positions(doc, r.page).into_iter().map(|(t, _, p)| (t, p)).collect()
                            });
                            let ends = crate::wires::loose_ends_at(doc, r.page, to, TOL);
                            if spots.iter().any(|(o, p)| *o != t && p.near(to, TOL)) || !ends.is_empty() {
                                clash.get_or_insert_with(|| format!("Point {k} would move onto another connection on this page."));
                            }
                        }
                        None => changes.push(format!("{k} moves {} mm", mm(d))),
                    }
                }
                if nd.label != td.label {
                    let (a, b) = (td.label.clone().unwrap_or_else(|| td.key.clone()), nd.label.clone().unwrap_or_else(|| nd.key.clone()));
                    changes.push(format!("label {a} → {b}"));
                    // Rule 3: a relabel that makes two of the device's points share a label.
                    let term = t.and_then(|t| doc.terminals.get(&t));
                    if term.is_some_and(|x| x.label == td.label)
                        && let Some(nl) = &nd.label
                    {
                        if let Some(ol) = &td.label
                            && let Some(n) = labels_after.get_mut(ol)
                        {
                            *n = n.saturating_sub(1);
                        }
                        *labels_after.entry(nl.clone()).or_default() += 1;
                    }
                }
            }
        }
    }
    for k in new_keys.keys().filter(|k| !old_keys.contains_key(*k)) {
        changes.push(format!("Point {k} added"));
        if let Some(l) = new_keys.get(k).and_then(|t| t.label.clone()) {
            *labels_after.entry(l).or_default() += 1;
        }
    }
    if let Some((l, _)) = labels_after.iter().find(|(_, n)| **n > 1)
        && clash.is_none()
        && r.connection_points.is_empty()
    {
        clash = Some(format!("Two of the device's points would both be labelled {l}."));
    }
    if old.graphics != new.graphics {
        changes.push("graphics change".into());
    }
    if old.texts != new.texts {
        changes.push("text slots move".into());
    }
    let builtin = !symbols::is_project_symbol(old);
    Some(Row { rep: rid, item, page: r.page, component: c, tag: crate::devices::tag_words(doc, c), from: old_id, changes, clash, builtin })
}

/// Every placed copy of `defs` and what moving it to `new` would do.
pub fn impact(doc: &Document, defs: &[DefinitionId], new: &Definition) -> Vec<Row> {
    copies_of(doc, defs)
        .into_iter()
        .filter_map(|(item, rid, _)| {
            let ItemKind::Symbol(s) = &doc.items.get(&item)?.kind else { return None };
            let old = doc.definitions.get(&s.definition)?;
            row(doc, item, rid, s.definition, old, new)
        })
        .collect()
}

/// The ops moving the copies of `rows` to definition `new_id` (`new`), per design 048 §5.3. The
/// component's own definition follows when it was the copy's old version.
fn update_ops(doc: &Document, ids: &mut ecad_model::IdAllocator, rows: &[&Row], new_id: DefinitionId, new: &Definition) -> Result<Vec<Op>, String> {
    let mut ops = Vec::new();
    let mut set_def: BTreeSet<ComponentId> = BTreeSet::new();
    let mut moved_routes: BTreeMap<ItemId, Item> = BTreeMap::new();
    let mut route_before: BTreeMap<ItemId, Item> = BTreeMap::new();
    for row in rows {
        if row.clash.is_some() {
            return Err(format!(
                "{} (p. {}) would clash: {}",
                row.tag,
                crate::references::page_number(doc, row.page),
                row.clash.clone().unwrap_or_default()
            ));
        }
        let item = doc.items.get(&row.item).ok_or("A copy is gone.")?;
        let ItemKind::Symbol(inst) = &item.kind else { continue };
        let old = doc.definitions.get(&inst.definition).ok_or("A copy's symbol is gone.")?;
        let r = doc.representations.get(&row.rep).ok_or("A copy is gone.")?;
        let c = row.component;
        let comp = doc.components.get(&c).ok_or("A copy's device is gone.")?;
        let mut after = item.clone();
        after.kind = ItemKind::Symbol(SymbolInstance { definition: new_id, placement: inst.placement });
        ops.push(Op::ModifyItem { id: row.item, before: item.clone(), after });
        if comp.definition == Some(inst.definition) && set_def.insert(c) {
            ops.push(Op::SetComponentDefinition { id: c, before: comp.definition, after: Some(new_id) });
        }
        let new_keys: BTreeMap<&str, &TerminalDef> = new.terminals.iter().map(|t| (t.key.as_str(), t)).collect();
        let mut cps = r.connection_points.clone();
        for td in &old.terminals {
            let Some(t) = shown_terminal(doc, c, r, old, &td.key) else { continue };
            let term = doc.terminals.get(&t).ok_or("A connection point is gone.")?;
            match new_keys.get(td.key.as_str()) {
                None => {
                    // Rows with a wired removal clash, so this one is unwired.
                    ops.push(Op::RemoveTerminal { id: t, terminal: term.clone() });
                    cps.remove(&t);
                }
                Some(nd) => {
                    if nd.label != td.label && term.label == td.label && r.connection_points.is_empty() {
                        ops.push(Op::RemoveTerminal { id: t, terminal: term.clone() });
                        ops.push(Op::AddTerminal { id: t, terminal: Terminal { label: nd.label.clone(), ..term.clone() } });
                    }
                    if !nd.at.near(td.at, TOL) {
                        if cps.contains_key(&t) {
                            cps.insert(t, nd.at);
                        }
                        let (from, to) = (inst.placement.apply(td.at), inst.placement.apply(nd.at));
                        for (cond, _) in doc.ends_at(t) {
                            follow(doc, cond, r.page, from, to, &mut route_before, &mut moved_routes);
                        }
                    }
                }
            }
        }
        for nd in new.terminals.iter().filter(|nd| !old.terminals.iter().any(|td| td.key == nd.key)) {
            let t: TerminalId = ids.fresh();
            let key = if r.connection_points.is_empty() { nd.key.clone() } else { nd.label.clone().unwrap_or_else(|| nd.key.clone()) };
            ops.push(Op::AddTerminal { id: t, terminal: Terminal { component: c, label: nd.label.clone(), key: Some(key) } });
            if !r.connection_points.is_empty() {
                cps.insert(t, nd.at);
            }
        }
        if cps != r.connection_points {
            ops.push(Op::ModifyRepresentation { id: row.rep, before: r.clone(), after: Representation { connection_points: cps, ..r.clone() } });
        }
    }
    for (id, after) in moved_routes {
        if let Some(before) = route_before.remove(&id) {
            ops.push(Op::ModifyItem { id, before, after });
        }
    }
    Ok(ops)
}

/// Move the end of conductor `c`'s route on `page` that lies at `from` to `to` (the route's other
/// vertices stay), accumulating several moves of one route.
fn follow(doc: &Document, c: ConductorId, page: PageId, from: Pt, to: Pt, before: &mut BTreeMap<ItemId, Item>, after: &mut BTreeMap<ItemId, Item>) {
    let Some((id, _)) = ecad_model::derive::route_on_page(doc, c, page) else { return };
    let Some(orig) = doc.items.get(&id) else { return };
    let cur = after.get(&id).cloned().unwrap_or_else(|| orig.clone());
    let ItemKind::Primitive(prim) = &cur.kind else { return };
    let Some((a, b)) = prim.path_ends() else { return };
    let end = if a.near(from, 1e-6) {
        End::A
    } else if b.near(from, 1e-6) {
        End::B
    } else {
        return;
    };
    let Some(moved) = prim.with_path_end(end, to) else { return };
    before.entry(id).or_insert_with(|| orig.clone());
    after.insert(id, Item { kind: ItemKind::Primitive(moved), ..cur });
}

/// Superseded project versions of `lineage` nothing places once `ops` are applied (FP8), as
/// removals; never `keep`.
fn superseded(doc: &Document, lineage: DefinitionId, keep: DefinitionId, ops: &[Op]) -> Vec<Op> {
    let mut leaving: BTreeMap<DefinitionId, usize> = BTreeMap::new();
    let mut comps_leaving: BTreeMap<DefinitionId, usize> = BTreeMap::new();
    for op in ops {
        match op {
            Op::ModifyItem { before: Item { kind: ItemKind::Symbol(a), .. }, after: Item { kind: ItemKind::Symbol(b), .. }, .. }
                if a.definition != b.definition =>
            {
                *leaving.entry(a.definition).or_default() += 1;
            }
            Op::SetComponentDefinition { before: Some(d), .. } => *comps_leaving.entry(*d).or_default() += 1,
            _ => {}
        }
    }
    symbols::versions_of(doc, lineage)
        .into_iter()
        .filter(|v| v.id != keep)
        .filter(|v| doc.items_using(v.id).len() <= leaving.get(&v.id).copied().unwrap_or(0))
        .filter(|v| doc.components_using(v.id).len() <= comps_leaving.get(&v.id).copied().unwrap_or(0))
        .map(|v| Op::RemoveDefinition { id: v.id, definition: v.def.clone() })
        .collect()
}

// ---------------------------------------------------------------- commands

fn copies_param(params: &Value) -> Result<Option<Vec<RepresentationId>>, String> {
    match params.get("copies") {
        None | Some(Value::Null) => Ok(None),
        Some(_) => crate::commands::id_list(params, "copies", MAX_COPIES).map(Some),
    }
}

fn result(doc: &Document, id: DefinitionId) -> Value {
    let d = doc.definitions.get(&id);
    let o = d.and_then(|d| d.origin.as_ref());
    json!({
        "definition": id.hex(),
        "lineage": o.map(|o| o.definition.hex()),
        "version": o.map(|o| o.library_seq),
        "key": d.and_then(|d| d.defaults.get("key")),
        "name": d.map(|d| d.name.clone()),
    })
}

fn run_symbolnew(s: &mut Session, params: &Value) -> cadcraft_engine::Result<Value> {
    let content = parse_content(params).map_err(err)?;
    let from: Option<DefinitionId> = opt_id(params, "from").map_err(err)?;
    with_project(s, |p, pg, _| {
        let doc = p.ledger.document();
        refuse_problems(&content, &Rules { doc, lineage: None, grid: true })?;
        let derived_from = match from {
            None => None,
            Some(f) => match doc.definitions.get(&f) {
                Some(d) => d.origin.clone().filter(|o| o.library != PROJECT_SYMBOLS),
                // A built-in the project has not placed yet: its catalogue definition's origin, so
                // the copy remembers where it came from (FP4) all the same.
                None => {
                    let s = symbols::STARTERS.iter().find(|s| s.id == f).ok_or("No such definition to copy from.")?;
                    s.definition()?.origin.clone()
                }
            },
        };
        let (id, lineage): (DefinitionId, DefinitionId) = (p.ids.fresh(), p.ids.fresh());
        let def = version_definition(&content, lineage, 1, derived_from)?;
        let label = format!("New symbol {}", content.name);
        p.apply_edit(pg, "symbolnew", &label, vec![Op::EmbedDefinition { id, definition: def }])?;
        Ok(result(p.ledger.document(), id))
    })
}

/// What `symboledit` and `symbolimpact` are asked to save.
struct Edit {
    lineage: DefinitionId,
    seq: u64,
    derived_from: Option<Origin>,
    /// The definitions whose copies are rows: every version of the lineage, or the built-in.
    defs: Vec<DefinitionId>,
    builtin: bool,
}

fn plan_edit(doc: &Document, ids: &mut ecad_model::IdAllocator, edited: DefinitionId, content: &Content, new_lineage: bool) -> Result<Edit, String> {
    let d = doc.definitions.get(&edited).ok_or("No such symbol.")?;
    match Version::of(edited, d) {
        Some(v) if !new_lineage => {
            let newest = symbols::newest(doc, v.lineage).ok_or("No such symbol.")?;
            let defs = symbols::versions_of(doc, v.lineage).iter().map(|x| x.id).collect();
            Ok(Edit { lineage: v.lineage, seq: newest.seq.saturating_add(1), derived_from: newest.def.derived_from.clone(), defs, builtin: false })
        }
        Some(v) => Ok(Edit { lineage: ids.fresh(), seq: 1, derived_from: v.def.derived_from.clone(), defs: Vec::new(), builtin: false }),
        None => {
            let _ = content;
            // A built-in (or any other library's) symbol: a project copy that remembers its source.
            let lineage = ids.fresh();
            Ok(Edit { lineage, seq: 1, derived_from: d.origin.clone(), defs: if new_lineage { Vec::new() } else { vec![edited] }, builtin: true })
        }
    }
}

fn run_symbolimpact(s: &mut Session, params: &Value) -> cadcraft_engine::Result<Value> {
    let content = parse_content(params).map_err(err)?;
    let edited: DefinitionId = crate::commands::id(params, "definition").map_err(err)?;
    let builtins = params.get("builtins").and_then(Value::as_bool).unwrap_or(false);
    with_project(s, |p, _, _| {
        let doc = p.ledger.document();
        let mut ids = p.ids.clone();
        let e = plan_edit(doc, &mut ids, edited, &content, false)?;
        let new = version_definition(&content, e.lineage, e.seq, e.derived_from.clone())?;
        let rows = impact(doc, &e.defs, &new);
        let shown: Vec<&Row> = rows.iter().filter(|r| !r.builtin || builtins || e.builtin).collect();
        let problems = problems(&content, &Rules { doc, lineage: (!e.builtin).then_some(e.lineage), grid: true });
        let pages: BTreeSet<PageId> = shown.iter().map(|r| r.page).collect();
        let book = ecad_model::book::Book::of(doc);
        Ok(json!({
            "name": content.name, "version": e.seq, "builtin": e.builtin,
            "copies": shown.len(), "pages": pages.len(), "clashes": shown.iter().filter(|r| r.clash.is_some()).count(),
            "rows": shown.iter().map(|r| r.json(doc, &book)).collect::<Vec<_>>(),
            "problems": problems,
        }))
    })
}

fn run_symboledit(s: &mut Session, params: &Value) -> cadcraft_engine::Result<Value> {
    let content = parse_content(params).map_err(err)?;
    let edited: DefinitionId = crate::commands::id(params, "definition").map_err(err)?;
    let update = params.get("update").and_then(Value::as_str).unwrap_or("all").trim().to_ascii_lowercase();
    if !matches!(update.as_str(), "all" | "selected" | "none" | "new") {
        return Err(err("`update` is all, selected, none or new."));
    }
    let copies = copies_param(params).map_err(err)?;
    let builtins = params.get("builtins").and_then(Value::as_bool).unwrap_or(false);
    with_project(s, |p, pg, _| {
        let doc = p.ledger.document();
        let new_lineage = update == "new";
        let e = plan_edit(doc, &mut p.ids, edited, &content, new_lineage)?;
        refuse_problems(&content, &Rules { doc, lineage: (!new_lineage && !e.builtin).then_some(e.lineage), grid: true })?;
        let def = version_definition(&content, e.lineage, e.seq, e.derived_from.clone())?;
        if !e.builtin
            && !new_lineage
            && let Some(n) = symbols::newest(doc, e.lineage)
            && n.def.origin.as_ref().map(|o| o.content_hash) == def.origin.as_ref().map(|o| o.content_hash)
        {
            return Err("Nothing changed.".into());
        }
        let id: DefinitionId = p.ids.fresh();
        let mut ops = vec![Op::EmbedDefinition { id, definition: def.clone() }];
        let rows = impact(doc, &e.defs, &def);
        let chosen: Vec<&Row> = match update.as_str() {
            "all" => {
                let take: Vec<&Row> = rows.iter().filter(|_| !e.builtin || builtins).collect();
                let clashing: Vec<&&Row> = take.iter().filter(|r| r.clash.is_some()).collect();
                if !clashing.is_empty() {
                    let names: Vec<String> =
                        clashing.iter().take(8).map(|r| format!("{} (p. {})", r.tag, crate::references::page_number(doc, r.page))).collect();
                    return Err(format!(
                        "{} placed {} would clash: {}. Use update \"selected\" or move their wires.",
                        clashing.len(),
                        if clashing.len() == 1 { "copy" } else { "copies" },
                        names.join(", ")
                    ));
                }
                take
            }
            "selected" => {
                let want: BTreeSet<RepresentationId> = copies.unwrap_or_default().into_iter().collect();
                if let Some(r) = want.iter().find(|r| !rows.iter().any(|x| x.rep == **r)) {
                    return Err(format!("{} is not a placed copy of this symbol.", r.hex()));
                }
                rows.iter().filter(|r| want.contains(&r.rep)).collect()
            }
            _ => Vec::new(),
        };
        let upd = update_ops(doc, &mut p.ids, &chosen, id, &def)?;
        ops.extend(upd);
        if !e.builtin && !new_lineage {
            // The new version is embedded in this transaction; earlier versions nothing places go.
            let gone = superseded(doc, e.lineage, id, &ops);
            ops.extend(gone);
        }
        let label = format!("Edit symbol {}", content.name);
        p.apply_edit(pg, "symboledit", &label, ops)?;
        let mut v = result(p.ledger.document(), id);
        v["updated"] = json!(chosen.len());
        v["left"] = json!(rows.len().saturating_sub(chosen.len()));
        Ok(v)
    })
}

fn run_symbolupdate(s: &mut Session, params: &Value) -> cadcraft_engine::Result<Value> {
    let copies = copies_param(params).map_err(err)?;
    let lineage: Option<DefinitionId> = opt_id(params, "lineage").map_err(err)?;
    let to: Option<DefinitionId> = opt_id(params, "to").map_err(err)?;
    with_project(s, |p, pg, _| {
        let doc = p.ledger.document();
        // The lineage: given, or the first named copy's.
        let lin = match (lineage, &copies) {
            (Some(l), _) => l,
            (None, Some(c)) => {
                let r = c.first().and_then(|r| doc.representations.get(r)).ok_or("Name a placed copy or a lineage.")?;
                let (d, _) = crate::devices::rep_definition(doc, r).ok_or("That is not a placed symbol.")?;
                d.origin.as_ref().filter(|o| o.library == PROJECT_SYMBOLS).map(|o| o.definition).ok_or("That copy is not of a project symbol.")?
            }
            (None, None) => return Err("Name the `copies` or the `lineage` to update.".into()),
        };
        let versions = symbols::versions_of(doc, lin);
        let target = match to {
            Some(t) => *versions.iter().find(|v| v.id == t).ok_or("`to` is not a version of that symbol.")?,
            None => *versions.last().ok_or("No such symbol.")?,
        };
        let others: Vec<DefinitionId> = versions.iter().map(|v| v.id).filter(|d| *d != target.id).collect();
        let rows = impact(doc, &others, target.def);
        let chosen: Vec<&Row> = match &copies {
            Some(c) => {
                let want: BTreeSet<&RepresentationId> = c.iter().collect();
                if let Some(r) = want.iter().find(|r| !rows.iter().any(|x| x.rep == ***r)) {
                    return Err(format!("{} does not need an update to this version.", r.hex()));
                }
                rows.iter().filter(|r| want.contains(&r.rep)).collect()
            }
            None => rows.iter().collect(),
        };
        if chosen.is_empty() {
            return Err("Nothing to update: every copy shows this version.".into());
        }
        let mut ops = update_ops(doc, &mut p.ids, &chosen, target.id, target.def)?;
        let gone = superseded(doc, lin, versions.last().map_or(target.id, |v| v.id), &ops);
        ops.extend(gone.into_iter().filter(|o| !matches!(o, Op::RemoveDefinition { id, .. } if *id == target.id)));
        let (label, tid, n) = (format!("Update symbol {}", target.def.name), target.id, chosen.len());
        p.apply_edit(pg, "symbolupdate", &label, ops)?;
        let mut v = result(p.ledger.document(), tid);
        v["updated"] = json!(n);
        Ok(v)
    })
}

// ---------------------------------------------------------------- make symbol (§4)

/// The handles of `params`, or the selection.
fn handles(s: &Session, params: &Value) -> Result<Vec<Handle>, String> {
    let Some(v) = params.get("handles").filter(|v| !v.is_null()) else { return Ok(s.selection()) };
    let a = v.as_array().ok_or("`handles` must be an array of hex handles")?;
    if a.len() > crate::commands::MAX_HANDLES {
        return Err(format!("at most {} handles per call", crate::commands::MAX_HANDLES));
    }
    a.iter().map(|h| h.as_str().and_then(Handle::parse_hex).ok_or_else(|| format!("`{h}` is not a valid handle"))).collect()
}

/// The grid point nearest the middle of `prims`' box (design 048 §4.1).
/// The point of the page a made symbol's (0, 0) goes to: the bounding-box centre of `prims` on
/// the nearest point-grid point (design 048 §4.1 step 2).
pub fn grid_origin(prims: &[Primitive]) -> Pt {
    let mut pts: Vec<Pt> = Vec::new();
    for p in prims {
        match p {
            Primitive::Line { a, b } => pts.extend([*a, *b]),
            Primitive::Polyline { vertices, .. } => pts.extend(vertices.iter().copied()),
            Primitive::Arc { center, radius, .. } | Primitive::Circle { center, radius } => {
                pts.extend([Pt::new(center.x - radius, center.y - radius), Pt::new(center.x + radius, center.y + radius)])
            }
            Primitive::Text { at, .. } | Primitive::Point { at } => pts.push(*at),
        }
    }
    let pts: Vec<Pt> = pts.into_iter().filter(|p| p.is_finite()).collect();
    if pts.is_empty() {
        return Pt::new(0.0, 0.0);
    }
    let (mut lo, mut hi) = (pts[0], pts[0]);
    for p in &pts {
        lo = Pt::new(lo.x.min(p.x), lo.y.min(p.y));
        hi = Pt::new(hi.x.max(p.x), hi.y.max(p.y));
    }
    let snap = |x: f64| (x / POINT_GRID_MM).round() * POINT_GRID_MM;
    Pt::new(snap((lo.x + hi.x) / 2.0), snap((lo.y + hi.y) / 2.0))
}

/// `p` moved by `-o` (into the frame whose origin is `o`).
pub fn shifted(p: &Primitive, o: Pt) -> Primitive {
    let m = |q: Pt| Pt::new(q.x - o.x, q.y - o.y);
    match p.clone() {
        Primitive::Line { a, b } => Primitive::Line { a: m(a), b: m(b) },
        Primitive::Polyline { vertices, closed } => Primitive::Polyline { vertices: vertices.into_iter().map(m).collect(), closed },
        Primitive::Arc { center, radius, start, end } => Primitive::Arc { center: m(center), radius, start, end },
        Primitive::Circle { center, radius } => Primitive::Circle { center: m(center), radius },
        Primitive::Text { at, height, rotation, value, halign, valign } => Primitive::Text { at: m(at), height, rotation, value, halign, valign },
        Primitive::Point { at } => Primitive::Point { at: m(at) },
    }
}

fn run_makesymbol(s: &mut Session, params: &Value) -> cadcraft_engine::Result<Value> {
    let component: Option<ComponentId> = opt_id(params, "component").map_err(err)?;
    let hs = if component.is_none() { handles(s, params).map_err(err)? } else { Vec::new() };
    let mut content = parse_content(params).map_err(err)?;
    let replace = params.get("replace").and_then(Value::as_bool).unwrap_or(true);
    with_project(s, |p, pg, page| match component {
        Some(c) => promote(p, pg, c, content),
        None => {
            let map = &p.pages.get(&page).ok_or("page view not found")?.map;
            let doc = p.ledger.document();
            let mut items: Vec<ItemId> = Vec::new();
            for h in hs {
                match map.get(h) {
                    Some(Mapped::Item(i)) if doc.representation_of_item(i).is_none() => items.push(i),
                    Some(Mapped::Item(i)) => return Err(not_plain(doc, i)),
                    Some(_) => return Err("Make symbol takes lines, arcs and text; part of the selection already has a meaning.".into()),
                    None => return Err("The selection holds an object this page does not know.".into()),
                }
            }
            if items.is_empty() {
                return Err("Select the lines, arcs and texts to make a symbol of.".into());
            }
            let mut prims = Vec::new();
            for i in &items {
                match doc.items.get(i).map(|x| &x.kind) {
                    Some(ItemKind::Primitive(pr)) => prims.push(pr.clone()),
                    _ => return Err("Make symbol takes lines, arcs and text.".into()),
                }
            }
            // The builder (048c) sends the drawing as edited, in the symbol's frame, with `at` the
            // page point its origin sits on; otherwise the page geometry is taken as it is and
            // points and slots are page points.
            let origin = match params.get("at").filter(|v| !v.is_null()) {
                Some(v) => pt_of(v).filter(|p| p.is_finite()).ok_or("`at` is a page point [x, y].")?,
                None => grid_origin(&prims),
            };
            if content.graphics.is_empty() {
                content.graphics = prims.iter().map(|x| shifted(x, origin)).collect();
                for pt in &mut content.points {
                    pt.at = Pt::new(pt.at.x - origin.x, pt.at.y - origin.y);
                }
                for sl in &mut content.slots {
                    sl.at = Pt::new(sl.at.x - origin.x, sl.at.y - origin.y);
                }
            }
            refuse_problems(&content, &Rules { doc, lineage: None, grid: true })?;
            let (id, lineage): (DefinitionId, DefinitionId) = (p.ids.fresh(), p.ids.fresh());
            let def = version_definition(&content, lineage, 1, None)?;
            let mut ops = vec![Op::EmbedDefinition { id, definition: def.clone() }];
            let mut placed = None;
            if replace {
                for i in &items {
                    if let Some(it) = doc.items.get(i) {
                        ops.push(Op::RemoveItem { id: *i, item: it.clone() });
                    }
                }
                let style = plain_style(p);
                let pl = Placement { origin, rotation: 0.0, mirror: false, scale: 1.0 };
                let plan = plan_place_project(p, page, style, id, &def, pl, None)?;
                placed = Some((plan.component, plan.representation, plan.tag.clone()));
                ops.extend(plan.ops);
            }
            let label = format!("Make symbol {}", content.name);
            p.apply_edit(pg, "makesymbol", &label, ops)?;
            let mut v = result(p.ledger.document(), id);
            if let Some((c, r, t)) = placed {
                v["component"] = json!(c.hex());
                v["representation"] = json!(r.hex());
                v["tag"] = json!(t);
            }
            Ok(v)
        }
    })
}

/// The refusal for item `i` of a selection that has a meaning (design 048 §9.5), naming its
/// object: `Wire 24V-05 is not plain geometry; Make symbol takes lines, arcs and text.`
fn not_plain(doc: &Document, i: ItemId) -> String {
    let what = match doc.representation_of_item(i).map(|(_, r)| r.subject) {
        Some(Subject::Conductor(c)) => format!("Wire {}", wire_number(doc, c)),
        Some(Subject::Component(c)) => crate::devices::tag_words(doc, c),
        Some(Subject::Cable(k)) => crate::references::cable_words(doc, k),
        None => "Part of the selection".to_string(),
    };
    format!("{what} is not plain geometry; Make symbol takes lines, arcs and text.")
}

/// Promote drawn component `c` (design 048 §4.2, §5.4): its drawn representation becomes a
/// symbol representation of the same component, keeping component, terminal and wire IDs.
fn promote(p: &mut Project, pg: &mut crate::access::Pages<'_>, c: ComponentId, mut content: Content) -> Result<Value, String> {
    let doc = p.ledger.document();
    let comp = doc.components.get(&c).ok_or("No such device.")?.clone();
    let symbols_of: Vec<(RepresentationId, &Representation)> = doc.representations_of(Subject::Component(c)).collect();
    if symbols_of.iter().any(|(_, r)| r.form == Form::Symbol) || comp.definition.is_some() {
        return Err("Only a drawn component with no symbols can be made into a symbol.".into());
    }
    let (rid, rep) = symbols_of
        .iter()
        .find(|(_, r)| r.form == Form::Drawn)
        .map(|(i, r)| (*i, (*r).clone()))
        .ok_or("That device has no drawing to make a symbol of.")?;
    let mut prims = Vec::new();
    for i in &rep.items {
        match doc.items.get(i).map(|x| &x.kind) {
            Some(ItemKind::Primitive(pr)) => prims.push(pr.clone()),
            _ => return Err("The drawing holds something that is not a plain shape.".into()),
        }
    }
    let origin = grid_origin(&prims);
    content.graphics = prims.iter().map(|x| shifted(x, origin)).collect();
    // Points: the drawing's connection points, keyed by terminal key, else label, else 1…n.
    let mut points = Vec::new();
    let mut rekey: Vec<(TerminalId, String)> = Vec::new();
    let mut used = BTreeSet::new();
    for (n, (t, at)) in rep.connection_points.iter().enumerate() {
        let term = doc.terminals.get(t).ok_or("A connection point is gone.")?;
        let mut key = term.key.clone().or_else(|| term.label.clone()).unwrap_or_else(|| (n + 1).to_string());
        if !used.insert(key.clone()) {
            key = format!("{}", n + 1);
            used.insert(key.clone());
        }
        if term.key.as_deref() != Some(key.as_str()) {
            rekey.push((*t, key.clone()));
        }
        points.push(TerminalDef { key, label: term.label.clone(), at: Pt::new(at.x - origin.x, at.y - origin.y) });
    }
    content.points = points;
    for sl in &mut content.slots {
        sl.at = Pt::new(sl.at.x - origin.x, sl.at.y - origin.y);
    }
    refuse_problems(&content, &Rules { doc, lineage: None, grid: false })?;
    let (id, lineage, item, new_rep): (DefinitionId, DefinitionId, ItemId, RepresentationId) =
        (p.ids.fresh(), p.ids.fresh(), p.ids.fresh(), p.ids.fresh());
    let def = version_definition(&content, lineage, 1, None)?;
    let style = rep.items.first().and_then(|i| doc.items.get(i)).map_or_else(|| plain_style(p), |i| i.style.clone());
    let mut ops = vec![Op::EmbedDefinition { id, definition: def }, Op::UnlinkRepresentation { id: rid, rep: rep.clone() }];
    for i in &rep.items {
        if let Some(it) = doc.items.get(i) {
            ops.push(Op::RemoveItem { id: *i, item: it.clone() });
        }
    }
    ops.push(Op::AddItem {
        id: item,
        item: Item {
            page: rep.page,
            style,
            kind: ItemKind::Symbol(SymbolInstance { definition: id, placement: Placement { origin, rotation: 0.0, mirror: false, scale: 1.0 } }),
        },
    });
    ops.push(Op::LinkRepresentation {
        id: new_rep,
        rep: Representation {
            subject: Subject::Component(c),
            page: rep.page,
            form: Form::Symbol,
            role: rep.role.clone(),
            items: vec![item],
            connection_points: BTreeMap::new(),
        },
    });
    ops.push(Op::SetComponentDefinition { id: c, before: comp.definition, after: Some(id) });
    for (t, key) in rekey {
        if let Some(term) = doc.terminals.get(&t) {
            ops.push(Op::RemoveTerminal { id: t, terminal: term.clone() });
            ops.push(Op::AddTerminal { id: t, terminal: Terminal { key: Some(key), ..term.clone() } });
        }
    }
    let label = format!("Make symbol {}", content.name);
    p.apply_edit(pg, "makesymbol", &label, ops)?;
    let mut v = result(p.ledger.document(), id);
    v["component"] = json!(c.hex());
    v["representation"] = json!(new_rep.hex());
    Ok(v)
}

// ---------------------------------------------------------------- placing (§5.1)

/// A planned placement of a project symbol.
pub struct PlacedProject {
    pub ops: Vec<Op>,
    pub component: ComponentId,
    pub representation: RepresentationId,
    pub tag: String,
}

/// Plan placing version `def_id` (`def`) at `pl` on `page`: a new device (`join: device`), or a
/// part of device `join` (`join: part`).
///
/// # Errors
/// No tag number left; a part with no device, or a label the device already has.
pub fn plan_place_project(
    p: &mut Project, page: PageId, style: ecad_model::Style, def_id: DefinitionId, def: &Definition, pl: Placement, join: Option<ComponentId>,
) -> Result<PlacedProject, String> {
    let key = def.defaults.get("key").cloned().unwrap_or_default();
    let (item, representation): (ItemId, RepresentationId) = (p.ids.fresh(), p.ids.fresh());
    let instance = Item { page, style, kind: ItemKind::Symbol(SymbolInstance { definition: def_id, placement: pl }) };
    let mut ops = Vec::new();
    let doc = p.ledger.document();
    match (symbols::join_of(def), join) {
        (Join::Part, None) => Err(format!("{} is part of a device: name the `device` it joins.", def.name)),
        (_, Some(c)) => {
            if !doc.components.contains_key(&c) {
                return Err("No such device.".into());
            }
            let have = crate::devices::labels_of(doc, c);
            let mut cps = BTreeMap::new();
            for td in &def.terminals {
                let label = td.label.clone().unwrap_or_else(|| td.key.clone());
                if have.contains(&label) {
                    return Err(format!("{} already has a connection point {label}.", crate::devices::tag_words(doc, c)));
                }
                let t: TerminalId = p.ids.fresh();
                ops.push(Op::AddTerminal { id: t, terminal: Terminal { component: c, label: Some(label.clone()), key: Some(label) } });
                cps.insert(t, td.at);
            }
            ops.push(Op::AddItem { id: item, item: instance });
            ops.push(Op::LinkRepresentation {
                id: representation,
                rep: Representation {
                    subject: Subject::Component(c),
                    page,
                    form: Form::Symbol,
                    role: key,
                    items: vec![item],
                    connection_points: cps,
                },
            });
            Ok(PlacedProject { ops, component: c, representation, tag: crate::devices::tag_words(doc, c) })
        }
        (Join::Device, None) => {
            let prefix = field_value(doc, &key, def, Field::Prefix).unwrap_or_else(|| "X".into());
            let tag = p.tag_high.next_auto(&p.ledger, &prefix)?;
            let doc = p.ledger.document();
            let c: ComponentId = p.ids.fresh();
            let mut properties = ecad_model::Properties::new();
            properties.insert(ecad_model::tag::TAG_KEY.into(), tag.clone());
            let description = field_value(doc, &key, def, Field::Description).unwrap_or_else(|| def.name.clone());
            properties.insert("description".into(), description);
            if let Some(r) = field_value(doc, &key, def, Field::Rating) {
                properties.insert(symbols::RATING_KEY.into(), r);
            }
            ops.push(Op::CreateComponent { id: c, component: Component { properties, definition: Some(def_id) } });
            for td in &def.terminals {
                let t: TerminalId = p.ids.fresh();
                ops.push(Op::AddTerminal { id: t, terminal: Terminal { component: c, label: td.label.clone(), key: Some(td.key.clone()) } });
            }
            ops.push(Op::AddItem { id: item, item: instance });
            ops.push(Op::LinkRepresentation {
                id: representation,
                rep: Representation {
                    subject: Subject::Component(c),
                    page,
                    form: Form::Symbol,
                    role: "main".into(),
                    items: vec![item],
                    connection_points: BTreeMap::new(),
                },
            });
            Ok(PlacedProject { ops, component: c, representation, tag })
        }
    }
}

/// `field` for a new placement of `def` (key `key`): the project's override, else the definition's
/// own value (the definition need not be embedded yet: `makesymbol` places it in its own
/// transaction).
fn field_value(doc: &Document, key: &str, def: &Definition, field: Field) -> Option<String> {
    crate::defaults::symbols::project_value(doc, key, field).or_else(|| crate::defaults::symbols::definition_value(def, field))
}

/// `place {symbol: <project key>, at, rotation?, mirror?, device?}` (design 048 §5.1): the newest
/// version of that project symbol. `None` when `key` names no project symbol.
pub(crate) fn place_project_key(s: &mut Session, params: &Value, key: &str) -> cadcraft_engine::Result<Option<Value>> {
    let at = params.get("at").and_then(crate::commands::point).ok_or_else(|| err("`at` [x,y] is required"))?;
    if !(at.x.abs() <= crate::interact::MAX_COORD && at.y.abs() <= crate::interact::MAX_COORD) {
        return Err(err(crate::interact::OUT_OF_RANGE));
    }
    let degrees = match params.get("rotation") {
        None | Some(Value::Null) => 0.0,
        Some(v) => v.as_f64().filter(|d| d.is_finite()).ok_or_else(|| err("`rotation` must be a finite number of degrees"))?,
    };
    let mirror = params.get("mirror").and_then(Value::as_bool).unwrap_or(false);
    let device: Option<ComponentId> = opt_id(params, "device").map_err(err)?;
    let key = key.to_string();
    with_project(s, |p, pg, page| {
        let doc = p.ledger.document();
        let Some(v) = symbols::project_by_key(doc, &key) else { return Ok(None) };
        let (id, def) = (v.id, v.def.clone());
        let pl = Placement { origin: at, rotation: degrees.to_radians(), mirror, scale: 1.0 };
        let style = plain_style(p);
        let plan = plan_place_project(p, page, style, id, &def, pl, device)?;
        let label = if device.is_some() { format!("Add symbol to {}", plan.tag) } else { "Place symbol".to_string() };
        p.apply_edit(pg, "place", &label, plan.ops)?;
        let doc = p.ledger.document();
        let terminals: Vec<Value> = doc.terminals_of(plan.component).map(|(t, x)| json!({"id": t.hex(), "key": x.key, "label": x.label})).collect();
        Ok(Some(json!({
            "component": plan.component.hex(), "representation": plan.representation.hex(), "definition": id.hex(),
            "tag": plan.tag, "joined": device.is_some(), "terminals": terminals,
        })))
    })
}

/// Whether placed copy `rep` shows an older version than its lineage's newest (design 048 §8.4).
pub fn update_available(doc: &Document, rep: RepresentationId) -> Option<(DefinitionId, u64, u64)> {
    let r = doc.representations.get(&rep)?;
    let (d, _) = crate::devices::rep_definition(doc, r)?;
    let o = d.origin.as_ref().filter(|o| o.library == PROJECT_SYMBOLS)?;
    let n = symbols::newest(doc, o.definition)?;
    (n.seq > o.library_seq).then_some((n.id, o.library_seq, n.seq))
}

// ---------------------------------------------------------------- family entries (FP6)

/// `setdefault {key: symbol.<key>.<field>, level?: project, value | reset: true}` for a symbol's
/// family entry (design 048 §7): the project level only. A prefix is written to the project's
/// `tag_prefixes`, a description or rating to the project property of the entry's key. `None`
/// when `key` names no family entry.
pub(crate) fn run_set_family(s: &mut Session, params: &Value, key: &str) -> cadcraft_engine::Result<Option<Value>> {
    // Only `symbol.<key>.<field>` keys: any other key is "No default" for the caller to say.
    if !key.trim().starts_with("symbol.") {
        return Ok(None);
    }
    let level = params.get("level").and_then(Value::as_str).unwrap_or("project").trim().to_ascii_lowercase();
    let reset = params.get("reset").and_then(Value::as_bool).unwrap_or(false);
    let value_param = params.get("value").cloned();
    let key = key.to_string();
    with_project(s, |p, pg, _| {
        let doc = p.ledger.document();
        let Some((symbol, field)) = crate::defaults::symbols::family(doc, &key) else { return Ok(None) };
        let value = match (value_param, reset) {
            (Some(Value::String(v)), false) => Some(v.trim().to_string()),
            (None | Some(Value::Null), true) => None,
            (Some(_), true) => return Err("Give either `value` or `reset: true`, not both.".into()),
            _ => return Err(crate::default_cmds::NEEDS_VALUE.into()),
        };
        if level != "project" {
            return Err(format!("{key} is set for the project; a symbol's own value is edited in the symbol."));
        }
        if !field.overridable() {
            return Err(format!("{key} is the symbol's own: edit the symbol to change it."));
        }
        if let Some(v) = &value
            && !field.accepts(v)
        {
            return Err(format!("{v} is not a {} ({} characters at most).", field.label().to_lowercase(), field.max_chars()));
        }
        let ops = match field {
            Field::Prefix => {
                let before = doc.settings.clone();
                let mut after = before.clone();
                let px = value.as_deref().and_then(crate::devices::valid_prefix);
                match px {
                    Some(px) => after.tag_prefixes.insert(symbol.clone(), px),
                    None => after.tag_prefixes.remove(&symbol),
                };
                if after == before { Vec::new() } else { vec![Op::SetProjectSettings { before, after }] }
            }
            _ => {
                let k = crate::defaults::symbols::family_property(&symbol, field).unwrap_or_default();
                let before = doc.properties.get(&k).cloned();
                if before == value {
                    Vec::new()
                } else {
                    vec![Op::SetProperty { object: ecad_model::ObjectRef::Project, key: k, before, after: value.clone() }]
                }
            }
        };
        let changed = !ops.is_empty();
        if changed {
            p.apply_edit(pg, "setdefault", &format!("Default {}: {symbol}", field.label()), ops)?;
        }
        let mut v = crate::defaults::symbols::family_json(p.ledger.document(), &symbol, field);
        v["changed"] = json!(changed);
        Ok(Some(v))
    })
}
