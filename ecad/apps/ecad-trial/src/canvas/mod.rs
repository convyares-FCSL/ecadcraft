//! The canvas (brief 040, U4a): ECAD's painter layer over the page that CADCraft draws through
//! US1 (design `ui-architecture.md` §1.5). It sets CADCraft's drawing-area preferences from the
//! tokens (the paper or dark sheet, the selection colour, no ViewCube, UCS icon or grid), then
//! paints what CADCraft's marks cannot: the sheet's surround and shadow, the 5 mm dot grid, the
//! error and warning markers, the provisional underline, the dashed outlines of a device's other
//! placements on the page, and the companion leader line. Wire colours are drawing content (they
//! print in colour mode), so the page's projection draws them for the sheet in use
//! (`canvassheet`); a picked dash is swapped for its wire here.
//!
//! # Semantics
//! - Everything painted here is overlay: egui shapes over the page, never entities, so none of it
//!   can reach a plot (spec principle 1).
//! - **Update map.** What is painted from the project is cached per (project serial, change
//!   count, page, selection, sheet), the key the trial already keeps for the Properties section
//!   (`Trial::props_seen`), and read again only when that key moves: a frame with the same key
//!   reads nothing ([`State::reads`] counts the reads). The grid and the sheet surround are worked
//!   out from the view each frame (no project read). The leader line repaints only while live.
//! - Nothing is painted when the page is not shown (Lists, Library or Publish hiding it,
//!   [`crate::shell::Shell::page_rect`] `None`), nor in a paper-space layout.
//!
//! Excluded: the inspector's list of related placements (042 shows [`related`]'s result), the
//! agent's last write (043), proposal ghosts (M2).

pub mod leader;
pub mod markers;
pub mod selection;
pub mod sheet;

use std::sync::{Arc, Mutex, PoisonError};

use cadcraft_engine::doc::Handle;
use eframe::egui::{self, Rect};
use serde::{Deserialize, Serialize};

pub use selection::{Related, related};

use crate::Trial;

/// The `ui.sheet` command (Text panel ▸ Commands): `ui.sheet paper|dark`, or alone to switch.
pub const SHEET_COMMAND: &str = "ui.sheet";

/// Which sheet the page is drawn on: a viewing preference of the person, kept with the shell
/// state (eframe storage), never in the project.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum Sheet {
    #[default]
    Paper,
    Dark,
}

impl Sheet {
    /// The sheet's tokens.
    pub fn tokens(self) -> &'static ecad_ui::tokens::Sheet {
        match self {
            Sheet::Paper => &ecad_ui::tokens::sheet::PAPER,
            Sheet::Dark => &ecad_ui::tokens::sheet::DARK,
        }
    }

    pub fn word(self) -> &'static str {
        match self {
            Sheet::Paper => "paper",
            Sheet::Dark => "dark",
        }
    }
}

/// The readable zoom a page first opens at (spec §4 "Type", design §4): a tag is this tall, px.
pub const TAG_PX: f64 = 13.0;

/// What the painter keeps between frames (in egui's temporary data, so the trial's own state is
/// unchanged).
#[derive(Debug, Default)]
pub struct State {
    key: Option<Key>,
    /// Project reads by the painter layer since start (tests: none while the key is unchanged).
    pub reads: u64,
    /// What the shown page needs painted, from the last read.
    pub page: PageData,
    /// The page last shown and the number of page views stored then (first-open zoom).
    seen_page: Option<ecad_model::PageId>,
    stored_views: usize,
    pub leader: Option<leader::Leader>,
}

/// The cache key: project serial, change count, page, selection, dark sheet.
type Key = (u64, u64, Option<ecad_model::PageId>, Vec<Handle>, bool);

/// The page's painted content, in drawing units.
#[derive(Debug, Clone, Default)]
pub struct PageData {
    /// The sheet frames (one per sheet window).
    pub frames: Vec<(ecad_model::Pt, ecad_model::Pt)>,
    pub painted: Vec<ecad_cadcraft::look::Painted>,
    /// Provisional tag texts: their handles (bounds read from the drawing when painted).
    pub provisional: Vec<Handle>,
    pub related: Related,
    /// The project's tag height, mm (the first-open zoom).
    pub tag_height: f64,
}

