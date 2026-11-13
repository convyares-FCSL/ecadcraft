//! Brief 031's commands (design 031 §10): `textstyle` (Q1), `showtext`, `hiddentext` and
//! `pagetexts` (Q3), `movetext` (Q2), and `defaults` (§10.8, the defaults registry's read-only list). Writes commit through [`crate::project::Project::apply_edit`] as
//! one transaction; reads change nothing and run with writes off.

use cadcraft_engine::{CommandSpec, Session};
use ecad_model::{ComponentId, Document, ObjectRef, Op, PageId, RepresentationId};
use serde_json::{Map, Value, json};

use crate::commands::{err, opt_id, with_any_page, with_project};
use crate::defaults::{self, Entry, Level, Resolved, Scope, Source};
use crate::text_style::{Fonts, TextKind};

/// Refusals (design 031 §10.1).
pub const BAD_HEIGHT: &str = "A text height is 0.5 to 10 mm.";
pub const BAD_WEIGHT: &str = "Weight is regular or bold.";
pub const BAD_SHOW: &str = "Shown is on, off or first (first: tag only).";
pub const BAD_FONT: &str = "A font name is 1 to 64 characters with no control characters.";
pub const NEEDS_KIND: &str = "height, weight and show need a kind.";

/// The commands of this module, appended to the registry by [`crate::commands::specs`].
pub fn specs() -> Vec<CommandSpec> {
    vec![
        CommandSpec::new("textstyle", "Text Styles", run_textstyle)
            .params(
                "{font?: string|null, kind?: word, height?: number|null (cap height, mm, 0.5-10), weight?: \"regular\"|\"bold\"|null, \
show?: \"on\"|\"off\"|\"first\"|null} (no fields: read only; all given fields are one transaction; null: back to the default)",
            )
            .noundo(),
        CommandSpec::new("showtext", "Show/Hide Text", run_showtext)
            .params(
                "{kind: word, show: \"on\"|\"off\"|\"first\"|null, representation?: id | component?: id | conductor?: id | page?: id} \
(at most one target; none = the project; null: inherit)",
            )
            .interactive(crate::interact::text_tools::showtext)
            .noundo(),
        CommandSpec::new("movetext", "Move Text", run_movetext)
            .params(
                "{representation: id, kind?: \"tag\"|\"point_label\", terminal?: id, to: [x,y], justify?: \"RT\"…} or {representation, kind?, terminal?, reset: true} \
(to is a page point; stored in the symbol's frame)",
            )
            .interactive(crate::interact::text_tools::movetext)
            .noundo(),
        CommandSpec::new("hiddentext", "Show Hidden Text", run_hiddentext).params("{on?: bool} (display only; no field toggles)").noundo(),
        CommandSpec::new("pagetexts", "Page Texts", run_pagetexts)
            .params("{page?: id} (every derived text on the page, hidden ones too; read only)")
            .noundo(),
        CommandSpec::new("defaults", "Defaults", run_defaults)
            .params("{group?: string, key?: string, page?: id, component?: id, representation?: id} (read only)")
            .noundo(),
    ]
}

/// `Unknown text kind "x". Kinds: tag, point_label, …`
fn unknown_kind(word: &str) -> String {
    let all: Vec<&str> = TextKind::ALL.iter().map(|k| k.word()).collect();
    format!("Unknown text kind \"{word}\". Kinds: {}.", all.join(", "))
}

/// The kind a call names.
pub(crate) fn kind_param(params: &Value) -> Result<Option<TextKind>, String> {
    match params.get("kind") {
        None | Some(Value::Null) => Ok(None),
        Some(Value::String(w)) => {
            let w = w.trim().to_ascii_lowercase();
            TextKind::from_word(&w).map(Some).ok_or_else(|| unknown_kind(&w))
        }
        Some(_) => Err(unknown_kind("?")),
    }
}

/// A field that may be absent (not given), `null` (back to the default) or a value.
enum Field<T> {
    Absent,
    Clear,
    Set(T),
}

