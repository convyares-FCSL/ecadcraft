//! Brief 022's commands (design 022 §10): `nets`, `net`, `wirenumber`, `potential`,
//! `wiresettings`, `renumberwires`, `junction`, `checks`, `wire`, `joinwires`, `category`, and the
//! continuation form of `place`. Their JSON forms commit through [`Project::apply_edit`], which
//! runs the numbering pass; reads change nothing.
//!
//! Each command is a thin shim (brief 070): its params struct reads the JSON, the rules in
//! [`ecad_electrical::cmd::wire`] decide, and the shim commits the ops and answers in JSON.

use std::collections::BTreeSet;

use cadcraft_engine::Session;
use ecad_electrical::CmdError;
use ecad_electrical::cmd::wire::{self as rules, CategoryName, Junctioned, NetChoice, NetView, Numbered, PotentialRow, WireSettingsChange, Wires};
use ecad_model::connectivity::{CONTINUATION, KIND, NAME};
use ecad_model::wiring;
use ecad_model::{ConductorId, Document, PageId, Placement, Pt, TerminalId};
use schemars::JsonSchema;
use serde::Deserialize;
use serde_json::{Value, json};

use crate::cmd::input::{self, Command, Typed, field};
use crate::cmd::{MAX_HANDLES, MAX_WIRE_POINTS, err, on_any_page, on_project, plain_style, with_project};
use crate::project::Project;
use crate::symbols::Starter;
use crate::wires;

/// The commands of this module with a typed input.
pub const TYPED: &[Typed] = &[
    Typed::of::<NetsParams>("nets"),
    Typed::of::<NetParams>("net"),
    Typed::of::<WirenumberParams>("wirenumber"),
    Typed::of::<PotentialParams>("potential"),
    Typed::of::<CategoryParams>("category"),
    Typed::of::<WiresettingsParams>("wiresettings"),
    Typed::of::<RenumberwiresParams>("renumberwires"),
    Typed::of::<JunctionParams>("junction"),
    Typed::of::<ChecksParams>("checks"),
    Typed::of::<WireParams>("wire"),
    Typed::of::<JoinwiresParams>("joinwires"),
];

// ---------------------------------------------------------------- fields

field!(fn conductor -> Option<ConductorId> = |v| input::opt_hex(v, "conductor"));
field!(fn conductor_required -> ConductorId = |v| input::hex(v, "conductor"));
field!(fn terminal -> Option<TerminalId> = |v| input::opt_hex(v, "terminal"));
field!(fn net_conductor -> Option<ConductorId> = |v| input::opt_hex(v, "net"));
field!(fn conductors -> Option<Vec<ConductorId>> = |v| input::opt_hex_list(v, "conductors", MAX_HANDLES));
field!(fn page -> Option<PageId> = |v| input::opt_hex(v, "page"));
field!(fn number -> Option<String> = |v| input::opt_string(v, "number"));
field!(fn after -> Option<String> = |v| input::opt_string(v, "after"));
field!(fn conflict -> Option<String> = |v| input::opt_string(v, "conflict"));
field!(fn lenient -> Option<bool> = input::lenient_bool);
field!(fn nets_limit -> usize = |v| input::limit(v, "limit", rules::DEFAULT_LIMIT, rules::MAX_LIMIT));
field!(fn checks_limit -> usize = |v| input::limit(v, "limit", crate::checks::DEFAULT_LIMIT, crate::checks::MAX_LIMIT));
field!(fn checks_after -> Option<u64> = input::lenient_u64);
field!(fn at -> Pt = |v| input::point(v, AT_REQUIRED));
field!(fn join -> Option<bool> = |v| input::opt_bool(v, "join"));
field!(fn junction -> Option<bool> = |v| input::opt_bool(v, "junction"));
field!(fn number_value -> Option<Option<String>> = |v| input::nullable_string(v, "`value` must be a string, or null to release a fixed number"));
field!(fn potential_value -> Option<Option<String>> = |v| input::nullable_string(v, POTENTIAL_VALUE));
field!(fn category_value -> Option<Option<String>> = |v| input::nullable_string(v, CATEGORY_VALUE));
field!(fn scheme -> Option<String> = |v| input::opt_string(v, "scheme"));
field!(fn colour_style -> Option<String> = |v| input::opt_string(v, "colour_style"));
field!(fn label_rule -> Option<String> = |v| input::opt_string(v, "label_rule"));
field!(fn unclassified_prefix -> Option<String> = |v| input::opt_string(v, "unclassified_prefix"));
field!(fn digits -> Option<u64> = |v| input::opt_u64(v, "`digits` must be 1 to 6"));
field!(fn label_height -> Option<f64> = |v| input::opt_f64(v, "`label_height` must be 0.5 to 10 (millimetres)"));
field!(fn label_min_length -> Option<f64> = |v| input::opt_f64(v, "Shortest labelled wire is 0 to 100 mm."));
field!(fn potentials -> Option<Vec<PotentialRow>> = potential_rows);
field!(fn scope_all -> bool = renumber_scope);
field!(fn kinds -> Option<BTreeSet<crate::checks::Kind>> = check_kinds);
field!(fn points -> Vec<Pt> = wire_points);
field!(fn wire_potential -> Option<String> = wire_potential_value);
field!(fn wire_category -> Option<String> = wire_category_value);

