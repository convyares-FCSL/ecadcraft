//! Brief 031's commands (design 031 §10): `textstyle` (Q1), `showtext`, `hiddentext` and
//! `pagetexts` (Q3), `movetext` (Q2), and `defaults` (§10.8, the defaults registry's read-only
//! list). Writes commit through [`crate::project::Project::apply_edit`] as one transaction; reads
//! change nothing and run with writes off.
//!
//! Each command is a thin shim (brief 070): its params struct reads the JSON, the rules in
//! [`ecad_electrical::cmd::text`] decide, and the shim commits the ops and answers in JSON.

use cadcraft_engine::{CommandSpec, Session};
use ecad_electrical::cmd::text::{self as rules, KindRow, ShowLevel, ShowTarget, TextStyleInput};
use ecad_model::Word;
use ecad_model::{ComponentId, ConductorId, Document, PageId, RepresentationId, TerminalId};
use schemars::JsonSchema;
use serde::Deserialize;
use serde_json::{Map, Value, json};

use crate::cmd::err;
use crate::cmd::input::{Bounded, Command, Point, Typed, nullable};
use crate::cmd::{on_any_page, on_project};
use crate::defaults::{self, Entry, Level, Resolved, Scope, Source};
pub use crate::text_style::kind_words;
use crate::text_style::{Fonts, TextKind};
pub use ecad_electrical::cmd::text::{BAD_FONT, BAD_HEIGHT, BAD_JUSTIFY, BAD_POINT, BAD_SHOW, BAD_WEIGHT, NEEDS_KIND, NEEDS_TERMINAL};

/// The commands of this module with a typed input.
pub const TYPED: &[Typed] = &[
    Typed::of::<TextstyleParams>("textstyle"),
    Typed::of::<ShowtextParams>("showtext"),
    Typed::of::<MovetextParams>("movetext"),
    Typed::of::<HiddentextParams>("hiddentext"),
    Typed::of::<PagetextsParams>("pagetexts"),
    Typed::of::<DefaultsParams>("defaults"),
];

/// The commands of this module, appended to the registry by [`crate::cmd::specs`].
pub fn specs() -> Vec<CommandSpec> {
    vec![
        CommandSpec::new("textstyle", "Text Styles", crate::cmd::input::typed::<TextstyleParams>)
            .params(
                "{font?: string|null, kind?: word, height?: number|null (cap height, mm, 0.5-10), weight?: \"regular\"|\"bold\"|null, \
show?: \"on\"|\"off\"|\"first\"|null} (no fields: read only; all given fields are one transaction; null: back to the default)",
            )
            .noundo(),
        CommandSpec::new("showtext", "Show/Hide Text", crate::cmd::input::typed::<ShowtextParams>)
            .params(
                "{kind: word, show: \"on\"|\"off\"|\"first\"|null, representation?: id | component?: id | conductor?: id | page?: id} \
(at most one target; none = the project; null: inherit)",
            )
            .interactive(crate::interact::text_tools::showtext)
            .noundo(),
        CommandSpec::new("movetext", "Move Text", crate::cmd::input::typed::<MovetextParams>)
            .params(
                "{representation: id, kind?: \"tag\"|\"point_label\", terminal?: id, to: [x,y], justify?: \"RT\"…} or {representation, kind?, terminal?, reset: true} \
(to is a page point; stored in the symbol's frame)",
            )
            .interactive(crate::interact::text_tools::movetext)
            .noundo(),
        CommandSpec::new("hiddentext", "Show Hidden Text", crate::cmd::input::typed::<HiddentextParams>)
            .params("{on?: bool} (display only; no field toggles)")
            .noundo(),
        CommandSpec::new("pagetexts", "Page Texts", crate::cmd::input::typed::<PagetextsParams>)
            .params("{page?: id} (every derived text on the page, hidden ones too; read only)")
            .noundo(),
        CommandSpec::new("defaults", "Defaults", crate::cmd::input::typed::<DefaultsParams>)
            .params("{group?: string, key?: string, page?: id, component?: id, representation?: id} (read only)")
            .noundo(),
    ]
}

// ---------------------------------------------------------------- textstyle (§10.1)

