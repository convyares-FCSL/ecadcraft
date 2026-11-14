//! The agent's last write on the page (brief 043, spec §4 "Agent's last write"): the objects of
//! the agent's last accepted transaction on the page shown get a dashed 0.6 mm outline in
//! `agent_write` with a name chip; a change selected in History gets the same outline in the
//! selection colour with its author.
//!
//! # Semantics
//! - Painted over the page with egui from CADCraft's view transform (`canvas.xf`), never as an
//!   entity, so it is never plotted or printed (`agent_write` does not print: `ecad-tokens`).
//! - Shown for as long as the banner shows that write as its last; gone when the banner is
//!   dismissed, and when Library or Publish hides the page (only where the page is shown:
//!   `Shell::page_rect`, so also beside Lists' companion).
//! - **Update map**: the outline's world boxes are read again only when the banner's last change,
//!   the History selection, the branch, the page shown or the drawing's revision changes (an edit
//!   on that page, by anyone); otherwise a frame only transforms the kept boxes to the screen. An
//!   idle frame reads nothing from the project ([`reads`] counts the reads).
//!
//! Excluded: CADCraft's canvas and ECAD's painter layer (040), marks (`overlay_marks`), plots.

use std::time::Instant;

use cadcraft_engine::doc::geom::Bounds2;
use ecad_cadcraft::checks::Object;
use ecad_model::{ObjectRef, PageId, TxId};
use ecad_ui::tokens::{Color, line, sheet, space, ui_type};
use eframe::egui;

use crate::Trial;

/// The outline's width on the sheet, mm (spec §4).
pub const OUTLINE_MM: f32 = 0.6;
/// Its dashes and gaps, mm.
const DASH_MM: f32 = 1.5;
const GAP_MM: f32 = 1.0;
/// How far the outline stands off the object's extents, mm.
const MARGIN_MM: f64 = 1.5;
/// Most objects outlined for one transaction (a large agent call outlines the first ones).
pub const MAX_OUTLINED: usize = 200;
/// The accessible name of the last-write chip (tests find it).
pub const CHIP: &str = "agent.lastwrite";
/// The accessible name of the History selection's chip.
pub const HISTORY_CHIP: &str = "history.touched";

/// What one outline is for.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    /// The agent's last write (`agent_write`).
    AgentWrite,
    /// A change selected in History (`selection`).
    Selected,
}

/// One transaction's outline on the page shown: world boxes and its chip.
#[derive(Debug, Clone, PartialEq)]
pub struct Outline {
    pub kind: Kind,
    pub chip: String,
    pub boxes: Vec<Bounds2>,
}

/// What the outlines were read for: (agent seq, selected seq, branch, page, drawing revision).
type Key = (Option<u64>, Option<u64>, String, Option<PageId>, u64);

/// The kept outlines (egui's memory, one per window).
#[derive(Debug, Clone, Default)]
struct Kept {
    key: Option<Key>,
    outlines: Vec<Outline>,
    reads: u64,
}

fn kept_id() -> egui::Id {
    egui::Id::unique("ecad.agent.lastwrite")
}

/// How many times the outlines were read from the project (tests: only when the update map says
/// so).
pub fn reads(ctx: &egui::Context) -> u64 {
    ctx.data(|d| d.get_temp::<Kept>(kept_id())).map_or(0, |k| k.reads)
}

/// The outlines kept for the page shown (tests).
pub fn outlines(ctx: &egui::Context) -> Vec<Outline> {
    ctx.data(|d| d.get_temp::<Kept>(kept_id())).map(|k| k.outlines).unwrap_or_default()
}

/// The wall clock, ms since the epoch, of `at`.
pub fn wall_ms(at: Instant) -> u64 {
    let now = u64::try_from(chrono::Utc::now().timestamp_millis()).unwrap_or(0);
    now.saturating_sub(u64::try_from(at.elapsed().as_millis()).unwrap_or(u64::MAX))
}

