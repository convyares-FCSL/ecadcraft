//! The session commands `wire` and `joinwires`, and the continuation form of `place`. Their
//! JSON forms commit through [`Project::apply_edit`], which runs the numbering pass.
//!
//! Each command is a thin shim: its params read the JSON, the rules in
//! [`ecad_electrical::wires::commands::wires`] decide, and the shim commits the ops and answers with its typed
//! reply. The wire commands that do not need the session (`nets`, `net`, `wirenumber`, `potential`,
//! `wiresettings`, `renumberwires`, `checks`, `category`, `connect`, `disconnect`) are generic, in
//! [`ecad_electrical::wires::commands`].

use cadcraft_engine::Session;
use ecad_electrical::CmdError;
use ecad_electrical::commands::run::{Reply, Request};
use ecad_electrical::wires::commands::connect::EndParam;
use ecad_electrical::wires::commands::wires::{self as rules};
use ecad_electrical::wires::joining::JoinHow;
use ecad_model::connectivity::wiring;
use ecad_model::geometry::units::{Degrees, Radians};
use ecad_model::{ConnectionPointId, ItemId, PageId, Placement, Pt, RouteId, WireId};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

use crate::commands::input::{Command, Typed};
use crate::commands::{MAX_WIRE_POINTS, err, in_project, on_project, plain_style};
use crate::project::Project;
use ecad_electrical::commands::input::{Bounded, Point};
use ecad_electrical::symbols::Starter;

/// The commands of this module with a typed input.
pub(crate) const TYPED: &[Typed] = &[Typed::of::<Wire>(), Typed::of::<Joinwires>()];

/// `auto` (any case) or nothing: leave it to the net.
fn not_auto(v: Option<String>) -> Option<String> {
    v.filter(|v| !v.trim().eq_ignore_ascii_case("auto"))
}

// ---------------------------------------------------------------- joinwires

/// On the active page, two open ends at `at` become one line; open ends on a line's middle, or three or more
/// ends, land on one line there (a T).
#[derive(Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct JoinwiresParams {
    /// Where the ends meet.
    pub at: Point,
}

/// What `joinwires` did.
#[derive(Serialize, Deserialize, JsonSchema, Debug, Clone, PartialEq)]
#[serde(tag = "how", rename_all = "lowercase")]
pub enum JoinwiresReply {
    /// Two open ends became one line.
    Merged {
        /// The line that remains.
        route: RouteId,
        /// The line merged into it and removed.
        removed: Vec<RouteId>,
        /// Where they met.
        at: Point,
    },
    /// Open ends landed on one line (a T).
    Tee {
        /// The line they landed on.
        route: RouteId,
        /// How many ends landed.
        landed: usize,
        /// Where.
        at: Point,
    },
}
impl Reply for JoinwiresReply {}

/// `joinwires`: join open wire ends where they meet.
pub enum Joinwires {}
impl Request for Joinwires {
    const ID: &'static str = "joinwires";
    type Params = JoinwiresParams;
    type Reply = JoinwiresReply;
}

impl Command for Joinwires {
    fn run(params: JoinwiresParams, s: &mut Session) -> Result<JoinwiresReply, CmdError> {
        in_project(s, |p, pg, page| {
            let quiet = quiet_ends(p, page);
            let done = rules::join_wires(p.ledger.document(), page, params.at.0, &quiet, MAX_WIRE_POINTS)?;
            Ok(match done {
                rules::Joined::Merged { ops, survivor, removed, at } => {
                    p.apply_edit(pg, "joinwires", "Join wires", ops)?;
                    JoinwiresReply::Merged { route: survivor, removed: vec![removed], at: Point(at) }
                }
                rules::Joined::Tee { ops, route, landed, at } => {
                    p.apply_edit(pg, "joinwires", "Join wires", ops)?;
                    JoinwiresReply::Tee { route, landed, at: Point(at) }
                }
            })
        })
    }
}

// ---------------------------------------------------------------- wire

/// A wire along `points`. Ends on connection points attach; an end on another wire's open end joins it into one
/// line, an end on a line's middle lands there (a T); open ends lying on the new line land on it.
#[derive(Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct WireParams {
    /// The route: 2 to 4,096 [x, y] points.
    pub points: Bounded<Point, 2, { MAX_WIRE_POINTS }>,
    /// The wire's potential ("auto" or none: the net's).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub potential: Option<String>,
    /// Join ends that meet other wires (default: the project's `wire.join`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub join: Option<bool>,
    /// The new wire's category, its ID or name ("auto" or none: the net's).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub category: Option<String>,
}

/// An end of the new line on a connection point.
#[derive(Serialize, Deserialize, JsonSchema, Debug, Clone, PartialEq)]
pub struct Attached {
    /// Which end.
    pub end: EndParam,
    /// The connection point it is on.
    pub connection_point: ConnectionPointId,
}