const AT_REQUIRED: &str = "`at` [x,y] is required";
const POTENTIAL_VALUE: &str = "`value` is required (a potential such as 24V, or null to clear it)";
const CATEGORY_VALUE: &str = "`value` is required: a wire category (its ID or name), or null to clear it";
const POINTS_REQUIRED: &str = "`points` [[x,y], …] is required";

/// The potentials table: rows read leniently (a word not given is empty).
fn potential_rows(v: Value) -> Result<Option<Vec<PotentialRow>>, CmdError> {
    let a = match v {
        Value::Null => return Ok(None),
        Value::Array(a) => a,
        _ => return Err(CmdError::input("`potentials` must be the whole table: [{name, prefix, numbered}]")),
    };
    let word = |r: &Value, k: &str| r.get(k).and_then(Value::as_str).map(str::trim).unwrap_or("").to_string();
    Ok(Some(
        a.iter()
            .map(|r| PotentialRow {
                name: word(r, "name"),
                prefix: word(r, "prefix"),
                numbered: r.get("numbered").and_then(Value::as_bool),
                colours: ["colour", "cross_section"].iter().any(|k| r.get(*k).is_some_and(|v| !v.is_null())),
            })
            .collect(),
    ))
}

/// `scope` of `renumberwires`: `all` is true.
fn renumber_scope(v: Value) -> Result<bool, CmdError> {
    match input::opt_string(v, "scope")?.map(|x| x.trim().to_ascii_lowercase()) {
        None => Ok(false),
        Some(x) if x == "unnumbered" => Ok(false),
        Some(x) if x == "all" => Ok(true),
        Some(x) => Err(CmdError::input(format!("`scope` must be \"unnumbered\" or \"all\", not {x:?}"))),
    }
}

/// `kinds` of `checks`.
fn check_kinds(v: Value) -> Result<Option<BTreeSet<crate::checks::Kind>>, CmdError> {
    match v {
        Value::Null => Ok(None),
        Value::Array(a) => a
            .iter()
            .map(|k| k.as_str().and_then(crate::checks::Kind::parse).ok_or_else(|| CmdError::input(format!("unknown check kind {k}"))))
            .collect::<Result<_, _>>()
            .map(Some),
        _ => Err(CmdError::input("`kinds` must be a list of check kinds")),
    }
}

/// `points` of `wire`: 2 to [`MAX_WIRE_POINTS`] finite points.
fn wire_points(v: Value) -> Result<Vec<Pt>, CmdError> {
    let raw = v.as_array().ok_or(CmdError::input(POINTS_REQUIRED))?;
    if raw.len() > MAX_WIRE_POINTS {
        return Err(CmdError::input(format!("at most {MAX_WIRE_POINTS} points per wire")));
    }
    let pts: Vec<Pt> = raw
        .iter()
        .map(|v| crate::cmd::point(v).ok_or_else(|| CmdError::input(format!("`{v}` is not a finite [x,y] point"))))
        .collect::<Result<_, _>>()?;
    if pts.len() < 2 {
        return Err(CmdError::input("`points` needs at least two [x,y] points"));
    }
    Ok(pts)
}

/// `potential` of `wire`: `auto` (or none) leaves it to the net (brief 022).
fn wire_potential_value(v: Value) -> Result<Option<String>, CmdError> {
    match v {
        Value::Null => Ok(None),
        Value::String(v) if v.trim().eq_ignore_ascii_case("auto") => Ok(None),
        Value::String(v) => wiring::valid_text("potential", Some(&v)).map_err(CmdError::input),
        _ => Err(CmdError::input("`potential` must be a string")),
    }
}

/// `category` of `wire` (brief 050b): `auto` (or none) leaves it to the net.
fn wire_category_value(v: Value) -> Result<Option<String>, CmdError> {
    match v {
        Value::Null => Ok(None),
        Value::String(v) if v.trim().eq_ignore_ascii_case("auto") => Ok(None),
        Value::String(v) => Ok(Some(v)),
        _ => Err(CmdError::input("`category` must be a wire category's ID or name")),
    }
}

