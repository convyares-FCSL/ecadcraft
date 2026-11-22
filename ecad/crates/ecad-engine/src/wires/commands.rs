//! Brief 022's commands (design 022 §10): `nets`, `net`, `wirenumber`, `potential`,
//! `wiresettings`, `renumberwires`, `checks`, `wire`, `joinwires`, `category`, and the
//! continuation form of `place`. Their JSON forms commit through [`Project::apply_edit`], which
//! runs the numbering pass; reads change nothing.
//!
//! Each command is a thin shim (brief 070): its params struct reads the JSON, the rules in
//! [`ecad_electrical::wires::commands::wires`] decide, and the shim commits the ops and answers in JSON.

use cadcraft_engine::Session;
use ecad_electrical::wires::commands::wires::{self as rules};
use ecad_model::connectivity::wiring;
use ecad_model::geometry::units::{Degrees, Radians};
use ecad_model::{PageId, Placement, Pt};
use schemars::JsonSchema;
use serde::Deserialize;
use serde_json::{Value, json};

use crate::commands::input::{Command, Typed};
use crate::commands::{MAX_WIRE_POINTS, err, on_project, plain_style};
use crate::project::Project;
use ecad_electrical::commands::input::{Bounded, Point};
use ecad_electrical::symbols::Starter;

/// The commands of this module with a typed input.
pub const TYPED: &[Typed] = &[Typed::of::<WireParams>("wire"), Typed::of::<JoinwiresParams>("joinwires")];

/// `auto` (any case) or nothing: leave it to the net.
fn not_auto(v: Option<String>) -> Option<String> {
    v.filter(|v| !v.trim().eq_ignore_ascii_case("auto"))
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
            let join = join.unwrap_or_else(|| rules::join_by_default(p.ledger.document()));
            let input = ecad_electrical::wires::WireInput {
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
            let wire = plan.wire.filter(|w| doc.wires.contains_key(w)).or_else(|| ecad_electrical::wires::wire_of_route(doc, plan.route));
            let number = wire.and_then(|w| rules::number_of(doc, w));
            let attached: Vec<Value> = plan.attached.iter().map(|(e, t)| json!({"end": format!("{e:?}"), "connection_point": t.hex()})).collect();
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
    crate::wires::draw::quiet_ends_of(p.ledger.document(), page, &p.views, p.tolerance)
}

// ---------------------------------------------------------------- place (the continuation form)

/// `place {symbol: "continuation", at, rotation?, mirror?, name, caption?}`: a continuation, a
/// drawing aid (design 022 §3.2, design 072 §6.2), part of `place`
/// ([`crate::devices::commands::PlaceParams`]). Answers its `aid`.
pub(crate) fn place_tagless(
    s: &mut Session, params: &crate::devices::commands::PlaceParams, starter: &'static Starter, at: Pt, degrees: Degrees, mirror: bool,
) -> cadcraft_engine::Result<Value> {
    let joins = params.device.is_some() || params.tag.is_some();
    let (name, caption) = ecad_electrical::devices::commands::place::tagless(starter, joins, params.name.as_deref(), params.caption.as_deref())
        .map_err(|e| err(e.to_string()))?;
    on_project(s, |p, pg, page| {
        let style = plain_style(p);
        let doc = p.ledger.document();
        let aid = ecad_electrical::wires::continuation_aid(name.as_deref().unwrap_or_default(), caption.as_deref())?;
        let placement = Placement { origin: at, rotation: Radians::from(degrees), mirror, scale: 1.0 };
        let t = ecad_electrical::wires::plan_tagless(doc, &mut p.ids, page, style, starter, placement, aid)?;
        p.apply_edit(pg, "place", starter.name, t.ops.clone())?;
        if let Some(n) = &name {
            p.last_continuation = Some(n.clone());
        }
        Ok(json!({
            "aid": t.aid.hex(),
            "representation": t.representation.hex(),
            "item": t.item.hex(),
            "definition": t.definition.hex(),
            "symbol": starter.key,
            "connection_point": t.connection_point.hex(),
            "name": name,
            "caption": caption,
            "attached": t.attached.iter().map(|(r, e)| json!({"route": r.hex(), "end": format!("{e:?}")})).collect::<Vec<_>>(),
        }))
    })
}
