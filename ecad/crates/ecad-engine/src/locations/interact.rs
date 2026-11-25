//! The Location Box tool and the Function Box tool:
//! a location (or function), then a rectangle (two corners), a closed polyline (points,
//! then Close) or an existing closed polyline (Select). Live, the device symbols the outline would
//! hold get a ring (`ecad.check-warning` for one with another value). Esc at any step writes
//! nothing; the commit is the `locationbox` / `functionbox` JSON form, one transaction.

use cadcraft_engine::doc::{Entity, EntityKind};
use cadcraft_engine::geom::Vec2;
use cadcraft_engine::{Accept, Input, Interactive, Prompt, Session, Step};
use ecad_model::{Primitive, Pt, Subject};
use serde_json::json;

use crate::convert::kind_for_primitive;
use crate::pointer::pick::{can_start, in_range, pick_radius, pt, read};
use crate::pointer::preview::{ATTACHED_ACI, preview_entity};
use crate::properties::forms::{R, Safe, commit, other, paper_guard, safely};
use ecad_electrical::commands::input::OUT_OF_RANGE;

/// `ecad.check-warning`.
const WARNING_ACI: u8 = 2;

use ecad_electrical::locations::designations::Aspect;

/// `locationbox`: the click-based form.
pub(crate) fn locationbox(s: &Session) -> R<Box<dyn Interactive>> {
    start(s, Aspect::Location)
}

/// `functionbox`: the same form for a function.
pub(crate) fn functionbox(s: &Session) -> R<Box<dyn Interactive>> {
    start(s, Aspect::Function)
}