/// The schema of one row of the potentials table.
#[derive(JsonSchema)]
#[allow(dead_code)]
struct PotentialRowSchema {
    /// The potential's name (24V, L1, PE…).
    name: String,
    /// The prefix of its wire numbers.
    prefix: Option<String>,
    /// Whether its wires are numbered (default true).
    numbered: Option<bool>,
}

// ---------------------------------------------------------------- JSON

fn net_json(n: &NetView) -> Value {
    let cat = |c: &CategoryName| json!({"id": c.id, "name": c.name});
    let mut out = json!({
        "number": n.number,
        "fixed": n.fixed,
        "potential": n.potential,
        "category": n.category.as_ref().map(cat),
        "conductors": n.conductors.iter().map(|c| c.hex()).collect::<Vec<_>>(),
        "points": n.points.iter().map(|(t, label)| json!({"terminal": t.hex(), "label": label})).collect::<Vec<_>>(),
        "continuations": n.continuations,
        "pages": n.pages.iter().map(|(p, c)| json!({"page": p, "column": c})).collect::<Vec<_>>(),
    });
    if let Some(list) = &n.categories {
        out["categories"] = json!(list.iter().map(cat).collect::<Vec<_>>());
    }
    out
}

fn junction_json(j: &Junctioned) -> Value {
    let mut r = json!({
        "junction": j.junction.hex(),
        "terminal": j.terminal.hex(),
        "conductor": j.conductor.map(|c| c.hex()),
        "at": [j.at.x, j.at.y],
    });
    if let Some(n) = j.attached {
        r["attached"] = json!(n);
    }
    r
}

/// The project's wiring settings as `wiresettings` shows them (the label rule as resolved).
fn settings_json(doc: &Document) -> Value {
    let w = &doc.settings.wiring;
    json!({
        "scheme": w.scheme.name(),
        "colour_style": w.colour_style.name(),
        "label_height": w.label_height(),
        "label_rule": wires::label_rule_word(doc),
        "label_min_length": wires::label_min_length(doc),
        "digits": w.digits,
        "unclassified_prefix": w.unclassified_prefix,
        "potentials": w.potentials.iter().map(|p| json!({
            "name": p.name, "prefix": p.prefix, "numbered": p.numbered,
        })).collect::<Vec<_>>(),
    })
}

// ---------------------------------------------------------------- nets and net

/// `nets {page?, number?, conductor?, terminal?, all?, limit?, after?}` (read only): the
/// project's nets, a page at a time.
#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub(crate) struct NetsParams {
    /// Only nets drawn on this page.
    #[serde(default, deserialize_with = "page")]
    #[schemars(with = "Option<String>")]
    page: Option<PageId>,
    /// Every net with this wire number.
    #[serde(default, deserialize_with = "number")]
    number: Option<String>,
    /// The net of this wire.
    #[serde(default, deserialize_with = "conductor")]
    #[schemars(with = "Option<String>")]
    conductor: Option<ConductorId>,
    /// The net of this connection point.
    #[serde(default, deserialize_with = "terminal")]
    #[schemars(with = "Option<String>")]
    terminal: Option<TerminalId>,
    /// Also nets without a wire.
    #[serde(default, deserialize_with = "lenient")]
    all: Option<bool>,
    /// At most this many (default 200, at most 2000).
    #[serde(default = "default_nets_limit", deserialize_with = "nets_limit")]
    limit: usize,
    /// The cursor `next` gave.
    #[serde(default, deserialize_with = "after")]
    after: Option<String>,
}

fn default_nets_limit() -> usize {
    rules::DEFAULT_LIMIT
}

impl Command for NetsParams {
    fn run(self, s: &mut Session) -> cadcraft_engine::Result<Value> {
        let q = rules::NetsQuery {
            page: self.page,
            choice: NetChoice { conductor: self.conductor, terminal: self.terminal, number: self.number },
            all: self.all.unwrap_or(false),
            limit: self.limit,
            after: self.after,
        };
        on_any_page(s, |p, _| {
            let found = rules::nets(p.ledger.document(), &q)?;
            let mut r = json!({ "nets": found.nets.iter().map(net_json).collect::<Vec<_>>() });
            if let Some(n) = found.next {
                r["next"] = json!(n);
            }
            Ok(r)
        })
    }
}

