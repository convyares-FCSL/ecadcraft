//! The box commands: `locationbox` and
//! `functionbox` draw a box carrying a location or a function, and propose its values for the
//! devices inside. The box's rules and the JSON it answers sit here; the commands are generic
//! ([`crate::commands::run`]). Excludes the click-based form, which is the engine's.

use std::collections::BTreeSet;

use ecad_model::{Document, IdAllocator, ItemId, Op, PageId, Pt};
use schemars::JsonSchema;
use serde::Deserialize;
use serde_json::{Value, json};

use crate::commands::input::{Bounded, Point};
use crate::commands::run::{Cx, Generic, Outcome, Slot, State};
use crate::error::CmdError;
use crate::locations;
use crate::locations::designations::{Aspect, Pending};
use crate::symbols::proposals::{self, Mode};

/// A rectangle's four corners from two opposite ones.
pub fn rectangle(a: Pt, b: Pt) -> Vec<Pt> {
    vec![
        Pt::new(a.x.min(b.x), a.y.min(b.y)),
        Pt::new(a.x.max(b.x), a.y.min(b.y)),
        Pt::new(a.x.max(b.x), a.y.max(b.y)),
        Pt::new(a.x.min(b.x), a.y.max(b.y)),
    ]
}

/// A box's outline as given.
#[derive(Debug, Clone, PartialEq)]
pub enum Shape {
    /// Two opposite corners of a rectangle.
    Corners(Pt, Pt),
    /// A closed outline.
    Points(Vec<Pt>),
    /// A closed polyline of the page.
    Item(ItemId),
}

impl Shape {
    /// The one outline given.
    ///
    /// # Errors
    /// None or more than one given.
    pub fn one_of(corners: Option<(Pt, Pt)>, points: Option<Vec<Pt>>, item: Option<ItemId>) -> Result<Shape, CmdError> {
        match (corners, points, item) {
            (Some((a, b)), None, None) => Ok(Shape::Corners(a, b)),
            (None, Some(v), None) => Ok(Shape::Points(v)),
            (None, None, Some(i)) => Ok(Shape::Item(i)),
            _ => Err(CmdError::input("give exactly one of `corners` (two corners), `points` (a closed outline) or `item` (a closed polyline)")),
        }
    }
}

/// What a box command records and says.
#[derive(Debug)]
pub(crate) struct BoxPlan {
    /// The box's ops, then the proposal's.
    pub ops: Vec<Op>,
    /// The box.
    pub planned: locations::Planned,
    /// What it proposes for the device symbols inside.
    pub proposed: proposals::Planned,
    /// "Function box +F1 =CP1" and so on: the transaction's label.
    pub label: String,
    /// How many devices are inside.
    pub inside: usize,
    /// How many of them keep another value.
    pub keeps: usize,
    /// The echo, in words.
    pub message: String,
}

/// A box on `page` carrying `function` and `location`
/// (at least one), drawn as `shape`, and the proposal it makes for the device symbols inside.
/// Unset devices take its values in the same transaction; devices with another value are listed as
/// conflicts (`mode`: propose, move or keep).
///
/// # Errors
/// No such page or item, an outline that is not a box, a value that is not a valid code, or
/// neither value given.
pub(crate) fn plan_box(
    doc: &ecad_model::Document, ids: &mut IdAllocator, page: PageId, function: Option<&str>, location: Option<&str>, shape: Shape, mode: &Mode,
) -> Result<BoxPlan, CmdError> {
    let mut values: Vec<(Aspect, &str)> = Vec::new();
    if let Some(f) = function {
        values.push((Aspect::Function, f));
    }
    if let Some(l) = location {
        values.push((Aspect::Location, l));
    }
    let planned = match shape {
        Shape::Corners(a, b) => locations::plan_box_with(doc, ids, page, &values, rectangle(a, b))?,
        Shape::Points(v) => locations::plan_box_with(doc, ids, page, &values, v)?,
        Shape::Item(i) => locations::plan_from_item_with(doc, ids, i, &values)?,
    };
    let b = locations::planned_box(doc, &planned).ok_or(CmdError::refused("the box could not be read back"))?;
    let shown = locations::label_of(&b).0;
    // The box proposes its values for the device symbols inside it.
    let spots = proposals::spots_inside(doc, &b);
    let modes = [if b.function_key.is_some() { mode.clone() } else { Mode::Keep }, if b.location_key.is_some() { mode.clone() } else { Mode::Keep }];
    let mut pending = Pending::default();
    let proposed = proposals::plan(doc, &planned.ops, ids, &spots, &modes, std::slice::from_ref(&b), &mut pending)?;
    let inside: BTreeSet<_> = spots.iter().map(|s| s.device).collect();
    let mut ops = planned.ops.clone();
    ops.extend(proposed.ops.iter().cloned());
    let kind = if b.function_key.is_some() { "Function box" } else { "Location box" };
    let set: BTreeSet<_> = proposed.set.iter().map(|(c, _, _)| *c).collect();
    let keeps: BTreeSet<_> = proposed.conflicts.iter().map(|c| c.component).collect();
    let n = inside.len();
    let devices = if n == 1 { "1 device".to_string() } else { format!("{n} devices") };
    let mut message = format!("{kind} {shown}: {devices} inside");
    if !set.is_empty() || !keeps.is_empty() {
        let same = n.saturating_sub(set.len() + keeps.len());
        message.push_str(&format!("; {} set to {shown}, {same} already {shown}, {} kept", set.len(), keeps.len()));
    }
    message.push('.');
    if let Some(a) = &planned.added {
        message = format!("{message} {a}");
    }
    Ok(BoxPlan { ops, label: format!("{kind} {shown}"), planned, proposed, inside: n, keeps: keeps.len(), message })
}

