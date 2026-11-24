//! The drawn cable commands: `cableslash`, `cableadd`, `cablecore`,
//! `cablefan` and `cableview`. The interactive forms (each feature's `interact` module) commit through these.
//! `cableview` records nothing: a cable's view is presentation only ([`crate::cables::views`]).
//!
//! Each command is a thin shim: its params struct reads the JSON, the rules in
//! [`ecad_electrical::cables::commands::draw`] decide, and the shim commits the ops and answers in JSON.
//! `cablefan` and `cableview` stay here whole: the fan's spine is measured with the engine's
//! fonts ([`crate::cables::draw`]), and a view is the session's.

use std::collections::BTreeMap;

use cadcraft_engine::Session;
use ecad_electrical::CmdError;
use ecad_electrical::cables::commands::draw::{self as rules, SlashTarget};
use ecad_model::{CableId, Form, Item, ItemId, ItemKind, Op, Primitive, Pt, Representation, RepresentationId, Subject, WireId};
use schemars::JsonSchema;
use serde::Deserialize;
use serde_json::{Value, json};

use crate::cables::views::{self, View};
use crate::commands::input::{Command, Typed};
use crate::commands::{err, on_any_page, on_project, plain_style};
use ecad_electrical::cables::cable_title;
use ecad_electrical::cables::layout::{FanEnd, MarkerEnd};
use ecad_electrical::cables::slash::MAX_CABLE_MEMBERS;
use ecad_electrical::commands::input::{Bounded, OrNull, Point, nullable};

/// The commands of this module with a typed input.
pub const TYPED: &[Typed] = &[
    Typed::of::<CableslashParams>("cableslash"),
    Typed::of::<CableaddParams>("cableadd"),
    Typed::of::<CablecoreParams>("cablecore"),
    Typed::of::<CablefanParams>("cablefan"),
    Typed::of::<CableviewParams>("cableview"),
];

/// The wires the slash from `from` to `to` crosses on the active page become one new cable, in crossing order,
/// with the slash as its drawing; one transaction. With `cable`, or the `tag` of a cable that exists, it
/// continues that cable, or assigns the wires to it when it is not drawn yet. Refused when it crosses no wire,
/// too many, or a wire already in a cable.
#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub(crate) struct CableslashParams {
    /// Where the slash starts.
    from: Point,
    /// Where it ends.
    to: Point,
    /// The new cable's tag, or the tag of a cable to continue.
    #[serde(default)]
    tag: Option<String>,
    /// A cable to continue (or assign the wires to).
    #[serde(default)]
    cable: Option<CableId>,
}

impl Command for CableslashParams {
    fn run(self, s: &mut Session) -> cadcraft_engine::Result<Value> {
        let from = self.from.within(ecad_electrical::commands::input::MAX_COORD, "from").map_err(|e| err(e.to_string()))?;
        let to = self.to.within(ecad_electrical::commands::input::MAX_COORD, "to").map_err(|e| err(e.to_string()))?;
        on_project(s, |p, pg, page| {
            let doc = p.ledger.document();
            let found = rules::crossed(doc, page, from, to, p.tolerance)?;
            match rules::slash_target(doc, self.cable, self.tag.as_deref())? {
                SlashTarget::Assign(k) => run_slash_assign(p, pg, page, k, from, to, &found),
                SlashTarget::Continue(k) => {
                    Ok(crate::references::draw::cables::run_slash_continue(p, pg, page, k, from, to, &found).map_err(CmdError::refused)?)
                }
                SlashTarget::New => {
                    let style = plain_style(p);
                    let plan = rules::slash_new(&p.ledger, &mut p.ids, &mut p.indexes.tag_high, page, style, from, to, &found, self.tag.as_deref())?;
                    p.apply_edit(pg, "cableslash", "Cable slash", plan.ops)?;
                    Ok(json!({
                        "cable": plan.cable.hex(),
                        "item": plan.item.hex(),
                        "representation": plan.rep.hex(),
                        "conductors": rules::crossing_wires(&found.members),
                        "lines": rules::crossing_lines(&found.members),
                        "ignored": found.ignored.iter().map(|(i, _)| i.hex()).collect::<Vec<_>>(),
                        "touched": found.touched.iter().map(|c| c.hex()).collect::<Vec<_>>(),
                    }))
                }
            }
        })
    }
}