/// `net {conductor | terminal | number}` (read only): one net.
#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub(crate) struct NetParams {
    /// The net of this wire.
    #[serde(default, deserialize_with = "conductor")]
    #[schemars(with = "Option<String>")]
    conductor: Option<ConductorId>,
    /// The net of this connection point.
    #[serde(default, deserialize_with = "terminal")]
    #[schemars(with = "Option<String>")]
    terminal: Option<TerminalId>,
    /// The (first) net with this wire number.
    #[serde(default, deserialize_with = "number")]
    number: Option<String>,
}

impl Command for NetParams {
    fn run(self, s: &mut Session) -> cadcraft_engine::Result<Value> {
        let choice = NetChoice { conductor: self.conductor, terminal: self.terminal, number: self.number };
        on_any_page(s, |p, _| Ok(net_json(&rules::net(p.ledger.document(), &choice)?)))
    }
}

// ---------------------------------------------------------------- wirenumber and potential

/// `wirenumber {conductor | terminal | number, value: string | null, conflict?}` or
/// `{conductor, fixed: bool}`: fix the net's number (null, or fixed false, releases it).
#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub(crate) struct WirenumberParams {
    /// A wire of the net.
    #[serde(default, deserialize_with = "conductor")]
    #[schemars(with = "Option<String>")]
    conductor: Option<ConductorId>,
    /// A connection point of the net.
    #[serde(default, deserialize_with = "terminal")]
    #[schemars(with = "Option<String>")]
    terminal: Option<TerminalId>,
    /// The net's current number.
    #[serde(default, deserialize_with = "number")]
    number: Option<String>,
    /// The number to fix, or null to release the fixed number.
    #[serde(default, deserialize_with = "number_value")]
    #[schemars(with = "Option<String>")]
    value: Option<Option<String>>,
    /// Without `value`: true fixes the net's current number, false releases it.
    #[serde(default, deserialize_with = "lenient")]
    fixed: Option<bool>,
    /// When another net uses the number: "swap", "another" or "merge".
    #[serde(default, deserialize_with = "conflict")]
    conflict: Option<String>,
}

impl Command for WirenumberParams {
    fn run(self, s: &mut Session) -> cadcraft_engine::Result<Value> {
        let choice = NetChoice { conductor: self.conductor, terminal: self.terminal, number: self.number };
        // `fixed` alone (the Properties panel's Fixed box): fix the net's current number, or
        // release it.
        let value = match (self.value, self.fixed) {
            (Some(v), _) => Some(v),
            (None, Some(false)) => Some(None),
            (None, Some(true)) => None,
            (None, None) => return Err(err("`value` is required (a wire number, or null to release a fixed number)")),
        };
        let value = match value {
            Some(Some(v)) => Some(Some(wiring::valid_number(&v).map_err(|e| err(e.to_string()))?)),
            other => other,
        };
        let conflict = self.conflict.map(|c| c.to_ascii_lowercase());
        on_project(s, |p, pg, _| {
            let style = plain_style(p);
            let doc = p.ledger.document();
            let value = match value {
                Some(v) => v,
                None => Some(wiring::valid_number(&rules::current_number(doc, &choice)?)?),
            };
            let done = rules::wirenumber(doc, &mut p.ids, style, &choice, value.as_deref(), conflict.as_deref())?;
            let (ops, label) = done.ops();
            let must_record = !matches!(done, Numbered::Released { .. } | Numbered::Fixed { .. });
            if must_record || !ops.is_empty() {
                p.apply_edit(pg, "wirenumber", label, ops.to_vec())?;
            }
            Ok(match done {
                Numbered::Released { ops, number, conductors } if ops.is_empty() => {
                    json!({"number": number, "fixed": false, "conductors": conductors, "changed": false})
                }
                Numbered::Released { number, conductors, .. } => json!({"number": number, "fixed": false, "conductors": conductors}),
                Numbered::Fixed { number, conductors, .. } | Numbered::Another { number, conductors, .. } => {
                    json!({"number": number, "fixed": true, "conductors": conductors})
                }
                Numbered::Swapped { number, conductors, swapped, .. } => {
                    json!({"number": number, "fixed": true, "conductors": conductors, "swapped": swapped})
                }
                Numbered::Merged { number, continuations, .. } => json!({
                    "number": number, "fixed": true, "merged": true,
                    "continuations": continuations.iter().map(|c| c.hex()).collect::<Vec<_>>(),
                }),
            })
        })
    }
}

/// `potential {conductor, value: string | null}`: the potential of the whole net.
#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub(crate) struct PotentialParams {
    /// A wire of the net.
    #[serde(deserialize_with = "conductor_required")]
    #[schemars(with = "String")]
    conductor: ConductorId,
    /// The potential (24V, L1…), or null to clear it.
    #[serde(default, deserialize_with = "potential_value")]
    #[schemars(with = "Option<String>")]
    value: Option<Option<String>>,
}

