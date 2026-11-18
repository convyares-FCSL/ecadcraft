//! The Page tab on a generated page (brief 054b, design 054 §8; board 1.16's right panel): on the
//! cover, a row per block with its summary, **Edit…** (the shared table editor,
//! [`crate::shell::table_editor`]) and its toggle; on a cover, revision history or contents page,
//! the generated pages with their toggles; on any generated page, the sheet read from its type.
//!
//! Each toggle is one `setdefault {key, level: "project", value}`; the editor's Save is one
//! `projectprops {props: {…}}` (the notes, or the wire colours' or legend's rows); a generated
//! page's toggle adds it (`newpage {type: "front", generated, after}`) or removes every page of
//! that kind (`deletepage {pages}`), one transaction each.
//!
//! # Update map (design 054 §10.2)
//! The tab's copy ([`PageView`]) is read again only when the project's change count moves or
//! another page is shown.

use ecad_cadcraft::front::Block;
use ecad_cadcraft::generated::GenKind;
use ecad_model::PageId;
use ecad_ui::tokens::{space, ui_type};
use eframe::egui;
use serde_json::json;

use super::widgets;
use crate::App;

/// Accessible names (design 054 §8).
pub const NOTES: &str = "ecad.page.notes";
/// A block's Edit… button: `ecad.page.edit.<block>`.
pub fn edit_name(b: Block) -> String {
    format!("ecad.page.edit.{}", b.entry().key.trim_start_matches("front.cover."))
}
pub const GOTO_TYPE: &str = "ecad.page.sheet.goto";

/// One front kind's row.
#[derive(Debug, Clone, PartialEq)]
pub struct FrontRow {
    pub kind: GenKind,
    pub pages: Vec<(PageId, String)>,
}

/// What the tab shows of the page on screen.
#[derive(Debug, Clone, PartialEq)]
pub struct PageView {
    pub page: PageId,
    pub at: u64,
    pub kind: GenKind,
    pub title: String,
    pub number: String,
    pub type_id: String,
    pub type_name: String,
    pub title_block: String,
    pub blocks: Vec<(Block, bool)>,
    pub notes: String,
    /// The wire colours' and the legend's rows with the person's overrides (design 054 §8.1a).
    pub wire_rows: Vec<ecad_cadcraft::front::EditedRow>,
    pub legend_rows: Vec<ecad_cadcraft::front::EditedRow>,
    /// The issued revisions, oldest first: `A · First issue · CvA · 2026-11-14`.
    pub revisions: Vec<String>,
    pub front: Vec<FrontRow>,
    /// The last front page in book order (a front page turned on goes after it).
    pub last_front: Option<PageId>,
}

/// The tab's state, kept while another page is shown.
#[derive(Debug, Default)]
pub struct PageTab {
    pub view: Option<PageView>,
    /// The notes being typed (`None`: showing the stored ones).
    pub notes_edit: Option<String>,
    pub error: Option<String>,
    /// The table editor while it is open (design 054 §8.1a).
    pub editor: Option<crate::shell::table_editor::TableEditor>,
    /// The sections folded.
    pub folded: std::collections::BTreeSet<String>,
}

/// Read the tab's copy for `page`, when it is generated.
fn read(t: &mut App, page: PageId) -> Option<PageView> {
    let p = t.project.clone()?;
    p.read(&mut t.cad.session, |p, _| {
        let doc = p.ledger.document();
        let kind = ecad_cadcraft::generated::kind_of(doc, page)?;
        let book = ecad_model::book::Book::of(doc);
        let ty = ecad_cadcraft::styles::page::type_of(doc, page);
        let sheet = ecad_model::sheet::sheet_of(doc, page);
        let front = [GenKind::Cover, GenKind::Revisions, GenKind::Contents]
            .into_iter()
            .map(|k| FrontRow {
                kind: k,
                pages: book
                    .order
                    .iter()
                    .filter(|x| ecad_cadcraft::generated::kind_of(doc, **x) == Some(k))
                    .map(|x| (*x, book.number(doc, *x)))
                    .collect(),
            })
            .collect();
        let last_front = book
            .order
            .iter()
            .rev()
            .find(|x| {
                ecad_cadcraft::styles::page::look_of(doc, &ecad_cadcraft::styles::page::type_of(doc, **x)).holds
                    == ecad_cadcraft::styles::page::Holds::Front
            })
            .copied();
        Some(PageView {
            page,
            at: p.change_count(),
            kind,
            title: doc.pages.get(&page).map(|x| x.title.to_string()).unwrap_or_default(),
            number: book.number(doc, page),
            type_name: ecad_cadcraft::styles::name(doc, ecad_model::StyleCatalogue::Page, &ty),
            type_id: ty.text(),
            title_block: crate::settings::pages::title_block_words(&sheet),
            blocks: Block::ALL.iter().map(|b| (*b, b.on(doc))).collect(),
            notes: ecad_cadcraft::frames::project_text(doc, ecad_cadcraft::front::NOTES_KEY).unwrap_or_default(),
            wire_rows: if kind == GenKind::Cover { ecad_cadcraft::front::wire_rows(doc).into_iter().map(|(_, r)| r).collect() } else { Vec::new() },
            legend_rows: if kind == GenKind::Cover {
                ecad_cadcraft::front::legend_rows(doc).into_iter().map(|(_, r)| r).collect()
            } else {
                Vec::new()
            },
            revisions: ecad_cadcraft::revisions::issued(doc)
                .iter()
                .rev()
                .map(|r| {
                    [r.rev(), r.get("description"), r.get("drawn"), r.get("date")]
                        .into_iter()
                        .filter(|x| !x.is_empty())
                        .collect::<Vec<_>>()
                        .join(" · ")
                })
                .collect(),
            front,
            last_front,
        })
    })
    .ok()
    .flatten()
}

