//! The nine primitives (design §3.3): [`button`], [`icon_button`], [`pill`], [`chip`],
//! [`badge`], [`field`], [`segmented`], [`keycap`], [`row`]; and the set-versus-read pair
//! [`read_value`] (a value read from the drawing: plain text, a go-to) and [`provisional`].
//!
//! Each returns an `egui::Response` (or a small struct holding one) and carries an accessible
//! label, so tests and the control channel find it by label. Sizes, radii, colours and fonts come
//! from `ecad-tokens` only.
//!
//! A primitive's [`Look`] can be forced (hover, pressed, focus) so the showcase and screenshots
//! show every state without a pointer; ordinary code leaves it at [`Look::Auto`].

mod button;
mod field;
mod label;
mod row;
mod segmented;

pub use button::{Button, IconButton, Kind, button, icon_button};
pub use field::{Field, FieldKind, field};
pub use label::{Chip, ChipResponse, badge, chip, keycap, pill};
pub use row::{RowKind, RowSpec, provisional, read_value, row};
pub use segmented::{Segmented, segmented};

use ecad_tokens::ui_type::TypeStyle;
use ecad_tokens::{Color, GRAPHITE};
use egui::{Color32, Galley, Response, Ui, WidgetInfo};
use std::sync::Arc;

/// A state a primitive is drawn in. `Auto` follows the pointer and keyboard; the others force a
/// look (the showcase draws every state with them).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Look {
    #[default]
    Auto,
    Hover,
    Pressed,
    Focus,
}

impl Look {
    pub(crate) fn hovered(self, r: &Response) -> bool {
        match self {
            Look::Auto => r.hovered(),
            Look::Hover => true,
            Look::Pressed | Look::Focus => false,
        }
    }
    pub(crate) fn pressed(self, r: &Response) -> bool {
        match self {
            Look::Auto => r.is_pointer_button_down_on(),
            Look::Pressed => true,
            Look::Hover | Look::Focus => false,
        }
    }
    pub(crate) fn focused(self, r: &Response) -> bool {
        match self {
            Look::Auto => r.has_focus(),
            Look::Focus => true,
            Look::Hover | Look::Pressed => false,
        }
    }
}

/// The status tone of a pill or badge.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Tone {
    #[default]
    Neutral,
    Ok,
    Warning,
    Error,
    Provisional,
    Agent,
}

impl Tone {
    pub const ALL: [Tone; 6] = [Tone::Neutral, Tone::Ok, Tone::Warning, Tone::Error, Tone::Provisional, Tone::Agent];

    /// The tone's text colour.
    pub fn text(self) -> Color {
        let s = GRAPHITE;
        match self {
            Tone::Neutral => s.text_2,
            Tone::Ok => s.ok,
            Tone::Warning => s.warning,
            Tone::Error => s.error,
            Tone::Provisional => s.provisional,
            Tone::Agent => s.agent,
        }
    }

    /// The tone's ground: `raised` when neutral, else its colour at the soft opacity.
    pub fn ground(self) -> Color {
        match self {
            Tone::Neutral => GRAPHITE.raised,
            t => t.text().at_percent(ecad_tokens::shell::SOFT_PERCENT),
        }
    }

    /// A readable name ("warning").
    pub fn name(self) -> &'static str {
        match self {
            Tone::Neutral => "neutral",
            Tone::Ok => "ok",
            Tone::Warning => "warning",
            Tone::Error => "error",
            Tone::Provisional => "provisional",
            Tone::Agent => "agent",
        }
    }
}

/// `text` laid out on one line in a UI type style; its colour is given when it is painted.
pub(crate) fn galley(ui: &Ui, text: &str, style: TypeStyle) -> Arc<Galley> {
    let font = crate::fonts::font(ui.ctx(), style);
    ui.painter().layout_no_wrap(text.to_string(), font, Color32::PLACEHOLDER)
}

/// Give `response` its accessible role, state and label, and an optional description (a disabled
/// control's reason, an invalid field's message).
pub(crate) fn accessible(response: &Response, info: impl Fn() -> WidgetInfo, description: Option<&str>) {
    response.widget_info(info);
    if let Some(d) = description {
        let d = d.to_string();
        response.ctx.accesskit_node_builder(response.id, |node| node.set_description(d));
    }
}