impl Command for PotentialParams {
    const REQUIRED: &'static [(&'static str, &'static str)] = &[("conductor", "`conductor` (hex id) is required")];

    fn run(self, s: &mut Session) -> cadcraft_engine::Result<Value> {
        let value = match self.value {
            Some(Some(v)) => wiring::valid_text("potential", Some(&v)).map_err(err)?,
            Some(None) => None,
            None => return Err(err(POTENTIAL_VALUE)),
        };
        let c = self.conductor;
        on_project(s, |p, pg, _| {
            let (ops, n) = rules::potential(p.ledger.document(), c, value.as_deref())?;
            if !ops.is_empty() {
                p.apply_edit(pg, "potential", "Set potential", ops)?;
            }
            let number = rules::number_of(p.ledger.document(), c);
            Ok(json!({"potential": value, "conductors": n, "number": number}))
        })
    }
}

// ---------------------------------------------------------------- category

/// `category {conductor | net | conductors, value: string | null}` (brief 050b, design 050
/// §6.9): a wire's own category (`net`: a wire whose whole net takes it).
#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub(crate) struct CategoryParams {
    /// One wire.
    #[serde(default, deserialize_with = "conductor")]
    #[schemars(with = "Option<String>")]
    conductor: Option<ConductorId>,
    /// A wire whose whole net takes the category.
    #[serde(default, deserialize_with = "net_conductor")]
    #[schemars(with = "Option<String>")]
    net: Option<ConductorId>,
    /// These wires.
    #[serde(default, deserialize_with = "conductors")]
    #[schemars(with = "Option<Vec<String>>")]
    conductors: Option<Vec<ConductorId>>,
    /// The wire category (its ID or name), or null to clear the wires' own.
    #[serde(default, deserialize_with = "category_value")]
    #[schemars(with = "Option<String>")]
    value: Option<Option<String>>,
}

impl Command for CategoryParams {
    fn run(self, s: &mut Session) -> cadcraft_engine::Result<Value> {
        let Some(value) = self.value else { return Err(err(CATEGORY_VALUE)) };
        let (one, net, many) = (self.conductor, self.net, self.conductors);
        on_project(s, |p, pg, _| {
            let which = match (one, net, many) {
                (Some(c), None, None) => Wires::One(c),
                (None, Some(c), None) => Wires::Net(c),
                (None, None, Some(v)) => Wires::Many(v),
                _ => return Err(CmdError::input("Give one of `conductor`, `net` or `conductors`.")),
            };
            let done = rules::category(p.ledger.document(), which, value.as_deref())?;
            let changed = !done.ops.is_empty();
            if changed {
                p.apply_edit(pg, "category", &done.label, done.ops)?;
            }
            Ok(json!({"changed": changed, "conductors": done.conductors, "category": done.category.map(|i| i.text()), "message": done.message}))
        })
    }
}

// ---------------------------------------------------------------- wiresettings and renumberwires

/// `wiresettings {scheme?, digits?, unclassified_prefix?, potentials?, colour_style?,
/// label_height?, label_rule?, label_min_length?}`: no parameters reads.
#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub(crate) struct WiresettingsParams {
    /// How wires are numbered: "potential", "page" or "page-column".
    #[serde(default, deserialize_with = "scheme")]
    scheme: Option<String>,
    /// Digits of a wire number's sequence, 1 to 6.
    #[serde(default, deserialize_with = "digits")]
    digits: Option<u64>,
    /// The prefix of wires of no potential.
    #[serde(default, deserialize_with = "unclassified_prefix")]
    unclassified_prefix: Option<String>,
    /// The whole potentials table (colours and cross-sections are wire categories').
    #[serde(default, deserialize_with = "potentials")]
    #[schemars(with = "Option<Vec<PotentialRowSchema>>")]
    potentials: Option<Vec<PotentialRow>>,
    /// "iec" (BN, BU, GNYE) or "three-letter" (BRN, BLU, GRN/YEL).
    #[serde(default, deserialize_with = "colour_style")]
    colour_style: Option<String>,
    /// Wire label height in mm, 0.5 to 10.
    #[serde(default, deserialize_with = "label_height")]
    label_height: Option<f64>,
    /// "net" (one label per net per page) or "each" (every wire).
    #[serde(default, deserialize_with = "label_rule")]
    label_rule: Option<String>,
    /// The shortest labelled wire in mm, 0 to 100.
    #[serde(default, deserialize_with = "label_min_length")]
    label_min_length: Option<f64>,
}

