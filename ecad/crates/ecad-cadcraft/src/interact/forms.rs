//! The interactive forms (design 008 §5 to §8, design 004 §5). Each one resolves values from
//! clicks, keys and typed points, then commits the command's JSON form with them.

use std::collections::BTreeSet;
use std::panic::{AssertUnwindSafe, catch_unwind};

use cadcraft_engine::doc::color::{Color as CColor, Rgb};
use cadcraft_engine::doc::{Entity, EntityKind, Handle};
use cadcraft_engine::geom::Vec2;
use cadcraft_engine::{Accept, EngineError, Input, Interactive, Prompt, Session, Step};
use ecad_model::{CableId, ComponentId, ConductorId, Definition, End, Placement, Primitive, Pt, Subject};
use serde_json::{Value, json};

use super::{
    ATTACHED_ACI, LOOSE_ACI, NO_OUTLINE, OUT_OF_RANGE, REFUSE_ACI, RING_PX, TERMINAL_ACI, TERMINAL_PX, can_start,
    capture_at, capture_preview, drawn_component, from_cursor, in_range, line_kind, nearest_on, next_terminal_name,
    pick, pixel, preview_entity, preview_text, pt, read, ring_kind, terminal_for, terminal_name, v2, valid_name,
};
use crate::commands::{MAX_HANDLES, MAX_WIRE_POINTS, builtin_coil, with_project};
use crate::project::Mapped;
use crate::slash;
use crate::views::View;

type R<T> = cadcraft_engine::Result<T>;

fn other(m: impl Into<String>) -> EngineError {
    EngineError::Other(m.into())
}

/// Run one step of a form. A panic (which the code is written never to raise) does not reach the
/// engine, which would put back the drawing from before the command although earlier objects of
/// this run are committed; instead the page is redrawn from the project (PROJ-02: roll forward)
/// and the command ends.
fn safely(s: &mut Session, name: &str, f: impl FnOnce(&mut Session) -> R<Step>) -> R<Step> {
    match catch_unwind(AssertUnwindSafe(|| f(&mut *s))) {
        Ok(r) => r,
        Err(_) => {
            let _ = with_project(s, |p, pg, _| {
                p.refresh_pages(pg, Some(name));
                Ok(())
            });
            s.echo(format!(
                "Internal error in {}; the page was redrawn from the project. Objects already added stay.",
                name.to_ascii_uppercase()
            ));
            Ok(Step::Done)
        }
    }
}

/// A form whose prompt and previews are guarded too. The engine's own guard would mark a form
/// whose `prompt` or `preview` panicked as broken and end it at the next input by putting back
/// the drawing from before the command, behind objects this run already committed. Here such a
/// panic gives a plain prompt or no preview instead; `input` and `begin` are guarded by
/// [`safely`].
struct Safe<M>(M);

/// The prompt shown when a form's own prompt failed.
fn fallback_prompt() -> Prompt {
    Prompt::new("Internal error in this prompt; press Enter or Esc to end the command", Accept::POINT)
}

/// Fault injection (`fault-injection` feature): while set, prompts and previews of the forms panic.
#[cfg(feature = "fault-injection")]
fn injected_fault(s: &Session) {
    if read(s, |p, _| Ok(p.faults.panic_in_form_preview)).unwrap_or(false) {
        crate::fault_panic();
    }
}

#[cfg(not(feature = "fault-injection"))]
fn injected_fault(_s: &Session) {}

impl<M: Interactive> Interactive for Safe<M> {
    fn name(&self) -> &'static str {
        self.0.name()
    }
    fn prompt(&self, s: &Session) -> Prompt {
        catch_unwind(AssertUnwindSafe(|| {
            injected_fault(s);
            self.0.prompt(s)
        }))
        .unwrap_or_else(|_| fallback_prompt())
    }
    fn input(&mut self, s: &mut Session, i: Input) -> R<Step> {
        self.0.input(s, i)
    }
    fn begin(&mut self, s: &mut Session) -> R<Step> {
        self.0.begin(s)
    }
    fn preview(&self, s: &Session, cursor: Vec2) -> Vec<EntityKind> {
        catch_unwind(AssertUnwindSafe(|| {
            injected_fault(s);
            self.0.preview(s, cursor)
        }))
        .unwrap_or_default()
    }
    fn preview_objects(&self, s: &Session, cursor: Vec2) -> Vec<Entity> {
        catch_unwind(AssertUnwindSafe(|| {
            injected_fault(s);
            self.0.preview_objects(s, cursor)
        }))
        .unwrap_or_default()
    }
}

/// Commit the command's JSON form with resolved values: the same path as `name {json}` typed at
/// `Command:` (start check, transaction, refresh, commit gate).
fn commit(s: &mut Session, id: &str, params: &Value) -> R<Value> {
    s.execute(id, params).map_err(|e| other(e.to_string()))
}

/// Points and picks are refused while a paper-space layout is shown (the user switched layouts
/// while the form ran): ECAD objects are drawn in the page's model space only.
fn paper_guard(s: &Session, i: &Input) -> R<()> {
    if matches!(i, Input::Point(_) | Input::Pick(_)) && !super::model_space(s) {
        return Err(other(super::PAPER_SPACE));
    }
    Ok(())
}

/// Text typed at a point prompt that is not a point or keyword.
fn not_a_point(t: &str) -> EngineError {
    if t.contains(',') || t.contains('<') { other(OUT_OF_RANGE) } else { other("Point or option keyword required.") }
}

fn hex_of(v: &Value, key: &str) -> String {
    v.get(key).and_then(Value::as_str).unwrap_or_default().to_string()
}

fn end_name(e: End) -> &'static str {
    match e {
        End::A => "A",
        End::B => "B",
    }
}

