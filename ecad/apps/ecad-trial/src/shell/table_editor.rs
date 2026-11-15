//! One reusable table editor popup (brief 054, design 054 §8.1a): a title, rows of text, and Save
//! and Cancel. What a caller may do with the rows is set by [`Caps`]: add, remove and edit them
//! (the cover's notes), hide, reorder and relabel generated rows (the wire colours and the symbol
//! legend), or only read them with a link to where they are edited (the revisions).
//!
//! The editor writes nothing itself: [`TableEditor::show`] returns [`Outcome::Save`] and the caller
//! turns the rows into one command (one transaction, one `u`). Esc and Cancel write nothing.

use ecad_ui::tokens::{space, ui_type};
use eframe::egui::{self, Key, Modifiers, vec2};

use crate::shell::bar;

/// Accessible names.
pub const EDITOR: &str = "ecad.table.editor";
pub const SAVE: &str = "ecad.table.save";
pub const CANCEL: &str = "ecad.table.cancel";
pub const ADD: &str = "ecad.table.add";
pub const LINK: &str = "ecad.table.link";

/// What the person may do with the rows.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Caps {
    /// Add and remove rows (a typed list).
    pub add_remove: bool,
    /// Edit a row's text.
    pub edit: bool,
    /// Move rows up and down.
    pub reorder: bool,
    /// Hide and show a row (a generated list).
    pub hide: bool,
}

impl Caps {
    /// A typed list: add, remove, reorder and edit.
    pub const TYPED: Caps = Caps { add_remove: true, edit: true, reorder: true, hide: false };
    /// A generated list: hide, reorder and relabel; nothing added or removed.
    pub const GENERATED: Caps = Caps { add_remove: false, edit: true, reorder: true, hide: true };
    /// Read only.
    pub const READ: Caps = Caps { add_remove: false, edit: false, reorder: false, hide: false };
}

/// One row.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Row {
    /// The caller's key (a generated row's), kept with the row through moves.
    pub key: String,
    /// The text shown and edited.
    pub text: String,
    /// The generated words, shown grey when the text differs (empty for a typed row).
    pub generated: String,
    pub shown: bool,
}

/// What the popup asked for this frame.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Outcome {
    None,
    Save,
    Cancel,
    /// The read-only link (`Edit in Revisions…`).
    Link,
}

/// The editor while it is open.
#[derive(Debug, Clone, PartialEq)]
pub struct TableEditor {
    /// What it edits, for the caller (`notes`, `wire_colours`…).
    pub what: &'static str,
    pub title: String,
    pub rows: Vec<Row>,
    pub caps: Caps,
    /// Most rows (add stops there).
    pub max_rows: usize,
    /// The read-only link's words, if any.
    pub link: Option<&'static str>,
    /// Why Save is refused (the caller's check, or the command's words).
    pub error: Option<String>,
    /// Save enabled (writes allowed).
    pub writable: bool,
}

impl TableEditor {
    /// An editor over `rows`.
    pub fn new(what: &'static str, title: impl Into<String>, rows: Vec<Row>, caps: Caps, writable: bool) -> TableEditor {
        TableEditor { what, title: title.into(), rows, caps, max_rows: 64, link: None, error: None, writable }
    }