impl Command for WiresettingsParams {
    fn run(self, s: &mut Session) -> cadcraft_engine::Result<Value> {
        let change = WireSettingsChange {
            scheme: self.scheme,
            digits: self.digits,
            colour_style: self.colour_style,
            label_height: self.label_height,
            label_rule: self.label_rule,
            label_min_length: self.label_min_length,
            unclassified_prefix: self.unclassified_prefix,
            potentials: self.potentials,
        };
        on_project(s, |p, pg, _| {
            if change == WireSettingsChange::default() {
                return Ok(settings_json(p.ledger.document()));
            }
            let before = p.ledger.document().settings.clone();
            let after = rules::wire_settings(&before, &change)?;
            if after != before {
                p.apply_edit(pg, "wiresettings", "Wire settings", vec![ecad_model::Op::SetProjectSettings { before, after }])?;
            }
            Ok(settings_json(p.ledger.document()))
        })
    }
}

/// `renumberwires {scope?: "unnumbered" | "all"}` (fixed numbers never change).
#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub(crate) struct RenumberwiresParams {
    /// "unnumbered" (the default) or "all".
    #[serde(default, rename = "scope", deserialize_with = "scope_all")]
    #[schemars(with = "Option<String>")]
    all: bool,
}

impl Command for RenumberwiresParams {
    fn run(self, s: &mut Session) -> cadcraft_engine::Result<Value> {
        on_project(s, |p, pg, _| {
            let (ops, nets) = rules::renumber(p.ledger.document(), self.all);
            if ops.is_empty() {
                return Ok(json!({"renumbered": 0}));
            }
            p.apply_edit(pg, "renumberwires", "Renumber wires", ops)?;
            Ok(json!({"renumbered": nets}))
        })
    }
}

// ---------------------------------------------------------------- junction and joinwires

/// `junction {at: [x,y]}` on the active page (design 022 §3.1).
#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub(crate) struct JunctionParams {
    /// The point on the wire, which is split there.
    #[serde(deserialize_with = "at")]
    #[schemars(with = "[f64; 2]")]
    at: Pt,
}

impl Command for JunctionParams {
    const REQUIRED: &'static [(&'static str, &'static str)] = &[("at", AT_REQUIRED)];

    fn run(self, s: &mut Session) -> cadcraft_engine::Result<Value> {
        on_project(s, |p, pg, page| {
            let style = plain_style(p);
            let (ops, j) = rules::junction_at(p.ledger.document(), &mut p.ids, style, page, self.at)?;
            p.apply_edit(pg, "junction", "Junction", ops)?;
            Ok(junction_json(&j))
        })
    }
}

/// `joinwires {at: [x,y]}` on the active page (design 032 §2.5): two open ends that meet become
/// one wire; an open end on a wire's interior, or three or more ends, get a junction.
#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub(crate) struct JoinwiresParams {
    /// Where the ends meet.
    #[serde(deserialize_with = "at")]
    #[schemars(with = "[f64; 2]")]
    at: Pt,
}

impl Command for JoinwiresParams {
    const REQUIRED: &'static [(&'static str, &'static str)] = &[("at", AT_REQUIRED)];

    fn run(self, s: &mut Session) -> cadcraft_engine::Result<Value> {
        on_project(s, |p, pg, page| {
            let style = plain_style(p);
            let quiet = quiet_ends(p, page);
            let done = rules::join_wires(p.ledger.document(), &mut p.ids, style, page, self.at, &quiet, MAX_WIRE_POINTS)?;
            Ok(match done {
                rules::Joined::Merged { ops, survivor, removed, at } => {
                    p.apply_edit(pg, "joinwires", "Join wires", ops)?;
                    json!({"how": "merged", "conductor": survivor.hex(), "removed": [removed.hex()], "at": [at.x, at.y]})
                }
                rules::Joined::Junction { ops, junction } => {
                    p.apply_edit(pg, "joinwires", "Join wires", ops)?;
                    let mut r = junction_json(&junction);
                    r["how"] = json!("junction");
                    r
                }
            })
        })
    }
}

// ---------------------------------------------------------------- checks

/// `checks {kinds?, limit?, after?}` (read only).
#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub(crate) struct ChecksParams {
    /// Only these kinds of check.
    #[serde(default, deserialize_with = "kinds")]
    #[schemars(with = "Option<Vec<String>>")]
    kinds: Option<BTreeSet<crate::checks::Kind>>,
    /// At most this many (default 200, at most 2000).
    #[serde(default = "default_checks_limit", deserialize_with = "checks_limit")]
    limit: usize,
    /// Skip this many (the `next` a previous call gave).
    #[serde(default, deserialize_with = "checks_after")]
    after: Option<u64>,
}

