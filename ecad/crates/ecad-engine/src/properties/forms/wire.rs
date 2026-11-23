//! Wire.

use super::{R, Safe, commit, hex_of, not_a_point, other, paper_guard, safely};
use crate::commands::{MAX_WIRE_POINTS, on_project};
use crate::interact::{
    ATTACHED_ACI, OUT_OF_RANGE, REFUSE_ACI, RING_PX, can_start, capture_at, capture_preview, connection_point_for, connection_point_name, in_range,
    line_kind, pixel, preview_entity, preview_text, pt, read, ring_kind, v2,
};
use cadcraft_engine::doc::{Entity, EntityKind};
use cadcraft_engine::geom::Vec2;
use cadcraft_engine::{Accept, Input, Interactive, Prompt, Session, Step};
use ecad_model::Pt;
use serde_json::{Value, json};

/// `wire`: click points; an end on a connection point attaches; a click on a connection point after the first
/// point (or Enter) commits the wire.
pub(crate) fn wire(s: &Session) -> R<Box<dyn Interactive>> {
    can_start(s)?;
    // Joining starts on (the registered default) every time the tool starts.
    let join = read(s, |p, _| Ok(ecad_electrical::wires::commands::wires::join_by_default(p.ledger.document()))).map_err(other)?;
    Ok(Box::new(Safe(Wire { join, ..Wire::default() })))
}

#[derive(Default)]
struct Wire {
    points: Vec<Pt>,
    /// The first point is on a connection point.
    first_on_connection_point: bool,
    /// Asking for the potential of new wires.
    asking_potential: bool,
    /// Ends that meet other wires join them (the Join keyword turns it off).
    join: bool,
    /// Asking for the category of new wires, and the one chosen
    /// for this run of the tool (`None`: auto, from the potential).
    asking_category: bool,
    category: Option<String>,
}

/// The page's quiet ends (cable reference ends): never joined.
fn quiet_on(p: &crate::project::Project, page: ecad_model::PageId) -> Vec<Pt> {
    crate::wires::commands::quiet_ends(p, page)
}

/// The Wire tool's potential for this session (`None`: auto).
fn wire_potential(s: &Session) -> Option<String> {
    read(s, |p, _| Ok(p.wire_potential.clone())).ok().flatten()
}

impl Wire {
    /// The open end of another wire within the pick radius of `p`.
    fn open_end(&self, s: &Session, p: Vec2) -> Option<Pt> {
        let tol = crate::interact::pick_radius(s).unwrap_or(ecad_electrical::wires::ON_WIRE).max(ecad_electrical::wires::ON_WIRE);
        read(s, |proj, page| Ok(ecad_electrical::wires::joining::open_end_near(proj.ledger.document(), page, pt(p), tol, &quiet_on(proj, page))))
            .ok()
            .flatten()
    }

    fn finish(&mut self, s: &mut Session) -> R<Step> {
        let points: Vec<[f64; 2]> = self.points.iter().map(|p| [p.x, p.y]).collect();
        let mut params = json!({ "points": points });
        if let Some(pot) = wire_potential(s) {
            params["potential"] = json!(pot);
        }
        if !self.join {
            params["join"] = json!(false);
        }
        if let Some(c) = &self.category {
            params["category"] = json!(c);
        }
        // A refusal draws nothing; the tool keeps its points, so the user can
        // type J and finish.
        let r = match commit(s, "wire", &params) {
            Ok(r) => r,
            Err(e) => {
                s.echo(e.to_string());
                return Ok(Step::Continue);
            }
        };
        let attached: Vec<(String, String)> =
            r.get("attached").and_then(Value::as_array).into_iter().flatten().map(|a| (hex_of(a, "end"), hex_of(a, "connection_point"))).collect();
        let names = read(s, |p, _| {
            let doc = p.ledger.document();
            Ok(attached
                .iter()
                .map(|(end, t)| {
                    let name = ecad_model::hex_id(t).map_or_else(|| t.clone(), |t| connection_point_name(doc, t.into()));
                    (end.clone(), name)
                })
                .collect::<Vec<_>>())
        })
        .unwrap_or_default();
        let number = r.get("number").and_then(Value::as_str).unwrap_or("?");
        let entry = |key: &str, e: &str| {
            r.get(key).and_then(Value::as_array).and_then(|a| a.iter().find(|x| x.get("end").and_then(Value::as_str) == Some(e)).cloned())
        };
        let outcome: Vec<String> = ["A", "B"]
            .iter()
            .map(|e| {
                if let Some(j) = entry("joined", e) {
                    if j.get("how").and_then(Value::as_str) == Some("merged") {
                        return format!("end {e} joins wire {number} (now one wire)");
                    }
                    return format!("end {e} lands on a wire there (a T)");
                }
                match names.iter().find(|(end, _)| end == e) {
                    Some((_, name)) => format!("end {e} attached to {name}"),
                    None => format!("end {e} not attached"),
                }
            })
            .collect();
        s.echo(format!("Wire {number} drawn; {}.", outcome.join(", ")));
        Ok(Step::Done)
    }