#[derive(Clone, Default)]
struct Slot(Arc<Mutex<State>>);

fn slot(ctx: &egui::Context) -> Arc<Mutex<State>> {
    ctx.data_mut(|d| d.get_temp_mut_or_default::<Slot>(egui::Id::unique("ecad.canvas")).0.clone())
}

/// Run `f` on the painter's state (tests: [`State::reads`], [`State::page`]).
pub fn with_state<R>(ctx: &egui::Context, f: impl FnOnce(&mut State) -> R) -> R {
    let s = slot(ctx);
    let mut g = s.lock().unwrap_or_else(PoisonError::into_inner);
    f(&mut g)
}

/// Apply the canvas preferences and paint the overlay over the page in `page_rect` (`None`: the
/// page is not shown). Called once a frame, right after CADCraft's drawing.
pub fn paint(t: &mut Trial, ui: &mut egui::Ui, page_rect: Option<Rect>) {
    apply_preferences(t);
    let ctx = ui.ctx().clone();
    let s = slot(&ctx);
    let mut st = s.lock().unwrap_or_else(PoisonError::into_inner);
    let Some(page_rect) = page_rect else { return };
    if t.shell.page_rect.is_none() {
        return;
    }
    let (Some(rect), Some(xf)) = (t.cad.canvas.rect, t.cad.canvas.xf) else { return };
    if t.cad.session.layout_space() != cadcraft_engine::doc::Space::Model {
        return;
    }
    let picked = refresh(t, &mut st);
    first_open_zoom(t, &mut st, rect);
    let now = ctx.input(|i| i.time);
    if picked && let Some(companion) = t.shell.slots.and_then(|s| s.companion) {
        st.leader = leader_to_selection(t, &xf, companion, now);
    }
    let sheet = t.shell.state.sheet.tokens();
    let painter = ui.painter_at(rect.intersect(page_rect));
    sheet::paint(&painter, &xf, sheet, &st.page.frames, rect);
    markers::paint(&painter, &xf, sheet, t, &st.page);
    if leader::paint(&painter, sheet, &mut st.leader, now) {
        ctx.request_repaint();
    }
}

/// The sheet's colours, the selection colour and CADCraft's view switches, every frame (cheap
/// assignments). The ViewCube, UCS icon and grid return with the developer switch.
fn apply_preferences(t: &mut Trial) {
    let sheet = t.shell.state.sheet;
    let tokens = sheet.tokens();
    t.cad.options.model_background = Some(ecad_ui::color(tokens.sheet));
    t.cad.options.selection_color = Some(ecad_ui::color(tokens.selection));
    // The projection draws wire colours for the sheet in use: tell it, and redraw, on a change.
    if (ecad_cadcraft::look::sheet().sheet == ecad_ui::tokens::sheet::DARK.sheet) != (sheet == Sheet::Dark) {
        let _ = t.cad.session.execute("canvassheet", &serde_json::json!({ "dark": sheet == Sheet::Dark }));
    }
    let chrome = t.shell.state.cad_chrome;
    t.cad.ui.show_viewcube = chrome;
    t.cad.ui.show_ucs_icon = chrome;
    if !chrome {
        t.cad.session.settings.gridmode = false;
    }
}