fn default_checks_limit() -> usize {
    crate::checks::DEFAULT_LIMIT
}

impl Command for ChecksParams {
    fn run(self, s: &mut Session) -> cadcraft_engine::Result<Value> {
        let (kinds, limit, after) = (self.kinds, self.limit, self.after.unwrap_or(0) as usize);
        on_any_page(s, |p, _| {
            // The project's kept report models (built on first use, then kept from each change).
            p.outputs();
            let doc = p.ledger.document();
            let outputs = p.outputs.as_ref().ok_or("the report models could not be built")?;
            let rows: Vec<_> = crate::checks::all_with(doc, p.tolerance, outputs)
                .into_iter()
                .filter(|c| kinds.as_ref().is_none_or(|k| k.contains(&c.kind)))
                .collect();
            let shown: Vec<Value> = rows.iter().skip(after).take(limit).map(|c| c.to_json(doc)).collect();
            let mut r = json!({"checks": shown, "total": rows.len()});
            if rows.len() > after + limit {
                r["next"] = json!(after + limit);
            }
            Ok(r)
        })
    }
}

// ---------------------------------------------------------------- wire (brief 032 §2)

/// `wire {points, potential?, join?: bool, junction?: bool, category?}` (design 032 §2.3).
#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub(crate) struct WireParams {
    /// The route: 2 to 10,000 [x, y] points.
    #[serde(deserialize_with = "points")]
    #[schemars(with = "Vec<[f64; 2]>")]
    points: Vec<Pt>,
    /// The wire's potential ("auto" or none: the net's).
    #[serde(default, deserialize_with = "wire_potential")]
    potential: Option<String>,
    /// Join ends that meet other wires (default: the project's `wire.join`).
    #[serde(default, deserialize_with = "join")]
    join: Option<bool>,
    /// The old name of `join`.
    #[serde(default, deserialize_with = "junction")]
    junction: Option<bool>,
    /// The new wire's category, its ID or name ("auto" or none: the net's).
    #[serde(default, deserialize_with = "wire_category")]
    category: Option<String>,
}

impl Command for WireParams {
    const REQUIRED: &'static [(&'static str, &'static str)] = &[("points", POINTS_REQUIRED)];

    fn run(self, s: &mut Session) -> cadcraft_engine::Result<Value> {
        let join = match (self.join, self.junction) {
            (Some(a), Some(b)) if a != b => return Err(err("`join` and `junction` disagree; give only `join`")),
            (a, b) => a.or(b),
        };
        let (points, potential, category) = (self.points, self.potential, self.category);
        on_project(s, |p, pg, page| {
            let style = plain_style(p);
            let quiet = quiet_ends(p, page);
            let join = join.unwrap_or_else(|| join_by_default(p.ledger.document()));
            let input = wires::WireInput {
                page,
                style,
                points,
                potential: potential.as_deref(),
                join,
                tol: p.tolerance,
                quiet: &quiet,
                max_points: MAX_WIRE_POINTS,
            };
            let mut plan = rules::wire(&p.ledger, &mut p.ids, input, category.as_deref())?;
            p.apply_edit(pg, "wire", "Wire", std::mem::take(&mut plan.ops))?;
            let number = rules::number_of(p.ledger.document(), plan.conductor);
            let attached: Vec<Value> = plan.attached.iter().map(|(e, t)| json!({"end": format!("{e:?}"), "terminal": t.hex()})).collect();
            let mut r = json!({
                "conductor": plan.conductor.hex(),
                "item": plan.item.hex(),
                "created": plan.created,
                "attached": attached,
                "potential": potential,
                "number": number,
            });
            if !plan.junctions.is_empty() {
                r["junctions"] = json!(
                    plan.junctions
                        .iter()
                        .map(|(e, j, at)| json!({"end": format!("{e:?}"), "junction": j.hex(), "at": [at.x, at.y]}))
                        .collect::<Vec<_>>()
                );
            }
            if !plan.joined.is_empty() {
                r["joined"] = json!(
                    plan.joined
                        .iter()
                        .map(|(e, c, how, n)| json!({"end": format!("{e:?}"), "conductor": c.hex(), "how": how, "wires": n}))
                        .collect::<Vec<_>>()
                );
            }
            if !plan.removed.is_empty() {
                r["removed"] = json!(plan.removed.iter().map(|c| c.hex()).collect::<Vec<_>>());
            }
            if !plan.splits.is_empty() {
                r["splits"] = json!(
                    plan.splits.iter().map(|(at, j, c)| json!({"at": [at.x, at.y], "junction": j.hex(), "conductor": c.hex()})).collect::<Vec<_>>()
                );
            }
            Ok(r)
        })
    }
}