/// The live agent session, for History's session groups: the banner's agent and the steps it owns,
/// while it can still be undone as a session (not finished or stopped).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LiveSession {
    pub agent: String,
    /// (session, steps, last change) the copy was taken at.
    pub key: (String, usize, Option<u64>),
    pub owned: std::collections::BTreeSet<TxId>,
}

/// [`LiveSession`] for the banner shown; `kept` is returned unchanged when the banner has not
/// changed since it was taken (no copy per frame).
pub fn live_session(t: &Trial, kept: Option<LiveSession>) -> Option<LiveSession> {
    let b = t.agent.banner.as_ref().filter(|b| !matches!(b.state, crate::agent::BannerState::Finished | crate::agent::BannerState::Error))?;
    let key = (b.session.clone(), b.steps, b.last_change.as_ref().map(|c| c.seq));
    if kept.as_ref().is_some_and(|k| k.key == key) {
        return kept;
    }
    let owned = t.agent.sessions.get(&b.session).map(|s| s.owned.ids().iter().copied().collect()).unwrap_or_default();
    Some(LiveSession { agent: b.agent.clone(), key, owned })
}

/// Paint the agent's last write and the History selection's change over the page shown (the
/// shell's `main_view`, after the page is drawn).
pub fn paint(t: &mut Trial, ui: &egui::Ui, page: Option<egui::Rect>) {
    t.paint_last_write(ui, page);
}

impl Trial {
    /// Paint the agent's last write and the History selection's change over the page shown
    /// (`page`: the page's rect in the shell; `None` when no page is shown).
    pub fn paint_last_write(&mut self, ui: &egui::Ui, page: Option<egui::Rect>) {
        let Some(rect) = page else { return };
        let agent = self.agent.banner.as_ref().and_then(|b| b.last_change.as_ref().map(|c| (c.seq, c.branch.clone(), b.agent.clone())));
        let selected = self.panel.history.selected_step();
        if agent.is_none() && selected.is_none() {
            return;
        }
        let page_id = self.panel.active;
        let revision = self.cad.session.state().map_or(0, |st| st.revision);
        let branch = agent.as_ref().map(|a| a.1.clone()).unwrap_or_default();
        let key: Key = (agent.as_ref().map(|a| a.0), selected, branch, page_id, revision);
        let ctx = ui.ctx().clone();
        let mut kept = ctx.data(|d| d.get_temp::<Kept>(kept_id())).unwrap_or_default();
        if kept.key.as_ref() != Some(&key) {
            kept.outlines = self.read_outlines(agent.as_ref(), selected, page_id);
            kept.reads += 1;
            kept.key = Some(key);
            ctx.data_mut(|d| d.insert_temp(kept_id(), kept.clone()));
        }
        let Some(xf) = self.cad.canvas.xf.as_ref() else { return };
        // Over the page, in the page's own layer: windows and popups stay above it.
        let painter = ui.painter_at(rect);
        for o in &kept.outlines {
            let c = match o.kind {
                Kind::AgentWrite => sheet::PAPER.agent_write,
                Kind::Selected => sheet::PAPER.selection,
            };
            let mut top: Option<egui::Pos2> = None;
            for b in &o.boxes {
                let corners =
                    [b.min, cadcraft_engine::doc::geom::Vec2::new(b.max.x, b.min.y), b.max, cadcraft_engine::doc::geom::Vec2::new(b.min.x, b.max.y)];
                let pts: Vec<egui::Pos2> = corners.iter().map(|p| xf.to_screen(*p)).collect();
                let width = (OUTLINE_MM * xf.scale as f32).max(line::HAIRLINE_PX);
                let (dash, gap) = ((DASH_MM * xf.scale as f32).max(3.0), (GAP_MM * xf.scale as f32).max(2.0));
                let mut ring = pts.clone();
                if let Some(first) = pts.first() {
                    ring.push(*first);
                }
                painter.extend(egui::Shape::dashed_line(&ring, egui::Stroke::new(width, ecad_ui::color(c)), dash, gap));
                let tl = pts.iter().fold(egui::pos2(f32::MAX, f32::MAX), |a, p| egui::pos2(a.x.min(p.x), a.y.min(p.y)));
                if top.is_none_or(|t| tl.y < t.y) {
                    top = Some(tl);
                }
            }
            if let Some(at) = top.filter(|p| p.is_finite()) {
                chip(ui, &painter, at, &o.chip, c, if o.kind == Kind::AgentWrite { CHIP } else { HISTORY_CHIP });
            }
        }
    }

