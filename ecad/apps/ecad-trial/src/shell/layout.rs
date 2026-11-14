//! The shell's layout (UI rev 3, "Shell at 1920 × 1080"; earlier design `ui-architecture.md`
//! §5.1–§5.3): pure arithmetic from the window's rect and the [`ShellState`] to the rect of every
//! slot. No egui context, so every size rule is a plain unit test.
//!
//! One bar at each edge: the top bar (40 px) and the bottom bar (32 px). Between them, in Draw:
//! the tool strip (56 px) at the left edge, the panel slot beside it while a panel is open (Book
//! and Devices 248 px, History 280 px), the canvas, and the inspector at the right (320 px open,
//! a 56 px rail collapsed). Project, Library and Publish have no strip and no panel slot; their
//! inspector is 340 px open. The agent banner sits under the top bar while an agent is connected.

use ecad_ui::tokens::size;
use eframe::egui::{Rect, pos2, vec2};

use super::state::{LeftTab, ShellState, Workspace};

/// The panel slot's default width (Book, Devices), its drag range, and History's default.
pub const LEFT_W: f32 = 248.0;
pub const LEFT_MIN: f32 = 200.0;
pub const LEFT_MAX: f32 = 420.0;
pub const HISTORY_W: f32 = 280.0;
/// The inspector's collapsed rail: the tool strip's width, its mirror image.
pub const RAIL_W: f32 = size::TOOL_STRIP;
/// The inspector's width in Draw, and in the other workspaces.
pub const RIGHT_W: f32 = 320.0;
pub const RIGHT_W_WIDE: f32 = 340.0;
/// Below this main-view width the shell collapses panels (the inspector, then the panel slot).
pub const MAIN_MIN_W: f32 = 760.0;
/// The companion split's handle, and the least width of each half.
pub const SPLIT_HANDLE: f32 = 6.0;
pub const HALF_MIN: f32 = 360.0;

/// Every slot's rect, and the collapses the window size forced (view state, never saved).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Slots {
    pub top_bar: Rect,
    pub agent_banner: Option<Rect>,
    /// The panel slot beside the strip, while a panel is open (Draw only).
    pub panel: Option<Rect>,
    pub tool_strip: Option<Rect>,
    pub main: Rect,
    pub companion: Option<Rect>,
    /// The companion split's drag handle, between `main` and `companion`.
    pub handle: Option<Rect>,
    pub right: Rect,
    /// The bottom bar.
    pub status: Rect,
    /// Whether the inspector shows as its rail, after forced collapses.
    pub right_rail: bool,
}

/// The panel slot's width for its tab, kept within the drag range.
pub fn left_width(state: &ShellState) -> f32 {
    let w = state.left.width_per_tab.get(state.left.tab.index()).copied().unwrap_or(LEFT_W);
    if w.is_finite() { w.clamp(LEFT_MIN, LEFT_MAX) } else { LEFT_W }
}

/// The inspector's open width in `workspace`.
pub fn right_width(workspace: Workspace) -> f32 {
    if workspace == Workspace::Draw { RIGHT_W } else { RIGHT_W_WIDE }
}

/// The slots for `window`. `agent`: an agent is connected (the banner shows).
pub fn layout(window: Rect, state: &ShellState, agent: bool) -> Slots {
    let draw = state.workspace == Workspace::Draw;
    let mut panel_open = draw && !state.left.collapsed;
    let mut right_rail = state.right_collapsed && !state.text_panel;
    let strip_w = if draw { size::TOOL_STRIP } else { 0.0 };
    let main_w = |panel_open: bool, right_rail: bool| {
        let l = if panel_open { left_width(state) } else { 0.0 };
        let r = if right_rail { RAIL_W } else { right_width(state.workspace) };
        window.width() - strip_w - l - r
    };
    // Collapse order: the inspector to its rail, then the panel slot closes. The Text panel keeps
    // the inspector open (it is drawn over it).
    if main_w(panel_open, right_rail) < MAIN_MIN_W {
        right_rail = !state.text_panel;
        if main_w(panel_open, right_rail) < MAIN_MIN_W {
            panel_open = false;
        }
    }
    let top_bar = Rect::from_min_size(window.min, vec2(window.width(), size::TOP_BAR));
    let mut y = top_bar.bottom();
    let agent_banner = agent.then(|| {
        let r = Rect::from_min_size(pos2(window.left(), y), vec2(window.width(), size::AGENT_BANNER));
        y = r.bottom();
        r
    });
    let status = Rect::from_min_max(pos2(window.left(), window.bottom() - size::STATUS_BAR), window.max);
    let bottom = status.top().max(y);
    let rw = if right_rail { RAIL_W } else { right_width(state.workspace) };
    let right = Rect::from_min_max(pos2((window.right() - rw).max(window.left()), y), pos2(window.right(), bottom));
    let tool_strip = draw.then(|| Rect::from_min_max(pos2(window.left(), y), pos2((window.left() + strip_w).min(right.left()), bottom)));
    let mut x = tool_strip.map_or(window.left(), |s| s.right());
    let panel = panel_open.then(|| {
        let r = Rect::from_min_max(pos2(x, y), pos2((x + left_width(state)).min(right.left()), bottom));
        x = r.right();
        r
    });
    let view = Rect::from_min_max(pos2(x, y), pos2(right.left().max(x), bottom));
    let (main, handle, companion) = split(view, state);
    Slots { top_bar, agent_banner, panel, tool_strip, main, companion, handle, right, status, right_rail }
}

/// The main view split for the companion: each half at least [`HALF_MIN`] wide, or no split.
fn split(view: Rect, state: &ShellState) -> (Rect, Option<Rect>, Option<Rect>) {
    let Some(c) = &state.companion else { return (view, None, None) };
    let room = view.width() - SPLIT_HANDLE;
    if room < 2.0 * HALF_MIN {
        return (view, None, None);
    }
    let ratio = if c.ratio.is_finite() { c.ratio } else { super::state::COMPANION_RATIO };
    let first = (room * ratio).clamp(HALF_MIN, room - HALF_MIN);
    let main = Rect::from_min_size(view.min, vec2(first, view.height()));
    let handle = Rect::from_min_size(pos2(main.right(), view.top()), vec2(SPLIT_HANDLE, view.height()));
    let companion = Rect::from_min_max(pos2(handle.right(), view.top()), view.max);
    (main, Some(handle), Some(companion))
}

/// The panel slot's default width for `tab`.
pub fn default_width(tab: LeftTab) -> f32 {
    match tab {
        LeftTab::History => HISTORY_W,
        LeftTab::Pages | LeftTab::Devices => LEFT_W,
    }
}
