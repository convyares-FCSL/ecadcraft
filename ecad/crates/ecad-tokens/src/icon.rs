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

    // 041 (brief 041, U4b): the actions' icons (wheel slots, flyouts, search).
    /// Delete: a bin.
    Delete,
    /// Go to: an arrow out of a box.
    GoTo,
    /// Show its row: table rows with one marked.
    ShowRow,
    /// Edit: a pencil.
    Edit,
    /// Properties: a list with a pointer.
    Properties,
    /// Rotate: a turning arrow.
    Rotate,
    /// Mirror: two halves about an axis.
    Mirror,
    /// Duplicate: two overlapping boxes.
    Duplicate,
    /// Connect: a lead into a ring.
    Connect,
    /// Disconnect: a lead apart from a ring.
    Disconnect,
    /// Number: a hash.
    Number,
    /// Move: four arrows.
    Move,
    /// Paste: a sheet on a board.
    Paste,
    /// New page: a sheet with a plus.
    NewPage,
    /// Line: a diagonal line with end points.
    Line,
    /// Polyline: a bent line.
    Polyline,
    /// Rectangle.
    Rectangle,
    /// Circle.
    Circle,
    /// Arc.
    Arc,
    /// Terminal: a terminal block with a screw ring.
    Terminal,
    /// Junction: a dot where lines meet.
    Junction,
    /// Revision cloud: a scalloped outline.
    Cloud,
    /// Show or hide: an eye.
    Eye,
    // 044
    /// Connector: a socket's cup over a plug's pin.
    Connector,
    // 048
    /// New symbol: a symbol box with a plus at its corner.
    NewSymbol,
    /// Connection point (the builder's C tool): a ring on a lead.
    ConnectionPoint,
    /// Text slot (the builder's S tool): a dashed box with an anchor dot.
    TextSlot,
    /// Origin (the builder's O tool): a cross in a ring.
    Origin,
    // UI rev 3
    /// Book: an open book (UI rev 3: the Book panel).
    Book,
    /// PLC: a rack with three cards (UI rev 3: the PLC group).
    Plc,
    /// Snap: a cross hair on a dot (UI rev 3: the Snap toggle).
    Snap,
    /// Grid: a hash of four lines (UI rev 3: the Grid toggle).
    Grid,
    /// Ortho: a right angle with a square corner mark (UI rev 3: the Ortho toggle).
    Ortho,
    /// Dark theme: a crescent (UI rev 3: the light/dark switch).
    Moon,
    /// Light theme: a ring with eight rays (UI rev 3: the light/dark switch).
    Sun,
    /// Help: a question mark in a ring (UI rev 3: the top bar).
    Help,
    /// Page group: a folder with a tab (UI rev 3: a page group in the well).
    Folder,
    /// Previous group: two chevrons (UI rev 3: the page well).
    ChevronsLeft,
    /// Next group: two chevrons (UI rev 3: the page well).
    ChevronsRight,
    /// Fit: four corner marks (UI rev 3: the view group).
    Fit,
    /// Zoom out: a dash (UI rev 3: the view group).
    Minus,
    /// Zoom in: a plus (UI rev 3: the view group).
    Plus,
    /// Draw tools: a pencil (UI rev 3: the Draw group).
    Pencil,
    /// Device tools: a body on a lead (UI rev 3: the Device group).
    Pin,
    /// Terminal tools: a lead through a ring with a slash (UI rev 3: the Terminal group).
    TerminalPin,
    /// Wire tools: a wire bending up from one end ring to another (UI rev 3: the Wire group).
    WireBend,
    /// Cable tools: three leads tied by two strokes (UI rev 3: the Cable group).
    CableBundle,
    /// Location tools: a dashed box with a marked corner (UI rev 3: the Location group).
    LocationBox,
    /// Devices panel: three squares and a plus (UI rev 3: the Devices panel).
    DeviceGrid,
}

impl Icon {
    /// Every icon, in declaration order.
    pub const ALL: [Icon; 72] = [
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
        // 041
        Icon::Delete,
        Icon::GoTo,
        Icon::ShowRow,
        Icon::Edit,
        Icon::Properties,
        Icon::Rotate,
        Icon::Mirror,
        Icon::Duplicate,
        Icon::Connect,
        Icon::Disconnect,
        Icon::Number,
        Icon::Move,
        Icon::Paste,
        Icon::NewPage,
        Icon::Line,
        Icon::Polyline,
        Icon::Rectangle,
        Icon::Circle,
        Icon::Arc,
        Icon::Terminal,
        Icon::Junction,
        Icon::Cloud,
        Icon::Eye,
        // 044
        Icon::Connector,
        // 048
        Icon::NewSymbol,
        Icon::ConnectionPoint,
        Icon::TextSlot,
        Icon::Origin,
        // UI rev 3
        Icon::Book,
        Icon::Plc,
        Icon::Snap,
        Icon::Grid,
        Icon::Ortho,
        Icon::Moon,
        Icon::Sun,
        Icon::Help,
        Icon::Folder,
        Icon::ChevronsLeft,
        Icon::ChevronsRight,
        Icon::Fit,
        Icon::Minus,
        Icon::Plus,
        Icon::Pencil,
        Icon::Pin,
        Icon::TerminalPin,
        Icon::WireBend,
        Icon::CableBundle,
        Icon::LocationBox,
        Icon::DeviceGrid,
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
            // 041
            Icon::Delete => "Delete",
            Icon::GoTo => "Go to",
            Icon::ShowRow => "Show row",
            Icon::Edit => "Edit",
            Icon::Properties => "Properties",
            Icon::Rotate => "Rotate",
            Icon::Mirror => "Mirror",
            Icon::Duplicate => "Duplicate",
            Icon::Connect => "Connect",
            Icon::Disconnect => "Disconnect",
            Icon::Number => "Number",
            Icon::Move => "Move",
            Icon::Paste => "Paste",
            Icon::NewPage => "New page",
            Icon::Line => "Line",
            Icon::Polyline => "Polyline",
            Icon::Rectangle => "Rectangle",
            Icon::Circle => "Circle",
            Icon::Arc => "Arc",
            Icon::Terminal => "Terminal",
            Icon::Junction => "Junction",
            Icon::Cloud => "Revision cloud",
            Icon::Eye => "Show or hide",
            // 044
            Icon::Connector => "Connector",
            // 048
            Icon::NewSymbol => "New symbol",
            Icon::ConnectionPoint => "Connection point",
            Icon::TextSlot => "Text slot",
            Icon::Origin => "Origin",
            // UI rev 3
            Icon::Book => "Book",
            Icon::Plc => "PLC",
            Icon::Snap => "Snap",
            Icon::Grid => "Grid",
            Icon::Ortho => "Ortho",
            Icon::Moon => "Dark theme",
            Icon::Sun => "Light theme",
            Icon::Help => "Help",
            Icon::Folder => "Page group",
            Icon::ChevronsLeft => "Previous group",
            Icon::ChevronsRight => "Next group",
            Icon::Fit => "Fit",
            Icon::Minus => "Zoom out",
            Icon::Plus => "Zoom in",
            Icon::Pencil => "Draw tools",
            Icon::Pin => "Device tools",
            Icon::TerminalPin => "Terminal tools",
            Icon::WireBend => "Wire tools",
            Icon::CableBundle => "Cable tools",
            Icon::LocationBox => "Location tools",
            Icon::DeviceGrid => "Devices panel",
        }
    }
}