// ---------------------------------------------------------------- Add Terminal (§5)

/// `addterminal`: click on a component's outline to add a terminal there; repeat until Enter.
pub(crate) fn addterminal(s: &Session) -> R<Box<dyn Interactive>> {
    can_start(s)?;
    Ok(Box::new(Safe(AddTerminal::default())))
}

#[derive(Default)]
struct AddTerminal {
    /// A name typed with the Name option, for the next terminal only.
    name: Option<String>,
    /// At the "Enter terminal name" prompt.
    naming: bool,
}

impl AddTerminal {
    /// The name the next terminal gets, when it is known: a typed one, else the default of the
    /// component under the cursor (the one a click there adds to, so the prompt and the commit
    /// agree). With the cursor off any component a typed point gets the default of the component
    /// it lands on: the name is known only when every drawn component on the page has the same
    /// default; otherwise `None` and the prompt promises no number.
    fn shown_name(&self, s: &Session) -> Option<String> {
        if let Some(n) = &self.name {
            return Some(n.clone());
        }
        if let Ok((_, c, _)) = Self::resolve(s, s.cursor, true) {
            return read(s, |p, _| Ok(next_terminal_name(p.ledger.document(), c))).ok();
        }
        // Off any component: a number only when every drawn component on the page would get the
        // same one, so whichever a typed point lands on gets what the prompt says.
        read(s, |p, page| {
            let doc = p.ledger.document();
            let names: BTreeSet<String> = doc
                .representations
                .values()
                .filter(|r| r.page == page && r.form == ecad_model::Form::Drawn)
                .filter_map(|r| match r.subject {
                    Subject::Component(c) => Some(next_terminal_name(doc, c)),
                    _ => None,
                })
                .collect();
            Ok(if names.len() == 1 { names.into_iter().next() } else { None })
        })
        .ok()
        .flatten()
    }

    /// Where a point puts a terminal: the picked outline object, its component and the point. A
    /// click lands on the outline (the nearest point of the picked object); a typed or snapped
    /// point on it is kept exactly.
    fn resolve(s: &Session, p: Vec2, clicked: bool) -> Result<(Handle, ComponentId, Pt), String> {
        if !in_range(p) {
            return Err(OUT_OF_RANGE.into());
        }
        let h = pick(s, p).ok_or(NO_OUTLINE)?;
        let px = pixel(s).unwrap_or(0.0);
        read(s, |proj, page| {
            let (c, prim) = drawn_component(proj, page, h)?;
            let at = match nearest_on(&prim, pt(p)) {
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
                    None => Err(other(super::BAD_NAME)),
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
                    None => read(s, |proj, _| Ok(next_terminal_name(proj.ledger.document(), c))).map_err(other)?,
                };
                commit(s, "addterminal", &json!({"handle": h.hex(), "at": [at.x, at.y], "label": label}))?;
                self.name = None;
                s.echo(format!("Terminal {label} added to component {}.", c.hex()));
                Ok(Step::Continue)
            }
            Input::Enter => Ok(Step::Done),
            Input::Text(t) => Err(not_a_point(&t)),
            _ => Ok(Step::Continue),
        }
    }
}

impl Interactive for AddTerminal {
    fn name(&self) -> &'static str {
        "ADDTERMINAL"
    }
    fn prompt(&self, s: &Session) -> Prompt {
        let shown = self.shown_name(s);
        if self.naming {
            let p = Prompt::new("Enter terminal name", Accept::TEXT);
            return match shown {
                Some(n) => p.default(n),
                None => p,
            };
        }
        let message = match shown {
            Some(n) => format!("Specify point for terminal {n} on a component"),
            // Off any component the number is not known yet: it is the smallest unused one of the
            // component the point lands on.
            None => "Specify point for a terminal on a component".to_string(),
        };
        Prompt::new(message, Accept::POINT).kw(&["Name"])
    }
    fn input(&mut self, s: &mut Session, i: Input) -> R<Step> {
        paper_guard(s, &i)?;
        safely(s, "addterminal", |s| self.step(s, i))
    }
    /// The terminal ring where a click would put the terminal.
    fn preview_objects(&self, s: &Session, cursor: Vec2) -> Vec<Entity> {
        let (Some(px), false) = (pixel(s), self.naming) else { return Vec::new() };
        match Self::resolve(s, cursor, true) {
            Ok((_, _, at)) => vec![preview_entity(s, ring_kind(at, TERMINAL_PX, px), TERMINAL_ACI)],
            Err(_) => Vec::new(),
        }
    }
}

// ---------------------------------------------------------------- Place Symbol (§6)

/// `place`: the built-in coil follows the cursor; Rotate and Mirror before the click; repeat
/// until Enter.
pub(crate) fn place(s: &Session) -> R<Box<dyn Interactive>> {
    can_start(s)?;
    Ok(Box::new(Safe(Place::default())))
}

#[derive(Default)]
struct Place {
    /// Degrees counter-clockwise: 0, 90, 180 or 270.
    rotation: u16,
    mirror: bool,
}

impl Place {
    /// The definition the JSON form places without `definition`: the project's coil, else the
    /// built-in one it would embed.
    fn definition(s: &Session) -> Definition {
        let name = builtin_coil().name;
        read(s, |p, _| Ok(p.ledger.document().definitions.values().find(|d| d.name == name).cloned()))
            .ok()
            .flatten()
            .unwrap_or_else(builtin_coil)
    }

    fn placement(&self, at: Vec2) -> Placement {
        Placement { origin: pt(at), rotation: f64::from(self.rotation).to_radians(), mirror: self.mirror, scale: 1.0 }
    }