/// The wires join `cable`, or a new cable (tagged `tag`, or the next free tag) when `cable` is not given.
#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub(crate) struct CableaddParams {
    /// The cable they join; without it, a new cable.
    #[serde(default)]
    cable: Option<CableId>,
    /// The wires, in order.
    conductors: Bounded<WireId, 1, MAX_CABLE_MEMBERS>,
    /// The new cable's tag.
    #[serde(default)]
    tag: Option<String>,
}

impl Command for CableaddParams {
    fn run(self, s: &mut Session) -> cadcraft_engine::Result<Value> {
        on_project(s, |p, pg, _| {
            let (cable, ops) =
                rules::add_to_cable(&p.ledger, &mut p.ids, &mut p.indexes.tag_high, self.cable, &self.conductors.0, self.tag.as_deref())?;
            p.apply_edit(pg, "cableadd", "Add to cable", ops)?;
            Ok(json!({"cable": cable.hex()}))
        })
    }
}

/// Sets or clears the core label of one member (`{conductor, core}`), or of every member of a cable at once
/// (`{cable, cores}`); null, empty or spaces clear. One transaction; when nothing changes, none.
#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub(crate) struct CablecoreParams {
    /// The wire whose core label is set.
    #[serde(default)]
    conductor: Option<WireId>,
    /// Its label, or null to clear it.
    #[serde(default, deserialize_with = "nullable")]
    core: Option<Option<String>>,
    /// The cable whose labels are all set.
    #[serde(default)]
    cable: Option<CableId>,
    /// One label (or null) per member, in member order.
    #[serde(default)]
    cores: Option<Vec<Option<String>>>,
}

impl Command for CablecoreParams {
    fn run(self, s: &mut Session) -> cadcraft_engine::Result<Value> {
        if let Some(given) = self.cores {
            let cable = self.cable.ok_or_else(|| err("`cable` (hex id) is required"))?;
            return on_project(s, |p, pg, _| {
                let (cores, ops) = rules::cores(p.ledger.document(), cable, &given)?;
                let changed = !ops.is_empty();
                if changed {
                    p.apply_edit(pg, "cablecore", "Core labels", ops)?;
                }
                Ok(json!({"cable": cable.hex(), "cores": cores, "changed": changed}))
            });
        }
        let conductor = self.conductor.ok_or_else(|| err("`conductor` (hex id) is required"))?;
        let core = self.core.ok_or_else(|| err("`core` must be a string, or null to clear it"))?;
        on_project(s, |p, pg, _| {
            let set = rules::core(p.ledger.document(), conductor, core.as_deref())?;
            let mut r = json!({
                "cable": set.cable.hex(), "conductor": set.conductor.hex(), "index": set.index,
                "core": set.core, "changed": !set.ops.is_empty(),
            });
            if !set.duplicates.is_empty() {
                r["duplicates"] = Value::Array(set.duplicates.iter().map(|w| Value::String(w.hex())).collect());
            }
            if !set.ops.is_empty() {
                p.apply_edit(pg, "cablecore", "Core label", set.ops)?;
            }
            Ok(r)
        })
    }
}

/// A fanned end of a cable.
#[derive(Deserialize, JsonSchema, Clone, Copy, PartialEq)]
pub(crate) enum FanEndParam {
    /// End A.
    A,
    /// End B.
    B,
}

/// Where the trunk joins a fanned end's spine.
#[derive(Deserialize, JsonSchema, Clone, Copy)]
#[serde(untagged)]
pub(crate) enum FanAt {
    /// A point: the nearest point of the spine.
    Point(Point),
    /// A fraction of the spine, from 0 to 1.
    Fraction(f64),
}

/// What `cablefan`'s `at` asks for.
enum JoinAt {
    Point(Pt),
    Fraction(f64),
    Middle,
}

/// Where the trunk joins one fanned end of a cable on the active page, snapped onto the spine; null removes it
/// and the trunk joins the middle again. Works whatever the cable's view.
#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub(crate) struct CablefanParams {
    /// The cable.
    cable: CableId,
    /// Which fanned end; needed unless `at` is a point.
    #[serde(default)]
    end: Option<FanEndParam>,
    /// A point, a fraction of the spine from 0 to 1, or null for the middle.
    #[serde(deserialize_with = "nullable")]
    #[schemars(required, with = "OrNull<FanAt>")]
    at: Option<Option<FanAt>>,
}

