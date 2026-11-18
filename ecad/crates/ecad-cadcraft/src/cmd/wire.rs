//! Brief 022's commands (design 022 §10): `nets`, `net`, `wirenumber`, `potential`,
//! `wiresettings`, `renumberwires`, `checks`, `wire`, `joinwires`, `category`, and the
//! continuation form of `place`. Their JSON forms commit through [`Project::apply_edit`], which
//! runs the numbering pass; reads change nothing.
//!
//! Each command is a thin shim (brief 070): its params struct reads the JSON, the rules in
//! [`ecad_electrical::cmd::wire`] decide, and the shim commits the ops and answers in JSON.

use cadcraft_engine::Session;
use ecad_electrical::CmdError;
use ecad_electrical::cmd::wire::{self as rules, CategoryName, Conflict, NetChoice, NetView, Numbered, PotentialRow, WireSettingsChange, Wires};
use ecad_model::connectivity::{KIND, NAME};
use ecad_model::units::{Degrees, Radians};
use ecad_model::wiring;
use ecad_model::{Document, PageId, Placement, Pt, TerminalId, WireId};
use schemars::JsonSchema;
use serde::Deserialize;
use serde_json::{Value, json};

use crate::cmd::input::{Bounded, Command, Point, Typed, nullable};
use crate::cmd::{MAX_HANDLES, MAX_WIRE_POINTS, err, on_any_page, on_project, plain_style};
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
    Typed::of::<ChecksParams>("checks"),
    Typed::of::<WireParams>("wire"),
    Typed::of::<JoinwiresParams>("joinwires"),
];

/// `auto` (any case) or nothing: leave it to the net.
fn not_auto(v: Option<String>) -> Option<String> {
    v.filter(|v| !v.trim().eq_ignore_ascii_case("auto"))
}

// ---------------------------------------------------------------- JSON

fn net_json(n: &NetView) -> Value {
    let cat = |c: &CategoryName| json!({"id": c.id, "name": c.name});
    let mut out = json!({
        "number": n.number,
        "fixed": n.fixed,
        "potential": n.potential,
        "category": n.category.as_ref().map(cat),
        "conductors": n.wires.iter().map(|c| c.hex()).collect::<Vec<_>>(),
        "points": n.points.iter().map(|(t, label)| json!({"terminal": t.hex(), "label": label})).collect::<Vec<_>>(),
        "continuations": n.continuations,
        "pages": n.pages.iter().map(|(p, c)| json!({"page": p, "column": c})).collect::<Vec<_>>(),
    });
    if let Some(list) = &n.categories {
        out["categories"] = json!(list.iter().map(cat).collect::<Vec<_>>());
    }
    out
}