/// One kind's row of `kinds`.
#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
struct KindRowParams {
    /// The kind: tag, point_label, …
    #[serde(default)]
    kind: Option<String>,
    /// The kind's own font, or null for the project font.
    #[serde(default, deserialize_with = "nullable")]
    font: Option<Option<String>>,
    /// The cap height, mm (0.5 to 10), or null for the default.
    #[serde(default, deserialize_with = "nullable")]
    height: Option<Option<f64>>,
    /// "regular" or "bold", or null for the default.
    #[serde(default, deserialize_with = "nullable")]
    weight: Option<Option<String>>,
    /// "on", "off" or "first", or null for the default.
    #[serde(default, deserialize_with = "nullable")]
    show: Option<Option<String>>,
}

impl From<KindRowParams> for KindRow {
    fn from(r: KindRowParams) -> KindRow {
        KindRow { kind: r.kind, font: r.font, height: r.height, weight: r.weight, show: r.show }
    }
}

/// `textstyle {font?, kind?, height?, weight?, show?, kinds?: [{kind, height?, weight?, show?}]}`
/// (design 031 §10.1; `kinds` lets the Text section write several kinds as one transaction). No
/// field reads the table.
#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub(crate) struct TextstyleParams {
    /// The project font (with `kind`: that kind's own font), or null for the default.
    #[serde(default, deserialize_with = "nullable")]
    font: Option<Option<String>>,
    /// The kind `height`, `weight` and `show` change: tag, point_label, …
    #[serde(default, deserialize_with = "nullable")]
    kind: Option<Option<String>>,
    /// The kind's cap height, mm (0.5 to 10), or null for the default.
    #[serde(default, deserialize_with = "nullable")]
    height: Option<Option<f64>>,
    /// "regular" or "bold", or null for the default.
    #[serde(default, deserialize_with = "nullable")]
    weight: Option<Option<String>>,
    /// "on", "off" or "first" (tag only), or null for the default.
    #[serde(default, deserialize_with = "nullable")]
    show: Option<Option<String>>,
    /// More kinds' rows, in the same transaction.
    #[serde(default, deserialize_with = "nullable")]
    kinds: Option<Option<Bounded<KindRowParams, 0, { rules::MAX_KINDS }>>>,
}

impl Command for TextstyleParams {
    fn run(self, s: &mut Session) -> cadcraft_engine::Result<Value> {
        let given =
            [self.font.is_some(), self.kind.is_some(), self.height.is_some(), self.weight.is_some(), self.show.is_some(), self.kinds.is_some()];
        if !given.contains(&true) {
            return on_any_page(s, |p, _| Ok(textstyle_json(p.ledger.document(), false)));
        }
        let input = TextStyleInput {
            font: self.font,
            row: KindRow { kind: self.kind.flatten(), font: None, height: self.height, weight: self.weight, show: self.show },
            kinds: self.kinds.flatten().map(|b| b.0).unwrap_or_default().into_iter().map(KindRow::from).collect(),
        };
        let changes = rules::textstyle_changes(input).map_err(|e| err(e.to_string()))?;
        on_project(s, |p, pg, _| {
            let ops = rules::textstyle_ops(p.ledger.document(), &changes);
            let changed = !ops.is_empty();
            if changed {
                p.apply_edit(pg, "textstyle", "Text styles", ops)?;
            }
            Ok(textstyle_json(p.ledger.document(), changed))
        })
    }
}