impl Command for CablefanParams {
    fn run(self, s: &mut Session) -> cadcraft_engine::Result<Value> {
        let cable = self.cable;
        let end = self.end.map(|e| if e == FanEndParam::A { MarkerEnd::A } else { MarkerEnd::B });
        let at = match self.at.flatten() {
            None => JoinAt::Middle,
            Some(FanAt::Fraction(f)) if f.is_finite() && (0.0..=1.0).contains(&f) => JoinAt::Fraction(f),
            Some(FanAt::Fraction(_)) => return Err(err("`at` as a fraction must be from 0 to 1")),
            Some(FanAt::Point(Point(q))) if q.x.abs() <= 1e12 && q.y.abs() <= 1e12 => JoinAt::Point(q),
            Some(FanAt::Point(_)) => return Err(err("Point out of range.")),
        };
        on_project(s, |p, pg, page| {
            let doc = p.ledger.document();
            if !doc.cables().contains_key(&cable) {
                return Err(CmdError::NotFound(ecad_model::Noun::Cable));
            }
            let title = cable_title(doc, cable);
            let ends = ecad_electrical::cables::layout::fan_ends(
                doc,
                page,
                cable,
                p.tolerance,
                &crate::cables::draw::font_width,
                &crate::styles::draw::LOOK,
            );
            if ends.is_empty() {
                return Err(CmdError::refused(format!("Cable {title} has no fanned end on this page.")));
            }
            let name = |e: MarkerEnd| if e == MarkerEnd::A { "A" } else { "B" };
            let fe = match (end, &at) {
                (Some(e), _) => ends
                    .iter()
                    .find(|f| f.end == e)
                    .ok_or_else(|| CmdError::refused(format!("End {} of cable {title} stops at a reference; it has no spine.", name(e))))?,
                (None, JoinAt::Point(q)) => ends
                    .iter()
                    .min_by(|a, b| {
                        let d = |f: &FanEnd| f.spine.join_for(*q).map_or(f64::INFINITY, |j| j.dist(*q));
                        d(a).total_cmp(&d(b))
                    })
                    .ok_or(CmdError::refused("no fanned end"))?,
                (None, _) => return Err(CmdError::input("`end` (\"A\" or \"B\") is required unless `at` is a point")),
            };
            let sp = fe.spine;
            let join = match at {
                JoinAt::Point(q) => Some(sp.join_for(q).ok_or(CmdError::refused("Point out of range."))?),
                JoinAt::Fraction(f) => Some(sp.point(sp.tmin + f * (sp.tmax - sp.tmin))),
                JoinAt::Middle => None,
            };
            let mut r = json!({
                "cable": cable.hex(), "end": name(fe.end),
                "at": join.map(|j| json!([j.x, j.y])), "changed": true,
            });
            let missing = || CmdError::refused("anchor item missing");
            let ops = match (fe.anchor, join) {
                (None, None) => {
                    r["changed"] = Value::Bool(false);
                    return Ok(r);
                }
                (Some((_, _, old)), Some(j)) if old.near(j, 1e-9) => {
                    r["changed"] = Value::Bool(false);
                    return Ok(r);
                }
                (Some((rid, item, _)), None) => {
                    let before = doc.items().get(&item).cloned().ok_or_else(missing)?;
                    let rep = doc.representations().get(&rid).cloned().ok_or(CmdError::refused("anchor missing"))?;
                    vec![Op::UnlinkRepresentation { id: rid, rep }, Op::RemoveItem { id: item, item: before }]
                }
                (Some((_, item, _)), Some(j)) => {
                    let before = doc.items().get(&item).cloned().ok_or_else(missing)?;
                    let after = Item { kind: ItemKind::Primitive(Primitive::Point { at: j }), ..before.clone() };
                    vec![Op::ModifyItem { id: item, before, after }]
                }
                (None, Some(j)) => {
                    let (item, rid): (ItemId, RepresentationId) = (p.ids.fresh(), p.ids.fresh());
                    let style = plain_style(p);
                    vec![
                        Op::AddItem { id: item, item: Item { page, style, kind: ItemKind::Primitive(Primitive::Point { at: j }) } },
                        Op::LinkRepresentation {
                            id: rid,
                            rep: Representation {
                                subject: Subject::Cable(cable),
                                page,
                                form: Form::Drawn,
                                role: ecad_model::Role::FanAnchor,
                                items: vec![item],
                                connection_points: BTreeMap::new(),
                            },
                        },
                    ]
                }
            };
            p.apply_edit(pg, "cablefan", "Cable join", ops)?;
            Ok(r)
        })
    }
}