    /// Draw it and answer the person's clicks and keys.
    pub fn show(&mut self, ctx: &egui::Context, check: impl Fn(&[Row]) -> Result<(), String>) -> Outcome {
        let s = ecad_ui::SHELL;
        let mut out = Outcome::None;
        if ctx.input_mut(|i| i.consume_key(Modifiers::NONE, Key::Escape)) {
            return Outcome::Cancel;
        }
        let checked = check(&self.rows);
        let edit = self.caps.edit && self.writable;
        let mut open = true;
        egui::Window::new(self.title.clone())
            .id(egui::Id::unique(EDITOR))
            .collapsible(false)
            .resizable(true)
            .default_width(560.0)
            .open(&mut open)
            .anchor(egui::Align2::CENTER_CENTER, vec2(0.0, 0.0))
            .order(egui::Order::Foreground)
            .show(ctx, |ui| {
                ui.interact(ui.max_rect(), egui::Id::unique((EDITOR, "ground")), egui::Sense::hover())
                    .widget_info(|| egui::WidgetInfo::labeled(egui::Role::Dialog, true, EDITOR));
                let mut remove = None;
                let mut swap = None;
                egui::ScrollArea::vertical().max_height(420.0).auto_shrink([false, true]).show(ui, |ui| {
                    let n = self.rows.len();
                    for (i, r) in self.rows.iter_mut().enumerate() {
                        ui.horizontal(|ui| {
                            ui.set_min_height(28.0);
                            let (num, _) = ui.allocate_exact_size(vec2(22.0, 22.0), egui::Sense::hover());
                            ui.painter().text(
                                num.center(),
                                egui::Align2::CENTER_CENTER,
                                format!("{}", i + 1),
                                bar::font(ui, ui_type::SMALL),
                                ecad_ui::color(s.text_3),
                            );
                            if self.caps.hide {
                                let name = format!("ecad.table.shown.{i}");
                                ecad_ui::control::Toggle::new(&mut r.shown, &name).read_only(!self.writable).show(ui);
                            }
                            // Right to left: the buttons at the end, then the entry takes the rest,
                            // so the rows never widen the window.
                            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                                let quiet = |t| ecad_ui::prim::Button::new(t).kind(ecad_ui::prim::Kind::Quiet).small();
                                if self.caps.add_remove && self.writable && ui.add(quiet("\u{d7}")).on_hover_text("Remove").clicked() {
                                    remove = Some(i);
                                }
                                if self.caps.reorder && self.writable {
                                    if ui.add_enabled(i + 1 < n, quiet("\u{2193}")).on_hover_text("Down").clicked() {
                                        swap = Some((i, i + 1));
                                    }
                                    if ui.add_enabled(i > 0, quiet("\u{2191}")).on_hover_text("Up").clicked() {
                                        swap = Some((i, i - 1));
                                    }
                                }
                                let name = format!("ecad.table.row.{i}");
                                if edit {
                                    ecad_ui::control::TextEntry::new(&mut r.text, &name).show(ui);
                                } else {
                                    ecad_ui::control::read_text(ui, &r.text, ecad_ui::control::Decor::default(), &name);
                                }
                            });
                        });
                        if !r.generated.is_empty() && r.text != r.generated {
                            ui.label(egui::RichText::new(format!("generated: {}", r.generated)).small().color(ecad_ui::color(s.text_3)));
                        }
                    }
                    if self.rows.is_empty() {
                        ui.label(egui::RichText::new("Nothing yet.").color(ecad_ui::color(s.text_3)));
                    }
                });
                if let Some((a, b)) = swap {
                    self.rows.swap(a, b);
                }
                if let Some(i) = remove
                    && i < self.rows.len()
                {
                    self.rows.remove(i);
                }
                ui.add_space(space::S);
                ui.horizontal(|ui| {
                    if self.caps.add_remove && self.writable {
                        let b = ui.add_enabled(self.rows.len() < self.max_rows, ecad_ui::prim::Button::new("+ Row").small());
                        b.widget_info(|| egui::WidgetInfo::labeled(egui::Role::Button, true, ADD));
                        if b.clicked() {
                            self.rows.push(Row { shown: true, ..Row::default() });
                        }
                    }
                    if let Some(words) = self.link {
                        let b = ecad_ui::control::link(ui, words);
                        b.widget_info(|| egui::WidgetInfo::labeled(egui::Role::Link, true, LINK));
                        if b.clicked() {
                            out = Outcome::Link;
                        }
                    }
                });
                let why = checked.as_ref().err().cloned().or_else(|| self.error.clone());
                if let Some(e) = &why {
                    ui.label(egui::RichText::new(e).small().color(ecad_ui::color(s.error)));
                }
                ui.add_space(space::S);
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    let read_only = self.caps == Caps::READ;
                    if !read_only {
                        let can = self.writable && checked.is_ok();
                        let b = ui.add_enabled(can, ecad_ui::prim::Button::new("Save").kind(ecad_ui::prim::Kind::Primary).small());
                        b.widget_info(|| egui::WidgetInfo::labeled(egui::Role::Button, can, SAVE));
                        if b.clicked() || (can && ui.input(|i| i.modifiers.command && i.key_pressed(Key::Enter))) {
                            out = Outcome::Save;
                        }
                    }
                    let c = ui.add(ecad_ui::prim::Button::new(if read_only { "Close" } else { "Cancel" }).small());
                    c.widget_info(|| egui::WidgetInfo::labeled(egui::Role::Button, true, CANCEL));
                    if c.clicked() {
                        out = Outcome::Cancel;
                    }
                });
            });
        if !open {
            return Outcome::Cancel;
        }
        out
    }
}