    fn step(&mut self, s: &mut Session, i: Input) -> R<Step> {
        if self.asking_category {
            let choice = match &i {
                Input::Enter | Input::Cancel => {
                    self.asking_category = false;
                    return Ok(Step::Continue);
                }
                Input::Keyword(k) | Input::Text(k) => k.trim().to_string(),
                _ => return Ok(Step::Continue),
            };
            self.asking_category = false;
            if choice.eq_ignore_ascii_case("auto") || choice.is_empty() {
                self.category = None;
                s.echo("New wires: category auto (from their potential).");
                return Ok(Step::Continue);
            }
            let id = read(s, |p, _| {
                ecad_electrical::styles::find(p.ledger.document(), &ecad_electrical::styles::wire::CATALOGUE, &choice).map_err(String::from)
            })
            .map_err(other)?;
            let name = read(s, |p, _| Ok(ecad_model::registry::styles::name(p.ledger.document(), &ecad_electrical::styles::wire::CATALOGUE, &id)))
                .unwrap_or_default();
            self.category = Some(id.text());
            s.echo(format!("New wires: category {name}."));
            return Ok(Step::Continue);
        }
        if self.asking_potential {
            let choice = match &i {
                Input::Enter => {
                    return {
                        self.asking_potential = false;
                        Ok(Step::Continue)
                    };
                }
                Input::Cancel => {
                    return {
                        self.asking_potential = false;
                        Ok(Step::Continue)
                    };
                }
                Input::Keyword(k) | Input::Text(k) => k.trim().to_string(),
                _ => return Ok(Step::Continue),
            };
            let value = if choice.eq_ignore_ascii_case("auto") || choice.is_empty() {
                None
            } else {
                Some(ecad_model::connectivity::wiring::valid_text("potential", Some(&choice)).map_err(other)?.unwrap_or_default())
            };
            on_project(s, |p, _, _| {
                // A table name is written as the table spells it.
                let v = value.map(|v| p.ledger.document().settings().wiring.potential(&v).map_or(v, |r| r.name.clone()));
                p.wire_potential = v;
                Ok(())
            })?;
            self.asking_potential = false;
            let shown = wire_potential(s).unwrap_or_else(|| "auto".into());
            s.echo(format!("New wires: potential {shown}."));
            return Ok(Step::Continue);
        }
        match i {
            Input::Keyword(k) if k == "Potential" && self.points.is_empty() => {
                self.asking_potential = true;
                Ok(Step::Continue)
            }
            Input::Keyword(k) if k == "Category" && self.points.is_empty() => {
                self.asking_category = true;
                Ok(Step::Continue)
            }
            Input::Keyword(k) if k == "Join" => {
                self.join = !self.join;
                s.echo(if self.join { "Joining is on." } else { "Joining is off: wire ends stay open where they meet other wires." });
                Ok(Step::Continue)
            }
            Input::Point(p) => {
                if !in_range(p) {
                    return Err(other(OUT_OF_RANGE));
                }
                let (q, on_connection_point) = match connection_point_for(s, p) {
                    Some((_, at, _)) => (at, true),
                    // A click near another wire's open end lands on it.
                    None if self.join => (self.open_end(s, p).unwrap_or_else(|| pt(p)), false),
                    None => (pt(p), false),
                };
                // The same point again adds no zero-length segment.
                if self.points.last().is_some_and(|l| l.near(q, 1e-12)) {
                    return Ok(Step::Continue);
                }
                if self.points.len() >= MAX_WIRE_POINTS {
                    s.echo(format!("A wire has at most {MAX_WIRE_POINTS} points; press Enter to finish."));
                    return Ok(Step::Continue);
                }
                self.points.push(q);
                if self.points.len() == 1 {
                    self.first_on_connection_point = on_connection_point;
                    return Ok(Step::Continue);
                }
                if on_connection_point { self.finish(s) } else { Ok(Step::Continue) }
            }
            Input::Keyword(k) if k == "Undo" => {
                self.points.pop();
                if self.points.is_empty() {
                    self.first_on_connection_point = false;
                }
                Ok(Step::Continue)
            }
            Input::Enter => match self.points.len() {
                0 => Ok(Step::Done),
                1 => {
                    s.echo("A wire needs at least two points; nothing was drawn.");
                    Ok(Step::Done)
                }
                _ => self.finish(s),
            },
            Input::Text(t) => Err(not_a_point(&t)),
            _ => Ok(Step::Continue),
        }
    }
}