/// A cable's view.
#[derive(Deserialize, JsonSchema, Clone, Copy)]
#[serde(rename_all = "lowercase")]
pub(crate) enum ViewParam {
    /// Every wire drawn.
    Traditional,
    /// The wires fanned from one trunk.
    Fan,
}

/// Shows a cable traditional or fan; without `view` it toggles, without `cable` it lists every cable's view.
/// Presentation only: nothing is recorded, and it is never refused because of the project's state (read-only, an
/// unavailable page).
#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub(crate) struct CableviewParams {
    /// The cable; without it, every cable's view is listed.
    #[serde(default)]
    cable: Option<CableId>,
    /// The view to show; without it, the other one.
    #[serde(default)]
    view: Option<ViewParam>,
}

impl Command for CableviewParams {
    fn run(self, s: &mut Session) -> cadcraft_engine::Result<Value> {
        let view = self.view.map(|v| match v {
            ViewParam::Traditional => View::Traditional,
            ViewParam::Fan => View::Fan,
        });
        if self.cable.is_none() && view.is_some() {
            return Err(err("`view` needs `cable`"));
        }
        let cable = self.cable;
        on_any_page(s, |p, pg| {
            let doc = p.ledger.document();
            let Some(k) = cable else {
                let views: serde_json::Map<String, Value> =
                    doc.cables().keys().map(|k| (k.hex(), Value::String(p.view_of(*k).name().into()))).collect();
                return Ok(json!({ "views": views }));
            };
            if !doc.cables().contains_key(&k) {
                return Err(CmdError::NotFound(ecad_model::Noun::Cable));
            }
            if p.faults.fail_view_switches > 0 {
                p.faults.fail_view_switches -= 1;
                return Err(CmdError::refused("injected view switch failure"));
            }
            let current = p.view_of(k);
            let target = view.unwrap_or_else(|| current.toggled());
            if target == View::Fan && current != View::Fan && !ecad_electrical::cables::layout::offers_fan(doc, k) {
                return Err(CmdError::refused(format!("Cable {} {}", cable_title(doc, k), views::ONE_WIRE)));
            }
            let (changed, stored) = p.set_view(pg, k, target);
            let mut r = json!({"cable": k.hex(), "view": target.name()});
            let remembered = match (&stored, p.display.view_settings.dir()) {
                (Ok(()), Some(_)) => true,
                (Err(e), Some(_)) => {
                    r["reason"] = Value::String(e.clone());
                    false
                }
                (_, None) => {
                    if changed && !p.display.said_not_remembered {
                        p.display.said_not_remembered = true;
                        r["notice"] = Value::String(views::NOT_REMEMBERED.into());
                    }
                    false
                }
            };
            r["remembered"] = Value::Bool(remembered);
            if !changed {
                r["changed"] = Value::Bool(false);
            }
            Ok(r)
        })
    }
}

// ---------------------------------------------------------------------------------------------
// The assign rule

/// `cableslash` onto cable `k` that is not drawn: commits
/// [`ecad_electrical::cables::commands::draw::plan_slash_assign`]'s ops as one transaction, `Cable slash`.
///
/// # Errors
/// A wire already in a cable, more wires than free cores, too many members; nothing is written.
pub(crate) fn run_slash_assign(
    p: &mut crate::project::Project, pg: &mut crate::access::Pages<'_>, page: ecad_model::PageId, k: CableId, from: ecad_model::Pt,
    to: ecad_model::Pt, found: &ecad_electrical::cables::slash::Crossings,
) -> Result<Value, CmdError> {
    let style = crate::commands::plain_style(p);
    let plan = ecad_electrical::cables::commands::draw::plan_slash_assign(p.ledger.document(), &mut p.ids, page, style, k, from, to, found)?;
    p.apply_edit(pg, "cableslash", "Cable slash", plan.ops)?;
    Ok(json!({
        "cable": k.hex(),
        "item": plan.item.hex(),
        "representation": plan.rep.hex(),
        "assigned": true,
        "conductors": ecad_electrical::cables::commands::draw::crossing_wires(&found.members),
        "lines": ecad_electrical::cables::commands::draw::crossing_lines(&found.members),
        "cores": plan.cores,
        "ignored": found.ignored.iter().map(|(i, _)| i.hex()).collect::<Vec<_>>(),
        "touched": found.touched.iter().map(|c| c.hex()).collect::<Vec<_>>(),
    }))
}