fn start(s: &Session, aspect: Aspect) -> R<Box<dyn Interactive>> {
    can_start(s)?;
    let page_default = read(s, |p, page| {
        let doc = p.ledger.document();
        // A stored entry key, shown by its code.
        let stored = ecad_electrical::locations::designations::inherited_key(doc, page, aspect);
        Ok(ecad_electrical::locations::designations::shown(doc, aspect, stored.as_deref()))
    })
    .unwrap_or(None);
    // The project's defaults: the value last given and the drawing mode last chosen.
    let (kept, polyline) = read(s, |p, _| Ok((p.defaults.last_box_value(aspect).clone(), p.defaults.box_polyline))).unwrap_or((None, false));
    let fallback = ecad_electrical::locations::designations::example(aspect).to_string();
    let default = kept.or(page_default).unwrap_or(fallback);
    Ok(Box::new(Safe(LocationBox { aspect, default, location: None, mode: Mode::Start, polyline, points: Vec::new(), anchors: Vec::new() })))
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Mode {
    Start,
    Rectangle,
    Polyline,
    Select,
}

struct LocationBox {
    /// Which value the box carries.
    aspect: Aspect,
    default: String,
    location: Option<String>,
    mode: Mode,
    /// The drawing mode a first point starts (the project's default, changed by a keyword).
    polyline: bool,
    points: Vec<Pt>,
    /// The device symbols on the page: anchor and own location.
    anchors: Vec<(Pt, Option<String>)>,
}

impl LocationBox {
    fn load_anchors(&mut self, s: &Session) {
        self.anchors = read(s, |p, page| {
            let doc = p.ledger.document();
            Ok(doc
                .representations_on(page)
                .filter_map(|(_, r)| {
                    let Subject::Device(c) = r.subject else { return None };
                    if !ecad_electrical::locations::is_device(doc, c) {
                        return None;
                    }
                    let own = ecad_electrical::locations::designations::shown(
                        doc,
                        self.aspect,
                        doc.devices().get(&c).and_then(|x| ecad_electrical::locations::designations::device_key(x, self.aspect)).as_deref(),
                    );
                    Some((ecad_electrical::locations::anchor(doc, r)?, own))
                })
                .collect())
        })
        .unwrap_or_default();
    }

    fn outline(&self, cursor: Option<Pt>) -> Vec<Pt> {
        match (self.mode, self.points.as_slice(), cursor) {
            (Mode::Rectangle, [a], Some(c)) => ecad_electrical::reports::commands::boxes::rectangle(*a, c),
            (Mode::Polyline, pts, c) => {
                let mut v = pts.to_vec();
                v.extend(c);
                v
            }
            _ => Vec::new(),
        }
    }

    /// The command and its value parameter.
    fn command(&self) -> (&'static str, &'static str) {
        match self.aspect {
            Aspect::Function => ("functionbox", "function"),
            Aspect::Location => ("locationbox", "location"),
        }
    }

    /// The JSON form with the value and `shape` (corners, points or item).
    fn params(&self, shape: (&str, serde_json::Value)) -> serde_json::Value {
        let mut v = json!({ self.command().1: self.location });
        v[shape.0] = shape.1;
        v
    }

    fn finish(&mut self, s: &mut Session, params: serde_json::Value) -> R<Step> {
        match commit(s, self.command().0, &params) {
            Ok(_) => {
                let (aspect, value) = (self.aspect, self.location.clone());
                let _ = crate::commands::on_any_page(s, |p, _| {
                    *p.defaults.last_box_value_mut(aspect) = value;
                    Ok(())
                });
                Ok(Step::Done)
            }
            Err(e) => {
                // The step stays open: too small, not a closed polyline…
                s.echo(e.to_string());
                if self.mode == Mode::Rectangle {
                    self.points.clear();
                }
                Ok(Step::Continue)
            }
        }
    }

    /// Draw closed polylines (`true`) or rectangles from now on, here and in the project's defaults.
    fn keep_mode(&mut self, s: &mut Session, polyline: bool) {
        self.polyline = polyline;
        let _ = crate::commands::on_any_page(s, |p, _| {
            p.defaults.box_polyline = polyline;
            Ok(())
        });
    }

    fn close(&mut self, s: &mut Session) -> R<Step> {
        if self.points.len() < 3 {
            s.echo("A location box needs at least three points.");
            return Ok(Step::Continue);
        }
        let pts: Vec<[f64; 2]> = self.points.iter().map(|p| [p.x, p.y]).collect();
        let params = self.params(("points", json!(pts)));
        self.finish(s, params)
    }

    fn step(&mut self, s: &mut Session, i: Input) -> R<Step> {
        if self.location.is_none() {
            return match i {
                Input::Text(t) if !t.trim().is_empty() => {
                    self.location = Some(t.trim().to_string());
                    self.load_anchors(s);
                    Ok(Step::Continue)
                }
                Input::Enter | Input::Text(_) => {
                    self.location = Some(self.default.clone());
                    self.load_anchors(s);
                    Ok(Step::Continue)
                }
                Input::Cancel => Ok(Step::Cancel),
                _ => Ok(Step::Continue),
            };
        }
        match i {
            Input::Cancel => Ok(Step::Cancel),
            Input::Keyword(k) | Input::Text(k) => match (self.mode, k.trim().to_ascii_lowercase().as_str()) {
                (Mode::Start | Mode::Rectangle, "p" | "polyline") if self.points.is_empty() => {
                    self.keep_mode(s, true);
                    self.mode = Mode::Polyline;
                    Ok(Step::Continue)
                }
                (Mode::Start | Mode::Polyline, "r" | "rectangle") if self.points.is_empty() => {
                    self.keep_mode(s, false);
                    self.mode = Mode::Rectangle;
                    Ok(Step::Continue)
                }
                (Mode::Start | Mode::Rectangle | Mode::Polyline, "s" | "select") if self.points.is_empty() => {
                    self.mode = Mode::Select;
                    Ok(Step::Continue)
                }
                (Mode::Polyline, "u" | "undo") => {
                    self.points.pop();
                    Ok(Step::Continue)
                }
                (Mode::Polyline, "c" | "close") => self.close(s),
                _ => Err(other(format!("\"{}\" is not an option here.", k.trim()))),
            },
            Input::Enter => match self.mode {
                Mode::Polyline if !self.points.is_empty() => self.close(s),
                Mode::Start if self.points.is_empty() => {
                    self.mode = Mode::Rectangle;
                    Ok(Step::Continue)
                }
                _ => Ok(Step::Done),
            },
            Input::Point(p) => {
                if !in_range(p) {
                    return Err(other(OUT_OF_RANGE));
                }
                let at = pt(p);
                match self.mode {
                    Mode::Start => {
                        self.mode = if self.polyline { Mode::Polyline } else { Mode::Rectangle };
                        self.points.push(at);
                        Ok(Step::Continue)
                    }
                    Mode::Rectangle => match self.points.first().copied() {
                        None => {
                            self.points.push(at);
                            Ok(Step::Continue)
                        }
                        Some(a) => {
                            let params = self.params(("corners", json!([[a.x, a.y], [at.x, at.y]])));
                            self.finish(s, params)
                        }
                    },
                    Mode::Polyline => {
                        let tol = pick_radius(s).unwrap_or(1.0);
                        if self.points.len() >= 3 && self.points.first().is_some_and(|f| f.dist(at) <= tol) {
                            return self.close(s);
                        }
                        self.points.push(at);
                        Ok(Step::Continue)
                    }
                    Mode::Select => {
                        let tol = pick_radius(s).unwrap_or(1.0);
                        let item =
                            read(s, |p, page| Ok(ecad_electrical::locations::outline_at(p.ledger.document(), page, at, tol))).map_err(other)?;
                        match item {
                            Some(id) => {
                                let params = self.params(("item", json!(id.hex())));
                                self.finish(s, params)
                            }
                            None => {
                                s.echo(ecad_electrical::locations::NOT_CLOSED);
                                Ok(Step::Continue)
                            }
                        }
                    }
                }
            }
            _ => Ok(Step::Continue),
        }
    }
}

impl Interactive for LocationBox {
    fn name(&self) -> &'static str {
        match self.aspect {
            Aspect::Function => "FUNCTIONBOX",
            Aspect::Location => "LOCATIONBOX",
        }
    }
    fn prompt(&self, _s: &Session) -> Prompt {
        if self.location.is_none() {
            let what = match self.aspect {
                Aspect::Function => "Function",
                Aspect::Location => "Location",
            };
            return Prompt::new(format!("{what} for the box <{}>", self.default), Accept::TEXT).default(self.default.clone());
        }
        match (self.mode, self.points.len()) {
            (Mode::Start, _) => {
                let mode = if self.polyline { "Polyline" } else { "Rectangle" };
                Prompt::new(format!("First corner or [Polyline/Rectangle/Select] <{mode}>"), Accept::POINT).kw(&["Polyline", "Rectangle", "Select"])
            }
            (Mode::Rectangle, 0) => Prompt::new("First corner or [Polyline/Select]", Accept::POINT).kw(&["Polyline", "Select"]),
            (Mode::Rectangle, _) => Prompt::new("Other corner", Accept::POINT),
            (Mode::Polyline, 0) => Prompt::new("First point or [Rectangle/Select]", Accept::POINT).kw(&["Rectangle", "Select"]),
            (Mode::Polyline, _) => Prompt::new("Next point or [Undo/Close]", Accept::POINT).kw(&["Undo", "Close"]),
            (Mode::Select, _) => Prompt::new("Pick a closed polyline", Accept::POINT),
        }
    }
    fn input(&mut self, s: &mut Session, i: Input) -> R<Step> {
        paper_guard(s, &i)?;
        let command = self.command().0;
        safely(s, command, |s| self.step(s, i))
    }
    fn preview(&self, _s: &Session, cursor: Vec2) -> Vec<EntityKind> {
        let v = self.outline(Some(pt(cursor)));
        if v.len() < 2 {
            return Vec::new();
        }
        vec![kind_for_primitive(&Primitive::Polyline { vertices: v, closed: true })]
    }
    fn preview_objects(&self, s: &Session, cursor: Vec2) -> Vec<Entity> {
        let v = self.outline(Some(pt(cursor)));
        if v.len() < 3 {
            return Vec::new();
        }
        let r = pick_radius(s).unwrap_or(1.0) * 1.5;
        let sign = self.aspect.sign();
        let loc = self.location.as_deref().map(|l| if l.starts_with(sign) { l.to_string() } else { format!("{sign}{l}") });
        self.anchors
            .iter()
            .filter(|(a, _)| ecad_electrical::locations::contains(&v, *a))
            .map(|(a, own)| {
                let aci = if own.is_some() && own.as_deref() != loc.as_deref() { WARNING_ACI } else { ATTACHED_ACI };
                preview_entity(s, kind_for_primitive(&Primitive::Circle { center: *a, radius: r }), aci)
            })
            .collect()
    }
}