impl Interactive for Wire {
    fn name(&self) -> &'static str {
        "WIRE"
    }
    fn prompt(&self, s: &Session) -> Prompt {
        if self.asking_category {
            let names = read(s, |p, _| {
                let doc = p.ledger.document();
                Ok(ecad_model::registry::styles::rows(doc, &ecad_electrical::styles::wire::CATALOGUE).into_iter().map(|r| r.name).collect::<Vec<_>>())
            })
            .unwrap_or_default();
            let mut p = Prompt::new(format!("Category for new wires [auto/{}]", names.join("/")), Accept::TEXT);
            p.default = Some("auto".into());
            return p;
        }
        if self.asking_potential {
            let names = read(s, |p, _| Ok(crate::wires::commands::potential_choices(p))).unwrap_or_default();
            let mut p = Prompt::new(format!("Potential for new wires [{}]", names.join("/")), Accept::TEXT);
            p.default = Some(wire_potential(s).unwrap_or_else(|| "auto".into()));
            return p;
        }
        match self.points.last() {
            None => {
                let mut p = Prompt::new("Specify first point of wire", Accept::POINT).kw(&["Potential", "Category", "Join"]);
                p.default = Some(wire_potential(s).unwrap_or_else(|| "auto".into()));
                p
            }
            Some(last) => {
                let text = if self.join { "Specify next point" } else { "Specify next point (join off)" };
                Prompt::new(text, Accept::POINT).kw(&["Undo", "Join"]).base(v2(*last))
            }
        }
    }
    fn input(&mut self, s: &mut Session, i: Input) -> R<Step> {
        paper_guard(s, &i)?;
        safely(s, "wire", |s| self.step(s, i))
    }
    /// The wire so far, and the rubber band to the cursor (or to the connection point it captures).
    fn preview(&self, s: &Session, cursor: Vec2) -> Vec<EntityKind> {
        if !crate::interact::model_space(s) {
            return Vec::new();
        }
        let mut out: Vec<EntityKind> = self.points.windows(2).filter_map(|w| Some(line_kind(*w.first()?, *w.get(1)?))).collect();
        if let Some(last) = self.points.last()
            && in_range(cursor)
        {
            let to = capture_at(s, cursor).map_or(pt(cursor), |c| c.at);
            out.push(line_kind(*last, to));
        }
        out
    }
    /// The attach ring and name of a captured connection point; a dot on a first point that attaches.
    fn preview_objects(&self, s: &Session, cursor: Vec2) -> Vec<Entity> {
        let captured = capture_at(s, cursor);
        let mut out = captured.as_ref().map(|c| capture_preview(s, c, ATTACHED_ACI)).unwrap_or_default();
        // An end on another wire's open end joins it (`join`), on its interior or
        // on two or more ends lands on it (a T), a refused merge shows the reason in red; with
        // Join off, nothing.
        if captured.is_none()
            && self.join
            && in_range(cursor)
            && let Some(px) = pixel(s)
        {
            let tol = crate::interact::pick_radius(s).unwrap_or(ecad_electrical::wires::ON_WIRE).max(ecad_electrical::wires::ON_WIRE);
            let potential = wire_potential(s);
            let seen = read(s, |p, page| {
                let doc = p.ledger.document();
                let quiet = quiet_on(p, page);
                let at = ecad_electrical::wires::joining::open_end_near(doc, page, pt(cursor), tol, &quiet)
                    .or_else(|| ecad_electrical::wires::placing::route_at(doc, page, pt(cursor), tol, None).map(|on| on.at));
                Ok(at.and_then(|at| ecad_electrical::wires::joining::join_preview(doc, page, at, &quiet, potential.as_deref())))
            });
            if let Ok(Some((at, text, refused))) = seen {
                let aci = if refused { REFUSE_ACI } else { ATTACHED_ACI };
                out.push(preview_entity(s, ring_kind(at, RING_PX, px), aci));
                out.push(preview_text(s, at, px, text, aci));
            }
        }
        if let (true, Some(first), Some(px)) = (self.first_on_connection_point, self.points.first(), pixel(s)) {
            out.extend([4.0, 2.0].map(|d| preview_entity(s, ring_kind(*first, d, px), ATTACHED_ACI)));
        }
        out
    }
}