/// The JSON a box answers: the box, the proposal it made for the
/// device symbols inside, and that proposal's conflicts.
pub(crate) fn box_json(doc: &Document, plan: &BoxPlan) -> Value {
    let (planned, proposed) = (&plan.planned, &plan.proposed);
    let mut v = json!({
        "aid": planned.aid.hex(), "representation": planned.rep.hex(), "item": planned.item.hex(),
        "inside": plan.inside, "keeps_own": plan.keeps, "message": plan.message,
    });
    if let (Some(o), Some(x)) = (v.as_object_mut(), proposals::json(doc, proposed).as_object()) {
        o.extend(x.clone());
    }
    let conflicts: Vec<Value> = proposed
        .conflicts
        .iter()
        .map(|c| json!({"component": c.component.hex(), "tag": crate::devices::tags::tag_words(doc, c.component), "aspect": c.aspect.word(), "from": c.from, "proposed": c.to}))
        .collect();
    v["conflicts"] = json!(conflicts);
    v
}

// ---------------------------------------------------------------------------------------------
// The commands

/// A location box: an outline carrying a location (and a function, when given). Give exactly one of `corners`,
/// `points` and `item`. It proposes its values for the devices inside. One transaction.
#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct LocationboxParams {
    /// The box's location: "+CP1".
    location: String,
    /// The box's function: "=F1".
    #[serde(default)]
    function: Option<String>,
    /// Two opposite corners [[x, y], [x, y]].
    #[serde(default)]
    corners: Option<Bounded<Point, 2, 2>>,
    /// A closed outline [[x, y], …].
    #[serde(default)]
    points: Option<Bounded<Point, 0, { crate::locations::MAX_VERTICES }>>,
    /// A closed polyline to take the outline from.
    #[serde(default)]
    item: Option<ItemId>,
    /// The page (default: the active page).
    #[serde(default)]
    page: Option<PageId>,
    /// What the box does to the devices inside: "propose" (default), "move" or "keep".
    #[serde(default)]
    proposal: Option<String>,
}

impl Generic for LocationboxParams {
    const ID: &'static str = "locationbox";
    const TITLE: &'static str = "Location Box";
    const STATE: State = State::Conflicts;

    fn run(self, cx: &mut Cx<'_>) -> Result<Outcome, CmdError> {
        let b = BoxInput { function: self.function, location: Some(self.location), corners: self.corners, points: self.points, item: self.item };
        b.run(cx, self.page, self.proposal)
    }
}

/// A function box: an outline carrying a function (and a location, when given), drawn dash-dot. Give exactly one
/// of `corners`, `points` and `item`. It proposes its values for the devices inside. One transaction.
#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct FunctionboxParams {
    /// The box's function: "=F1".
    function: String,
    /// The box's location: "+CP1".
    #[serde(default)]
    location: Option<String>,
    /// Two opposite corners [[x, y], [x, y]].
    #[serde(default)]
    corners: Option<Bounded<Point, 2, 2>>,
    /// A closed outline [[x, y], …].
    #[serde(default)]
    points: Option<Bounded<Point, 0, { crate::locations::MAX_VERTICES }>>,
    /// A closed polyline to take the outline from.
    #[serde(default)]
    item: Option<ItemId>,
    /// The page (default: the active page).
    #[serde(default)]
    page: Option<PageId>,
    /// What the box does to the devices inside: "propose" (default), "move" or "keep".
    #[serde(default)]
    proposal: Option<String>,
}

impl Generic for FunctionboxParams {
    const ID: &'static str = "functionbox";
    const TITLE: &'static str = "Function Box";
    const STATE: State = State::Conflicts;

    fn run(self, cx: &mut Cx<'_>) -> Result<Outcome, CmdError> {
        let b = BoxInput { function: Some(self.function), location: self.location, corners: self.corners, points: self.points, item: self.item };
        b.run(cx, self.page, self.proposal)
    }
}

/// What both box commands take.
struct BoxInput {
    function: Option<String>,
    location: Option<String>,
    corners: Option<Bounded<Point, 2, 2>>,
    points: Option<Bounded<Point, 0, { crate::locations::MAX_VERTICES }>>,
    item: Option<ItemId>,
}

impl BoxInput {
    /// A box: its values, outline, and the proposal it
    /// makes for the device symbols inside. The conflicts it finds wait for an answer when no
    /// device was moved.
    fn run(self, cx: &mut Cx<'_>, page: Option<PageId>, proposal: Option<String>) -> Result<Outcome, CmdError> {
        let mode = crate::pages::commands::sheets::proposal_mode(proposal.as_deref())?;
        let corners = self.corners.and_then(|c| match c.0.as_slice() {
            [a, b] => Some((a.0, b.0)),
            _ => None,
        });
        let points = self.points.map(|v| v.0.into_iter().map(|p| p.0).collect());
        let shape = Shape::one_of(corners, points, self.item)?;
        let page = page.or(cx.page).ok_or(CmdError::refused("no active page"))?;
        let mut plan = plan_box(cx.doc, cx.ids, page, self.function.as_deref(), self.location.as_deref(), shape, &mode)?;
        let ops = std::mem::take(&mut plan.ops);
        let mut lines = vec![plan.message.clone()];
        lines.extend(proposals::conflict_words(cx.doc, &plan.proposed.conflicts));
        if plan.proposed.moved == 0
            && let Slot::Conflicts { set, .. } = &mut cx.state
        {
            **set = Some(plan.proposed.conflicts.clone());
        }
        let label = plan.label.clone();
        Ok(Outcome::edit_then(label, ops, move |doc| box_json(doc, &plan)).echo_lines(lines))
    }
}