/// An end of the new line that met another line.
#[derive(Serialize, Deserialize, JsonSchema, Debug, Clone, PartialEq)]
pub struct JoinedEnd {
    /// Which end.
    pub end: EndParam,
    /// The line it met.
    pub route: RouteId,
    /// How.
    pub how: JoinHow,
}

/// Another line's open end that landed on the new line (a T).
#[derive(Serialize, Deserialize, JsonSchema, Debug, Clone, PartialEq)]
pub struct Tee {
    /// Where.
    pub at: Point,
    /// The line whose end landed there.
    pub route: RouteId,
}

/// What `wire` drew.
#[derive(Serialize, Deserialize, JsonSchema, Debug, Clone, PartialEq)]
pub struct WireReply {
    /// The line the drawn points now belong to: a new one, or the one it extends.
    pub route: RouteId,
    /// The wire it carries (none: it connects nothing yet).
    pub conductor: Option<WireId>,
    /// The line's drawing item.
    pub item: ItemId,
    /// False when the drawn points were merged into an existing line.
    pub created: bool,
    /// Its ends on connection points.
    pub attached: Vec<Attached>,
    /// The potential given.
    pub potential: Option<String>,
    /// The wire's number.
    pub number: Option<String>,
    /// Its ends that met other lines.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub joined: Vec<JoinedEnd>,
    /// Lines merged into it and removed.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub removed: Vec<RouteId>,
    /// Other lines' open ends that landed on it.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub tees: Vec<Tee>,
}
impl Reply for WireReply {}

/// `wire`: draw a wire.
pub enum Wire {}
impl Request for Wire {
    const ID: &'static str = "wire";
    type Params = WireParams;
    type Reply = WireReply;
}

impl Command for Wire {
    fn run(params: WireParams, s: &mut Session) -> Result<WireReply, CmdError> {
        let potential = match not_auto(params.potential) {
            Some(v) => wiring::valid_text("potential", Some(&v)).map_err(|e| CmdError::refused(e.to_string()))?,
            None => None,
        };
        let join = params.join;
        let points: Vec<Pt> = params.points.0.into_iter().map(|p| p.0).collect();
        let category = not_auto(params.category);
        in_project(s, |p, pg, page| {
            let style = plain_style(p);
            let quiet = quiet_ends(p, page);
            let join = join.unwrap_or_else(|| rules::join_by_default(p.ledger.document()));
            let input = ecad_electrical::wires::joining::WireInput {
                page,
                style,
                points,
                potential: potential.as_deref(),
                join,
                tol: p.tolerance,
                quiet: &quiet,
                max_points: MAX_WIRE_POINTS,
            };
            let mut plan = rules::wire(&mut p.ledger, &mut p.ids, input, category.as_deref())?;
            p.apply_edit(pg, "wire", "Wire", std::mem::take(&mut plan.ops))?;
            let doc = p.ledger.document();
            let wire = plan.wire.filter(|w| doc.wires().contains_key(w)).or_else(|| ecad_electrical::wires::routes::wire_of_route(doc, plan.route));
            let number = wire.and_then(|w| rules::number_of(doc, w));
            Ok(WireReply {
                route: plan.route,
                conductor: wire,
                item: plan.item,
                created: plan.created,
                attached: plan.attached.iter().map(|(e, t)| Attached { end: (*e).into(), connection_point: *t }).collect(),
                potential,
                number,
                joined: plan.joined.iter().map(|(e, x, how)| JoinedEnd { end: (*e).into(), route: *x, how: *how }).collect(),
                removed: plan.removed.clone(),
                tees: plan.tees.iter().map(|(at, x)| Tee { at: Point(*at), route: *x }).collect(),
            })
        })
    }
}

// ---------------------------------------------------------------- helpers for other modules

/// For the Wire tool: the potentials a person can choose (`auto` and the table's names).
pub(crate) fn potential_choices(p: &Project) -> Vec<String> {
    let mut v = vec!["auto".to_string()];
    v.extend(p.ledger.document().settings().wiring.potentials.iter().map(|x| x.name.clone()));
    v
}

/// The quiet wire ends on `page` (cable reference ends): never joined. Derived
/// from the document, not from what is on screen; only a page with a
/// cable reference has any, so other pages cost nothing.
pub(crate) fn quiet_ends(p: &Project, page: PageId) -> Vec<Pt> {
    crate::wires::draw::quiet_ends_of(p.ledger.document(), page, &p.display.views, p.tolerance)
}

// ---------------------------------------------------------------- place (the continuation form)

/// `place {symbol: "continuation", at, rotation?, mirror?, name, caption?}`: a continuation, a
/// drawing aid, part of `place`
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
        let t = ecad_electrical::wires::placing::plan_tagless(doc, &mut p.ids, page, style, starter, placement, aid)?;
        p.apply_edit(pg, "place", starter.name, t.ops.clone())?;
        if let Some(n) = &name {
            p.defaults.last_continuation = Some(n.clone());
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