/// The style table as `textstyle` returns it: `{font, drawn_with, status, height_is, kinds: [...],
/// changed}`. Heights are cap heights (brief 040, owner answer 9): CADCraft draws a text's height
/// as the height of its capitals, so `height` 2.5 is 2.5 mm from baseline to the top of a `K`.
pub fn textstyle_json(doc: &Document, changed: bool) -> Value {
    let fonts = Fonts::resolve(doc);
    let kinds: Vec<Value> = TextKind::ALL
        .iter()
        .map(|k| {
            let id = crate::text_style::style_of(doc, *k, Scope::project());
            let f = |key: &str| crate::styles::field_of(doc, ecad_model::StyleCatalogue::Text, &id, key, None);
            let h = f("height").unwrap_or(Resolved { value: defaults::Value::Length(2.5), from: Source::Builtin });
            let w = f("weight").unwrap_or(Resolved { value: defaults::Value::Choice("regular"), from: Source::Builtin });
            let colour = f("colour").map(|r| r.text());
            let sh = k.show_entry().map(|e| defaults::resolve(doc, e, Scope::project()));
            let default = h.from == Source::Builtin && w.from == Source::Builtin && sh.is_none_or(|r| r.from == Source::Builtin);
            let builtin = |e: &Entry| Resolved { value: e.builtin(), from: Source::Builtin };
            let cat = crate::styles::catalogue(ecad_model::StyleCatalogue::Text);
            let b = |key: &str| cat.builtin(&k.builtin_style()).and_then(|s| s.value(key));
            json!({
                "kind": k.word(),
                "style": id.text(),
                "height": h.number(),
                "weight": w.word(),
                "colour": colour,
                "font": crate::text_style::kind_font(doc, *k),
                "show": sh.map(|r| r.word()),
                "default": default,
                "builtin": {
                    "height": b("height").map_or(0.0, |v| Resolved { value: v, from: Source::Builtin }.number()),
                    "weight": b("weight").map_or("regular", |v| Resolved { value: v, from: Source::Builtin }.word()),
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
        defaults::Value::Flag(b) => json!(ecad_model::words::flag_word(b)),
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
    let builtin = Resolved { value: e.builtin(), from: Source::Builtin };
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

/// `defaults {group?, key?, page?, component?, representation?}` (design 031 §10.8, read only):
/// the defaults registry, resolved at a symbol, a device and a page, or the project.
#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub(crate) struct DefaultsParams {
    /// Only this group's entries ("Text", "Project symbols", …; any case).
    #[serde(default)]
    group: Option<String>,
    /// Only this entry.
    #[serde(default)]
    key: Option<String>,
    /// Resolve on this page.
    #[serde(default)]
    page: Option<PageId>,
    /// Resolve for this device.
    #[serde(default)]
    component: Option<ComponentId>,
    /// Resolve for this symbol (wins over `component` and `page`).
    #[serde(default)]
    representation: Option<RepresentationId>,
}

impl Command for DefaultsParams {
    fn run(self, s: &mut Session) -> cadcraft_engine::Result<Value> {
        let group = self.group.map(|g| g.trim().to_string());
        let key = self.key.map(|k| k.trim().to_string());
        let (rep, component, page) = (self.representation, self.component, self.page);
        on_any_page(s, |p, _| {
            let doc = p.ledger.document();
            let at = rules::scope(doc, rep, component, page)?;
            if let Some(k) = &key {
                rules::default_key(doc, k)?;
            }
            let mut list: Vec<Value> = defaults::all()
                .filter(|e| group.as_deref().is_none_or(|g| e.group.eq_ignore_ascii_case(g)))
                .filter(|e| key.as_deref().is_none_or(|k| e.key == k))
                .map(|e| entry_json(doc, e, at))
                .collect();
            // Brief 048 (FP6): one line per field of each project symbol, and the asked-for family entry.
            let mut family = defaults::symbols::family_lines(doc);
            if let Some((sym, f)) = key.as_deref().and_then(|k| defaults::symbols::family(doc, k))
                && !family.contains(&(sym.clone(), f))
            {
                family.push((sym, f));
            }
            list.extend(
                family
                    .into_iter()
                    .filter(|_| group.as_deref().is_none_or(|g| g.eq_ignore_ascii_case("Project symbols")))
                    .filter(|(sym, f)| key.as_deref().is_none_or(|k| k == defaults::symbols::family_key(sym, *f)))
                    .map(|(sym, f)| defaults::symbols::family_json(doc, &sym, f)),
            );
            Ok(json!({ "defaults": list }))
        })
    }
}

// ---------------------------------------------------------------- showtext (§10.2)

/// `showtext {kind, show, representation? | component? | conductor? | page?}` (design 031 §10.2):
/// whether a kind of text is shown, at one level.
#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub(crate) struct ShowtextParams {
    /// The kind: tag, point_label, …
    kind: String,
    /// "on", "off" or "first" (tag only), or null to inherit.
    #[serde(deserialize_with = "nullable")]
    #[schemars(required, with = "crate::cmd::input::OrNull<String>")]
    show: Option<Option<String>>,
    /// A symbol of a device.
    #[serde(default)]
    representation: Option<RepresentationId>,
    /// A device.
    #[serde(default)]
    component: Option<ComponentId>,
    /// A wire.
    #[serde(default)]
    conductor: Option<ConductorId>,
    /// A page.
    #[serde(default)]
    page: Option<PageId>,
}

impl Command for ShowtextParams {
    fn run(self, s: &mut Session) -> cadcraft_engine::Result<Value> {
        let kind = rules::kind(&self.kind).map_err(|e| err(e.to_string()))?;
        let show = rules::show_word(self.show.flatten().as_deref()).map_err(|e| err(e.to_string()))?;
        let target = ShowTarget { representation: self.representation, component: self.component, conductor: self.conductor, page: self.page };
        on_project(s, |p, pg, _| {
            let doc = p.ledger.document();
            let level = rules::show_level(doc, &target)?;
            let ops = rules::show_ops(doc, level, kind, show.as_deref())?;
            let label = rules::show_label(doc, level, kind, show.as_deref());
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
}

/// `hiddentext {on?}`: Show Hidden Text on, off, or toggled (design 031 §5.7). Display only.
#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub(crate) struct HiddentextParams {
    /// On or off (default: toggle).
    #[serde(default)]
    on: Option<bool>,
}

impl Command for HiddentextParams {
    fn run(self, s: &mut Session) -> cadcraft_engine::Result<Value> {
        let on = self.on;
        on_any_page(s, |p, pg| {
            let want = on.unwrap_or(!p.show_hidden_text);
            let changed = p.set_hidden_text(pg, want);
            Ok(json!({"on": want, "changed": changed}))
        })
    }
}

/// Most rows `pagetexts` returns.
pub const MAX_TEXT_ROWS: usize = 5_000;

/// `pagetexts {page?}` (design 031 §10.4; named so because `texts` is CADCraft's): every derived
/// text on a page, hidden ones too.
#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub(crate) struct PagetextsParams {
    /// The page (default: the page shown).
    #[serde(default)]
    page: Option<PageId>,
}

impl Command for PagetextsParams {
    fn run(self, s: &mut Session) -> cadcraft_engine::Result<Value> {
        let page = self.page;
        on_any_page(s, |p, _| {
            let shown = p.active_page();
            let doc = p.ledger.document();
            let page = rules::text_page(doc, page, shown)?;
            Ok(pagetexts_json(doc, page))
        })
    }
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

/// `movetext {representation, kind?, terminal?, to, justify?}` or `{…, reset: true}` (§10.3): a
/// symbol's tag or connection point label moved to a page point, or back to its computed place.
#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub(crate) struct MovetextParams {
    /// The symbol.
    representation: RepresentationId,
    /// "tag" (default) or "point_label".
    #[serde(default)]
    kind: Option<String>,
    /// The connection point whose label moves (point_label).
    #[serde(default)]
    terminal: Option<TerminalId>,
    /// The page point the text moves to.
    #[serde(default)]
    to: Option<Point>,
    /// The text's justification: "RT", "LB", … (default: as it is).
    #[serde(default)]
    justify: Option<String>,
    /// Back to the computed place (`to` is not read).
    #[serde(default)]
    reset: Option<bool>,
}

impl Command for MovetextParams {
    fn run(self, s: &mut Session) -> cadcraft_engine::Result<Value> {
        let rep = self.representation;
        let kind = rules::opt_kind(self.kind.as_deref()).map_err(|e| err(e.to_string()))?.unwrap_or(TextKind::Tag);
        let terminal = self.terminal;
        let to = match self.to {
            _ if self.reset == Some(true) => None,
            Some(p) => Some(p.0),
            None => return Err(err(BAD_POINT)),
        };
        let justify = rules::justify(self.justify.as_deref()).map_err(|e| err(e.to_string()))?;
        on_project(s, |p, pg, _| {
            let doc = p.ledger.document();
            let t = rules::move_target(doc, rep, kind, terminal)?;
            let current = current_text(doc, rep, &t).map(|x| (x.text.halign, x.text.valign));
            let plan = rules::plan_move(doc, rep, kind, &t, to, justify, current)?;
            let changed = plan.op.is_some();
            if let Some(op) = plan.op {
                p.apply_edit(pg, "movetext", &plan.label, vec![op])?;
            }
            let doc = p.ledger.document();
            let now = rules::move_target(doc, rep, kind, terminal).ok().and_then(|t| current_text(doc, rep, &t));
            Ok(json!({
                "representation": rep.hex(),
                "kind": kind.word(),
                "at": if plan.after.is_some() { now.as_ref().map(|t| json!([t.text.at.x, t.text.at.y])) } else { None },
                "justify": now.as_ref().map(|t| crate::text_style::justify_code(t.text.halign, t.text.valign)),
                "changed": changed,
            }))
        })
    }
}

/// The text `t` names as drawn now.
fn current_text(doc: &Document, rep: RepresentationId, t: &rules::MoveTarget) -> Option<crate::devices::PageText> {
    crate::devices::all_texts_on_page(doc, t.page).into_iter().find(|x| x.text.rep == rep && x.text.of == t.of)
}
