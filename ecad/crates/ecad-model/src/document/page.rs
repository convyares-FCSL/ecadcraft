//! What a page is (design 072 §5.1): its kind and its title.
//!
//! # Semantics
//! - [`PageKind`] says what a page is: a drawn page, or a page whose content ECAD generates from the
//!   project ([`GenKind`]). It is set when the page is made and never inferred from its title,
//!   type or contents.
//! - A page's look (its page type, sheet, frame, title block) is separate: the page type is a
//!   style reference ([`crate::Page::page_type`]), and a Panel layout page is a schematic-kind page
//!   whose page type is `panel`.
//!
//! This module excludes how a generated page is drawn and paginated (ecad-electrical's
//! `generated`).
#![deny(clippy::wildcard_enum_match_arm)]

use serde::{Deserialize, Serialize};

use crate::id::DeviceId;
use crate::text::Text;

/// The old page property naming a terminal plan's one strip; now [`PageKind::TerminalStrip`]
/// (a reserved key).
pub const STRIP_KEY: &str = "strip";

/// Most characters of a page title.
pub const MAX_PAGE_TITLE: usize = 200;

/// A page's title: one line of 1 to [`MAX_PAGE_TITLE`] characters.
pub type PageTitle = Text<MAX_PAGE_TITLE>;

crate::word_enum! {
    /// What a generated page shows (briefs 024, 028, 045, 051).
    #[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
    pub enum GenKind {
        Cover,
        Contents,
        Revisions,
        #[strum(serialize = "bom-location")]
        BomLocation,
        /// The terminal plan of every strip (one strip's is [`PageKind::TerminalStrip`]).
        Terminals,
        /// The cable schedule.
        Cables,
        /// The rack overview, before the I/O list.
        #[strum(serialize = "plcrack")]
        PlcRack,
        /// The I/O list.
        #[strum(serialize = "iolist")]
        IoList,
    }
}

impl GenKind {
    /// Whether its content comes from the report models (built for it on first show).
    pub fn needs_reports(self) -> bool {
        match self {
            GenKind::BomLocation | GenKind::Terminals | GenKind::Cables | GenKind::PlcRack | GenKind::IoList => true,
            GenKind::Cover | GenKind::Contents | GenKind::Revisions => false,
        }
    }

    /// Whether it belongs to the front matter (the Front sheet page type); every other kind is a
    /// report's.
    pub fn is_front(self) -> bool {
        match self {
            GenKind::Cover | GenKind::Contents | GenKind::Revisions => true,
            GenKind::BomLocation | GenKind::Terminals | GenKind::Cables | GenKind::PlcRack | GenKind::IoList => false,
        }
    }

    /// The default title of a new page of this kind.
    pub fn title(self) -> &'static str {
        match self {
            GenKind::Cover => "Cover",
            GenKind::Contents => "Contents",
            // Brief 054 (design 054 §7): a new revisions page is the Revision history.
            GenKind::Revisions => "Revision history",
            GenKind::BomLocation => "BOM by location",
            GenKind::Terminals => "Terminal plan",
            GenKind::Cables => "Cable schedule",
            GenKind::PlcRack => "PLC racks",
            GenKind::IoList => "I/O list",
        }
    }
}

/// What a page is (design 072 §5.1).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", deny_unknown_fields)]
pub enum PageKind {
    /// A page a person draws on.
    #[default]
    Schematic,
    /// A page whose content ECAD draws from the project; nothing drawn on it is stored.
    /// `Generated(GenKind::Terminals)` is a real page: the terminal plan of every strip in the
    /// project, one after another (brief 028); a plan of one strip is [`PageKind::TerminalStrip`].
    Generated(GenKind),
    /// The terminal plan of one terminal strip (brief 028, FP3): a generated
    /// [`GenKind::Terminals`] page that shows only `strip`.
    TerminalStrip { strip: DeviceId },
}

impl PageKind {
    /// A generated page of kind `what` showing everything.
    pub fn generated(what: GenKind) -> PageKind {
        PageKind::Generated(what)
    }

    /// What it generates, if it is generated: a one-strip terminal plan is a
    /// [`GenKind::Terminals`] page.
    pub fn generated_kind(self) -> Option<GenKind> {
        match self {
            PageKind::Generated(what) => Some(what),
            PageKind::TerminalStrip { .. } => Some(GenKind::Terminals),
            PageKind::Schematic => None,
        }
    }

    /// The one strip a terminal plan shows, if it is a one-strip plan.
    pub fn strip(self) -> Option<DeviceId> {
        match self {
            PageKind::TerminalStrip { strip } => Some(strip),
            PageKind::Generated(_) | PageKind::Schematic => None,
        }
    }

    /// Whether it is a drawn page (left out of records: the default).
    pub fn is_schematic(&self) -> bool {
        *self == PageKind::Schematic
    }
}
