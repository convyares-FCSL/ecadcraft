//! Line weights (spec §4, "Line weights"). UI lines are in screen px; canvas lines are in mm on
//! paper and scale with zoom, with a floor of [`SCREEN_FLOOR_PX`] on screen.

/// UI hairline, px.
pub const HAIRLINE_PX: f32 = 1.0;
/// UI icon stroke, px, on a 16 px grid with round caps and joins.
pub const ICON_STROKE_PX: f32 = 1.5;
/// Focus ring, px, in `accent_text`.
pub const FOCUS_RING_PX: f32 = 2.0;
/// Symbol outline, mm.
pub const SYMBOL_MM: f32 = 0.35;
/// Control wire, mm.
pub const CONTROL_WIRE_MM: f32 = 0.4;
/// Power wire and PE, mm.
pub const POWER_WIRE_MM: f32 = 0.6;
/// Frame, mm.
pub const FRAME_MM: f32 = 0.5;
/// Title block and table rules, mm.
pub const RULE_MM: f32 = 0.2;
/// Mechanical link, mm.
pub const MECHANICAL_LINK_MM: f32 = 0.25;
/// Mechanical link dash on, mm.
pub const MECHANICAL_LINK_DASH_MM: f32 = 1.2;
/// Mechanical link gap, mm.
pub const MECHANICAL_LINK_GAP_MM: f32 = 1.0;
/// Cable slash, mm.
pub const CABLE_SLASH_MM: f32 = 0.5;
/// Terminal link (jumper) bar, mm.
pub const TERMINAL_LINK_MM: f32 = 0.6;
/// Terminal link dash on, mm.
pub const TERMINAL_LINK_DASH_MM: f32 = 1.8;
/// Terminal link gap, mm.
pub const TERMINAL_LINK_GAP_MM: f32 = 1.4;
/// Selection outline, mm.
pub const SELECTION_MM: f32 = 0.5;
/// Agent's last write outline, mm.
pub const AGENT_WRITE_MM: f32 = 0.6;
/// The thinnest a canvas line is drawn on screen, px.
pub const SCREEN_FLOOR_PX: f32 = 1.0;

/// Every screen token as `(name, px)` and canvas token as `(name, mm)`, names as in `tokens.md`.
pub const PX: [(&str, f32); 4] = [
    ("line.hairline", HAIRLINE_PX),
    ("line.icon_stroke", ICON_STROKE_PX),
    ("line.focus_ring", FOCUS_RING_PX),
    ("line.screen_floor", SCREEN_FLOOR_PX),
];
pub const MM: [(&str, f32); 14] = [
    ("line.symbol", SYMBOL_MM),
    ("line.control_wire", CONTROL_WIRE_MM),
    ("line.power_wire", POWER_WIRE_MM),
    ("line.frame", FRAME_MM),
    ("line.rule", RULE_MM),
    ("line.mechanical_link", MECHANICAL_LINK_MM),
    ("line.mechanical_link_dash", MECHANICAL_LINK_DASH_MM),
    ("line.mechanical_link_gap", MECHANICAL_LINK_GAP_MM),
    ("line.cable_slash", CABLE_SLASH_MM),
    ("line.terminal_link", TERMINAL_LINK_MM),
    ("line.terminal_link_dash", TERMINAL_LINK_DASH_MM),
    ("line.terminal_link_gap", TERMINAL_LINK_GAP_MM),
    ("line.selection", SELECTION_MM),
    ("line.agent_write", AGENT_WRITE_MM),
];
