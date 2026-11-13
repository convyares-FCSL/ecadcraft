//! The names of the shell's icons (design §3.4). `ecad-ui` paints each one in code from lines,
//! arcs and circles on a 16 px grid; nothing here draws. Later briefs add names here and a
//! painter in `ecad-ui` together.

/// An icon of the shell.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Icon {
    // The seven tool groups of the Draw workspace (spec §2).
    /// Select (V): an arrow pointer.
    Select,
    /// Wire (W): a line between two end rings.
    Wire,
    /// Place (P): a symbol box with a connection point ring on a line.
    Place,
    /// Cable (C): a slash across three lines.
    Cable,
    /// Draw (D): an arc over a line.
    Draw,
    /// Box (B): a dashed box.
    Box,
    /// Text (T): a letter T.
    Text,
    // The four workspaces.
    /// Draw workspace: a page.
    WorkspaceDraw,
    /// Lists workspace: rows.
    WorkspaceLists,
    /// Library workspace: shelves of symbols.
    WorkspaceLibrary,
    /// Publish workspace: a printer with a sheet coming out.
    WorkspacePublish,
    // Shell.
    /// Search: a lens.
    Search,
    /// Checks: a ticked ring.
    Checks,
    /// Agent presence: a ring with two eyes.
    Agent,
    /// History: a clock arc with an arrow.
    History,
    /// Pages: stacked sheets.
    Pages,
    /// Devices: a component box with two leads.
    Devices,
    /// Collapse a panel: a bar and an arrow into it.
    Collapse,
    /// Close: a cross.
    Close,
    /// Chevron pointing left.
    ChevronLeft,
    /// Chevron pointing right.
    ChevronRight,
    /// Chevron pointing up.
    ChevronUp,
    /// Chevron pointing down.
    ChevronDown,
}

impl Icon {
    /// Every icon, in declaration order.
    pub const ALL: [Icon; 23] = [
        Icon::Select,
        Icon::Wire,
        Icon::Place,
        Icon::Cable,
        Icon::Draw,
        Icon::Box,
        Icon::Text,
        Icon::WorkspaceDraw,
        Icon::WorkspaceLists,
        Icon::WorkspaceLibrary,
        Icon::WorkspacePublish,
        Icon::Search,
        Icon::Checks,
        Icon::Agent,
        Icon::History,
        Icon::Pages,
        Icon::Devices,
        Icon::Collapse,
        Icon::Close,
        Icon::ChevronLeft,
        Icon::ChevronRight,
        Icon::ChevronUp,
        Icon::ChevronDown,
    ];

    /// A readable name, used as the accessible label of an icon with no label of its own.
    pub const fn name(self) -> &'static str {
        match self {
            Icon::Select => "Select",
            Icon::Wire => "Wire",
            Icon::Place => "Place",
            Icon::Cable => "Cable",
            Icon::Draw => "Draw",
            Icon::Box => "Box",
            Icon::Text => "Text",
            Icon::WorkspaceDraw => "Draw workspace",
            Icon::WorkspaceLists => "Lists workspace",
            Icon::WorkspaceLibrary => "Library workspace",
            Icon::WorkspacePublish => "Publish workspace",
            Icon::Search => "Search",
            Icon::Checks => "Checks",
            Icon::Agent => "Agent",
            Icon::History => "History",
            Icon::Pages => "Pages",
            Icon::Devices => "Devices",
            Icon::Collapse => "Collapse",
            Icon::Close => "Close",
            Icon::ChevronLeft => "Chevron left",
            Icon::ChevronRight => "Chevron right",
            Icon::ChevronUp => "Chevron up",
            Icon::ChevronDown => "Chevron down",
        }
    }
}
