//! A dialog: an opaque box over the dimmed page (UI rev 4; owner, review of 054). Every popup that
//! asks for something (a table to edit, a choice to confirm) uses it, so they all look alike: the
//! `panel` ground with a hairline and the one shadow, the title with a close cross, the body, and
//! the footer's buttons tight to the bottom edge, right-aligned.
//!
//! Esc and the cross close it; a click on the dimmed page does not, so a stray click never loses
//! what was typed. The dialog's accessible role is `Dialog`, named by the caller.

use ecad_tokens::{Icon, line, radius, size, space, ui_type};
use egui::{Align, Context, Frame, Id, Key, Layout, Margin, Modifiers, Stroke, Ui, vec2};

use crate::prim::Kind;
use crate::style::{corner, shadow};
use crate::{SHELL, color, fonts};

/// The page behind a dialog: dimmed, black at 45 % (the shadow's colour).
pub const SCRIM: ecad_tokens::Color = ecad_tokens::shadow::COLOR;

/// A dialog's frame and layout. [`Dialog::show`] draws it and says whether it was closed.
#[derive(Debug, Clone, Copy)]
pub struct Dialog<'a> {
    id: Id,
    title: &'a str,
    name: &'a str,
    width: f32,
}

/// What [`Dialog::show`] returns: the body's and the footer's results, and whether the person
/// closed it (Esc or the cross).
#[derive(Debug)]
pub struct Shown<B, F> {
    pub body: B,
    pub footer: F,
    pub closed: bool,
}

/// One button of a dialog's footer (laid right to left: add the primary first). `kind` is
/// [`Kind::Primary`] for the one action that answers the dialog, [`Kind::Danger`] when that
/// action deletes or discards, [`Kind::Secondary`] for Cancel and the rest. `name` is its
/// accessible name; `disabled` says why it cannot be pressed yet.
///
/// # Example
/// ```
/// # egui::__run_test_ui(|ui| {
/// use ecad_ui::dialog::action;
/// use ecad_ui::prim::Kind;
/// let ok = action(ui, "Merge", Kind::Primary, "ecad.merge.ok", Some("Choose for 2 conflicts first")).clicked();
/// let cancel = action(ui, "Cancel", Kind::Secondary, "ecad.merge.cancel", None).clicked();
/// # let _ = (ok, cancel);
/// # });
/// ```
pub fn action(ui: &mut Ui, label: &str, kind: Kind, name: &str, disabled: Option<&str>) -> egui::Response {
    let mut b = crate::prim::Button::new(label).kind(kind);
    if let Some(why) = disabled {
        b = b.disabled(why);
    }
    ui.add(b).accessible_name(name)
}

/// The close cross's accessible name.
pub const CLOSE: &str = "ecad.dialog.close";

impl<'a> Dialog<'a> {
    /// A dialog titled `title`, its accessible name `name`.
    pub fn new(id: Id, title: &'a str, name: &'a str) -> Self {
        Self { id, title, name, width: 480.0 }
    }

    /// The content's width, px. Prefer [`Self::small`] or [`Self::medium`].
    pub fn width(mut self, width: f32) -> Self {
        self.width = width;
        self
    }

    /// A small dialog, [`size::DIALOG_S`] wide: a question, a name to type.
    pub fn small(self) -> Self {
        self.width(size::DIALOG_S)
    }

    /// The usual dialog, [`size::DIALOG_M`] wide: a short form, a list to choose from.
    pub fn medium(self) -> Self {
        self.width(size::DIALOG_M)
    }

    /// Draw it centred over the dimmed page: `body` under the title, `footer` laid right to left
    /// in one control-high row at the bottom (put the primary button first).
    pub fn show<B, F>(self, ctx: &Context, body: impl FnOnce(&mut Ui) -> B, footer: impl FnOnce(&mut Ui) -> F) -> Shown<B, F> {
        let s = SHELL;
        let frame = Frame::new()
            .fill(color(s.panel))
            .stroke(Stroke::new(line::HAIRLINE_PX, color(s.line)))
            .corner_radius(corner(radius::POPOVER))
            .shadow(shadow())
            .inner_margin(Margin::same(space::L.round().clamp(0.0, 127.0) as i8));
        let name = self.name;
        let r = egui::Modal::new(self.id)
            // Opaque from its first frame: no fade-in that shows the page through it.
            .area(egui::Modal::default_area(self.id).fade_in(false))
            .backdrop_color(color(SCRIM))
            .frame(frame)
            .show(ctx, |ui| {
                ui.set_width(self.width);
                ui.interact(ui.max_rect(), self.id.with("ground"), egui::Sense::hover())
                    .widget_info(|| egui::WidgetInfo::labeled(egui::Role::Dialog, true, name));
                // The title and the cross.
                let cross = ui
                    .allocate_ui_with_layout(vec2(self.width, size::CONTROL), Layout::left_to_right(Align::Center), |ui| {
                        ui.label(egui::RichText::new(self.title).font(fonts::font(ui.ctx(), ui_type::TITLE)).color(color(s.text)));
                        ui.with_layout(Layout::right_to_left(Align::Center), |ui| ui.add(crate::prim::IconButton::new(Icon::Close, CLOSE)).clicked())
                            .inner
                    })
                    .inner;
                ui.add_space(space::S);
                let body = body(ui);
                ui.add_space(space::M);
                let footer = ui.allocate_ui_with_layout(vec2(self.width, size::CONTROL), Layout::right_to_left(Align::Center), footer).inner;
                (body, footer, cross)
            });
        let (body, footer, cross) = r.inner;
        let esc = r.is_top_modal && ctx.input_mut(|i| i.consume_key(Modifiers::NONE, Key::Escape));
        Shown { body, footer, closed: cross || esc }
    }
}