    /// Read the outlines' world boxes on `page` (the update map's one read).
    fn read_outlines(&mut self, agent: Option<&(u64, String, String)>, selected: Option<u64>, page: Option<PageId>) -> Vec<Outline> {
        let (Some(p), Some(page)) = (self.project.clone(), page) else { return Vec::new() };
        p.read(&mut self.cad.session, |p, pages| {
            let current = ecad_cadcraft::history::current(p).hex();
            let mut out = Vec::new();
            let mut one = |seq: u64, kind: Kind, chip: Option<&str>| {
                let Some(t) = p.ledger.transaction(seq) else { return };
                let Some(touched) = ecad_cadcraft::props::touched(p.ledger.document(), &t.ops) else { return };
                let Some(drawing) = pages.drawing_of(p, page) else { return };
                let mut boxes = Vec::new();
                for o in touched.iter().take(MAX_OUTLINED) {
                    let object = match o {
                        ObjectRef::Component(c) => Object::Component(*c),
                        ObjectRef::Conductor(c) => Object::Conductor(*c),
                        ObjectRef::Cable(c) => Object::Cable(*c),
                        ObjectRef::Page(_) | ObjectRef::Project | ObjectRef::Standard => continue,
                    };
                    let mut b = Bounds2::EMPTY;
                    for h in ecad_cadcraft::checks::handles_of(p, page, object) {
                        if let Some(e) = drawing.entity(h) {
                            b = b.union(&cadcraft_engine::doc::entity_bounds(&drawing, e, 0));
                        }
                    }
                    if !b.is_empty() && b.min.is_finite() && b.max.is_finite() {
                        let m = cadcraft_engine::doc::geom::Vec2::new(MARGIN_MM, MARGIN_MM);
                        boxes.push(Bounds2 { min: b.min - m, max: b.max + m });
                    }
                }
                if !boxes.is_empty() {
                    let chip = chip.map_or_else(|| ecad_cadcraft::history::author_text(&t.author), str::to_string);
                    out.push(Outline { kind, chip: if chip == "you" { "You".into() } else { chip }, boxes });
                }
            };
            if let Some((seq, branch, name)) = agent
                && *branch == current
            {
                one(*seq, Kind::AgentWrite, Some(name));
            }
            if let Some(seq) = selected.filter(|s| agent.is_none_or(|a| a.0 != *s)) {
                one(seq, Kind::Selected, None);
            }
            out
        })
        .unwrap_or_default()
    }
}

/// The outline's name chip above its top-left corner: the name on a ground of its colour.
fn chip(ui: &egui::Ui, painter: &egui::Painter, at: egui::Pos2, name: &str, c: Color, label: &str) {
    let font = ecad_ui::fonts::font(ui.ctx(), ui_type::CAPTION);
    let g = painter.layout_no_wrap(name.to_string(), font, ecad_ui::color(ecad_ui::tokens::WHITE));
    let size = g.size() + egui::vec2(2.0 * space::S, 2.0 * space::XXS);
    let r = egui::Rect::from_min_size(egui::pos2(at.x, at.y - size.y - space::XXS), size);
    painter.rect_filled(r, ecad_ui::style::corner(ecad_ui::tokens::radius::BADGE), ecad_ui::color(c));
    painter.galley(r.min + egui::vec2(space::S, space::XXS), g, ecad_ui::color(ecad_ui::tokens::WHITE));
    // Its accessible name, so tests and the control channel find it.
    let resp = ui.interact(r, egui::Id::unique((label, name)), egui::Sense::hover());
    resp.widget_info(|| egui::WidgetInfo::labeled(egui::Role::Label, true, format!("{label}: {name}")));
}