fn field<T>(params: &Value, key: &str, parse: impl FnOnce(&Value) -> Option<T>, bad: &str) -> Result<Field<T>, String> {
    match params.get(key) {
        None => Ok(Field::Absent),
        Some(Value::Null) => Ok(Field::Clear),
        Some(v) => parse(v).map(Field::Set).ok_or_else(|| bad.to_string()),
    }
}

/// One `text.*` project property change, or nothing when it is already so.
fn set_project(doc: &Document, key: &str, after: Option<String>, ops: &mut Vec<Op>) {
    let before = doc.properties.get(key).cloned();
    if before != after {
        ops.push(Op::SetProperty { object: ObjectRef::Project, key: key.to_string(), before, after });
    }
}

/// The stored text of a number: as short as it reads (`3`, `2.5`).
fn number_text(x: f64) -> String {
    format!("{x}")
}

/// One kind's changes in a `textstyle` call.
struct KindChange {
    kind: TextKind,
    /// The kind's own font (owner, 2026-11-12); `null` goes back to the project font.
    font: Field<String>,
    height: Field<f64>,
    weight: Field<String>,
    show: Field<String>,
}

/// The kind changes of `params` (`kind` with `height`/`weight`/`show`); `None` when it names no
/// kind and changes none of them.
fn kind_change(params: &Value) -> Result<Option<KindChange>, String> {
    let kind = kind_param(params)?;
    let height = field(params, "height", |v| v.as_f64().filter(|h| h.is_finite() && (0.5..=10.0).contains(h)), BAD_HEIGHT)?;
    let weight =
        field(params, "weight", |v| v.as_str().map(|w| w.trim().to_ascii_lowercase()).filter(|w| w == "regular" || w == "bold"), BAD_WEIGHT)?;
    let show = field(
        params,
        "show",
        |v| v.as_str().map(|w| w.trim().to_ascii_lowercase()).filter(|w| ["on", "off", "first"].contains(&w.as_str())),
        BAD_SHOW,
    )?;
    let changes = !matches!(height, Field::Absent) || !matches!(weight, Field::Absent) || !matches!(show, Field::Absent);
    let Some(kind) = kind else {
        return if changes { Err(NEEDS_KIND.into()) } else { Ok(None) };
    };
    // With a kind, `font` is that kind's own font.
    let font = field(params, "font", |v| v.as_str().map(str::trim).map(str::to_string), BAD_FONT)?;
    if let Field::Set(f) = &font
        && kind.font_entry().parse(f).is_none()
    {
        return Err(BAD_FONT.into());
    }
    if let Field::Set(w) = &show {
        let entry = kind.show_entry().ok_or_else(|| format!("{} has no Shown setting.", kind.word()))?;
        if entry.parse(w).is_none() {
            return Err(BAD_SHOW.into());
        }
    }
    Ok(Some(KindChange { kind, font, height, weight, show }))
}

/// The ops of one kind's changes.
fn kind_ops(doc: &Document, c: &KindChange, ops: &mut Vec<Op>) {
    let k = c.kind;
    let h = match c.height {
        Field::Absent => None,
        Field::Clear => Some(None),
        Field::Set(h) => Some(Some(h)),
    };
    if let Some(h) = h {
        if k == TextKind::WireLabel {
            // One stored value (§3.4): the wiring setting, never a property. A later row of the
            // same call replaces an earlier one.
            ops.retain(|o| !matches!(o, Op::SetProjectSettings { .. }));
            let before = doc.settings.clone();
            let mut after = before.clone();
            after.wiring.label_height = h.unwrap_or(ecad_model::wiring::DEFAULT_LABEL_HEIGHT);
            if after != before {
                ops.push(Op::SetProjectSettings { before, after });
            }
        } else {
            set_project(doc, k.height_entry().key, h.map(number_text), ops);
        }
    }
    match &c.font {
        Field::Absent => {}
        Field::Clear => set_project(doc, k.font_entry().key, None, ops),
        Field::Set(f) => set_project(doc, k.font_entry().key, Some(f.clone()), ops),
    }
    match &c.weight {
        Field::Absent => {}
        Field::Clear => set_project(doc, k.weight_entry().key, None, ops),
        Field::Set(w) => set_project(doc, k.weight_entry().key, Some(w.clone()), ops),
    }
    if let Some(entry) = k.show_entry() {
        match &c.show {
            Field::Absent => {}
            Field::Clear => set_project(doc, entry.key, None, ops),
            Field::Set(w) => set_project(doc, entry.key, Some(w.clone()), ops),
        }
    }
}

