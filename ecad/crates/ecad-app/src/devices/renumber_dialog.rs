//! The Renumber dialog, also Finalise numbering: what to number
//! (devices, cables, wires, all), which (all, the selection, a prefix or potential, a folder, the
//! provisional items), start and digits, and the preview of every row, old → new, before
//! anything is written. Apply runs `renumber` (or `finalise`) with the preview's change count:
//! one transaction; a stale preview is refused by the command and computed again here.
//!
//! Update map: the preview is computed when the dialog opens and when an option changes, never on
//! commits; a refused Apply asks for it again. Words are short: `Dev`,
//! `Cbl`, `Wire`, `Sel`, `Prov`, `At`, row marks `→ F = –`.

use ecad_electrical::numbering::commands::{Preview, RowChange};
use ecad_model::naming::numbering::Kind as RowKind;
use ecad_widgets::control::{self, Decor, Select, TextEntry};
use ecad_widgets::dialog::{Dialog, action};
use ecad_widgets::prim::{self, Checkbox, Kind, Radio, Segmented, Tone};
use ecad_widgets::tokens::{size, space};
use eframe::egui::{self, RichText};
use serde_json::{Value, json};

/// The dialog's window id.
pub(crate) const DIALOG_ID: &str = "ecad.renumber.dialog";
/// Row height of the preview list.
const ROW_H: f32 = 16.0;

/// What to number.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum What {
    Devices,
    Cables,
    Wires,
    /// Terminal labels per strip.
    TerminalBlocks,
    All,
}

impl What {
    fn word(self) -> &'static str {
        match self {
            What::Devices => "devices",
            What::Cables => "cables",
            What::Wires => "wires",
            What::TerminalBlocks => "terminal_blocks",
            What::All => "all",
        }
    }
}

/// Which items.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Which {
    All,
    Selection,
    Prefix,
    Folder,
    Provisional,
}

