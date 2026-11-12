//! Brief 024's Location Box tool (design 024 §3.5): a location, then a rectangle (two corners), a
//! closed polyline (points, then Close) or an existing closed polyline (Select). Live, the device
//! symbols the outline would hold get a ring. Esc at any step writes nothing; the commit is the
//! `locationbox` JSON form, one transaction.

use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, Ordering};

use cadcraft_engine::doc::{Entity, EntityKind};
use cadcraft_engine::geom::Vec2;
use cadcraft_engine::{Accept, Input, Interactive, Prompt, Session, Step};
use ecad_model::book::LOCATION_KEY;
use ecad_model::{ItemId, ItemKind, Primitive, Pt, Subject};
use serde_json::json;

use super::forms::{R, Safe, commit, other, paper_guard, safely};
use super::{ATTACHED_ACI, OUT_OF_RANGE, can_start, in_range, pick_radius, preview_entity, pt, read};
use crate::convert::kind_for_primitive;

/// The drawing mode kept for the session (rectangle unless Polyline was chosen).
static POLYLINE: AtomicBool = AtomicBool::new(false);
/// The last location used, the next prompt's default.
static LAST: Mutex<Option<String>> = Mutex::new(None);
/// `ecad.check-warning`.
const WARNING_ACI: u8 = 2;

/// `locationbox`: the click-based form.
pub(crate) fn locationbox(s: &Session) -> R<Box<dyn Interactive>> {
    can_start(s)?;
    let page_default = read(s, |p, page| {
        let doc = p.ledger.document();
        Ok(doc.pages.get(&page).and_then(|x| x.properties.get(LOCATION_KEY)).or_else(|| doc.properties.get(LOCATION_KEY)).cloned())
    })
    .unwrap_or(None);
    let default = LAST.lock().ok().and_then(|g| g.clone()).or(page_default).unwrap_or_else(|| "+CP1".into());
    Ok(Box::new(Safe(LocationBox { default, location: None, mode: Mode::Start, points: Vec::new(), anchors: Vec::new() })))
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Mode {
    Start,
    Rectangle,
    Polyline,
    Select,
}

struct LocationBox {
    default: String,
    location: Option<String>,
    mode: Mode,
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
                    let Subject::Component(c) = r.subject else { return None };
                    if !crate::locations::is_device(doc, c) {
                        return None;
                    }
                    let own = doc.components.get(&c).and_then(|x| x.properties.get(LOCATION_KEY)).cloned();
                    Some((crate::locations::anchor(doc, r)?, own))
                })
                .collect())
        })
        .unwrap_or_default();
    }

    fn outline(&self, cursor: Option<Pt>) -> Vec<Pt> {
        match (self.mode, self.points.as_slice(), cursor) {
            (Mode::Rectangle, [a], Some(c)) => crate::output_cmds::rectangle(*a, c),
            (Mode::Polyline, pts, c) => {
                let mut v = pts.to_vec();
                v.extend(c);
                v
            }
            _ => Vec::new(),
        }
    }

    fn finish(&mut self, s: &mut Session, params: serde_json::Value) -> R<Step> {
        match commit(s, "locationbox", &params) {
            Ok(r) => {
                if let Ok(mut g) = LAST.lock() {
                    *g = self.location.clone();
                }
                let _ = r;
                Ok(Step::Done)
            }
            Err(e) => {
                // The step stays open (design 024 §3.5): too small, not a closed polyline…
                s.echo(e.to_string());
                if self.mode == Mode::Rectangle {
                    self.points.clear();
                }
                Ok(Step::Continue)
            }
        }
    }

    fn close(&mut self, s: &mut Session) -> R<Step> {
        if self.points.len() < 3 {
            s.echo("A location box needs at least three points.");
            return Ok(Step::Continue);
        }
        let pts: Vec<[f64; 2]> = self.points.iter().map(|p| [p.x, p.y]).collect();
        let params = json!({"location": self.location, "points": pts});
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
                    POLYLINE.store(true, Ordering::Relaxed);
                    self.mode = Mode::Polyline;
                    Ok(Step::Continue)
                }
                (Mode::Start | Mode::Polyline, "r" | "rectangle") if self.points.is_empty() => {
                    POLYLINE.store(false, Ordering::Relaxed);
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
                        self.mode = if POLYLINE.load(Ordering::Relaxed) { Mode::Polyline } else { Mode::Rectangle };
                        self.points.push(at);
                        Ok(Step::Continue)
                    }
                    Mode::Rectangle => match self.points.first().copied() {
                        None => {
                            self.points.push(at);
                            Ok(Step::Continue)
                        }
                        Some(a) => {
                            let params = json!({"location": self.location, "corners": [[a.x, a.y], [at.x, at.y]]});
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
                        let item = read(s, |p, page| Ok(outline_at(p.ledger.document(), page, at, tol))).map_err(other)?;
                        match item {
                            Some(id) => {
                                let params = json!({"location": self.location, "item": id.hex()});
                                self.finish(s, params)
                            }
                            None => {
                                s.echo(crate::locations::NOT_CLOSED);
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

/// The polyline item on `page` whose outline passes within `tol` of `at` (any, closed or not:
/// the command says which ones are refused).
fn outline_at(doc: &ecad_model::Document, page: ecad_model::PageId, at: Pt, tol: f64) -> Option<ItemId> {
    doc.items
        .iter()
        .filter(|(_, i)| i.page == page)
        .filter_map(|(id, i)| match &i.kind {
            ItemKind::Primitive(Primitive::Polyline { vertices, closed }) => {
                let n = vertices.len();
                let segs = if *closed { n } else { n.saturating_sub(1) };
                let d = (0..segs).filter_map(|k| Some(seg_dist(at, *vertices.get(k)?, *vertices.get((k + 1) % n)?))).fold(f64::INFINITY, f64::min);
                (d <= tol).then_some((d, *id))
            }
            _ => None,
        })
        .min_by(|a, b| a.0.total_cmp(&b.0))
        .map(|(_, id)| id)
}

fn seg_dist(p: Pt, a: Pt, b: Pt) -> f64 {
    let (dx, dy) = (b.x - a.x, b.y - a.y);
    let len2 = dx * dx + dy * dy;
    let t = if len2 > 0.0 { (((p.x - a.x) * dx + (p.y - a.y) * dy) / len2).clamp(0.0, 1.0) } else { 0.0 };
    p.dist(Pt::new(a.x + t * dx, a.y + t * dy))
}

impl Interactive for LocationBox {
    fn name(&self) -> &'static str {
        "LOCATIONBOX"
    }
    fn prompt(&self, _s: &Session) -> Prompt {
        if self.location.is_none() {
            return Prompt::new(format!("Location for the box <{}>", self.default), Accept::TEXT).default(self.default.clone());
        }
        match (self.mode, self.points.len()) {
            (Mode::Start, _) => {
                let mode = if POLYLINE.load(Ordering::Relaxed) { "Polyline" } else { "Rectangle" };
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
        safely(s, "locationbox", |s| self.step(s, i))
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
        let loc = self.location.as_deref().map(|l| if l.starts_with('+') { l.to_string() } else { format!("+{l}") });
        self.anchors
            .iter()
            .filter(|(a, _)| crate::locations::contains(&v, *a))
            .map(|(a, own)| {
                let aci = if own.is_some() && own.as_deref() != loc.as_deref() { WARNING_ACI } else { ATTACHED_ACI };
                preview_entity(s, kind_for_primitive(&Primitive::Circle { center: *a, radius: r }), aci)
            })
            .collect()
    }
}