/// Most kinds one `textstyle` call changes (the style table has fewer; a guard on hostile input).
const MAX_KINDS: usize = 64;

/// `textstyle {font?, kind?, height?, weight?, show?, kinds?: [{kind, height?, weight?, show?}]}`
/// (design 031 §10.1; `kinds` lets the Text section write several kinds as one transaction).
fn run_textstyle(s: &mut Session, params: &Value) -> cadcraft_engine::Result<Value> {
    let given = ["font", "kind", "height", "weight", "show", "kinds"].iter().any(|k| params.get(*k).is_some());
    if !given {
        return with_any_page(s, |p, _| Ok(textstyle_json(p.ledger.document(), false)));
    }
    // The project font: `font` with no `kind` (with a kind it is that kind's font).
    let font = if params.get("kind").is_some_and(|k| !k.is_null()) {
        Field::Absent
    } else {
        field(params, "font", |v| v.as_str().map(str::trim).map(str::to_string), BAD_FONT).map_err(err)?
    };
    if let Field::Set(f) = &font
        && defaults::text::FONT.parse(f).is_none()
    {
        return Err(err(BAD_FONT));
    }
    let mut changes: Vec<KindChange> = kind_change(params).map_err(err)?.into_iter().collect();
    match params.get("kinds") {
        None | Some(Value::Null) => {}
        Some(Value::Array(a)) if a.len() <= MAX_KINDS => {
            for row in a {
                match kind_change(row).map_err(err)? {
                    Some(c) => changes.push(c),
                    None => return Err(err(NEEDS_KIND)),
                }
            }
        }
        Some(_) => return Err(err(format!("`kinds` is a list of at most {MAX_KINDS} {{kind, height?, weight?, show?}}."))),
    }
    with_project(s, |p, pg, _| {
        let doc = p.ledger.document();
        let mut ops = Vec::new();
        match &font {
            Field::Absent => {}
            Field::Clear => set_project(doc, defaults::text::FONT.key, None, &mut ops),
            Field::Set(f) => set_project(doc, defaults::text::FONT.key, Some(f.clone()), &mut ops),
        }
        for c in &changes {
            kind_ops(doc, c, &mut ops);
        }
        // A key given twice (two rows of one kind): the last value wins, in one op.
        let mut seen = std::collections::BTreeSet::new();
        let mut kept: Vec<Op> = Vec::new();
        for op in ops.into_iter().rev() {
            let key = match &op {
                Op::SetProperty { key, .. } => Some(key.clone()),
                _ => None,
            };
            if key.as_ref().is_none_or(|k| seen.insert(k.clone())) {
                kept.push(op);
            }
        }
        kept.reverse();
        let changed = !kept.is_empty();
        if changed {
            p.apply_edit(pg, "textstyle", "Text styles", kept)?;
        }
        Ok(textstyle_json(p.ledger.document(), changed))
    })
}