/// Read the shown page's painted content again when the cache key moved. Whether only the
/// selection moved, to something (a companion leader line is due).
fn refresh(t: &mut Trial, st: &mut State) -> bool {
    let Some((serial, count, page, selection)) = t.props_seen.clone() else {
        st.key = None;
        st.page = PageData::default();
        return false;
    };
    let key: Key = (serial, count, page, selection.clone(), t.shell.state.sheet == Sheet::Dark);
    if st.key.as_ref() == Some(&key) {
        return false;
    }
    let picked = !selection.is_empty() && st.key.as_ref().is_some_and(|k| (k.0, k.1, k.2) == (serial, count, page) && k.3 != selection);
    let Some(p) = t.project.clone() else { return false };
    let Ok(uid) = t.cad.session.state().map(|s| s.uid) else { return false };
    st.reads += 1;
    let read = p.read(&mut t.cad.session, |p, _| {
        let Some(page) = p.page_of(uid) else { return (PageData::default(), None) };
        let doc = p.ledger.document();
        let pick = ecad_cadcraft::look::pick_routes(p, page, &selection);
        let data = PageData {
            frames: ecad_cadcraft::generated::sheet_windows(doc, page, p.outputs.as_ref()),
            painted: ecad_cadcraft::look::painted_marks(p, uid),
            provisional: ecad_cadcraft::look::provisional_tag_handles(p, page),
            related: related(p, page, &selection),
            tag_height: ecad_cadcraft::text_style::text_style(doc, ecad_cadcraft::text_style::TextKind::Tag).height,
        };
        (data, pick)
    });
    match read {
        // A dash picked: select its wire instead (the next frame reads again for it).
        Ok((_, Some(routes))) => t.cad.session.set_selection(routes),
        Ok((data, None)) => {
            st.page = data;
            st.key = Some(key);
        }
        Err(_) => {}
    }
    picked
}

/// A leader from the companion's edge to the selected objects on the page, if they show.
fn leader_to_selection(t: &Trial, xf: &cadcraft_ui_egui::canvas::Xf, companion: Rect, now: f64) -> Option<leader::Leader> {
    let d = t.cad.session.doc().ok()?;
    let to = t
        .cad
        .session
        .selection()
        .iter()
        .take(selection::MAX_LOOKED_AT)
        .filter_map(|h| d.entity(*h).map(|e| cadcraft_engine::doc::extent_bounds(d, e)))
        .map(|b| Rect::from_two_pos(xf.to_screen(b.min), xf.to_screen(b.max)))
        .filter(|r| r.is_finite())
        .reduce(|a, b| a.union(b))?
        .center();
    let x = if to.x < companion.center().x { companion.left() } else { companion.right() };
    let from = egui::pos2(x, to.y.clamp(companion.top(), companion.bottom()));
    Some(leader::Leader { from, to, since: now })
}

/// A page shown for the first time (no view stored for it before this frame) opens with the
/// sheet fitted, frame and title block visible, a 24 px margin (UI rev 3, "Canvas"; brief 040
/// opened it where a tag is [`TAG_PX`] tall); the shell then remembers its zoom (038's page
/// views).
fn first_open_zoom(t: &mut Trial, st: &mut State, rect: Rect) {
    let page = t.props_seen.as_ref().and_then(|k| k.2);
    let stored: usize = t.shell.views.projects.values().map(|p| p.pages.len()).sum();
    let switched = page != st.seen_page;
    let first = switched && st.seen_page.is_some() && stored == st.stored_views.saturating_add(1);
    st.seen_page = page;
    st.stored_views = stored;
    if !first || rect.height() <= 0.0 {
        return;
    }
    t.fit_sheet_in(rect);
}

/// Give CADCraft's font table the sheet font (US3; `ecad_cadcraft::install` does it too, and a
/// print). Whether it is registered.
pub fn register_sheet_faces() -> bool {
    ecad_cadcraft::text_style::register_sheet_font()
}

/// Whether `line` is the `ui.sheet` command.
pub fn is_sheet_command(line: &str) -> bool {
    line.split_whitespace().next().is_some_and(|w| w.eq_ignore_ascii_case(SHEET_COMMAND))
}

impl Trial {
    /// `ui.sheet [paper|dark]`: the sheet the page is drawn on; the reply says which it is now.
    pub fn sheet_command(&mut self, line: &str) {
        let arg = line.split_whitespace().nth(1).map(str::to_ascii_lowercase);
        let sheet = match arg.as_deref() {
            Some("paper" | "light") => Sheet::Paper,
            Some("dark") => Sheet::Dark,
            None if self.shell.state.sheet == Sheet::Paper => Sheet::Dark,
            None => Sheet::Paper,
            Some(other) => {
                self.developer.reply = vec![format!("ui.sheet takes paper or dark, not {other:?}.")];
                return;
            }
        };
        self.shell.state.sheet = sheet;
        self.developer.reply = vec![format!("The page is drawn on the {} sheet.", sheet.word())];
    }
}