    fn step(&mut self, s: &mut Session, i: Input) -> R<Step> {
        match i {
            Input::Keyword(k) if k == "Rotate" => {
                self.rotation = (self.rotation + 90) % 360;
                Ok(Step::Continue)
            }
            Input::Keyword(k) if k == "Mirror" => {
                self.mirror = !self.mirror;
                Ok(Step::Continue)
            }
            Input::Point(p) => {
                if !in_range(p) {
                    return Err(other(OUT_OF_RANGE));
                }
                let r = commit(
                    s,
                    "place",
                    &json!({"at": [p.x, p.y], "rotation": f64::from(self.rotation), "mirror": self.mirror}),
                )?;
                let keys: Vec<&str> = r
                    .get("terminals")
                    .and_then(Value::as_array)
                    .into_iter()
                    .flatten()
                    .filter_map(|t| t.get("key").and_then(Value::as_str))
                    .collect();
                s.echo(format!("Coil placed: component {}, terminals {}.", hex_of(&r, "component"), keys.join(", ")));
                Ok(Step::Continue)
            }
            Input::Enter => Ok(Step::Done),
            Input::Text(t) => Err(not_a_point(&t)),
            _ => Ok(Step::Continue),
        }
    }
}

/// A primitive moved by a placement (the shapes the coil and its kind use).
fn placed(prim: &Primitive, pl: &Placement) -> Option<Primitive> {
    Some(match prim {
        Primitive::Line { a, b } => Primitive::Line { a: pl.apply(*a), b: pl.apply(*b) },
        Primitive::Polyline { vertices, closed } => {
            Primitive::Polyline { vertices: vertices.iter().map(|v| pl.apply(*v)).collect(), closed: *closed }
        }
        Primitive::Circle { center, radius } => Primitive::Circle { center: pl.apply(*center), radius: *radius },
        Primitive::Point { at } => Primitive::Point { at: pl.apply(*at) },
        _ => return None,
    })
}

impl Interactive for Place {
    fn name(&self) -> &'static str {
        "PLACE"
    }
    fn prompt(&self, _s: &Session) -> Prompt {
        let mirrored = if self.mirror { "mirrored" } else { "not mirrored" };
        Prompt::new(format!("Specify insertion point (rotation {}, {mirrored})", self.rotation), Accept::POINT)
            .kw(&["Rotate", "Mirror"])
    }
    fn input(&mut self, s: &mut Session, i: Input) -> R<Step> {
        paper_guard(s, &i)?;
        safely(s, "place", |s| self.step(s, i))
    }
    /// The coil at the cursor, as it would be placed.
    fn preview(&self, s: &Session, cursor: Vec2) -> Vec<EntityKind> {
        if !super::model_space(s) {
            return Vec::new();
        }
        if !in_range(cursor) {
            return Vec::new();
        }
        let pl = self.placement(cursor);
        Self::definition(s)
            .graphics
            .iter()
            .filter_map(|g| placed(g, &pl))
            .map(|g| crate::convert::kind_for_primitive(&g))
            .collect()
    }
    /// Its terminal rings at their would-be positions.
    fn preview_objects(&self, s: &Session, cursor: Vec2) -> Vec<Entity> {
        let Some(px) = pixel(s).filter(|_| in_range(cursor)) else { return Vec::new() };
        let pl = self.placement(cursor);
        Self::definition(s)
            .terminals
            .iter()
            .map(|t| preview_entity(s, ring_kind(pl.apply(t.at), TERMINAL_PX, px), TERMINAL_ACI))
            .collect()
    }
}

// ---------------------------------------------------------------- Wire (§7)

/// `wire`: click points; an end on a terminal attaches; a click on a terminal after the first
/// point (or Enter) commits the wire.
pub(crate) fn wire(s: &Session) -> R<Box<dyn Interactive>> {
    can_start(s)?;
    Ok(Box::new(Safe(Wire::default())))
}

#[derive(Default)]
struct Wire {
    points: Vec<Pt>,
    /// The first point is on a terminal.
    first_on_terminal: bool,
}

impl Wire {
    fn finish(&mut self, s: &mut Session) -> R<Step> {
        let points: Vec<[f64; 2]> = self.points.iter().map(|p| [p.x, p.y]).collect();
        let r = commit(s, "wire", &json!({ "points": points }))?;
        let attached: Vec<(String, String)> = r
            .get("attached")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
            .map(|a| (hex_of(a, "end"), hex_of(a, "terminal")))
            .collect();
        let names = read(s, |p, _| {
            let doc = p.ledger.document();
            Ok(attached
                .iter()
                .map(|(end, t)| {
                    let name = u128::from_str_radix(t, 16).map_or_else(|_| t.clone(), |t| terminal_name(doc, t.into()));
                    (end.clone(), name)
                })
                .collect::<Vec<_>>())
        })
        .unwrap_or_default();
        let outcome: Vec<String> = ["A", "B"]
            .iter()
            .map(|e| match names.iter().find(|(end, _)| end == e) {
                Some((_, name)) => format!("end {e} attached to {name}"),
                None => format!("end {e} not attached"),
            })
            .collect();
        s.echo(format!("Wire {} drawn; {}.", hex_of(&r, "conductor"), outcome.join(", ")));
        Ok(Step::Done)
    }