/// The style table as `textstyle` returns it: `{font, drawn_with, status, height_is, kinds: [...],
/// changed}`. Heights are cap heights (brief 040, owner answer 9): CADCraft draws a text's height
/// as the height of its capitals, so `height` 2.5 is 2.5 mm from baseline to the top of a `K`.
pub fn textstyle_json(doc: &Document, changed: bool) -> Value {
    let fonts = Fonts::resolve(doc);
    let kinds: Vec<Value> = TextKind::ALL
        .iter()
        .map(|k| {
            let h = defaults::resolve(doc, k.height_entry(), Scope::project());
            let w = defaults::resolve(doc, k.weight_entry(), Scope::project());
            let sh = k.show_entry().map(|e| defaults::resolve(doc, e, Scope::project()));
            let default = h.from == Source::Builtin && w.from == Source::Builtin && sh.is_none_or(|r| r.from == Source::Builtin);
            let builtin = |e: &Entry| Resolved { value: e.builtin, from: Source::Builtin };
            json!({
                "kind": k.word(),
                "height": h.number(),
                "weight": w.word(),
                "font": crate::text_style::kind_font(doc, *k),
                "show": sh.map(|r| r.word()),
                "default": default,
                "builtin": {
                    "height": builtin(k.height_entry()).number(),
                    "weight": builtin(k.weight_entry()).word(),
                    "show": k.show_entry().map(|e| builtin(e).word()),
                },
            })
        })
        .collect();
    json!({
        "font": fonts.wanted,
        "drawn_with": fonts.drawn_with,
        "status": fonts.status(),
        "height_is": "cap height",
        "kinds": kinds,
        "changed": changed,
    })
}

/// The JSON form of a registry value.
fn value_json(r: &Resolved<'_>) -> Value {
    match r.value {
        defaults::Value::Length(x) | defaults::Value::Number(x) => json!(x),
        defaults::Value::Choice(w) => json!(w),
        defaults::Value::Text(t) => json!(t),
    }
}

fn level_word(l: Level) -> &'static str {
    match l {
        Level::Standard => "standard",
        Level::Project => "project",
        Level::Page => "page",
        Level::Object => "object",
    }
}

/// One registry entry as `defaults` lists it, resolved at `at`.
pub fn entry_json(doc: &Document, e: &Entry, at: Scope) -> Value {
    let r = defaults::resolve(doc, e, at);
    let builtin = Resolved { value: e.builtin, from: Source::Builtin };
    let mut m = Map::new();
    m.insert("key".into(), json!(e.key));
    m.insert("label".into(), json!(e.label));
    m.insert("group".into(), json!(e.group));
    m.insert("brief".into(), json!(e.brief));
    m.insert("kind".into(), json!(e.kind_word()));
    m.insert("builtin".into(), value_json(&builtin));
    m.insert("levels".into(), json!(e.levels.iter().map(|l| level_word(*l)).collect::<Vec<_>>()));
    m.insert("value".into(), value_json(&r));
    m.insert("from".into(), json!(r.from.as_str()));
    Value::Object(m)
}

/// The scope a `defaults` (or `showtext`) call names, checked against `doc`.
pub(crate) fn scope_param(doc: &Document, params: &Value) -> Result<Scope, String> {
    let rep: Option<RepresentationId> = opt_id(params, "representation")?;
    let component: Option<ComponentId> = opt_id(params, "component")?;
    let page: Option<PageId> = opt_id(params, "page")?;
    if let Some(r) = rep {
        if !doc.representations.contains_key(&r) {
            return Err(format!("No representation {}.", r.hex()));
        }
        return Ok(Scope::symbol(doc, r));
    }
    let mut at = Scope::project();
    if let Some(c) = component {
        if !doc.components.contains_key(&c) {
            return Err(format!("No component {}.", c.hex()));
        }
        at.component = Some(c);
    }
    if let Some(pg) = page {
        if !doc.pages.contains_key(&pg) {
            return Err(format!("No page {}.", pg.hex()));
        }
        at.page = Some(pg);
    }
    Ok(at)
}

/// `defaults {group?, key?, page?, component?, representation?}` (design 031 §10.8).
fn run_defaults(s: &mut Session, params: &Value) -> cadcraft_engine::Result<Value> {
    let group = params.get("group").and_then(Value::as_str).map(str::trim).map(str::to_string);
    let key = params.get("key").and_then(Value::as_str).map(str::trim).map(str::to_string);
    with_any_page(s, |p, _| {
        let doc = p.ledger.document();
        let at = scope_param(doc, params)?;
        if let Some(k) = &key
            && defaults::find(k).is_none()
        {
            return Err(format!("No default \"{k}\"."));
        }
        let list: Vec<Value> = defaults::all()
            .filter(|e| group.as_deref().is_none_or(|g| e.group.eq_ignore_ascii_case(g)))
            .filter(|e| key.as_deref().is_none_or(|k| e.key == k))
            .map(|e| entry_json(doc, e, at))
            .collect();
        Ok(json!({ "defaults": list }))
    })
}

