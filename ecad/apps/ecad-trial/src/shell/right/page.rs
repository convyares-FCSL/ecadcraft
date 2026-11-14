//! The Page tab on a generated page (brief 054b, design 054 §8; board 1.16's right panel): on the
//! cover, the block toggles and the notes; on any front page, the front pages with their toggles;
//! on any generated page, the sheet read from its page type.
//!
//! Each toggle is one `setdefault {key, level: "project", value}`; the notes write on blur or
//! Ctrl Enter (`projectprops {props: {"cover.notes": …}}`), Esc reverts; a front page's toggle adds
//! it (`newpage {type: "front", generated, after}`) or removes every page of that kind
//! (`deletepage {pages}`), one transaction each.
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
use crate::Trial;

/// Accessible names (design 054 §8).
pub const NOTES: &str = "ecad.page.notes";
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
    /// The sections folded.
    pub folded: std::collections::BTreeSet<String>,
}

/// Read the tab's copy for `page`, when it is generated.
fn read(t: &mut Trial, page: PageId) -> Option<PageView> {
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
            title: doc.pages.get(&page).map(|x| x.title.clone()).unwrap_or_default(),
            number: book.number(doc, page),
            type_name: ecad_cadcraft::styles::name(doc, ecad_cadcraft::styles::page::WORD, &ty),
            type_id: ty.text(),
            title_block: crate::settings::pages::title_block_words(&sheet),
            blocks: Block::ALL.iter().map(|b| (*b, b.on(doc))).collect(),
            notes: doc.properties.get(ecad_cadcraft::front::NOTES_KEY).cloned().unwrap_or_default(),
            front,
            last_front,
        })
    })
    .ok()
    .flatten()
}

/// The page's copy, read again when the project changed or another page is shown. `None` on a
/// drawn page.
pub fn current(t: &mut Trial) -> Option<PageView> {
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

/// The header's words: `Front sheet · generated · page 01`.
pub fn head_words(v: &PageView) -> (String, String) {
    (v.title.clone(), format!("{} · generated · page {}", v.type_name, v.number))
}

/// What the tab asked for.
enum Do {
    Block(Block, bool),
    Notes(String),
    Front(GenKind, bool),
    Settings(String),
}

/// The tab's body.
pub fn show(t: &mut Trial, ui: &mut egui::Ui, v: &PageView, writable: bool, full: egui::Rangef) {
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
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        let name = format!("ecad.page.block.{}", b.entry().key.trim_start_matches("front.cover."));
                        if crate::settings::pages::switch(ui, *on, writable, &name).clicked() {
                            todo.push(Do::Block(*b, !*on));
                        }
                    });
                });
                if *b == Block::Notes && *on {
                    let pt = &mut t.settings.page_tab;
                    let mut text = pt.notes_edit.clone().unwrap_or_else(|| v.notes.clone());
                    let invalid = ecad_cadcraft::front::check_notes(&text).is_err();
                    let stroke =
                        if invalid { egui::Stroke::new(1.0, ecad_ui::color(s.error)) } else { egui::Stroke::new(1.0, ecad_ui::color(s.line)) };
                    let r = egui::Frame::new()
                        .stroke(stroke)
                        .corner_radius(4.0)
                        .inner_margin(4)
                        .show(ui, |ui| {
                            ui.add_enabled(
                                writable,
                                egui::TextEdit::multiline(&mut text)
                                    .desired_rows(4)
                                    .desired_width(ui.available_width())
                                    .hint_text("One note per line"),
                            )
                        })
                        .inner;
                    r.widget_info(|| egui::WidgetInfo::labeled(egui::Role::TextInput, writable, NOTES));
                    let esc = r.has_focus() && ui.input(|i| i.key_pressed(egui::Key::Escape));
                    let ctrl_enter = r.has_focus() && ui.input(|i| i.modifiers.command && i.key_pressed(egui::Key::Enter));
                    if esc {
                        pt.notes_edit = None;
                        r.surrender_focus();
                    } else if r.changed() || pt.notes_edit.is_some() {
                        pt.notes_edit = Some(text.clone());
                    }
                    if invalid {
                        ui.label(egui::RichText::new(ecad_cadcraft::front::NOTES_TOO_LONG).small().color(ecad_ui::color(s.error)));
                    }
                    if (r.lost_focus() || ctrl_enter) && !esc && !invalid && pt.notes_edit.as_ref().is_some_and(|n| *n != v.notes) {
                        todo.push(Do::Notes(text));
                    }
                }
            }
        });
    }
    if ecad_cadcraft::styles::page::Holds::of_kind(v.kind) == ecad_cadcraft::styles::page::Holds::Front {
        widgets::section(ui, &mut folded, full, "page.front", "Front pages", "in the Book", false, |ui| {
            for row in &v.front {
                ui.horizontal(|ui| {
                    ui.set_min_height(32.0);
                    ui.label(widgets::rich(ui, row.kind.title(), ui_type::BODY, widgets::text()));
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        let on = !row.pages.is_empty();
                        if row.kind != GenKind::Cover {
                            let name = format!("ecad.page.front.{}", row.kind.value());
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
        widgets::read(ui, "Type", &v.type_name, "ecad.page.sheet.type");
        widgets::read(ui, "Title block", &v.title_block, "ecad.page.sheet.titleblock");
        ui.add_space(space::XS);
        let b = ui.add(ecad_ui::prim::Button::new("Change in Pages and sheets").small());
        b.widget_info(|| egui::WidgetInfo::labeled(egui::Role::Link, true, GOTO_TYPE));
        if b.clicked() {
            todo.push(Do::Settings(v.type_id.clone()));
        }
    });
    t.settings.page_tab.folded = folded;
    if let Some(e) = &t.settings.page_tab.error {
        ui.label(egui::RichText::new(e).small().color(ecad_ui::color(s.error)));
    }
    for d in todo {
        act(t, v, d);
    }
}

fn act(t: &mut Trial, v: &PageView, d: Do) {
    let r = match d {
        Do::Block(b, on) => {
            t.run_value("setdefault", json!({"key": b.entry().key, "level": "project", "value": if on { "on" } else { "off" }})).map(|_| ())
        }
        Do::Notes(text) => {
            let r = t.run_value("projectprops", json!({"props": {ecad_cadcraft::front::NOTES_KEY: text}})).map(|_| ());
            if r.is_ok() {
                t.settings.page_tab.notes_edit = None;
            }
            r
        }
        Do::Front(kind, true) => {
            let mut p = json!({"type": "front", "generated": kind.value()});
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
            t.settings.page_tab.error = Some(e);
        }
    }
}