    fn step(&mut self, s: &mut Session, i: Input) -> R<Step> {
        match i {
            Input::Point(p) => {
                if !in_range(p) {
                    return Err(other(OUT_OF_RANGE));
                }
                let (q, on_terminal) = match terminal_for(s, p) {
                    Some((_, at, _)) => (at, true),
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
                    self.first_on_terminal = on_terminal;
                    return Ok(Step::Continue);
                }
                if on_terminal { self.finish(s) } else { Ok(Step::Continue) }
            }
            Input::Keyword(k) if k == "Undo" => {
                self.points.pop();
                if self.points.is_empty() {
                    self.first_on_terminal = false;
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
    fn prompt(&self, _s: &Session) -> Prompt {
        match self.points.last() {
            None => Prompt::new("Specify first point of wire", Accept::POINT),
            Some(last) => Prompt::new("Specify next point", Accept::POINT).kw(&["Undo"]).base(v2(*last)),
        }
    }
    fn input(&mut self, s: &mut Session, i: Input) -> R<Step> {
        paper_guard(s, &i)?;
        safely(s, "wire", |s| self.step(s, i))
    }
    /// The wire so far, and the rubber band to the cursor (or to the terminal it captures).
    fn preview(&self, s: &Session, cursor: Vec2) -> Vec<EntityKind> {
        if !super::model_space(s) {
            return Vec::new();
        }
        let mut out: Vec<EntityKind> =
            self.points.windows(2).filter_map(|w| Some(line_kind(*w.first()?, *w.get(1)?))).collect();
        if let Some(last) = self.points.last()
            && in_range(cursor)
        {
            let to = capture_at(s, cursor).map_or(pt(cursor), |c| c.at);
            out.push(line_kind(*last, to));
        }
        out
    }
    /// The attach ring and name of a captured terminal; a dot on a first point that attaches.
    fn preview_objects(&self, s: &Session, cursor: Vec2) -> Vec<Entity> {
        let mut out = capture_at(s, cursor).map(|c| capture_preview(s, &c, ATTACHED_ACI)).unwrap_or_default();
        if let (true, Some(first), Some(px)) = (self.first_on_terminal, self.points.first(), pixel(s)) {
            out.extend([4.0, 2.0].map(|d| preview_entity(s, ring_kind(*first, d, px), ATTACHED_ACI)));
        }
        out
    }
}

// ---------------------------------------------------------------- Connect, Disconnect (§8.1, §8.2)

/// `connect`: pick a wire near the end, then a terminal; repeat until Enter.
pub(crate) fn connect(s: &Session) -> R<Box<dyn Interactive>> {
    can_start(s)?;
    Ok(Box::new(Safe(Link { connect: true, end: None })))
}

/// `disconnect`: pick a wire near an attached end; repeat until Enter.
pub(crate) fn disconnect(s: &Session) -> R<Box<dyn Interactive>> {
    can_start(s)?;
    Ok(Box::new(Safe(Link { connect: false, end: None })))
}

const NOT_WIRE: &str = "That is not a wire.";
const NO_TERMINAL: &str = "No terminal there. Move onto a terminal until the green ring shows.";

struct Link {
    connect: bool,
    /// Connect: the wire end chosen, waiting for its terminal.
    end: Option<(ConductorId, End, Pt)>,
}

/// A picked wire's end nearer to `p` (A on a tie), its position and whether it is attached.
fn wire_end(s: &Session, p: Vec2) -> Result<(ConductorId, End, Pt, bool), String> {
    let h = pick(s, p).ok_or(NOT_WIRE)?;
    read(s, |proj, page| {
        let doc = proj.ledger.document();
        let map = proj.pages.get(&page).map(|v| &v.map);
        let i = match map.and_then(|m| m.get(h)) {
            Some(Mapped::Item(i)) => i,
            Some(Mapped::Fan(k)) => {
                return Err(map.map_or(crate::views::FAN_REFUSED, |m| crate::views::refusal(m, k)).into());
            }
            _ => return Err(NOT_WIRE.into()),
        };
        let Some((_, rep)) = doc.representation_of_item(i) else { return Err(NOT_WIRE.into()) };
        let Subject::Conductor(w) = rep.subject else { return Err(NOT_WIRE.into()) };
        let (_, route) = ecad_model::derive::route_on_page(doc, w, page).ok_or(NOT_WIRE)?;
        let (a, b) = route.path_ends().ok_or(NOT_WIRE)?;
        let q = pt(p);
        let (end, at) = if q.dist(a) <= q.dist(b) { (End::A, a) } else { (End::B, b) };
        let attached = doc.conductors.get(&w).and_then(|c| c.end(end)).is_some();
        Ok((w, end, at, attached))
    })
}

impl Link {
    fn step(&mut self, s: &mut Session, i: Input) -> R<Step> {
        match (i, self.end) {
            (Input::Point(p), _) if !in_range(p) => Err(other(OUT_OF_RANGE)),
            (Input::Point(p), None) => {
                let (w, end, at, _) = wire_end(s, p).map_err(other)?;
                if self.connect {
                    self.end = Some((w, end, at));
                    return Ok(Step::Continue);
                }
                commit(s, "disconnect", &json!({"conductor": w.hex(), "end": end_name(end)}))?;
                s.echo(format!("End {} of wire {} disconnected.", end_name(end), w.hex()));
                Ok(Step::Continue)
            }
            (Input::Point(p), Some((w, end, _))) => {
                let (t, _, name) = terminal_for(s, p).ok_or_else(|| other(NO_TERMINAL))?;
                commit(s, "connect", &json!({"conductor": w.hex(), "end": end_name(end), "terminal": t.hex()}))?;
                s.echo(format!("End {} of wire {} connected to {name}.", end_name(end), w.hex()));
                self.end = None;
                Ok(Step::Continue)
            }
            (Input::Enter, Some(_)) => {
                self.end = None;
                Ok(Step::Continue)
            }
            (Input::Enter, None) => Ok(Step::Done),
            (Input::Text(t), _) => Err(not_a_point(&t)),
            _ => Ok(Step::Continue),
        }
    }
}

impl Interactive for Link {
    fn name(&self) -> &'static str {
        if self.connect { "CONNECT" } else { "DISCONNECT" }
    }
    fn prompt(&self, _s: &Session) -> Prompt {
        match (self.end, self.connect) {
            (None, true) => Prompt::new("Select wire near the end to connect", Accept::POINT),
            (None, false) => Prompt::new("Select wire near the end to disconnect", Accept::POINT),
            (Some((_, end, at)), _) => {
                Prompt::new(format!("Select terminal for end {}", end_name(end)), Accept::POINT).base(v2(at))
            }
        }
    }
    fn input(&mut self, s: &mut Session, i: Input) -> R<Step> {
        paper_guard(s, &i)?;
        let name = if self.connect { "connect" } else { "disconnect" };
        safely(s, name, |s| self.step(s, i))
    }
    /// Connect: the rubber band from the chosen end to the cursor or the captured terminal.
    fn preview(&self, s: &Session, cursor: Vec2) -> Vec<EntityKind> {
        if !super::model_space(s) {
            return Vec::new();
        }
        match self.end {
            Some((_, _, at)) if in_range(cursor) => {
                vec![line_kind(at, capture_at(s, cursor).map_or(pt(cursor), |c| c.at))]
            }
            _ => Vec::new(),
        }
    }
    /// The end a click would choose, or the terminal a click would connect to.
    fn preview_objects(&self, s: &Session, cursor: Vec2) -> Vec<Entity> {
        if self.end.is_some() {
            return capture_at(s, cursor).map(|c| capture_preview(s, &c, ATTACHED_ACI)).unwrap_or_default();
        }
        let (Some(px), Ok((_, _, at, attached))) = (pixel(s), wire_end(s, cursor)) else { return Vec::new() };
        let aci = match (self.connect, attached) {
            (true, _) => ATTACHED_ACI,
            (false, true) => LOOSE_ACI,
            (false, false) => return Vec::new(),
        };
        [RING_PX - 1.0, RING_PX, RING_PX + 1.0]
            .into_iter()
            .map(|d| preview_entity(s, ring_kind(at, d, px), aci))
            .collect()
    }
}

// ---------------------------------------------------------------- Convert to Component (§8.3)

/// `convertcomponent`: the current selection at once, else "Select objects", then one
/// conversion of what was selected.
pub(crate) fn convert(s: &Session) -> R<Box<dyn Interactive>> {
    can_start(s)?;
    Ok(Box::new(Safe(Convert::default())))
}

#[derive(Default)]
struct Convert {
    picked: Vec<Handle>,
    members: BTreeSet<Handle>,
    removing: bool,
    /// What each pick changed, for Undo: (handles, whether they were removed).
    history: Vec<(Vec<Handle>, bool)>,
}

/// Most picks the selection's Undo remembers.
const MAX_SELECT_HISTORY: usize = 1000;
/// Most picked objects highlighted in the preview.
const MAX_HIGHLIGHT: usize = 2000;

impl Convert {
    fn run(s: &mut Session, hs: &[Handle]) -> R<Step> {
        if hs.len() > MAX_HANDLES {
            s.echo(format!("At most {MAX_HANDLES} objects can be converted at once."));
            return Ok(Step::Done);
        }
        let handles: Vec<String> = hs.iter().map(|h| h.hex()).collect();
        match commit(s, "convertcomponent", &json!({ "handles": handles })) {
            Ok(r) => s.echo(format!("Component {} created from {} objects.", hex_of(&r, "component"), hs.len())),
            // Refused: the selection itself is wrong, so the command ends (no retry loop).
            Err(e) => s.echo(e.to_string()),
        }
        Ok(Step::Done)
    }

    fn set(&mut self, hs: &[Handle], remove: bool) {
        if remove {
            self.picked.retain(|h| !hs.contains(h));
            for h in hs {
                self.members.remove(h);
            }
        } else {
            for h in hs {
                if self.members.insert(*h) {
                    self.picked.push(*h);
                }
            }
        }
    }

    fn step(&mut self, s: &mut Session, i: Input) -> R<Step> {
        match i {
            Input::Pick(hs) => {
                let changed: Vec<Handle> = if self.removing {
                    hs.iter().copied().filter(|h| self.members.contains(h)).collect()
                } else {
                    hs.iter()
                        .copied()
                        .filter(|h| !self.members.contains(h))
                        .collect::<BTreeSet<_>>()
                        .into_iter()
                        .collect()
                };
                if !self.removing && self.picked.len().saturating_add(changed.len()) > MAX_HANDLES {
                    return Err(other(format!("At most {MAX_HANDLES} objects can be converted at once.")));
                }
                self.set(&changed, self.removing);
                if !changed.is_empty() {
                    if self.history.len() >= MAX_SELECT_HISTORY {
                        self.history.remove(0);
                    }
                    self.history.push((changed, self.removing));
                }
                s.echo(format!("{} found, {} total", hs.len(), self.picked.len()));
                Ok(Step::Continue)
            }
            Input::Text(t) | Input::Keyword(t) => {
                match t.trim().to_ascii_lowercase().as_str() {
                    "r" | "remove" => self.removing = true,
                    "a" | "add" => self.removing = false,
                    "u" | "undo" => match self.history.pop() {
                        Some((hs, removed)) => {
                            self.set(&hs, !removed);
                            s.echo(format!("{} total", self.picked.len()));
                        }
                        None => s.echo("Nothing to undo."),
                    },
                    _ => s.echo("*Invalid selection*"),
                }
                Ok(Step::Continue)
            }
            Input::Enter if self.picked.is_empty() => {
                s.echo("Nothing selected; nothing converted.");
                Ok(Step::Done)
            }
            Input::Enter => {
                let hs = std::mem::take(&mut self.picked);
                Self::run(s, &hs)
            }
            _ => Ok(Step::Continue),
        }
    }
}

impl Interactive for Convert {
    fn name(&self) -> &'static str {
        "CONVERTCOMPONENT"
    }
    /// With objects selected it converts them at once, as the JSON form converts the selection.
    fn begin(&mut self, s: &mut Session) -> R<Step> {
        safely(s, "convertcomponent", |s| {
            let sel = s.selection();
            if sel.is_empty() { Ok(Step::Continue) } else { Self::run(s, &sel) }
        })
    }
    fn prompt(&self, _s: &Session) -> Prompt {
        Prompt::new(
            if self.removing { "Remove objects" } else { "Select objects to convert to a component" },
            Accept::SELECT,
        )
    }
    fn input(&mut self, s: &mut Session, i: Input) -> R<Step> {
        paper_guard(s, &i)?;
        safely(s, "convertcomponent", |s| self.step(s, i))
    }
    /// The objects picked so far, highlighted (the selection itself is left alone, so Esc
    /// leaves it as it was).
    fn preview_objects(&self, s: &Session, _cursor: Vec2) -> Vec<Entity> {
        let Ok(d) = s.doc() else { return Vec::new() };
        self.picked
            .iter()
            .take(MAX_HIGHLIGHT)
            .filter_map(|h| d.entity(*h))
            .map(|e| {
                let mut e = (**e).clone();
                e.common.color = CColor::True(Rgb(0x2f, 0x6b, 0xff));
                e
            })
            .collect()
    }
}

// ---------------------------------------------------------------- Cable Slash (design 004 §5)

/// `cableslash`: two clicks across wires make one cable of the wires crossed.
pub(crate) fn cableslash(s: &Session) -> R<Box<dyn Interactive>> {
    can_start(s)?;
    Ok(Box::new(Safe(CableSlash::default())))
}

#[derive(Default)]
struct CableSlash {
    first: Option<Pt>,
}

/// Rings drawn by the crossing preview at most (the text still counts every crossing).
const MAX_PREVIEW_RINGS: usize = 200;

/// What the crossing preview shows for a slash: ring positions (will join, blocks) and the text
/// line, with whether the slash would be refused.
struct SlashPreview {
    join: Vec<Pt>,
    block: Vec<Pt>,
    text: String,
    refused: bool,
}

impl CableSlash {
    fn tolerance(s: &Session) -> f64 {
        read(s, |p, _| Ok(p.tolerance)).unwrap_or(1e-6)
    }

    /// The preview for `from → to`, from the same crossing function the JSON form uses. `None`
    /// when the project cannot be read now (busy) or the slash has no length.
    fn look(s: &Session, from: Pt, to: Pt) -> Option<SlashPreview> {
        read(s, |p, page| {
            if from.near(to, p.tolerance) {
                return Ok(None);
            }
            let doc = p.ledger.document();
            let found = slash::crossings(doc, page, from, to, p.tolerance);
            let blocked = slash::blocked(doc, &found.members);
            let blocking: BTreeSet<ConductorId> = blocked.iter().map(|(m, _)| m.conductor).collect();
            let (n, others) = (found.members.len(), found.ignored.len());
            let (text, refused) = if n > slash::MAX_CABLE_MEMBERS {
                (format!("-> refused: more than {} wires", slash::MAX_CABLE_MEMBERS), true)
            } else if !blocked.is_empty() {
                (format!("-> refused: {} already in a cable", slash::wires(blocked.len())), true)
            } else if n == 0 {
                // Refused too (design 004 §5.2: red whenever the slash would be refused).
                ("-> no wires".to_string(), true)
            } else if others > 0 {
                let objects =
                    if others == 1 { "1 other object".to_string() } else { format!("{others} other objects") };
                (format!("-> {}, {objects} ignored", slash::wires(n)), false)
            } else {
                (format!("-> {}", slash::wires(n)), false)
            };
            let (mut join, mut block) = (Vec::new(), Vec::new());
            for m in found.members.iter().take(MAX_PREVIEW_RINGS) {
                if blocking.contains(&m.conductor) {
                    block.push(m.at);
                } else {
                    join.push(m.at);
                }
            }
            Ok(Some(SlashPreview { join, block, text, refused }))
        })
        .ok()
        .flatten()
    }

    fn finish(s: &mut Session, from: Pt, to: Pt) -> R<Step> {
        let r = commit(s, "cableslash", &json!({"from": [from.x, from.y], "to": [to.x, to.y]}))?;
        let ids = |key: &str| -> Vec<u128> {
            r.get(key)
                .and_then(Value::as_array)
                .into_iter()
                .flatten()
                .filter_map(|v| v.as_str().and_then(|t| u128::from_str_radix(t, 16).ok()))
                .collect()
        };
        let (members, ignored, touched) = (ids("conductors"), ids("ignored").len(), ids("touched").len());
        let cable = u128::from_str_radix(&hex_of(&r, "cable"), 16).unwrap_or_default();
        let lines = read(s, |p, _| {
            let members: Vec<ConductorId> = members.iter().map(|c| ConductorId::from(*c)).collect();
            Ok(slash::created_lines(p.ledger.document(), cable.into(), &members, ignored, touched))
        })
        .unwrap_or_else(|_| vec![format!("Cable {} created.", hex_of(&r, "cable"))]);
        for l in lines {
            s.echo(l);
        }
        Ok(Step::Done)
    }

    fn step(&mut self, s: &mut Session, i: Input) -> R<Step> {
        match i {
            Input::Point(p) => {
                if !in_range(p) {
                    return Err(other(OUT_OF_RANGE));
                }
                let q = pt(p);
                match self.first {
                    None => {
                        self.first = Some(q);
                        Ok(Step::Continue)
                    }
                    // The same point again is ignored (as Wire ignores a repeated point).
                    Some(a) if a.near(q, Self::tolerance(s)) => Ok(Step::Continue),
                    // A refusal keeps the first point: the prompt of step 2 repeats.
                    Some(a) => Self::finish(s, a, q),
                }
            }
            Input::Keyword(k) if k == "Undo" => {
                self.first = None;
                Ok(Step::Continue)
            }
            // A slash needs two clicks: Enter never commits.
            Input::Enter => Ok(Step::Done),
            Input::Text(t) => Err(not_a_point(&t)),
            _ => Ok(Step::Continue),
        }
    }
}

impl Interactive for CableSlash {
    fn name(&self) -> &'static str {
        "CABLESLASH"
    }
    fn prompt(&self, _s: &Session) -> Prompt {
        match self.first {
            None => Prompt::new("Specify first point of cable slash", Accept::POINT),
            Some(a) => Prompt::new("Specify second point of cable slash", Accept::POINT).kw(&["Undo"]).base(v2(a)),
        }
    }
    fn input(&mut self, s: &mut Session, i: Input) -> R<Step> {
        paper_guard(s, &i)?;
        safely(s, "cableslash", |s| self.step(s, i))
    }
    /// The rubber band from the first point to the cursor.
    fn preview(&self, s: &Session, cursor: Vec2) -> Vec<EntityKind> {
        match self.first {
            Some(a) if super::model_space(s) && in_range(cursor) => vec![line_kind(a, pt(cursor))],
            _ => Vec::new(),
        }
    }
    /// The crossing preview (design 004 §5.2): a green ring on each wire that will join, a red one
    /// on each wire that blocks the slash, and one line of text beside the cursor.
    fn preview_objects(&self, s: &Session, cursor: Vec2) -> Vec<Entity> {
        let (Some(a), Some(px)) = (self.first, pixel(s)) else { return Vec::new() };
        if !in_range(cursor) {
            return Vec::new();
        }
        let Some(look) = Self::look(s, a, pt(cursor)) else { return Vec::new() };
        let ring = |at: &Pt, aci: u8| {
            [RING_PX - 1.0, RING_PX, RING_PX + 1.0].map(|d| preview_entity(s, ring_kind(*at, d, px), aci))
        };
        let mut out: Vec<Entity> = look.join.iter().flat_map(|at| ring(at, ATTACHED_ACI)).collect();
        out.extend(look.block.iter().flat_map(|at| ring(at, REFUSE_ACI)));
        let at = Pt::new(cursor.x + 10.0 * px, cursor.y + 10.0 * px);
        out.push(preview_text(s, at, px, look.text, if look.refused { REFUSE_ACI } else { ATTACHED_ACI }));
        out
    }
}

// ---------------------------------------------------------------- Cable View (design 005 §3)

/// `cableview`: switch the selected cables at once, else click cables one by one; presentation
/// only, so Esc has nothing to drop.
pub(crate) fn cableview(s: &Session) -> R<Box<dyn Interactive>> {
    can_start(s)?;
    Ok(Box::new(Safe(CableView)))
}

struct CableView;

const NOTHING_THERE: &str = "Nothing there. Click a cable slash, a fan or a wire in a cable.";
const WIRE_NOT_IN_CABLE: &str = "That wire is not in a cable.";
const NOT_A_CABLE: &str = "That is not a cable or a wire in a cable.";

/// The cable a mapped object names (design 005 §3.3): a slash, a fan line, or a wire in a cable.
fn cable_named(doc: &ecad_model::Document, m: Mapped) -> Result<CableId, &'static str> {
    match m {
        Mapped::Fan(k) => Ok(k),
        Mapped::Item(i) => match doc.representation_of_item(i).map(|(_, r)| r.subject) {
            Some(Subject::Cable(k)) => Ok(k),
            Some(Subject::Conductor(w)) => doc.cables_of(w).first().copied().ok_or(WIRE_NOT_IN_CABLE),
            _ => Err(NOT_A_CABLE),
        },
        Mapped::Marker(..) => Err(NOT_A_CABLE),
    }
}