// ---------------------------------------------------------------- showtext (§10.2)

/// The level a `showtext` call writes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ShowLevel {
    Symbol(RepresentationId, ComponentId),
    Device(ComponentId),
    Conductor(ecad_model::ConductorId),
    Page(PageId),
    Project,
}

impl ShowLevel {
    /// The word `showtext` returns.
    pub fn word(self) -> &'static str {
        match self {
            ShowLevel::Symbol(..) => "symbol",
            ShowLevel::Device(_) => "device",
            ShowLevel::Conductor(_) => "conductor",
            ShowLevel::Page(_) => "page",
            ShowLevel::Project => "project",
        }
    }
}

/// The kinds `showtext` sets at a level (design 031 §5.1).
fn kinds_at(level: ShowLevel) -> &'static [TextKind] {
    use TextKind::*;
    match level {
        ShowLevel::Symbol(..) | ShowLevel::Device(_) => &[Tag, PointLabel, Description, PartNumber, ContinuationName],
        ShowLevel::Conductor(_) => &[WireLabel],
        ShowLevel::Page(_) | ShowLevel::Project => &[Tag, PointLabel, Description, PartNumber, ContinuationName, Reference, LocationLabel, WireLabel],
    }
}

/// The object and key a visibility at `level` is stored under.
pub fn show_key(level: ShowLevel, kind: TextKind) -> Option<(ObjectRef, String)> {
    let entry = kind.show_entry()?;
    Some(match level {
        ShowLevel::Symbol(rep, c) => (ObjectRef::Component(c), defaults::symbol_key(entry.key, rep)),
        ShowLevel::Device(c) => (ObjectRef::Component(c), entry.key.to_string()),
        ShowLevel::Conductor(c) => (ObjectRef::Conductor(c), entry.key.to_string()),
        ShowLevel::Page(p) => (ObjectRef::Page(p), entry.key.to_string()),
        ShowLevel::Project => (ObjectRef::Project, entry.key.to_string()),
    })
}

/// The level a `showtext` call names, checked against `doc`.
pub fn show_level(doc: &Document, params: &Value) -> Result<ShowLevel, String> {
    let rep: Option<RepresentationId> = opt_id(params, "representation")?;
    let comp: Option<ComponentId> = opt_id(params, "component")?;
    let cond: Option<ecad_model::ConductorId> = opt_id(params, "conductor")?;
    let page: Option<PageId> = opt_id(params, "page")?;
    if [rep.is_some(), comp.is_some(), cond.is_some(), page.is_some()].iter().filter(|x| **x).count() > 1 {
        return Err("Name at most one of representation, component, conductor and page.".into());
    }
    if let Some(r) = rep {
        let x = doc.representations.get(&r).ok_or_else(|| format!("No representation {}.", r.hex()))?;
        return match x.subject {
            ecad_model::Subject::Component(c) if x.form == ecad_model::Form::Symbol => Ok(ShowLevel::Symbol(r, c)),
            _ => Err("That representation is not a symbol.".into()),
        };
    }
    if let Some(c) = comp {
        return doc.components.contains_key(&c).then_some(ShowLevel::Device(c)).ok_or_else(|| format!("No component {}.", c.hex()));
    }
    if let Some(c) = cond {
        return doc.conductors.contains_key(&c).then_some(ShowLevel::Conductor(c)).ok_or_else(|| format!("No conductor {}.", c.hex()));
    }
    if let Some(p) = page {
        return doc.pages.contains_key(&p).then_some(ShowLevel::Page(p)).ok_or_else(|| format!("No page {}.", p.hex()));
    }
    Ok(ShowLevel::Project)
}

