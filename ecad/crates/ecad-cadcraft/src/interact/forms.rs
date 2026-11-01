//! The interactive forms (design 008 §5 to §8). Each one resolves values from clicks, keys and
//! typed points, then commits the command's JSON form with them.

use std::collections::BTreeSet;
use std::panic::{AssertUnwindSafe, catch_unwind};

use cadcraft_engine::doc::color::{Color as CColor, Rgb};
use cadcraft_engine::doc::{Entity, EntityKind, Handle};
use cadcraft_engine::geom::Vec2;
use cadcraft_engine::{Accept, EngineError, Input, Interactive, Prompt, Session, Step};
use ecad_model::{ComponentId, ConductorId, Definition, End, Placement, Primitive, Pt, Subject};
use serde_json::{Value, json};

use super::{
    ATTACHED_ACI, LOOSE_ACI, NO_OUTLINE, OUT_OF_RANGE, RING_PX, TERMINAL_ACI, TERMINAL_PX, can_start, capture_at,
    capture_preview, drawn_component, from_cursor, in_range, line_kind, nearest_on, next_terminal_name, pick, pixel,
    preview_entity, pt, read, ring_kind, terminal_for, terminal_name, v2, valid_name,
};
use crate::commands::{MAX_HANDLES, MAX_WIRE_POINTS, builtin_coil, with_project};
use crate::project::Mapped;

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

/// Commit the command's JSON form with resolved values: the same path as `name {json}` typed at
/// `Command:` (start check, transaction, refresh, commit gate).
fn commit(s: &mut Session, id: &str, params: &Value) -> R<Value> {
    s.execute(id, params).map_err(|e| other(e.to_string()))
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
    Ok(Box::new(AddTerminal::default()))
}

#[derive(Default)]
struct AddTerminal {
    /// A name typed with the Name option, for the next terminal only.
    name: Option<String>,
    /// At the "Enter terminal name" prompt.
    naming: bool,
    /// The component the last terminal of this run went to (the prompt shows its next name).
    last: Option<ComponentId>,
}

impl AddTerminal {
    fn shown_name(&self, s: &Session) -> String {
        if let Some(n) = &self.name {
            return n.clone();
        }
        self.last
            .and_then(|c| read(s, |p, _| Ok(next_terminal_name(p.ledger.document(), c))).ok())
            .unwrap_or_else(|| "1".into())
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
                self.last = Some(c);
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
        if self.naming {
            return Prompt::new("Enter terminal name", Accept::TEXT).default(self.shown_name(s));
        }
        Prompt::new(format!("Specify point for terminal {} on a component", self.shown_name(s)), Accept::POINT)
            .kw(&["Name"])
    }
    fn input(&mut self, s: &mut Session, i: Input) -> R<Step> {
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
    Ok(Box::new(Place::default()))
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
        safely(s, "place", |s| self.step(s, i))
    }
    /// The coil at the cursor, as it would be placed.
    fn preview(&self, s: &Session, cursor: Vec2) -> Vec<EntityKind> {
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
    Ok(Box::new(Wire::default()))
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
        safely(s, "wire", |s| self.step(s, i))
    }
    /// The wire so far, and the rubber band to the cursor (or to the terminal it captures).
    fn preview(&self, s: &Session, cursor: Vec2) -> Vec<EntityKind> {
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
    Ok(Box::new(Link { connect: true, end: None }))
}

/// `disconnect`: pick a wire near an attached end; repeat until Enter.
pub(crate) fn disconnect(s: &Session) -> R<Box<dyn Interactive>> {
    can_start(s)?;
    Ok(Box::new(Link { connect: false, end: None }))
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
        let Some(Mapped::Item(i)) = proj.pages.get(&page).and_then(|v| v.map.get(h)) else {
            return Err(NOT_WIRE.into());
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
        let name = if self.connect { "connect" } else { "disconnect" };
        safely(s, name, |s| self.step(s, i))
    }
    /// Connect: the rubber band from the chosen end to the cursor or the captured terminal.
    fn preview(&self, s: &Session, cursor: Vec2) -> Vec<EntityKind> {
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
    Ok(Box::new(Convert::default()))
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