impl CableView {
    /// Switch `cable` to `view` through the JSON form and say so (with why it was not remembered).
    fn switch(s: &mut Session, cable: CableId, view: Option<View>) -> R<String> {
        let mut params = json!({"cable": cable.hex()});
        if let Some(v) = view {
            params["view"] = Value::String(v.name().into());
        }
        let r = commit(s, "cableview", &params)?;
        let shown = r.get("view").and_then(Value::as_str).and_then(View::parse).unwrap_or(View::Traditional);
        let mut line = read(s, |p, _| Ok(crate::views::switched(p.ledger.document(), cable, shown)))
            .unwrap_or_else(|_| format!("Cable {}… now shown in {} view.", short_hex(cable), shown.name()));
        if let Some(reason) = r.get("reason").and_then(Value::as_str) {
            line.pop();
            line.push_str(&format!(" (not remembered: {reason})."));
        }
        if let Some(notice) = r.get("notice").and_then(Value::as_str) {
            s.echo(notice);
        }
        Ok(line)
    }

    /// Step 0: the cables the selection names, switched together (all fan → traditional,
    /// otherwise all → fan; owner answer O6). `None` when the selection names no cable.
    fn selection(s: &mut Session) -> R<Option<Step>> {
        let sel = s.selection();
        let found = read(s, |p, page| {
            let Some(map) = p.pages.get(&page).map(|v| &v.map) else { return Ok(Vec::new()) };
            let doc = p.ledger.document();
            let mut seen = BTreeSet::new();
            let mut out = Vec::new();
            for h in sel.iter().take(MAX_HANDLES) {
                if let Some(k) = map.get(*h).and_then(|m| cable_named(doc, m).ok())
                    && seen.insert(k)
                {
                    let one_wire = !crate::views::offers_fan(doc, k);
                    out.push((k, p.view_of(k), one_wire, crate::views::cable_title(doc, k)));
                }
            }
            Ok(out)
        })
        .map_err(other)?;
        if found.is_empty() {
            return Ok(None);
        }
        let target = if found.iter().all(|(_, v, _, _)| *v == View::Fan) { View::Traditional } else { View::Fan };
        // Each cable's own result; one that fails never stops the others (review M5).
        let (mut switched, mut other_lines) = (Vec::new(), Vec::new());
        for (k, current, one_wire, title) in found {
            if target == View::Fan && current != View::Fan && one_wire {
                other_lines.push(format!("Cable {title} {}", crate::views::ONE_WIRE));
                continue;
            }
            match Self::switch(s, k, Some(target)) {
                Ok(line) => switched.push(line),
                Err(e) => other_lines.push(format!("Cable {title} was not switched: {e}")),
            }
        }
        let shown = switched.len().min(slash::MAX_NAMES);
        for l in switched.iter().take(shown) {
            s.echo(l.clone());
        }
        if switched.len() > shown {
            s.echo(format!("and {} more cables switched.", switched.len() - shown));
        }
        let shown = other_lines.len().min(slash::MAX_NAMES);
        for l in other_lines.iter().take(shown) {
            s.echo(l.clone());
        }
        if other_lines.len() > shown {
            s.echo(format!("and {} more cables not switched.", other_lines.len() - shown));
        }
        // The selected objects may no longer be drawn.
        s.set_selection(Vec::new());
        Ok(Some(Step::Done))
    }

