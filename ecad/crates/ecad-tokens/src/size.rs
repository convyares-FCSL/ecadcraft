//! Control, row, bar and tool sizes (spec §4, "Spacing, sizes, radii"), px at 100 % UI scale.

/// Small control height.
pub const CONTROL_SMALL: f32 = 24.0;
/// Default control height (buttons, fields).
pub const CONTROL: f32 = 28.0;
/// Search field height.
pub const SEARCH_FIELD: f32 = 32.0;
/// List row.
pub const ROW_LIST: f32 = 28.0;
/// Table row.
pub const ROW_TABLE: f32 = 30.0;
/// Group row.
pub const ROW_GROUP: f32 = 32.0;
/// Top bar (UI rev 3: 40 px).
pub const TOP_BAR: f32 = 40.0;
/// Panel header.
pub const PANEL_HEADER: f32 = 36.0;
/// Agent banner.
pub const AGENT_BANNER: f32 = 32.0;
/// The bottom bar (UI rev 3: status, the page well and the view, 32 px).
pub const STATUS_BAR: f32 = 32.0;
/// Tool button height (UI rev 3: 54 × 44, an icon over a label).
pub const TOOL_BUTTON: f32 = 44.0;
/// Tool button width in the strip.
pub const TOOL_BUTTON_W: f32 = 54.0;
/// Tool strip width; the inspector's collapsed rail is the same.
pub const TOOL_STRIP: f32 = 56.0;
/// Icon inside a tool button.
pub const TOOL_ICON: f32 = 18.0;
/// The label under a tool button's icon.
pub const TOOL_LABEL: f32 = 10.0;
/// The bottom bar's page well.
pub const PAGE_WELL: f32 = 880.0;
/// Icon button outside the tool strip (design §3.3): the default control height.
pub const ICON_BUTTON: f32 = 28.0;
/// Icon grid: icons are drawn on a 16 px grid ([`crate::line::ICON_STROKE_PX`]).
pub const ICON: f32 = 16.0;

/// Every token as `(name, px)`, names as in `tokens.md`.
pub const ALL: [(&str, f32); 18] = [
    ("size.control_small", CONTROL_SMALL),
    ("size.control", CONTROL),
    ("size.search_field", SEARCH_FIELD),
    ("size.row_list", ROW_LIST),
    ("size.row_table", ROW_TABLE),
    ("size.row_group", ROW_GROUP),
    ("size.top_bar", TOP_BAR),
    ("size.panel_header", PANEL_HEADER),
    ("size.agent_banner", AGENT_BANNER),
    ("size.status_bar", STATUS_BAR),
    ("size.tool_button", TOOL_BUTTON),
    ("size.tool_button_w", TOOL_BUTTON_W),
    ("size.tool_strip", TOOL_STRIP),
    ("size.tool_icon", TOOL_ICON),
    ("size.tool_label", TOOL_LABEL),
    ("size.page_well", PAGE_WELL),
    ("size.icon_button", ICON_BUTTON),
    ("size.icon", ICON),
];