// ---------------------------------------------------------------- helpers for other modules

/// For the Wire tool: the potentials a person can choose (`auto` and the table's names).
pub(crate) fn potential_choices(p: &Project) -> Vec<String> {
    let mut v = vec!["auto".to_string()];
    v.extend(p.ledger.document().settings.wiring.potentials.iter().map(|x| x.name.clone()));
    v
}

/// The quiet wire ends on `page` (cable reference ends, brief 009/010): never joined. Derived
/// from the document (review of #65, finding 5), not from what is on screen; only a page with a
/// cable reference has any, so other pages cost nothing.
pub(crate) fn quiet_ends(p: &Project, page: PageId) -> Vec<Pt> {
    wires::quiet_ends_of(p.ledger.document(), page, &p.views, p.tolerance)
}

/// Whether `wire` joins ends that meet other wires when `join` is not given (design 032 §4.5):
/// the registered default `wire.join`.
pub(crate) fn join_by_default(doc: &Document) -> bool {
    crate::defaults::resolve(doc, &crate::defaults::wires::WIRE_JOIN, crate::defaults::Scope::project()).word() == "on"
}

// ---------------------------------------------------------------- place (the continuation form)

fn opt_str<'a>(p: &'a Value, key: &str) -> Result<Option<&'a str>, String> {
    match p.get(key) {
        None | Some(Value::Null) => Ok(None),
        Some(Value::String(s)) => Ok(Some(s.as_str())),
        Some(_) => Err(format!("`{key}` must be a string")),
    }
}

/// `place {symbol: "continuation" | "junction", at, rotation?, mirror?, name?}`: a tagless
/// component (design 022 §3.2). Part of `place`, which moves to a typed input with `cmd/device.rs`.
pub(crate) fn place_tagless(
    s: &mut Session, params: &Value, starter: &'static Starter, at: Pt, degrees: f64, mirror: bool,
) -> cadcraft_engine::Result<Value> {
    if params.get("device").is_some_and(|v| !v.is_null()) || params.get("tag").is_some_and(|v| !v.is_null()) {
        return Err(err(format!("A {} has no tag and is not part of a device.", starter.name.to_lowercase())));
    }
    let name = match opt_str(params, "name").map_err(err)? {
        Some(n) => Some(wires::valid_name(n).map_err(err)?),
        None => None,
    };
    if starter.key == CONTINUATION && name.is_none() {
        return Err(err("`name` is required for a continuation (the potential or signal it carries, e.g. 24V)"));
    }
    // Brief 049 (design 049 §5.1): a continuation may be placed with its caption.
    let caption = match params.get(crate::continuations::CAPTION_KEY) {
        None | Some(Value::Null) => None,
        Some(_) if starter.key != CONTINUATION => return Err(err("Only continuation arrows have a caption; that is a junction.")),
        Some(Value::String(t)) => crate::continuations::caption_text(Some(t)).map_err(err)?,
        Some(_) => return Err(err("`caption` must be text")),
    };
    with_project(s, |p, pg, page| {
        let style = plain_style(p);
        let doc = p.ledger.document();
        let mut props = ecad_model::Properties::new();
        props.insert(KIND.into(), starter.key.into());
        if let Some(n) = &name {
            props.insert(NAME.into(), n.clone());
        }
        if let Some(t) = &caption {
            props.insert(crate::continuations::CAPTION_KEY.into(), t.clone());
        }
        let placement = Placement { origin: at, rotation: degrees.to_radians(), mirror, scale: 1.0 };
        let t = wires::plan_tagless(doc, &mut p.ids, page, style, starter, placement, props)?;
        let label = if starter.key == CONTINUATION { "Continuation" } else { "Junction" };
        p.apply_edit(pg, "place", label, t.ops.clone())?;
        if let Some(n) = &name {
            p.last_continuation = Some(n.clone());
        }
        Ok(json!({
            "component": t.component.hex(),
            "representation": t.representation.hex(),
            "item": t.item.hex(),
            "definition": t.definition.hex(),
            "symbol": starter.key,
            "terminal": t.terminal.hex(),
            "name": name,
            "caption": caption,
            "attached": t.attached.iter().map(|(c, e)| json!({"conductor": c.hex(), "end": format!("{e:?}")})).collect::<Vec<_>>(),
        }))
    })
}