    fn step(&mut self, s: &mut Session, i: Input) -> R<Step> {
        match i {
            Input::Point(p) => {
                if !in_range(p) {
                    return Err(other(OUT_OF_RANGE));
                }
                let h = pick(s, p).ok_or_else(|| other(NOTHING_THERE))?;
                let (k, current, one_wire, title) = read(s, |proj, page| {
                    let m = proj.pages.get(&page).and_then(|v| v.map.get(h)).ok_or(NOT_A_CABLE)?;
                    let doc = proj.ledger.document();
                    let k = cable_named(doc, m)?;
                    Ok((k, proj.view_of(k), !crate::views::offers_fan(doc, k), crate::views::cable_title(doc, k)))
                })
                .map_err(other)?;
                if current == View::Traditional && one_wire {
                    return Err(other(format!("Cable {title} {}", crate::views::ONE_WIRE)));
                }
                let line = Self::switch(s, k, None)?;
                s.echo(line);
                Ok(Step::Continue)
            }
            Input::Enter => Ok(Step::Done),
            Input::Text(t) => Err(not_a_point(&t)),
            _ => Ok(Step::Continue),
        }
    }
}

// ---------------------------------------------------------------- Core Label (design 009 §5.1)

/// `cablecore`: click a wire in a cable, type its core number or colour; repeat until Enter.
pub(crate) fn cablecore(s: &Session) -> R<Box<dyn Interactive>> {
    can_start(s)?;
    Ok(Box::new(Safe(CableCore::default())))
}