/// Words for a kind in labels and echoes: `tag`, `connection point labels`…
pub fn kind_words(kind: TextKind) -> &'static str {
    match kind {
        TextKind::Tag => "tag",
        TextKind::PointLabel => "connection point labels",
        TextKind::Description => "description",
        TextKind::PartNumber => "part number",
        TextKind::ContinuationName => "continuation name",
        TextKind::Reference => "references",
        TextKind::LocationLabel => "location labels",
        TextKind::WireLabel => "wire labels",
        _ => kind.word(),
    }
}

/// The ops of one `showtext` change: `[]` when it is already so.
pub fn show_ops(doc: &Document, level: ShowLevel, kind: TextKind, show: Option<&str>) -> Result<Vec<Op>, String> {
    let allowed = kinds_at(level);
    if !allowed.contains(&kind) {
        let words: Vec<&str> = allowed.iter().map(|k| k.word()).collect();
        return Err(format!("At {} level the kinds are {}.", level.word(), words.join(", ")));
    }
    if show == Some("first") && kind != TextKind::Tag {
        return Err(BAD_SHOW.into());
    }
    let (object, key) = show_key(level, kind).ok_or(BAD_SHOW)?;
    let props = match object {
        ObjectRef::Component(c) => doc.components.get(&c).map(|x| &x.properties),
        ObjectRef::Conductor(c) => doc.conductors.get(&c).map(|x| &x.properties),
        ObjectRef::Page(p) => doc.pages.get(&p).map(|x| &x.properties),
        ObjectRef::Project => Some(&doc.properties),
        ObjectRef::Cable(_) => None,
    }
    .ok_or("No such object.")?;
    let before = props.get(&key).cloned();
    let after = show.map(str::to_string);
    Ok(if before == after { Vec::new() } else { vec![Op::SetProperty { object, key, before, after }] })
}

/// The transaction label of a `showtext` change (`Hide tag -X01`, `Show tag`, `Reset tag`).
pub fn show_label(doc: &Document, level: ShowLevel, kind: TextKind, show: Option<&str>) -> String {
    let verb = match show {
        Some("off") => "Hide",
        Some(_) => "Show",
        None => "Reset",
    };
    let of = match level {
        ShowLevel::Symbol(_, c) | ShowLevel::Device(c) => format!(" {}", crate::devices::tag_words(doc, c)),
        _ => String::new(),
    };
    format!("{verb} {}{of}", kind_words(kind))
}

/// `showtext {kind, show, representation? | component? | conductor? | page?}` (design 031 §10.2).
fn run_showtext(s: &mut Session, params: &Value) -> cadcraft_engine::Result<Value> {
    let kind = kind_param(params).map_err(err)?.ok_or_else(|| err("`kind` is required."))?;
    let show = match params.get("show") {
        Some(Value::Null) => None,
        Some(Value::String(w)) => {
            let w = w.trim().to_ascii_lowercase();
            if !["on", "off", "first"].contains(&w.as_str()) {
                return Err(err(BAD_SHOW));
            }
            Some(w)
        }
        _ => return Err(err("`show` is required: \"on\", \"off\", \"first\" or null.")),
    };
    with_project(s, |p, pg, _| {
        let doc = p.ledger.document();
        let level = show_level(doc, params)?;
        let ops = show_ops(doc, level, kind, show.as_deref())?;
        let label = show_label(doc, level, kind, show.as_deref());
        let changed = !ops.is_empty();
        if changed {
            p.apply_edit(pg, "showtext", &label, ops)?;
        }
        let target = match level {
            ShowLevel::Symbol(r, _) => json!(r.hex()),
            ShowLevel::Device(c) => json!(c.hex()),
            ShowLevel::Conductor(c) => json!(c.hex()),
            ShowLevel::Page(pg) => json!(pg.hex()),
            ShowLevel::Project => Value::Null,
        };
        Ok(json!({"level": level.word(), "target": target, "kind": kind.word(), "show": show, "changed": changed}))
    })
}