/// The page's copy, read again when the project changed or another page is shown. `None` on a
/// drawn page.
pub fn current(t: &mut App) -> Option<PageView> {
    let page = t.panel.active?;
    let p = t.project.clone()?;
    let at = p.read(&mut t.cad.session, |p, _| p.change_count()).ok()?;
    let fresh = t.settings.page_tab.view.as_ref().is_some_and(|v| v.page == page && v.at == at);
    if !fresh {
        t.settings.page_tab.view = read(t, page);
        if t.settings.page_tab.view.as_ref().is_none_or(|v| v.page != page) {
            t.settings.page_tab.notes_edit = None;
        }
    }
    t.settings.page_tab.view.clone()
}

/// The header's words: `Cover sheet · generated · page 01`.
pub fn head_words(v: &PageView) -> (String, String) {
    (v.title.clone(), format!("{} · generated · page {}", v.type_name, v.number))
}

/// What the tab asked for.
enum Do {
    Block(Block, bool),
    Edit(Block),
    Notes(String),
    Rows(&'static str, String),
    Revisions,
    Front(GenKind, bool),
    Settings(String),
}

/// The tab's body.
pub fn show(t: &mut App, ui: &mut egui::Ui, v: &PageView, writable: bool, full: egui::Rangef) {
    let mut todo = Vec::new();
    let s = ecad_ui::SHELL;
    let mut folded = std::mem::take(&mut t.settings.page_tab.folded);
    if v.kind == GenKind::Cover {
        let on = v.blocks.iter().filter(|(_, o)| *o).count();
        let summary = format!("{on} of {} blocks", v.blocks.len());
        widgets::section(ui, &mut folded, full, "page.blocks", "On this page", &summary, false, |ui| {
            for (b, on) in &v.blocks {
                let label = b.entry().label;
                ui.horizontal(|ui| {
                    ui.set_min_height(32.0);
                    ui.label(widgets::rich(ui, label, ui_type::BODY, widgets::text()));
                    let summary = summary_of(v, *b);
                    if !summary.is_empty() {
                        ui.label(widgets::rich(ui, &summary, ui_type::SMALL, widgets::text_3()));
                    }
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        let name = format!("ecad.page.block.{}", b.entry().key.trim_start_matches("front.cover."));
                        if crate::settings::pages::switch(ui, *on, writable, &name).clicked() {
                            todo.push(Do::Block(*b, !*on));
                        }
                        if editable(*b) {
                            let e = ui.add(ecad_ui::prim::Button::new("Edit\u{2026}").small());
                            e.widget_info(|| egui::WidgetInfo::labeled(egui::Role::Button, true, edit_name(*b)));
                            if e.clicked() {
                                todo.push(Do::Edit(*b));
                            }
                        }
                    });
                });
            }
        });
    }
    if ecad_cadcraft::styles::page::holds_of_kind(v.kind) == ecad_cadcraft::styles::page::Holds::Front {
        widgets::section(ui, &mut folded, full, "page.front", "Generated pages", "in the Book", false, |ui| {
            for row in &v.front {
                ui.horizontal(|ui| {
                    ui.set_min_height(32.0);
                    ui.label(widgets::rich(ui, row.kind.title(), ui_type::BODY, widgets::text()));
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        let on = !row.pages.is_empty();
                        if row.kind != GenKind::Cover {
                            let name = format!("ecad.page.front.{}", ecad_model::Word::word(row.kind));
                            if crate::settings::pages::switch(ui, on, writable, &name).clicked() {
                                todo.push(Do::Front(row.kind, !on));
                            }
                            ui.add_space(space::S);
                        }
                        let numbers: Vec<&str> = row.pages.iter().map(|(_, n)| n.as_str()).collect();
                        ui.label(widgets::rich(ui, numbers.join(", "), ui_type::BODY, widgets::text_2()));
                    });
                });
            }
        });
    }
    widgets::section(ui, &mut folded, full, "page.sheet", "Sheet", "from page type", false, |ui| {
        // The type with a small Edit… like the block rows: Settings ▸ Pages and sheets at it.
        widgets::row(ui, "Type", |ui| {
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                let b = ui.add(ecad_ui::prim::Button::new("Edit\u{2026}").small()).on_hover_text("Settings \u{25b8} Pages and sheets");
                b.widget_info(|| egui::WidgetInfo::labeled(egui::Role::Button, true, GOTO_TYPE));
                if b.clicked() {
                    todo.push(Do::Settings(v.type_id.clone()));
                }
                ui.with_layout(egui::Layout::left_to_right(egui::Align::Center), |ui| {
                    ui.add(egui::Label::new(widgets::rich(ui, &v.type_name, ui_type::BODY, widgets::text_2())).truncate())
                        .widget_info(|| egui::WidgetInfo::labeled(egui::Role::Label, true, "ecad.page.sheet.type"));
                });
            });
        });
        widgets::read(ui, "Title block", &v.title_block, "ecad.page.sheet.titleblock");
    });
    t.settings.page_tab.folded = folded;
    // The shared table editor (design 054 §8.1a).
    if let Some(mut ed) = t.settings.page_tab.editor.take() {
        let what = ed.what;
        let out = ed.show(ui.ctx(), |rows| check_rows(what, rows));
        match out {
            crate::shell::table_editor::Outcome::None => t.settings.page_tab.editor = Some(ed),
            crate::shell::table_editor::Outcome::Cancel => {}
            crate::shell::table_editor::Outcome::Link => todo.push(Do::Revisions),
            crate::shell::table_editor::Outcome::Save => {
                match what {
                    "notes" => todo.push(Do::Notes(notes_text(&ed.rows))),
                    "wire_colours" => todo.push(Do::Rows(ecad_cadcraft::front::WIRE_ROWS_KEY, rows_text(&ed.rows))),
                    _ => todo.push(Do::Rows(ecad_cadcraft::front::LEGEND_ROWS_KEY, rows_text(&ed.rows))),
                }
                // Kept open until the write is accepted (a refusal shows in it).
                t.settings.page_tab.editor = Some(ed);
            }
        }
    }
    if let Some(e) = &t.settings.page_tab.error {
        ui.label(egui::RichText::new(e).small().color(ecad_ui::color(s.error)));
    }
    for d in todo {
        act(t, v, d);
    }
}

