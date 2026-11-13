//! The shell's layout (design `ui-architecture.md` §5.1–§5.3): pure arithmetic from the window's
//! rect and the [`ShellState`] to the rect of every slot. No egui context, so every size rule is
//! a plain unit test.
//!
//! Sizes are spec §2's at 100 % interface scale. The drawer is 150 px open and 36 px collapsed;
//! the main view's height at 1920 × 1080 is design §5.2's (824 with the drawer open, 974
//! collapsed), which counts the drawer's 36 px header row on top of the 150 px body when open.

use ecad_ui::tokens::size;
use eframe::egui::{Rect, pos2, vec2};

use super::state::{LeftTab, ShellState, Workspace};

/// The left panel's default width, its drag range and its rail (spec §2).
pub const LEFT_W: f32 = 264.0;
pub const LEFT_MIN: f32 = 200.0;
pub const LEFT_MAX: f32 = 420.0;
/// History's default width (design §5.2).
pub const HISTORY_W: f32 = 400.0;
/// A collapsed side panel's rail.
pub const RAIL_W: f32 = 44.0;
/// The right panel's width.
pub const RIGHT_W: f32 = 340.0;
/// The drawer's body when open, and its header row (all that shows when collapsed).
pub const DRAWER_BODY: f32 = 150.0;
pub const DRAWER_HEADER: f32 = 36.0;
/// Below this main-view width the shell collapses panels (right, then left, then the drawer).
pub const MAIN_MIN_W: f32 = 760.0;
/// The companion split's handle, and the least width of each half.
pub const SPLIT_HANDLE: f32 = 6.0;
pub const HALF_MIN: f32 = 360.0;

/// Every slot's rect, and the collapses the window size forced (view state, never saved).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Slots {
    pub top_bar: Rect,
    pub agent_banner: Option<Rect>,
    pub left: Rect,
    pub tool_strip: Option<Rect>,
    pub main: Rect,
    pub companion: Option<Rect>,
    /// The companion split's drag handle, between `main` and `companion`.
    pub handle: Option<Rect>,
    pub drawer: Rect,
    pub right: Rect,
    pub status: Rect,
    /// Whether each panel is shown as its rail / the drawer open, after forced collapses.
    pub left_rail: bool,
    pub right_rail: bool,
    pub drawer_open: bool,
}

/// The left panel's width for its tab, kept within the drag range.
pub fn left_width(state: &ShellState) -> f32 {
    let w = state.left.width_per_tab.get(state.left.tab.index()).copied().unwrap_or(LEFT_W);
    if w.is_finite() { w.clamp(LEFT_MIN, LEFT_MAX) } else { LEFT_W }
}

/// The slots for `window`. `agent`: an agent is connected (the banner shows).
pub fn layout(window: Rect, state: &ShellState, agent: bool) -> Slots {
    let (mut left_rail, mut right_rail, mut drawer_open) = (state.left.collapsed, state.right_collapsed && !state.text_panel, state.drawer_open);
    let strip = state.workspace == Workspace::Draw;
    let main_w = |left_rail: bool, right_rail: bool| {
        let l = if left_rail { RAIL_W } else { left_width(state) };
        let r = if right_rail { RAIL_W } else { RIGHT_W };
        window.width() - l - r - if strip { size::TOOL_STRIP } else { 0.0 }
    };
    // Collapse order (design §5.2): the right panel, then the left, then the drawer.
    // The Text panel keeps the right panel open (it is drawn over it).
    if main_w(left_rail, right_rail) < MAIN_MIN_W {
        right_rail = !state.text_panel;
        if main_w(left_rail, right_rail) < MAIN_MIN_W {
            left_rail = true;
            if main_w(left_rail, right_rail) < MAIN_MIN_W {
                drawer_open = false;
            }
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
    let lw = if left_rail { RAIL_W } else { left_width(state) };
    let rw = if right_rail { RAIL_W } else { RIGHT_W };
    let left = Rect::from_min_max(pos2(window.left(), y), pos2(window.left() + lw, bottom));
    let right = Rect::from_min_max(pos2((window.right() - rw).max(left.right()), y), pos2(window.right(), bottom));
    let tool_strip = strip.then(|| Rect::from_min_max(pos2(left.right(), y), pos2((left.right() + size::TOOL_STRIP).min(right.left()), bottom)));
    let main_left = tool_strip.map_or(left.right(), |s| s.right());
    let drawer_h = if drawer_open { DRAWER_HEADER + DRAWER_BODY } else { DRAWER_HEADER };
    let drawer_top = (bottom - drawer_h).max(y);
    let drawer = Rect::from_min_max(pos2(main_left, drawer_top), pos2(right.left().max(main_left), bottom));
    let view = Rect::from_min_max(pos2(main_left, y), pos2(right.left().max(main_left), drawer_top));
    let (main, handle, companion) = split(view, state);
    Slots { top_bar, agent_banner, left, tool_strip, main, companion, handle, drawer, right, status, left_rail, right_rail, drawer_open }
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

/// The left panel's default width for `tab`.
pub fn default_width(tab: LeftTab) -> f32 {
    match tab {
        LeftTab::History => HISTORY_W,
        LeftTab::Pages | LeftTab::Devices => LEFT_W,
    }
}