/// `hiddentext {on?}`: Show Hidden Text on, off, or toggled (design 031 §5.7). Display only.
fn run_hiddentext(s: &mut Session, params: &Value) -> cadcraft_engine::Result<Value> {
    let on = match params.get("on") {
        None | Some(Value::Null) => None,
        Some(v) => Some(v.as_bool().ok_or_else(|| err("`on` is true or false."))?),
    };
    with_any_page(s, |p, pg| {
        let want = on.unwrap_or(!p.show_hidden_text);
        let changed = p.set_hidden_text(pg, want);
        Ok(json!({"on": want, "changed": changed}))
    })
}

/// Most rows `pagetexts` returns.
pub const MAX_TEXT_ROWS: usize = 5_000;

/// `pagetexts {page?}` (design 031 §10.4; named so because `texts` is CADCraft's).
fn run_pagetexts(s: &mut Session, params: &Value) -> cadcraft_engine::Result<Value> {
    let page: Option<PageId> = opt_id(params, "page").map_err(err)?;
    with_any_page(s, |p, _| {
        let doc = p.ledger.document();
        let page = page.or(p.active_page()).ok_or("`page` (hex id) is required: no page is shown")?;
        if !doc.pages.contains_key(&page) {
            return Err(format!("No page {}.", page.hex()));
        }
        Ok(pagetexts_json(doc, page))
    })
}

/// The rows of `pagetexts` for `page`.
pub fn pagetexts_json(doc: &Document, page: PageId) -> Value {
    use crate::text_style::{TextStyle, Weight, justify_code, text_box, width_in};
    let look = crate::text_style::Look::new(doc);
    let all = crate::devices::all_texts_on_page(doc, page);
    let more = all.len().saturating_sub(MAX_TEXT_ROWS);
    let rows: Vec<Value> = all
        .iter()
        .take(MAX_TEXT_ROWS)
        .map(|t| {
            let d = &t.text;
            let kind = crate::devices::kind_of(d.of);
            let style = look.style(kind);
            let w = width_in(look.fonts_for(kind), TextStyle { height: d.height, ..style }, &d.value);
            let (lo, hi) = text_box(w, d.height, d.at, d.halign, d.valign);
            let component = doc.representations.get(&d.rep).and_then(|r| match r.subject {
                ecad_model::Subject::Component(c) => Some(c.hex()),
                _ => None,
            });
            let weight = match style.weight {
                Weight::Bold => "bold",
                Weight::Regular => "regular",
            };
            let mut row = json!({
                "representation": d.rep.hex(),
                "component": component,
                "kind": kind.word(),
                "value": d.value,
                "at": [d.at.x, d.at.y],
                "justify": justify_code(d.halign, d.valign),
                "height": d.height,
                "weight": weight,
                "shown": t.shown,
                "moved": t.moved,
                "overlaps": t.overlaps,
                "box": [[lo.x, lo.y], [hi.x, hi.y]],
            });
            if let crate::devices::TextOf::Terminal(id) = d.of {
                row["terminal"] = json!(id.hex());
            }
            row
        })
        .collect();
    let mut v = json!({ "texts": rows });
    if more > 0 {
        v["more"] = json!(more);
    }
    v
}

// ---------------------------------------------------------------- movetext (§10.3)

pub const BAD_POINT: &str = "A text position must be a finite point.";
pub const BAD_JUSTIFY: &str = "Justify is two letters: L, C or R, then T, M or B.";
pub const NEEDS_TERMINAL: &str = "point_label needs the connection point's key.";