/// A block's grey summary on its row: `4 lines`, `6 rows`, `3 issued`.
fn summary_of(v: &PageView, b: Block) -> String {
    let n = |k: usize, one: &str, many: &str| format!("{k} {}", if k == 1 { one } else { many });
    match b {
        Block::Notes => n(v.notes.lines().filter(|l| !l.trim().is_empty()).count(), "line", "lines"),
        Block::WireColours => n(v.wire_rows.iter().filter(|r| r.shown).count(), "row", "rows"),
        Block::SymbolLegend => n(v.legend_rows.iter().filter(|r| r.shown).count(), "row", "rows"),
        Block::Revisions => n(v.revisions.len(), "issued", "issued"),
        Block::StructureCodes | Block::CustomerSite => String::new(),
    }
}

/// Whether a block has a table to edit.
fn editable(b: Block) -> bool {
    matches!(b, Block::Notes | Block::WireColours | Block::SymbolLegend | Block::Revisions)
}

/// The editor of block `b`, filled from the page's copy.
fn editor_of(v: &PageView, b: Block, writable: bool) -> Option<crate::shell::table_editor::TableEditor> {
    use crate::shell::table_editor::{Caps, Row, TableEditor};
    let generated = |rows: &[ecad_cadcraft::front::EditedRow]| -> Vec<Row> {
        rows.iter().map(|r| Row { key: r.key.clone(), text: r.label.clone(), generated: r.generated.clone(), shown: r.shown }).collect()
    };
    Some(match b {
        Block::Notes => {
            let rows = v
                .notes
                .lines()
                .map(str::trim)
                .filter(|l| !l.is_empty())
                .map(|l| Row { text: l.to_string(), shown: true, ..Row::default() })
                .collect();
            let mut e = TableEditor::new("notes", "Notes", rows, Caps::TYPED, writable);
            e.max_rows = ecad_cadcraft::front::MAX_NOTES;
            e
        }
        Block::WireColours => TableEditor::new("wire_colours", "Wire colours", generated(&v.wire_rows), Caps::GENERATED, writable),
        Block::SymbolLegend => TableEditor::new("symbol_legend", "Symbol legend", generated(&v.legend_rows), Caps::GENERATED, writable),
        Block::Revisions => {
            let rows = v.revisions.iter().map(|t| Row { text: t.clone(), shown: true, ..Row::default() }).collect();
            let mut e = TableEditor::new("revisions", "Revisions", rows, Caps::READ, writable);
            e.link = Some("Edit in Revisions\u{2026}");
            e
        }
        Block::StructureCodes | Block::CustomerSite => return None,
    })
}

