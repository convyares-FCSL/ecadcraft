//! Core Label.

use super::cable_view::{WIRE_NOT_IN_CABLE, cabled_wire};
use super::{R, Safe, commit, not_a_point, other, paper_guard, safely};
use crate::interact::{ATTACHED_ACI, can_start, in_range, pick, preview_entity, read};
use crate::project::Mapped;
use cadcraft_engine::doc::{Entity, Handle};
use cadcraft_engine::geom::Vec2;
use cadcraft_engine::{Accept, Input, Interactive, Prompt, Session, Step};
use ecad_electrical::cables::slash;
use ecad_electrical::commands::input::OUT_OF_RANGE;
use ecad_model::{Primitive, Subject, WireId};
use serde_json::{Value, json};

/// `cablecore`: click a wire in a cable, type its core number or colour; repeat until Enter.
pub(crate) fn cablecore(s: &Session) -> R<Box<dyn Interactive>> {
    can_start(s)?;
    Ok(Box::new(Safe(CableCore::default())))
}

/// The wire picked and its current label, while the text prompt is shown.
#[derive(Default)]
struct CableCore {
    picked: Option<(WireId, String, Option<String>)>,
}

const CORE_NOTHING: &str = "Nothing there. Click a wire in a cable.";
const CORE_IS_CABLE: &str = "That is the cable; click one of its wires.";
const CORE_NOT_WIRE: &str = "That is not a wire in a cable.";

impl CableCore {
    /// The member a pick names: a route, a fan tail or bend, or a core
    /// label; anything else is refused in words.
    fn member(s: &Session, h: Handle) -> R<(WireId, String, Option<String>)> {
        read(s, |p, page| {
            let map = &p.pages.get(&page).ok_or(CORE_NOT_WIRE)?.map;
            let doc = p.ledger.document();
            let w = match map.get(h) {
                Some(Mapped::Fan(_)) => map.core_of(h).ok_or(CORE_IS_CABLE)?,
                Some(Mapped::Item(i)) => match doc.representation_of_item(i).map(|(_, r)| r.subject) {
                    Some(Subject::Wire(w)) => w,
                    Some(Subject::Route(r)) => cabled_wire(doc, r).ok_or(CORE_NOT_WIRE)?,
                    Some(Subject::Cable(_)) => return Err(CORE_IS_CABLE.into()),
                    _ => return Err(CORE_NOT_WIRE.into()),
                },
                _ => return Err(CORE_NOT_WIRE.into()),
            };
            let k = doc.cables_of(w).first().copied().ok_or(WIRE_NOT_IN_CABLE)?;
            let core =
                doc.cables().get(&k).and_then(|c| c.cores.iter().find(|m| m.wire() == Some(w))).and_then(|m| m.label_str().map(str::to_string));
            Ok((w, slash::wire_name(doc, w), core))
        })
        .map_err(other)
    }

    fn step(&mut self, s: &mut Session, i: Input) -> R<Step> {
        match (self.picked.take(), i) {
            (None, Input::Point(p)) => {
                if !in_range(p) {
                    return Err(other(OUT_OF_RANGE));
                }
                let h = pick(s, p).ok_or_else(|| other(CORE_NOTHING))?;
                self.picked = Some(Self::member(s, h)?);
                Ok(Step::Continue)
            }
            (None, Input::Enter | Input::Cancel) => Ok(Step::Done),
            (None, Input::Text(t)) => Err(not_a_point(&t)),
            (None, _) => Ok(Step::Continue),
            (Some((_, name, _)), Input::Enter) => {
                s.echo(format!("Core of {name} unchanged."));
                Ok(Step::Continue)
            }
            (Some((w, name, old)), Input::Text(t)) => {
                // `.` clears, and so does a blank label, as in the JSON form.
                let clear = t.trim() == "." || t.trim().is_empty();
                if clear && old.is_none() {
                    s.echo(format!("{name} has no core label."));
                    return Ok(Step::Continue);
                }
                let core = if clear { Value::Null } else { Value::String(t.clone()) };
                // Refused: the message, and the same wire's prompt again.
                let r = match commit(s, "cablecore", &json!({"conductor": w.hex(), "core": core})) {
                    Ok(r) => r,
                    Err(e) => {
                        self.picked = Some((w, name, old));
                        return Err(e);
                    }
                };
                let cable = read(s, |p, _| {
                    let doc = p.ledger.document();
                    Ok(doc.cables_of(w).first().map(|k| ecad_electrical::cables::cable_title(doc, *k)).unwrap_or_default())
                })
                .unwrap_or_default();
                match r.get("core").and_then(Value::as_str) {
                    Some(c) => s.echo(format!("Wire {name} is now core {c} of cable {cable}.")),
                    None => s.echo(format!("Core label of {name} cleared.")),
                }
                let dups: Vec<String> = r
                    .get("duplicates")
                    .and_then(Value::as_array)
                    .into_iter()
                    .flatten()
                    .filter_map(|v| v.as_str().and_then(WireId::parse))
                    .filter_map(|d| read(s, |p, _| Ok(slash::wire_name(p.ledger.document(), d))).ok())
                    .collect();
                if let (false, Some(c)) = (dups.is_empty(), r.get("core").and_then(Value::as_str)) {
                    s.echo(format!("Note: core {c} is also used by {} in this cable.", slash::name_list(&dups)));
                }
                Ok(Step::Continue)
            }
            (Some(_), Input::Cancel) => Ok(Step::Done),
            (Some(picked), _) => {
                self.picked = Some(picked);
                Ok(Step::Continue)
            }
        }
    }
}

impl Interactive for CableCore {
    fn name(&self) -> &'static str {
        "CABLECORE"
    }
    /// The picked wire is highlighted while its label is asked for.
    fn preview_objects(&self, s: &Session, _cursor: Vec2) -> Vec<Entity> {
        let Some((w, _, _)) = &self.picked else { return Vec::new() };
        let route = read(s, |p, page| {
            let doc = p.ledger.document();
            Ok(ecad_electrical::wires::routes::route_of_wire_on(doc, *w, page).and_then(|r| doc.routes().get(&r)).map(|r| r.points.clone()))
        });
        match route {
            Ok(Some(points)) => {
                let prim = Primitive::Polyline { vertices: points, closed: false };
                vec![preview_entity(s, crate::convert::kind_for_primitive(&prim), ATTACHED_ACI)]
            }
            _ => Vec::new(),
        }
    }
    fn prompt(&self, _s: &Session) -> Prompt {
        match &self.picked {
            None => Prompt::new("Select a wire in a cable", Accept::POINT),
            Some((_, name, core)) => {
                let mut p = Prompt::new(format!("Core number or colour for {name}"), Accept::TEXT);
                p.default = Some(core.clone().unwrap_or_else(|| "none".into()));
                p
            }
        }
    }
    fn input(&mut self, s: &mut Session, i: Input) -> R<Step> {
        paper_guard(s, &i)?;
        safely(s, "cablecore", |s| self.step(s, i))
    }
}
