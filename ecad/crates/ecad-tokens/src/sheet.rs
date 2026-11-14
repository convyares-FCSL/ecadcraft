//! Canvas colours: the paper sheet (default) and the dark sheet, a second token set of the same
//! meanings (spec §4, "Canvas colours"). Whether a meaning prints is data ([`prints`]): ink
//! prints, overlay never does.

use crate::Color;

/// What a mark on the sheet means. Each meaning has a colour on each sheet and a shape
/// ([`crate::marker`]), so none relies on hue alone.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Meaning {
    /// The paper itself.
    Sheet,
    /// Symbols, text, default wire.
    Ink,
    /// Zone labels, pin labels, references.
    Ink2,
    /// The 5 mm dot grid.
    Grid,
    /// A connection point: hollow ring.
    ConnectionPoint,
    /// A wire end attached to a connection point: solid dot.
    WireEndAttached,
    /// A wire end attached to nothing: hollow square.
    LooseEnd,
    /// Cable slash and cable references.
    Cable,
    /// Error, orphan reference, refused preview.
    Error,
    /// Warning (overlay).
    Warning,
    /// The missing-data warning marker: the one warning that prints.
    MissingData,
    /// Provisional tag or wire number, pending revision.
    Provisional,
    /// Terminal link (jumper).
    TerminalLink,
    /// Selection.
    Selection,
    /// Placement preview.
    PlacementPreview,
    /// The agent's last write.
    AgentWrite,
}

impl Meaning {
    /// Every meaning, in the spec's order.
    pub const ALL: [Meaning; 16] = [
        Meaning::Sheet,
        Meaning::Ink,
        Meaning::Ink2,
        Meaning::Grid,
        Meaning::ConnectionPoint,
        Meaning::WireEndAttached,
        Meaning::LooseEnd,
        Meaning::Cable,
        Meaning::Error,
        Meaning::Warning,
        Meaning::MissingData,
        Meaning::Provisional,
        Meaning::TerminalLink,
        Meaning::Selection,
        Meaning::PlacementPreview,
        Meaning::AgentWrite,
    ];

    /// The token name in `tokens.md` (`connection_point`).
    pub const fn name(self) -> &'static str {
        match self {
            Meaning::Sheet => "sheet",
            Meaning::Ink => "ink",
            Meaning::Ink2 => "ink_2",
            Meaning::Grid => "grid",
            Meaning::ConnectionPoint => "connection_point",
            Meaning::WireEndAttached => "wire_end_attached",
            Meaning::LooseEnd => "loose_end",
            Meaning::Cable => "cable",
            Meaning::Error => "error",
            Meaning::Warning => "warning",
            Meaning::MissingData => "missing_data",
            Meaning::Provisional => "provisional",
            Meaning::TerminalLink => "terminal_link",
            Meaning::Selection => "selection",
            Meaning::PlacementPreview => "placement_preview",
            Meaning::AgentWrite => "agent_write",
        }
    }
}

/// Whether marks of this meaning are printed (in ink). The sheet itself prints as white paper,
/// which is not a mark; every overlay meaning (markers, selection, previews, agent) never prints.
///
/// # Example
/// ```
/// use ecad_tokens::sheet::{prints, Meaning};
/// assert!(prints(Meaning::Ink));
/// assert!(!prints(Meaning::Selection));
/// ```
pub const fn prints(meaning: Meaning) -> bool {
    matches!(meaning, Meaning::Ink | Meaning::Ink2 | Meaning::Cable | Meaning::Provisional | Meaning::TerminalLink | Meaning::MissingData)
}

/// One sheet's colours.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Sheet {
    pub sheet: Color,
    pub ink: Color,
    pub ink_2: Color,
    pub grid: Color,
    pub connection_point: Color,
    pub wire_end_attached: Color,
    pub loose_end: Color,
    pub cable: Color,
    pub error: Color,
    /// Warnings, the missing-data marker included.
    pub warning: Color,
    pub provisional: Color,
    pub terminal_link: Color,
    /// The selection colour; its 12 % fill is [`Sheet::selection_fill`].
    pub selection: Color,
    /// The placement preview's colour (the selection colour); its 8 % fill is [`Sheet::preview_fill`].
    pub placement_preview: Color,
    pub agent_write: Color,
}