/// The key and current text of a movable derived text: a symbol's tag block, or one of its
/// connection point labels.
pub fn move_target(
    doc: &Document, rep: RepresentationId, kind: TextKind, terminal: Option<ecad_model::TerminalId>,
) -> Result<(ComponentId, String, Option<crate::devices::PageText>), String> {
    let r = doc.representations.get(&rep).ok_or_else(|| format!("No representation {}.", rep.hex()))?;
    let c = match r.subject {
        ecad_model::Subject::Component(c) if r.form == ecad_model::Form::Symbol => c,
        _ => return Err("That representation is not a symbol.".into()),
    };
    let (key, of) = match kind {
        TextKind::Tag => (format!("{}tag.at", crate::text_style::symbol_prefix(rep)), crate::devices::TextOf::Tag),
        TextKind::PointLabel => {
            let t = terminal.ok_or(NEEDS_TERMINAL)?;
            if doc.terminals.get(&t).is_none_or(|x| x.component != c) {
                return Err(format!("No connection point {} on that symbol.", t.hex()));
            }
            (crate::devices::point_label_key(doc, rep, t), crate::devices::TextOf::Terminal(t))
        }
        _ => return Err("Only a tag or a connection point label can be moved.".into()),
    };
    let current = crate::devices::all_texts_on_page(doc, r.page).into_iter().find(|t| t.text.rep == rep && t.text.of == of);
    Ok((c, key, current))
}

/// `movetext {representation, kind, terminal?, to, justify?}` or `{…, reset: true}` (§10.3).
fn run_movetext(s: &mut Session, params: &Value) -> cadcraft_engine::Result<Value> {
    let rep: RepresentationId = opt_id(params, "representation").map_err(err)?.ok_or_else(|| err("`representation` (hex id) is required."))?;
    let kind = kind_param(params).map_err(err)?.unwrap_or(TextKind::Tag);
    let terminal: Option<ecad_model::TerminalId> = opt_id(params, "terminal").map_err(err)?;
    let reset = params.get("reset").and_then(Value::as_bool) == Some(true);
    let to = match params.get("to") {
        _ if reset => None,
        Some(Value::Array(a)) => match (a.first().and_then(Value::as_f64), a.get(1).and_then(Value::as_f64), a.len()) {
            (Some(x), Some(y), 2) if x.is_finite() && y.is_finite() => Some(ecad_model::Pt::new(x, y)),
            _ => return Err(err(BAD_POINT)),
        },
        _ => return Err(err(BAD_POINT)),
    };
    let justify = match params.get("justify") {
        None | Some(Value::Null) => None,
        Some(v) => Some(v.as_str().and_then(crate::text_style::parse_justify).ok_or_else(|| err(BAD_JUSTIFY))?),
    };
    with_project(s, |p, pg, _| {
        let doc = p.ledger.document();
        let (c, key, current) = move_target(doc, rep, kind, terminal)?;
        let r = doc.representations.get(&rep).ok_or("No such representation.")?;
        let (_, pl) = crate::devices::rep_definition(doc, r).ok_or("That symbol cannot be drawn.")?;
        let after = match to {
            None => None,
            Some(to) => {
                let (h, v) = justify
                    .or_else(|| current.as_ref().map(|t| (t.text.halign, t.text.valign)))
                    .unwrap_or((cadcraft_engine::doc::HAlign::Left, cadcraft_engine::doc::VAlign::Top));
                let local = crate::text_style::to_local(&pl, to).ok_or(BAD_POINT)?;
                Some(crate::text_style::at_text(local, h, v))
            }
        };
        let before = doc.components.get(&c).and_then(|x| x.properties.get(&key)).cloned();
        let changed = before != after;
        if changed {
            let what = match kind {
                TextKind::Tag => format!("tag {}", crate::devices::tag_words(doc, c)),
                _ => "label".to_string(),
            };
            let label = if after.is_some() { format!("Move {what}") } else { format!("Reset {what}") };
            p.apply_edit(pg, "movetext", &label, vec![Op::SetProperty { object: ObjectRef::Component(c), key, before, after: after.clone() }])?;
        }
        let doc = p.ledger.document();
        let now = move_target(doc, rep, kind, terminal).ok().and_then(|(_, _, t)| t);
        Ok(json!({
            "representation": rep.hex(),
            "kind": kind.word(),
            "at": if after.is_some() { now.as_ref().map(|t| json!([t.text.at.x, t.text.at.y])) } else { None },
            "justify": now.as_ref().map(|t| crate::text_style::justify_code(t.text.halign, t.text.valign)),
            "changed": changed,
        }))
    })
}