/// The wire picked and its current label, while the text prompt is shown.
#[derive(Default)]
struct CableCore {
    picked: Option<(ConductorId, String, Option<String>)>,
}

const CORE_NOTHING: &str = "Nothing there. Click a wire in a cable.";
const CORE_IS_CABLE: &str = "That is the cable; click one of its wires.";
const CORE_NOT_WIRE: &str = "That is not a wire in a cable.";

impl CableCore {
    /// The member a pick names (design 009 §5.1 step 2): a route, a fan tail or bend, or a core
    /// label; anything else is refused in words.
    fn member(s: &Session, h: Handle) -> R<(ConductorId, String, Option<String>)> {
        read(s, |p, page| {
            let map = &p.pages.get(&page).ok_or(CORE_NOT_WIRE)?.map;
            let doc = p.ledger.document();
            let w = match map.get(h) {
                Some(Mapped::Fan(_)) => map.core_of(h).ok_or(CORE_IS_CABLE)?,
                Some(Mapped::Item(i)) => match doc.representation_of_item(i).map(|(_, r)| r.subject) {
                    Some(Subject::Conductor(w)) => w,
                    Some(Subject::Cable(_)) => return Err(CORE_IS_CABLE.into()),
                    _ => return Err(CORE_NOT_WIRE.into()),
                },
                _ => return Err(CORE_NOT_WIRE.into()),
            };
            let k = doc.cables_of(w).first().copied().ok_or(WIRE_NOT_IN_CABLE)?;
            let core = doc
                .cables
                .get(&k)
                .and_then(|c| c.members.iter().find(|m| m.conductor == w))
                .and_then(|m| m.core.clone());
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
                let clear = t.trim() == ".";
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
                    Ok(doc.cables_of(w).first().map(|k| crate::views::cable_title(doc, *k)).unwrap_or_default())
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
                    .filter_map(|v| v.as_str().and_then(ConductorId::parse))
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

fn short_hex(k: CableId) -> String {
    k.hex().get(..8).unwrap_or_default().to_string()
}

impl Interactive for CableView {
    fn name(&self) -> &'static str {
        "CABLEVIEW"
    }
    /// With cable objects selected they switch at once and the command ends.
    fn begin(&mut self, s: &mut Session) -> R<Step> {
        safely(s, "cableview", |s| Ok(Self::selection(s)?.unwrap_or(Step::Continue)))
    }
    fn prompt(&self, _s: &Session) -> Prompt {
        Prompt::new("Select a cable to switch its view", Accept::POINT)
    }
    fn input(&mut self, s: &mut Session, i: Input) -> R<Step> {
        paper_guard(s, &i)?;
        safely(s, "cableview", |s| self.step(s, i))
    }
}