/// The paper sheet, the default.
pub const PAPER: Sheet = Sheet {
    sheet: Color::hex(0xF6F4EF),
    ink: Color::hex(0x16191D),
    ink_2: Color::hex(0x5A616B),
    grid: Color::hex(0xD6D2C6),
    connection_point: Color::hex(0x0A8F94),
    wire_end_attached: Color::hex(0x17A05B),
    loose_end: Color::hex(0x7C8591),
    cable: Color::hex(0x7A3FE0),
    error: Color::hex(0xD7281E),
    warning: Color::hex(0xA86400),
    provisional: Color::hex(0xCF1F7A),
    terminal_link: Color::hex(0x0A8F94),
    selection: Color::hex(0x1F6FEB),
    placement_preview: Color::hex(0x1F6FEB),
    agent_write: Color::hex(0x5F8F00),
};

/// The dark sheet.
pub const DARK: Sheet = Sheet {
    sheet: Color::hex(0x1B1D21),
    ink: Color::hex(0xE4E6E9),
    ink_2: Color::hex(0x9299A3),
    grid: Color::hex(0x30333A),
    connection_point: Color::hex(0x45CFD3),
    wire_end_attached: Color::hex(0x55D68B),
    loose_end: Color::hex(0x8B94A1),
    cable: Color::hex(0xB48CFF),
    error: Color::hex(0xFF6B5E),
    warning: Color::hex(0xF2B84B),
    provisional: Color::hex(0xFF7AB8),
    terminal_link: Color::hex(0x45CFD3),
    selection: Color::hex(0x5B9BFF),
    placement_preview: Color::hex(0x5B9BFF),
    agent_write: Color::hex(0xB9E34A),
};

/// A page thumbnail's paper in the Book panel (UI rev 3, board 1.02).
pub const THUMBNAIL_PAPER: Color = Color::hex(0xE8E8E8);
/// A page's preview paper in the status bar's page menu (UI rev 3).
pub const PREVIEW_PAPER: Color = Color::hex(0xECECEC);
/// The ink of page thumbnails and previews (UI rev 3).
pub const THUMBNAIL_INK: Color = Color::hex(0x464646);

/// Selection fill opacity, % (spec: "12 % fill").
pub const SELECTION_FILL_PERCENT: u8 = 12;
/// Placement preview fill opacity, % (spec: "8 % fill").
pub const PREVIEW_FILL_PERCENT: u8 = 8;

impl Sheet {
    /// The colour of a meaning on this sheet.
    pub const fn color(&self, meaning: Meaning) -> Color {
        match meaning {
            Meaning::Sheet => self.sheet,
            Meaning::Ink => self.ink,
            Meaning::Ink2 => self.ink_2,
            Meaning::Grid => self.grid,
            Meaning::ConnectionPoint => self.connection_point,
            Meaning::WireEndAttached => self.wire_end_attached,
            Meaning::LooseEnd => self.loose_end,
            Meaning::Cable => self.cable,
            Meaning::Error => self.error,
            Meaning::Warning | Meaning::MissingData => self.warning,
            Meaning::Provisional => self.provisional,
            Meaning::TerminalLink => self.terminal_link,
            Meaning::Selection => self.selection,
            Meaning::PlacementPreview => self.placement_preview,
            Meaning::AgentWrite => self.agent_write,
        }
    }

    /// The selection's fill: the selection colour at 12 %.
    pub const fn selection_fill(&self) -> Color {
        self.selection.at_percent(SELECTION_FILL_PERCENT)
    }

    /// The placement preview's fill: its colour at 8 %.
    pub const fn preview_fill(&self) -> Color {
        self.placement_preview.at_percent(PREVIEW_FILL_PERCENT)
    }
}
