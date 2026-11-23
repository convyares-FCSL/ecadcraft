//! Add Connection Point.

use super::{R, Safe, commit, not_a_point, other, paper_guard, safely};
use crate::interact::{
    CONNECTION_POINT_ACI, CONNECTION_POINT_RING_PX, NO_OUTLINE, can_start, drawn_component, from_cursor, in_range, next_connection_point_name, pick,
    pixel, preview_entity, pt, read, ring_kind, v2, valid_name,
};
use cadcraft_engine::doc::{Entity, Handle};
use cadcraft_engine::geom::Vec2;
use cadcraft_engine::{Accept, Input, Interactive, Prompt, Session, Step};
use ecad_electrical::commands::input::OUT_OF_RANGE;
use ecad_model::{DeviceId, Pt, Subject};
use serde_json::json;
use std::collections::BTreeSet;

/// `addconnectionpoint`: click on a component's outline to add a connection point there; repeat until Enter.
pub(crate) fn addconnectionpoint(s: &Session) -> R<Box<dyn Interactive>> {
    can_start(s)?;
    Ok(Box::new(Safe(AddConnectionPoint::default())))
}

#[derive(Default)]
struct AddConnectionPoint {
    /// A name typed with the Name option, for the next connection point only.
    name: Option<String>,
    /// At the "Enter connection point name" prompt.
    naming: bool,
}

impl AddConnectionPoint {
    /// The name the next connection point gets, when it is known: a typed one, else the default of the
    /// component under the cursor (the one a click there adds to, so the prompt and the commit
    /// agree). With the cursor off any component a typed point gets the default of the component
    /// it lands on: the name is known only when every drawn component on the page has the same
    /// default; otherwise `None` and the prompt promises no number.
    fn shown_name(&self, s: &Session) -> Option<String> {
        if let Some(n) = &self.name {
            return Some(n.clone());
        }
        if let Ok((_, c, _)) = Self::resolve(s, s.cursor, true) {
            return read(s, |p, _| Ok(next_connection_point_name(p.ledger.document(), c))).ok();
        }
        // Off any component: a number only when every drawn component on the page would get the
        // same one, so whichever a typed point lands on gets what the prompt says.
        read(s, |p, page| {
            let doc = p.ledger.document();
            let names: BTreeSet<String> = doc
                .representations()
                .values()
                .filter(|r| r.page == page && r.form == ecad_model::Form::Drawn)
                .filter_map(|r| match r.subject {
                    Subject::Device(c) => Some(next_connection_point_name(doc, c)),
                    _ => None,
                })
                .collect();
            Ok(if names.len() == 1 { names.into_iter().next() } else { None })
        })
        .ok()
        .flatten()
    }

    /// Where a point puts a connection point: the picked outline object, its component and the point. A
    /// click lands on the outline (the nearest point of the picked object); a typed or snapped
    /// point on it is kept exactly.
    fn resolve(s: &Session, p: Vec2, clicked: bool) -> Result<(Handle, DeviceId, Pt), String> {
        if !in_range(p) {
            return Err(OUT_OF_RANGE.into());
        }
        let h = pick(s, p).ok_or(NO_OUTLINE)?;
        let px = pixel(s).unwrap_or(0.0);
        read(s, |proj, page| {
            let (c, prim) = drawn_component(proj, page, h)?;
            let at = match ecad_model::geometry::geom::nearest_on(&prim, pt(p)) {
                Some(q) if clicked && q.dist(pt(p)) > px * 1e-6 && in_range(v2(q)) => q,
                _ => pt(p),
            };
            Ok((h, c, at))
        })
    }

    fn step(&mut self, s: &mut Session, i: Input) -> R<Step> {
        if self.naming {
            return match i {
                Input::Enter => {
                    self.naming = false;
                    Ok(Step::Continue)
                }
                Input::Text(t) | Input::Keyword(t) => match valid_name(&t) {
                    Some(n) => {
                        self.name = Some(n);
                        self.naming = false;
                        Ok(Step::Continue)
                    }
                    None => Err(other(crate::interact::BAD_NAME)),
                },
                _ => Ok(Step::Continue),
            };
        }
        match i {
            Input::Keyword(k) if k == "Name" => {
                self.naming = true;
                Ok(Step::Continue)
            }
            Input::Point(p) => {
                let clicked = from_cursor(s, p);
                let (h, c, at) = Self::resolve(s, p, clicked).map_err(other)?;
                let label = match &self.name {
                    Some(n) => n.clone(),
                    None => read(s, |proj, _| Ok(next_connection_point_name(proj.ledger.document(), c))).map_err(other)?,
                };
                commit(s, "addconnectionpoint", &json!({"handle": h.hex(), "at": [at.x, at.y], "label": label}))?;
                self.name = None;
                s.echo(format!("Connection point {label} added to component {}.", c.hex()));
                Ok(Step::Continue)
            }
            Input::Enter => Ok(Step::Done),
            Input::Text(t) => Err(not_a_point(&t)),
            _ => Ok(Step::Continue),
        }
    }
}

impl Interactive for AddConnectionPoint {
    fn name(&self) -> &'static str {
        "ADDCONNECTIONPOINT"
    }
    fn prompt(&self, s: &Session) -> Prompt {
        let shown = self.shown_name(s);
        if self.naming {
            let p = Prompt::new("Enter connection point name", Accept::TEXT);
            return match shown {
                Some(n) => p.default(n),
                None => p,
            };
        }
        let message = match shown {
            Some(n) => format!("Specify point for connection point {n} on a component"),
            // Off any component the number is not known yet: it is the smallest unused one of the
            // component the point lands on.
            None => "Specify point for a connection point on a component".to_string(),
        };
        Prompt::new(message, Accept::POINT).kw(&["Name"])
    }
    fn input(&mut self, s: &mut Session, i: Input) -> R<Step> {
        paper_guard(s, &i)?;
        safely(s, "addconnectionpoint", |s| self.step(s, i))
    }
    /// The connection point ring where a click would put the connection point.
    fn preview_objects(&self, s: &Session, cursor: Vec2) -> Vec<Entity> {
        let (Some(px), false) = (pixel(s), self.naming) else { return Vec::new() };
        match Self::resolve(s, cursor, true) {
            Ok((_, _, at)) => vec![preview_entity(s, ring_kind(at, CONNECTION_POINT_RING_PX, px), CONNECTION_POINT_ACI)],
            Err(_) => Vec::new(),
        }
    }
}