/// Whether the editor's rows may be saved.
fn check_rows(what: &str, rows: &[crate::shell::table_editor::Row]) -> Result<(), String> {
    match what {
        "notes" => ecad_cadcraft::front::check_notes(&notes_text(rows)).map_err(String::from),
        "revisions" => Ok(()),
        _ => ecad_cadcraft::front::check_overrides(&rows_text(rows)).map_err(String::from),
    }
}

/// The notes as stored: one line per row, empty rows left out.
fn notes_text(rows: &[crate::shell::table_editor::Row]) -> String {
    rows.iter().map(|r| r.text.trim()).filter(|t| !t.is_empty()).collect::<Vec<_>>().join("\n")
}

/// A generated block's rows as stored (design 054 §8.1a): an empty label is the generated one.
fn rows_text(rows: &[crate::shell::table_editor::Row]) -> String {
    let edited: Vec<ecad_cadcraft::front::EditedRow> = rows
        .iter()
        .map(|r| {
            let label = if r.text.trim().is_empty() { r.generated.clone() } else { r.text.trim().to_string() };
            ecad_cadcraft::front::EditedRow { key: r.key.clone(), generated: r.generated.clone(), label, shown: r.shown }
        })
        .collect();
    ecad_cadcraft::front::overrides_text(&edited)
}

fn act(t: &mut App, v: &PageView, d: Do) {
    let writable = t.writable();
    let r = match d {
        Do::Edit(b) => {
            t.settings.page_tab.editor = editor_of(v, b, writable);
            Ok(())
        }
        Do::Revisions => {
            t.settings.page_tab.editor = None;
            t.open_revisions();
            Ok(())
        }
        Do::Rows(key, text) => {
            let value = if text.trim().is_empty() { serde_json::Value::Null } else { serde_json::Value::String(text) };
            let r = t.run_value("projectprops", json!({"props": {key: value}})).map(|_| ());
            if r.is_ok() {
                t.settings.page_tab.editor = None;
            }
            r
        }
        Do::Block(b, on) => {
            t.run_value("setdefault", json!({"key": b.entry().key, "level": "project", "value": ecad_model::words::flag_word(on)})).map(|_| ())
        }
        Do::Notes(text) => {
            let value = if text.trim().is_empty() { serde_json::Value::Null } else { serde_json::Value::String(text) };
            let r = t.run_value("projectprops", json!({"props": {ecad_cadcraft::front::NOTES_KEY: value}})).map(|_| ());
            if r.is_ok() {
                t.settings.page_tab.editor = None;
            }
            r
        }
        Do::Front(kind, true) => {
            let mut p = json!({"type": "front", "generated": ecad_model::Word::word(kind)});
            if let Some(a) = v.last_front {
                p["after"] = json!(a.hex());
            }
            let back = v.page;
            let r = t.run_value("newpage", p).map(|_| ());
            // Stay on the page holding the toggles.
            let _ = t.run_value("openpage", json!({"page": back.hex()}));
            r
        }
        Do::Front(kind, false) => {
            let pages: Vec<String> = v.front.iter().filter(|f| f.kind == kind).flat_map(|f| f.pages.iter().map(|(p, _)| p.hex())).collect();
            t.run_value("deletepage", json!({"pages": pages})).map(|_| ())
        }
        Do::Settings(id) => {
            t.open_settings(crate::settings::Section::Pages);
            t.settings.pages.selected = ecad_model::StyleRef::parse(&id);
            Ok(())
        }
    };
    match r {
        Ok(()) => t.settings.page_tab.error = None,
        Err(e) => {
            t.cad.session.echo(e.clone());
            if let Some(ed) = &mut t.settings.page_tab.editor {
                ed.error = Some(e.clone());
            }
            t.settings.page_tab.error = Some(e);
        }
    }
}