/// What the dialog asks the app to do.
#[derive(Debug, Clone, PartialEq)]
pub enum Out {
    /// Ask for the preview with these parameters (`renumber` or `finalise` with `preview`).
    Preview(&'static str, Value),
    /// Apply with these parameters.
    Apply(&'static str, Value),
    /// Show this row's anchor (page, point); the dialog stays open.
    Goto(ecad_model::PageId, [f64; 2]),
    /// Close without writing.
    Cancel,
}

/// The dialog's state.
#[derive(Debug, Clone, PartialEq)]
pub struct RenumberDialog {
    pub what: What,
    pub which: Which,
    /// Finalise numbering: all kinds, provisional, start `next` (read-only).
    pub finalise: bool,
    pub prefixes: Vec<String>,
    pub potentials: Vec<String>,
    pub prefix: String,
    /// `(folder hex, path)`.
    pub folders: Vec<(ecad_model::FolderId, String)>,
    pub folder: usize,
    /// The selection's devices, cables and wires (hex IDs), when opened.
    pub selection: Vec<String>,
    pub provisional: usize,
    pub start: String,
    pub digits: String,
    pub new_tags: bool,
    pub read_only: bool,
    /// The latest preview.
    pub preview: Preview,
    pub error: Option<String>,
}

impl RenumberDialog {
    /// A dialog with `choices` (from `ecad_electrical::numbering::commands::choices`), the selection's objects and the
    /// project's digits.
    pub fn new(choices: &ecad_electrical::numbering::commands::Choices, selection: Vec<String>, digits: u8, read_only: bool, finalise: bool) -> Self {
        let mut d = RenumberDialog {
            what: if finalise { What::All } else { What::Devices },
            which: if finalise { Which::Provisional } else { Which::All },
            finalise,
            prefixes: choices.prefixes.clone(),
            potentials: choices.potentials.clone(),
            prefix: choices.prefixes.first().cloned().unwrap_or_else(|| "-K".into()),
            folders: choices.folders.clone(),
            folder: 0,
            selection,
            provisional: choices.provisional,
            start: if finalise { "next".into() } else { "1".into() },
            digits: digits.to_string(),
            new_tags: false,
            read_only,
            preview: Preview::default(),
            error: None,
        };
        if finalise {
            d.start = "next".into();
        }
        d
    }

    /// Opened from Renumber Wires: Wires chosen.
    pub fn for_wires(mut self, digits: u8) -> Self {
        self.what = What::Wires;
        self.digits = digits.to_string();
        self
    }

    /// Opened from a folder row: Folder chosen.
    pub fn for_folder(mut self, folder: ecad_model::FolderId) -> Self {
        if let Some(i) = self.folders.iter().position(|(f, _)| *f == folder) {
            self.which = Which::Folder;
            self.folder = i;
        }
        self
    }

    /// The command and its parameters for a preview (`preview`) or an Apply.
    ///
    /// # Errors
    /// Start or digits are not whole numbers in range.
    pub fn params(&self, preview: bool) -> Result<(&'static str, Value), String> {
        let mut v = json!({});
        if preview {
            v["preview"] = json!(true);
        } else if self.preview != Preview::default() {
            v["based_on"] = json!(self.preview.change_count);
        }
        if self.finalise {
            return Ok(("finalise", v));
        }
        v["kind"] = json!(self.what.word());
        v["scope"] = match self.which {
            Which::All => json!("all"),
            Which::Provisional => json!("provisional"),
            Which::Selection => {
                v["objects"] = json!(self.selection);
                json!("selection")
            }
            Which::Prefix => json!({"prefix": self.prefix}),
            Which::Folder => json!({"folder": self.folders.get(self.folder).map(|f| f.0.hex()).unwrap_or_default()}),
        };
        if self.which != Which::Provisional || self.start.trim() != "next" {
            let s = ecad_electrical::input::renumber_start(&self.start).map_err(|e: ecad_electrical::input::InputError| e.to_string())?;
            v["start"] = json!(s);
        }
        let d = ecad_electrical::input::renumber_digits(&self.digits).map_err(|e| e.to_string())?;
        v["digits"] = json!(d);
        if self.new_tags {
            v["digits_for_new"] = json!(true);
        }
        Ok(("renumber", v))
    }

    fn ask(&self) -> Option<Out> {
        self.params(true).ok().map(|(c, v)| Out::Preview(c, v))
    }

    /// The dialog.
    pub fn show(&mut self, ctx: &egui::Context) -> Option<Out> {
        let title = if self.finalise { "Finalise numbering" } else { "Renumber" };
        let n = self.preview.changes;
        let blocked = if n == 0 { Some("Nothing to change.") } else { self.params(false).is_err().then_some("Fix Start and Digits first.") };
        let read_only = self.read_only;
        let shown = Dialog::new(egui::Id::unique(DIALOG_ID), title, DIALOG_ID).medium().show(
            ctx,
            |ui| self.body(ui),
            |ui| {
                // Laid right to left: the primary first.
                let apply = !read_only && action(ui, &format!("Apply {n}"), Kind::Primary, "ecad.renumber.apply", blocked).clicked();
                let cancel = action(ui, "Cancel", Kind::Secondary, "ecad.renumber.cancel", None).clicked();
                (apply, cancel)
            },
        );
        let (apply, cancel) = shown.footer;
        if apply && let Ok((c, v)) = self.params(false) {
            return Some(Out::Apply(c, v));
        }
        if cancel || shown.closed {
            return Some(Out::Cancel);
        }
        match shown.body {
            (Some(o), _) => Some(o),
            (None, true) => self.ask(),
            (None, false) => None,
        }
    }

    /// The options and the preview. Returns a go-to, and whether an option changed.
    fn body(&mut self, ui: &mut egui::Ui) -> (Option<Out>, bool) {
        let mut out = None;
        let mut changed = false;
        ui.spacing_mut().item_spacing.y = space::XS;
        let fixed = self.finalise;
        ui.add_enabled_ui(!fixed, |ui| {
            prim::property_row(ui, "What", |ui| {
                const WHAT: [What; 5] = [What::Devices, What::Cables, What::Wires, What::TerminalBlocks, What::All];
                let mut k = WHAT.iter().position(|w| *w == self.what).unwrap_or(0);
                if Segmented::new(&mut k, &["Devices", "Cables", "Wires", "Terminal blocks", "All"])
                    .name("ecad.renumber.kind")
                    .show(ui)
                    .clicked
                    .is_some()
                {
                    let w = WHAT.get(k).copied().unwrap_or(What::Devices);
                    self.what = w;
                    if w == What::Wires && self.which == Which::Prefix {
                        self.prefix = self.potentials.first().cloned().unwrap_or_default();
                    }
                    changed = true;
                }
            });
            prim::property_row(ui, "Which", |ui| {
                ui.horizontal_wrapped(|ui| {
                    if Radio::new(self.which == Which::All, "All").show(ui).accessible_name("ecad.renumber.scope.all").clicked() {
                        self.which = Which::All;
                        changed = true;
                    }
                    let n = self.selection.len();
                    let label = format!("Selection ({n})");
                    let sel = ui.add_enabled_ui(n > 0, |ui| Radio::new(self.which == Which::Selection, &label).show(ui)).inner;
                    if sel.accessible_name("ecad.renumber.scope.selection").clicked() {
                        self.which = Which::Selection;
                        changed = true;
                    }
                    let prov = format!("Provisional ({})", self.provisional);
                    if Radio::new(self.which == Which::Provisional, &prov).show(ui).accessible_name("ecad.renumber.scope.provisional").clicked() {
                        self.which = Which::Provisional;
                        changed = true;
                    }
                });
            });
            prim::property_row(ui, "", |ui| {
                if Radio::new(self.which == Which::Prefix, "Prefix").show(ui).accessible_name("ecad.renumber.scope.prefix").clicked() {
                    self.which = Which::Prefix;
                    changed = true;
                }
                let list = if self.what == What::Wires { self.potentials.clone() } else { self.prefixes.clone() };
                let names: Vec<&str> = list.iter().map(String::as_str).collect();
                let mut k = list.iter().position(|p| *p == self.prefix).unwrap_or(usize::MAX);
                if Select::new(&mut k, &names, "ecad.renumber.prefix").width(PICK_WIDTH).show(ui).changed()
                    && let Some(p) = list.get(k)
                {
                    self.prefix = p.clone();
                    self.which = Which::Prefix;
                    changed = true;
                }
                ui.add_space(space::L);
                let has_folders = !self.folders.is_empty();
                let folder = ui.add_enabled_ui(has_folders, |ui| Radio::new(self.which == Which::Folder, "Folder").show(ui)).inner;
                if folder.accessible_name("ecad.renumber.scope.folder").clicked() {
                    self.which = Which::Folder;
                    changed = true;
                }
                let paths: Vec<&str> = self.folders.iter().map(|(_, p)| p.as_str()).collect();
                let mut k = self.folder;
                let r = ui.add_enabled_ui(has_folders, |ui| Select::new(&mut k, &paths, "ecad.renumber.folder").width(PICK_WIDTH).show(ui)).inner;
                if r.changed() && k < self.folders.len() {
                    self.folder = k;
                    self.which = Which::Folder;
                    changed = true;
                }
            });
        });
        prim::property_row(ui, "Order", |ui| control::read_text(ui, "page \u{b7} column \u{b7} row", Decor::default(), "Order"));
        prim::property_row(ui, "Start", |ui| {
            let next = self.finalise || (self.which == Which::Provisional && self.start.trim() == "next");
            let r = ui.add_enabled_ui(!next, |ui| TextEntry::new(&mut self.start, "ecad.renumber.start").width(NUMBER_WIDTH).show(ui)).inner;
            if r.on_disabled_hover_text("Each prefix continues after its highest number in use.").changed() {
                changed = true;
            }
            ui.add_space(space::L);
            prim::caption(ui, "DIGITS");
            let r =
                ui.add_enabled_ui(!self.finalise, |ui| TextEntry::new(&mut self.digits, "ecad.renumber.digits").width(NUMBER_WIDTH).show(ui)).inner;
            if r.changed() {
                changed = true;
            }
            if !self.finalise {
                ui.add_space(space::L);
                Checkbox::new(&mut self.new_tags, "Also for new tags").show(ui).accessible_name("ecad.renumber.newtags");
            }
        });
        if let Err(e) = self.params(true) {
            prim::note(ui, &e, Tone::Error);
        }
        ui.separator();
        let p = self.preview.clone();
        prim::body(ui, &p.summary, false);
        let all = self.what == What::All || self.finalise;
        let s = ecad_widgets::SHELL;
        let mono = |text: String, ink| RichText::new(text).monospace().color(ecad_widgets::color(ink));
        ui.label(mono(format!("{}{:<8} {:<3}{:<8} {:<10} Desc", if all { "     " } else { "" }, "Old", "", "New", "At"), s.text_3));
        let total = p.rows.len() + p.skipped.len();
        egui::ScrollArea::vertical().id_salt("ecad.renumber.rows").max_height(ROWS_HEIGHT).auto_shrink([false, true]).show_rows(
            ui,
            ROW_H,
            total,
            |ui, range| {
                for i in range {
                    // The numbered rows, then the ones left alone.
                    let (kind, old, new, mark, at, why, skipped, goto) = match p.rows.get(i) {
                        Some(r) => {
                            let new = if r.change == RowChange::Skipped { String::new() } else { r.new.clone() };
                            let goto = r.page.zip(r.point).map(|(page, pt)| Out::Goto(page, pt));
                            (r.kind, r.old.as_str(), new, r.change.glyph(), r.at.as_str(), r.desc.as_str(), false, goto)
                        }
                        None => match p.skipped.get(i - p.rows.len()) {
                            Some(r) => (r.kind, r.tag.as_str(), String::new(), RowChange::Skipped.glyph(), "", r.why.as_str(), true, None),
                            None => continue,
                        },
                    };
                    let kind = match kind {
                        RowKind::Devices => "Dev  ",
                        RowKind::Cables => "Cbl  ",
                        RowKind::TerminalBlocks => "Trm  ",
                        RowKind::Wires => "Wire ",
                    };
                    let text = format!("{}{old:<8} {mark:<3}{new:<8} {at:<10} {why}", if all { kind } else { "" });
                    // A provisional tag in the provisional colour, as everywhere.
                    let ink = if old.contains('?') && !skipped { s.provisional } else { s.text };
                    let resp = ui.add(egui::Label::new(mono(text, ink)).sense(egui::Sense::click()));
                    resp.widget_info(|| egui::WidgetInfo::labeled(egui::Role::Button, true, format!("ecad.renumber.row.{i}")));
                    if resp.clicked()
                        && let Some(g) = goto
                    {
                        out = Some(g);
                    }
                }
            },
        );
        if p.more > 0 {
            prim::body(ui, &format!("\u{2026} {} more", p.more), true);
        }
        if self.read_only {
            prim::note(ui, "Read-only: preview only.", Tone::Neutral);
        }
        if let Some(e) = &self.error {
            prim::note(ui, e, Tone::Error);
        }
        (out, changed)
    }
}

/// The height of the preview's rows.
const ROWS_HEIGHT: f32 = size::DIALOG_S - size::ROW_GROUP * 2.0;
/// The width of the prefix and folder pickers.
const PICK_WIDTH: f32 = size::LABEL_COLUMN + size::CONTROL;
/// The width of Start and Digits.
const NUMBER_WIDTH: f32 = size::LABEL_COLUMN * 0.75;