/// The project's wiring settings as `wiresettings` shows them (the label rule as resolved).
fn settings_json(doc: &Document) -> Value {
    let w = &doc.settings.wiring;
    json!({
        "scheme": w.scheme.name(),
        "colour_style": ecad_model::words::Word::word(w.colour_style()),
        "label_height": crate::cmd::sheet::wire_label_height(doc),
        "label_rule": wires::label_rule_word(doc),
        "label_min_length": wires::label_min_length(doc),
        "digits": w.digits(),
        "number_by": ecad_model::Word::word(w.number_by),
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
    #[serde(default)]
    page: Option<PageId>,
    /// Every net with this wire number.
    #[serde(default)]
    number: Option<String>,
    /// The net of this wire.
    #[serde(default)]
    conductor: Option<WireId>,
    /// The net of this connection point.
    #[serde(default)]
    terminal: Option<TerminalId>,
    /// Also nets without a wire.
    #[serde(default)]
    all: Option<bool>,
    /// At most this many (default 200, at most 2000).
    #[serde(default)]
    limit: Option<usize>,
    /// The cursor `next` gave.
    #[serde(default)]
    after: Option<String>,
}

impl Command for NetsParams {
    fn run(self, s: &mut Session) -> cadcraft_engine::Result<Value> {
        let q = rules::NetsQuery {
            page: self.page,
            choice: NetChoice { conductor: self.conductor, terminal: self.terminal, number: self.number },
            all: self.all.unwrap_or(false),
            limit: self.limit.unwrap_or(rules::DEFAULT_LIMIT).clamp(1, rules::MAX_LIMIT),
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
    #[serde(default)]
    conductor: Option<WireId>,
    /// The net of this connection point.
    #[serde(default)]
    terminal: Option<TerminalId>,
    /// The (first) net with this wire number.
    #[serde(default)]
    number: Option<String>,
}

impl Command for NetParams {
    fn run(self, s: &mut Session) -> cadcraft_engine::Result<Value> {
        let choice = NetChoice { conductor: self.conductor, terminal: self.terminal, number: self.number };
        on_any_page(s, |p, _| Ok(net_json(&rules::net(p.ledger.document(), &choice)?)))
    }
}

// ---------------------------------------------------------------- wirenumber and potential

/// What to do when another net uses the number.
#[derive(Deserialize, JsonSchema, Clone, Copy)]
#[serde(rename_all = "lowercase")]
enum ConflictParam {
    /// That net takes this one's number.
    Swap,
    /// This net takes the next free number.
    Another,
    /// Join the nets with a continuation pair named for the number.
    Merge,
}

/// `wirenumber {conductor | terminal | number, value: string | null, conflict?}` or
/// `{conductor, fixed: bool}`: fix the net's number (null, or fixed false, releases it).
#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub(crate) struct WirenumberParams {
    /// A wire of the net.
    #[serde(default)]
    conductor: Option<WireId>,
    /// A connection point of the net.
    #[serde(default)]
    terminal: Option<TerminalId>,
    /// The net's current number.
    #[serde(default)]
    number: Option<String>,
    /// The number to fix, or null to release the fixed number.
    #[serde(default, deserialize_with = "nullable")]
    value: Option<Option<String>>,
    /// Without `value`: true fixes the net's current number, false releases it.
    #[serde(default)]
    fixed: Option<bool>,
    /// When another net uses the number.
    #[serde(default)]
    conflict: Option<ConflictParam>,
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
            (None, None) => return Err(err("`value` (text, or null) is required")),
        };
        let value = match value {
            Some(Some(v)) => Some(Some(wiring::valid_number(&v).map_err(|e| err(e.to_string()))?)),
            other => other,
        };
        let conflict = self.conflict.map(|c| match c {
            ConflictParam::Swap => Conflict::Swap,
            ConflictParam::Another => Conflict::Another,
            ConflictParam::Merge => Conflict::Merge,
        });
        on_project(s, |p, pg, _| {
            let style = plain_style(p);
            let doc = p.ledger.document();
            let value = match value {
                Some(v) => v,
                None => Some(wiring::valid_number(&rules::current_number(doc, &choice)?)?),
            };
            let done = rules::wirenumber(doc, &mut p.ids, style, &choice, value.as_deref(), conflict)?;
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
    conductor: WireId,
    /// The potential (24V, L1…), or null to clear it.
    #[serde(deserialize_with = "nullable")]
    #[schemars(required, with = "crate::cmd::input::OrNull<String>")]
    value: Option<Option<String>>,
}

impl Command for PotentialParams {
    fn run(self, s: &mut Session) -> cadcraft_engine::Result<Value> {
        let value = match self.value.flatten() {
            Some(v) => wiring::valid_text("potential", Some(&v)).map_err(err)?,
            None => None,
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
    #[serde(default)]
    conductor: Option<WireId>,
    /// A wire whose whole net takes the category.
    #[serde(default)]
    net: Option<WireId>,
    /// These wires.
    #[serde(default)]
    conductors: Option<Bounded<WireId, 0, { MAX_HANDLES }>>,
    /// The wire category (its ID or name), or null to clear the wires' own.
    #[serde(deserialize_with = "nullable")]
    #[schemars(required, with = "crate::cmd::input::OrNull<String>")]
    value: Option<Option<String>>,
}

impl Command for CategoryParams {
    fn run(self, s: &mut Session) -> cadcraft_engine::Result<Value> {
        let value = self.value.flatten();
        let (one, net, many) = (self.conductor, self.net, self.conductors.map(|b| b.0));
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
            Ok(json!({"changed": changed, "conductors": done.wires, "category": done.category.map(|i| i.text()), "message": done.message}))
        })
    }
}

// ---------------------------------------------------------------- wiresettings and renumberwires

/// One row of the potentials table.
#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
struct PotentialRowParam {
    /// The potential's name (24V, L1, PE…).
    #[serde(default)]
    name: String,
    /// The prefix of its wire numbers.
    #[serde(default)]
    prefix: String,
    /// Whether its wires are numbered (default true).
    #[serde(default)]
    numbered: Option<bool>,
    /// Refused: colours belong to wire categories now.
    #[serde(default)]
    colour: Option<Value>,
    /// Refused: cross-sections belong to wire categories now.
    #[serde(default)]
    cross_section: Option<Value>,
}

/// `wiresettings {scheme?, digits?, unclassified_prefix?, potentials?, colour_style?,
/// label_height?, label_rule?, label_min_length?, number_by?}`: no parameters reads.
#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub(crate) struct WiresettingsParams {
    /// How wires are numbered: "potential", "page" or "page-column".
    #[serde(default)]
    scheme: Option<String>,
    /// Digits of a wire number's sequence, 1 to 6.
    #[serde(default)]
    digits: Option<u64>,
    /// The prefix of wires of no potential.
    #[serde(default)]
    unclassified_prefix: Option<String>,
    /// The whole potentials table (colours and cross-sections are wire categories').
    #[serde(default)]
    potentials: Option<Vec<PotentialRowParam>>,
    /// "code2" (BN, BU, GN/YE), "code3" (BRN, BLU, GRN/YEL) or "name" (Brown, Blue, Green/Yellow).
    #[serde(default)]
    colour_style: Option<String>,
    /// Wire label height in mm, 0.5 to 10.
    #[serde(default)]
    label_height: Option<f64>,
    /// "net" (one label per net per page) or "each" (every wire).
    #[serde(default)]
    label_rule: Option<String>,
    /// The shortest labelled wire in mm, 0 to 100.
    #[serde(default)]
    label_min_length: Option<f64>,
    /// What one wire number counts: "net" (every wire of a net shows the net's number) or
    /// "conductor" (every wire its own). Switching renumbers every net in the same edit.
    #[serde(default)]
    number_by: Option<String>,
}

impl Command for WiresettingsParams {
    fn run(self, s: &mut Session) -> cadcraft_engine::Result<Value> {
        let rows = self.potentials.map(|rows| {
            rows.into_iter()
                .map(|r| PotentialRow {
                    name: r.name,
                    prefix: r.prefix,
                    numbered: r.numbered,
                    colours: r.colour.is_some() || r.cross_section.is_some(),
                })
                .collect()
        });
        let change = WireSettingsChange {
            scheme: self.scheme,
            digits: self.digits,
            colour_style: self.colour_style,
            label_height: self.label_height,
            label_rule: self.label_rule,
            label_min_length: self.label_min_length,
            number_by: self.number_by,
            unclassified_prefix: self.unclassified_prefix,
            potentials: rows,
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

/// Which wires `renumberwires` renumbers.
#[derive(Deserialize, JsonSchema, Clone, Copy, PartialEq)]
#[serde(rename_all = "lowercase")]
enum Scope {
    /// Only wires without a number (the default).
    Unnumbered,
    /// Every wire without a fixed number.
    All,
}

/// `renumberwires {scope?: "unnumbered" | "all"}` (fixed numbers never change).
#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub(crate) struct RenumberwiresParams {
    /// Which wires (default "unnumbered").
    #[serde(default)]
    scope: Option<Scope>,
}

impl Command for RenumberwiresParams {
    fn run(self, s: &mut Session) -> cadcraft_engine::Result<Value> {
        let all = self.scope == Some(Scope::All);
        on_project(s, |p, pg, _| {
            let (ops, nets) = rules::renumber(p.ledger.document(), all);
            if ops.is_empty() {
                return Ok(json!({"renumbered": 0}));
            }
            p.apply_edit(pg, "renumberwires", "Renumber wires", ops)?;
            Ok(json!({"renumbered": nets}))
        })
    }
}

// ---------------------------------------------------------------- joinwires

/// `joinwires {at: [x,y]}` on the active page (design 032 §2.5, design 072 §4.3): two free route
/// ends that meet become one route; free ends on a route's middle, or three or more ends, land on
/// one route there (a T).
#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub(crate) struct JoinwiresParams {
    /// Where the ends meet.
    at: Point,
}

impl Command for JoinwiresParams {
    fn run(self, s: &mut Session) -> cadcraft_engine::Result<Value> {
        on_project(s, |p, pg, page| {
            let quiet = quiet_ends(p, page);
            let done = rules::join_wires(p.ledger.document(), page, self.at.0, &quiet, MAX_WIRE_POINTS)?;
            Ok(match done {
                rules::Joined::Merged { ops, survivor, removed, at } => {
                    p.apply_edit(pg, "joinwires", "Join wires", ops)?;
                    json!({"how": "merged", "route": survivor.hex(), "removed": [removed.hex()], "at": [at.x, at.y]})
                }
                rules::Joined::Tee { ops, route, landed, at } => {
                    p.apply_edit(pg, "joinwires", "Join wires", ops)?;
                    json!({"how": "tee", "route": route.hex(), "landed": landed, "at": [at.x, at.y]})
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
    #[serde(default)]
    kinds: Option<Vec<String>>,
    /// At most this many (default 200, at most 2000).
    #[serde(default)]
    limit: Option<usize>,
    /// Skip this many (the `next` a previous call gave).
    #[serde(default)]
    after: Option<usize>,
}

impl Command for ChecksParams {
    fn run(self, s: &mut Session) -> cadcraft_engine::Result<Value> {
        let kinds = match self.kinds {
            Some(words) => Some(
                words
                    .iter()
                    .map(|k| crate::checks::Kind::parse(k).ok_or_else(|| err(format!("unknown check kind {k:?}"))))
                    .collect::<cadcraft_engine::Result<std::collections::BTreeSet<_>>>()?,
            ),
            None => None,
        };
        let limit = self.limit.unwrap_or(crate::checks::DEFAULT_LIMIT).clamp(1, crate::checks::MAX_LIMIT);
        let after = self.after.unwrap_or(0);
        on_any_page(s, |p, _| {
            // The project's kept report models (built on first use, then kept from each change).
            p.outputs();
            let doc = p.ledger.document();
            let outputs = p.outputs.as_ref().ok_or(CmdError::refused("the report models could not be built"))?;
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

/// `wire {points, potential?, join?: bool, category?}` (design 032 §2.3).
#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub(crate) struct WireParams {
    /// The route: 2 to 4,096 [x, y] points.
    points: Bounded<Point, 2, { MAX_WIRE_POINTS }>,
    /// The wire's potential ("auto" or none: the net's).
    #[serde(default)]
    potential: Option<String>,
    /// Join ends that meet other wires (default: the project's `wire.join`).
    #[serde(default)]
    join: Option<bool>,
    /// The new wire's category, its ID or name ("auto" or none: the net's).
    #[serde(default)]
    category: Option<String>,
}

impl Command for WireParams {
    fn run(self, s: &mut Session) -> cadcraft_engine::Result<Value> {
        let potential = match not_auto(self.potential) {
            Some(v) => wiring::valid_text("potential", Some(&v)).map_err(err)?,
            None => None,
        };
        let join = self.join;
        let points: Vec<Pt> = self.points.0.into_iter().map(|p| p.0).collect();
        let category = not_auto(self.category);
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
            let doc = p.ledger.document();
            let wire = plan.wire.filter(|w| doc.wires.contains_key(w)).or_else(|| wires::wire_of_route(doc, plan.route));
            let number = wire.and_then(|w| rules::number_of(doc, w));
            let attached: Vec<Value> = plan.attached.iter().map(|(e, t)| json!({"end": format!("{e:?}"), "terminal": t.hex()})).collect();
            let mut r = json!({
                "route": plan.route.hex(),
                "conductor": wire.map(|w| w.hex()),
                "item": plan.item.hex(),
                "created": plan.created,
                "attached": attached,
                "potential": potential,
                "number": number,
            });
            if !plan.joined.is_empty() {
                r["joined"] =
                    json!(plan.joined.iter().map(|(e, x, how)| json!({"end": format!("{e:?}"), "route": x.hex(), "how": how})).collect::<Vec<_>>());
            }
            if !plan.removed.is_empty() {
                r["removed"] = json!(plan.removed.iter().map(|c| c.hex()).collect::<Vec<_>>());
            }
            if !plan.tees.is_empty() {
                r["tees"] = json!(plan.tees.iter().map(|(at, x)| json!({"at": [at.x, at.y], "route": x.hex()})).collect::<Vec<_>>());
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
    crate::defaults::resolve(doc, &crate::defaults::wires::WIRE_JOIN, crate::defaults::Scope::project()).flag()
}

// ---------------------------------------------------------------- place (the continuation form)

/// `place {symbol: "continuation", at, rotation?, mirror?, name?, caption?}`: a
/// tagless component (design 022 §3.2), part of `place` ([`crate::cmd::device::PlaceParams`]).
pub(crate) fn place_tagless(
    s: &mut Session, params: &crate::cmd::device::PlaceParams, starter: &'static Starter, at: Pt, degrees: Degrees, mirror: bool,
) -> cadcraft_engine::Result<Value> {
    let joins = params.device.is_some() || params.tag.is_some();
    let (name, caption) =
        ecad_electrical::cmd::place::tagless(starter, joins, params.name.as_deref(), params.caption.as_deref()).map_err(|e| err(e.to_string()))?;
    on_project(s, |p, pg, page| {
        let style = plain_style(p);
        let doc = p.ledger.document();
        let mut props = ecad_model::Properties::new();
        props.insert(KIND.into(), starter.key.to_string());
        if let Some(n) = &name {
            props.insert(NAME.into(), n.clone());
        }
        if let Some(t) = &caption {
            props.insert(crate::continuations::CAPTION_KEY.into(), t.clone());
        }
        let placement = Placement { origin: at, rotation: Radians::from(degrees), mirror, scale: 1.0 };
        let t = wires::plan_tagless(doc, &mut p.ids, page, style, starter, placement, props)?;
        p.apply_edit(pg, "place", starter.name, t.ops.clone())?;
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
            "attached": t.attached.iter().map(|(r, e)| json!({"route": r.hex(), "end": format!("{e:?}")})).collect::<Vec<_>>(),
        }))
    })
}
