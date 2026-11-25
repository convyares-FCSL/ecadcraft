//! What one commit changed, as the pipeline reports it to the engine: [`Changes`].
//!
//! The record is engine-free: page and object ids, flags and lines. ecad-engine's
//! `Project::apply_changes` turns it into the change log entry, the redraw and the echo.

use std::collections::BTreeSet;

use ecad_model::document::view::PageSet;
use ecad_model::{ObjectRef, PageId};

use crate::devices::device_list::DeviceTouch;

/// What one commit changed, merged from every kept view's outcome in a fixed order.
#[derive(Debug, Clone, PartialEq)]
pub struct Changes {
    /// Pages whose drawing could have changed.
    pub pages: PageSet,
    /// Pages the ops name directly (added, removed, or drawn on): an undo or redo brings one of
    /// them to the front.
    pub named: BTreeSet<PageId>,
    /// What the Devices list and symbol search must read again.
    pub devices: DeviceTouch,
    /// The objects whose shown properties could have changed; `None` when any could have.
    pub props: Option<BTreeSet<ObjectRef>>,
    /// The page list or tree must be read again.
    pub list: bool,
    /// The kept frame must be drawn again.
    pub frame_stale: bool,
    /// Lines for the command line, in order.
    pub echo: Vec<String>,
    /// Verify-mode disagreements.
    pub problems: Vec<String>,
}

impl Default for Changes {
    /// Nothing changed.
    fn default() -> Self {
        Changes {
            pages: PageSet::default(),
            named: BTreeSet::new(),
            devices: DeviceTouch::default(),
            props: Some(BTreeSet::new()),
            list: false,
            frame_stale: false,
            echo: Vec::new(),
            problems: Vec::new(),
        }
    }
}
