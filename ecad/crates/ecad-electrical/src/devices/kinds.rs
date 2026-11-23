//! The kinds of object a person can point at on a page: what
//! the wheel and the action table are keyed by. Shared by the engine's hover (`interact::kind`)
//! and the app's action table.

/// The kinds of thing a wheel is for.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum ObjectKind {
    /// A device's symbol (a tagged or untagged component, not a terminal strip).
    Device,
    /// A drawing aid: a continuation arrow or a location box.
    Aid,
    /// A wire, away from its ends.
    Wire,
    /// A wire's end (attached or loose).
    WireEnd,
    /// A terminal of a terminal strip (the DIN-rail device), or the strip's link line.
    TerminalBlock,
    /// A cable's slash or fan.
    Cable,
    /// A text: a device's tag, a wire's label, or free text.
    Text,
    /// A cable reference end or a device cross-reference.
    Reference,
    /// The page where nothing is under the cursor.
    EmptyPage,
    /// A page's row in the Pages tab.
    PageRow,
    /// More than one object selected, or plain drawn geometry.
    Mixed,
}

impl ObjectKind {
    pub const ALL: [ObjectKind; 11] = [
        ObjectKind::Device,
        ObjectKind::Aid,
        ObjectKind::Wire,
        ObjectKind::WireEnd,
        ObjectKind::TerminalBlock,
        ObjectKind::Cable,
        ObjectKind::Text,
        ObjectKind::Reference,
        ObjectKind::EmptyPage,
        ObjectKind::PageRow,
        ObjectKind::Mixed,
    ];

    /// Its name in words.
    pub const fn words(self) -> &'static str {
        match self {
            ObjectKind::Device => "device",
            ObjectKind::Aid => "drawing aid",
            ObjectKind::Wire => "wire",
            ObjectKind::WireEnd => "wire end",
            ObjectKind::TerminalBlock => "terminal_block",
            ObjectKind::Cable => "cable",
            ObjectKind::Text => "text",
            ObjectKind::Reference => "reference",
            ObjectKind::EmptyPage => "page",
            ObjectKind::PageRow => "page row",
            ObjectKind::Mixed => "selection",
        }
    }
}
