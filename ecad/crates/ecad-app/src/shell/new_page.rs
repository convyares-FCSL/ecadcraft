//! The + Page picker (brief 054, design 054 §4.1): every + that adds a page asks for a page type
//! (and, for Front sheet and Report, the generated kind), then runs one `newpage {type,
//! generated?, after | folder}`. The page goes right after the open page (the page well's + and
//! the Book header's +) or last in a group (a group row's +); the type never moves it (owner, Q1).
//!
//! Keys: ↑ ↓ move, → opens a kind list, ← closes it, Enter adds, Esc closes and writes nothing.
//! Clicking outside closes it.
//!
//! # Update map (design 054 §10.2)
//! None while open: it reads the types, the book and the next number once, on opening.

use ecad_cadcraft::generated::GenKind;
use ecad_cadcraft::styles::page::{self as ptype, Holds};
use ecad_model::book::Book;
use ecad_model::{FolderId, PageId, StyleRef};
use ecad_ui::tokens::{space, ui_type};
use eframe::egui::{self, Key, Modifiers, Pos2, Rect, Sense, pos2, vec2};
use serde_json::json;

use crate::App;
use crate::shell::bar;

/// Accessible names (design 054 §4.1).
pub const PICKER: &str = "ecad.newpage.picker";
/// Panel layout's tooltip (Q3).
pub const PANEL_LATER: &str = "Panel layout pages come with the panel layout page.";
/// The list's width and row height.
pub const WIDTH: f32 = 220.0;
pub const ROW_H: f32 = 28.0;

/// One generated kind offered under Front sheet or Report.
#[derive(Debug, Clone, PartialEq)]
pub struct KindRow {
    pub label: &'static str,
    /// The `generated` value, and the terminal plan's style for Terminal strips.
    pub kind: GenKind,
    pub style: Option<&'static str>,
    /// The numbers of the pages of this kind already in the book.
    pub in_book: Vec<String>,
}

impl KindRow {
    /// Its accessible name's last word: `cover`, `terminals-graphic`.
    pub fn word(&self) -> String {
        match self.style {
            Some(s) => format!("{}-{s}", ecad_model::Word::word(self.kind)),
            None => ecad_model::Word::word(self.kind).to_string(),
        }
    }
}

/// One page type row.
#[derive(Debug, Clone, PartialEq)]
pub struct TypeRow {
    pub id: StyleRef,
    pub name: String,
    pub holds: Holds,
    pub enabled: bool,
    pub kinds: Vec<KindRow>,
}

/// The picker while it is open.
#[derive(Debug, Clone, PartialEq)]
pub struct Picker {
    /// `None`: at the pointer, when first drawn.
    pub anchor: Option<Pos2>,
    /// Opens above the anchor (the bottom bar), else below it (the Book).
    pub above: bool,
    pub after: Option<PageId>,
    pub folder: Option<FolderId>,
    pub rows: Vec<TypeRow>,
    /// The number the new page will get.
    pub number: String,
    pub hi: usize,
    /// The open kind list's highlighted row.
    pub sub: Option<usize>,
    /// Frames shown: a click outside on the frame it opened is the click that opened it.
    frames: u32,
}

fn kind_rows(holds: Holds, doc: &ecad_model::Document, book: &Book) -> Vec<KindRow> {
    let mut out = Vec::new();
    for k in ptype::kinds(holds) {
        let in_book = |style: Option<&str>| -> Vec<String> {
            book.order
                .iter()
                .filter(|p| ecad_cadcraft::generated::kind_of(doc, **p) == Some(*k))
                .filter(|p| {
                    *k != GenKind::Terminals
                        || (ecad_cadcraft::strip_page::style_of(doc, **p) == ecad_cadcraft::strip_page::Style::Graphic) == style.is_some()
                })
                .map(|p| book.number(doc, *p))
                .collect()
        };
        out.push(KindRow { label: k.title(), kind: *k, style: None, in_book: in_book(None) });
        if *k == GenKind::Terminals {
            let g = ecad_cadcraft::defaults::strip_pages::GRAPHIC;
            out.push(KindRow { label: ecad_cadcraft::strip_page::TITLE, kind: *k, style: Some(g), in_book: in_book(Some(g)) });
        }
    }
    out
}

impl App {
    /// Open the picker at `anchor`: after `after` (the open page) or last in `folder`.
    pub fn open_new_page_picker(&mut self, anchor: Option<Pos2>, above: bool, after: Option<PageId>, folder: Option<FolderId>) {
        let Some(p) = self.project.clone() else { return };
        let built = p.read(&mut self.cad.session, |p, _| {
            let doc = p.ledger.document();
            let book = Book::of(doc);
            let after = after.filter(|a| doc.pages.contains_key(a));
            let folder = folder.filter(|f| doc.folders.contains_key(f));
            let (f, _, place) = ecad_electrical::cmd::sheet::new_page_at(doc, &book, folder, after).ok()?;
            let number = ecad_electrical::cmd::sheet::next_number(doc, &book, f, place, p.outputs.as_ref().map(|o| &o.0));
            let rows: Vec<TypeRow> = ptype::ordered(doc)
                .into_iter()
                .map(|r| {
                    let holds = ptype::look_of(doc, &r.id).holds;
                    let panel = r.id == StyleRef::Builtin(ptype::PANEL.into());
                    TypeRow { kinds: kind_rows(holds, doc, &book), enabled: !panel, name: r.name, holds, id: r.id }
                })
                .collect();
            // The open page's type from the page well, the group's from a group row (its last
            // page's), Schematic otherwise.
            let like =
                after.or_else(|| folder.and_then(|f| book.order.iter().rev().find(|p| ecad_model::book::folder_of(doc, **p) == Some(f)).copied()));
            let want = like.map_or(StyleRef::Builtin(ptype::SCHEMATIC.into()), |pg| ptype::type_of(doc, pg));
            let hi = rows
                .iter()
                .position(|r| r.id == want && r.enabled)
                .or_else(|| rows.iter().position(|r| r.id.text() == ptype::SCHEMATIC))
                .unwrap_or(0);
            Some(Picker { anchor, above, after, folder, rows, number, hi, sub: None, frames: 0 })
        });
        if let Ok(Some(picker)) = built {
            self.shell.new_page = Some(picker);
        }
    }

    /// Run the row (and kind) chosen: one `newpage`, then show the page and put the Book's
    /// rename field on its title.
    fn add_from_picker(&mut self, row: usize, kind: Option<usize>) {
        let Some(pk) = self.shell.new_page.take() else { return };
        let Some(r) = pk.rows.get(row) else { return };
        let mut v = json!({"type": r.id.text()});
        if let Some(k) = kind.and_then(|k| r.kinds.get(k)) {
            v["generated"] = json!(ecad_model::Word::word(k.kind));
            if let Some(s) = k.style {
                v["style"] = json!(s);
            }
        }
        match (pk.after, pk.folder) {
            (Some(a), _) => v["after"] = json!(a.hex()),
            (None, Some(f)) => v["folder"] = json!(f.hex()),
            _ => {}
        }
        let after_number = pk.after.and_then(|a| self.panel.rows().iter().find(|x| x.id == a).map(|x| x.number.clone()));
        match self.run_value("newpage", v) {
            Ok(res) => {
                let number = res.get("number").and_then(serde_json::Value::as_str).unwrap_or_default().to_string();
                let line = match after_number {
                    Some(n) => format!("Added page {number} ({}) after {n}.", r.name),
                    None => format!("Added page {number} ({}).", r.name),
                };
                self.cad.session.echo(line);
                let page = res.get("page").and_then(serde_json::Value::as_str).and_then(|t| u128::from_str_radix(t, 16).ok()).map(PageId::from);
                if let Some(page) = page {
                    self.refresh_panel();
                    self.panel.start_rename(page);
                }
            }
            Err(e) => self.cad.session.echo(e),
        }
    }

    /// Draw the picker when it is open, and take its keys. Called before the shell's keys.
    pub(crate) fn new_page_frame(&mut self, ctx: &egui::Context) {
        let Some(pk) = self.shell.new_page.clone() else { return };
        let n = pk.rows.len();
        // Keys first, so the shell's arrows and Enter never reach the page under it.
        let (up, down, right, left, enter, esc) = ctx.input_mut(|i| {
            (
                i.consume_key(Modifiers::NONE, Key::ArrowUp),
                i.consume_key(Modifiers::NONE, Key::ArrowDown),
                i.consume_key(Modifiers::NONE, Key::ArrowRight),
                i.consume_key(Modifiers::NONE, Key::ArrowLeft),
                i.consume_key(Modifiers::NONE, Key::Enter),
                i.consume_key(Modifiers::NONE, Key::Escape),
            )
        });
        if esc {
            self.shell.new_page = None;
            return;
        }
        let mut pk = pk;
        pk.frames += 1;
        if pk.anchor.is_none() {
            let at = ctx.input(|i| i.pointer.interact_pos()).unwrap_or_else(|| ctx.content_rect().center());
            pk.above = at.y > ctx.content_rect().center().y;
            pk.anchor = Some(at);
        }
        let anchor = pk.anchor.unwrap_or_default();
        let kinds = |pk: &Picker| pk.rows.get(pk.hi).map_or(0, |r| r.kinds.len());
        match pk.sub {
            Some(s) => {
                let k = kinds(&pk);
                if up && s > 0 {
                    pk.sub = Some(s - 1);
                }
                if down && s + 1 < k {
                    pk.sub = Some(s + 1);
                }
                if left {
                    pk.sub = None;
                }
            }
            None => {
                let step = |from: usize, d: isize| -> usize {
                    let mut i = from as isize;
                    for _ in 0..n {
                        i = (i + d).rem_euclid(n.max(1) as isize);
                        if pk.rows.get(i as usize).is_some_and(|r| r.enabled) {
                            break;
                        }
                    }
                    i as usize
                };
                if up {
                    pk.hi = step(pk.hi, -1);
                }
                if down {
                    pk.hi = step(pk.hi, 1);
                }
                if right && kinds(&pk) > 0 {
                    pk.sub = Some(0);
                }
            }
        }
        if enter {
            let (row, sub, has) = (pk.hi, pk.sub, kinds(&pk) > 0);
            self.shell.new_page = Some(pk);
            match (sub, has) {
                (Some(k), _) => self.add_from_picker(row, Some(k)),
                (None, true) => {
                    if let Some(p) = &mut self.shell.new_page {
                        p.sub = Some(0);
                    }
                }
                (None, false) => self.add_from_picker(row, None),
            }
            return;
        }
        let s = ecad_ui::SHELL;
        let h = ROW_H * n as f32 + 2.0 * space::XS;
        let top = if pk.above { anchor.y - h - space::S } else { anchor.y + space::S };
        let screen = ctx.content_rect();
        let x = (anchor.x - WIDTH / 2.0).clamp(screen.left() + space::S, (screen.right() - 2.0 * WIDTH - space::S).max(screen.left()));
        let list = Rect::from_min_size(pos2(x, top.max(screen.top())), vec2(WIDTH, h));
        let mut chosen: Option<(usize, Option<usize>)> = None;
        let mut open_sub: Option<(usize, Option<usize>)> = None;
        let area = egui::Area::new(egui::Id::unique(PICKER)).order(egui::Order::Foreground).fixed_pos(list.min).show(ctx, |ui| {
            ui.set_min_size(list.size());
            let p = ui.painter().clone();
            p.rect_filled(list, 6.0, ecad_ui::color(s.raised));
            p.rect_stroke(list, 6.0, egui::Stroke::new(1.0, ecad_ui::color(s.line)), egui::StrokeKind::Inside);
            ui.interact(list, egui::Id::unique((PICKER, "ground")), Sense::hover())
                .widget_info(|| egui::WidgetInfo::labeled(egui::Role::Menu, true, PICKER));
            for (i, row) in pk.rows.iter().enumerate() {
                let r = Rect::from_min_size(
                    pos2(list.left() + space::XS, list.top() + space::XS + ROW_H * i as f32),
                    vec2(WIDTH - 2.0 * space::XS, ROW_H),
                );
                let resp =
                    ui.interact(r, egui::Id::unique(("ecad.newpage.type", row.id.text())), if row.enabled { Sense::click() } else { Sense::hover() });
                resp.widget_info(|| {
                    egui::WidgetInfo::selected(egui::Role::MenuItem, row.enabled, i == pk.hi, format!("ecad.newpage.type.{}", row.id.text()))
                });
                let resp = if row.enabled { resp } else { resp.on_hover_text(PANEL_LATER) };
                if (i == pk.hi || resp.hovered()) && row.enabled {
                    p.rect_filled(r, 4.0, ecad_ui::color(if i == pk.hi { s.accent_soft } else { s.hover }));
                }
                if resp.hovered() && row.enabled && !row.kinds.is_empty() {
                    open_sub = Some((i, pk.sub.filter(|_| pk.hi == i)));
                }
                if resp.clicked() {
                    if row.kinds.is_empty() {
                        chosen = Some((i, None));
                    } else {
                        open_sub = Some((i, Some(0)));
                    }
                }
                let th = Rect::from_min_size(pos2(r.left() + space::S, r.center().y - 8.0), vec2(24.0, 16.0));
                crate::settings::pages::thumbnail(&p, th, &row.id, row.holds);
                let ink = if row.enabled { s.text } else { s.text_3 };
                p.text(
                    pos2(th.right() + space::M, r.center().y),
                    egui::Align2::LEFT_CENTER,
                    &row.name,
                    bar::font(ui, ui_type::BODY),
                    ecad_ui::color(ink),
                );
                let right = if row.kinds.is_empty() { pk.number.clone() } else { "\u{203a}".to_string() };
                p.text(
                    pos2(r.right() - space::S, r.center().y),
                    egui::Align2::RIGHT_CENTER,
                    right,
                    bar::font(ui, ui_type::SMALL),
                    ecad_ui::color(s.text_3),
                );
            }
        });
        // The kind list beside the highlighted row.
        let mut sub_rect = None;
        if let (Some(si), Some(row)) = (pk.sub, pk.rows.get(pk.hi)) {
            let k = row.kinds.len();
            let sh = ROW_H * k as f32 + 2.0 * space::XS;
            let y0 = (list.top() + space::XS + ROW_H * pk.hi as f32 - space::XS).min(screen.bottom() - sh).max(screen.top());
            let sl = Rect::from_min_size(pos2(list.right() + space::XXS, y0), vec2(WIDTH, sh));
            sub_rect = Some(sl);
            egui::Area::new(egui::Id::unique((PICKER, "kinds"))).order(egui::Order::Foreground).fixed_pos(sl.min).show(ctx, |ui| {
                ui.set_min_size(sl.size());
                let p = ui.painter().clone();
                p.rect_filled(sl, 6.0, ecad_ui::color(s.raised));
                p.rect_stroke(sl, 6.0, egui::Stroke::new(1.0, ecad_ui::color(s.line)), egui::StrokeKind::Inside);
                for (j, kr) in row.kinds.iter().enumerate() {
                    let r = Rect::from_min_size(
                        pos2(sl.left() + space::XS, sl.top() + space::XS + ROW_H * j as f32),
                        vec2(WIDTH - 2.0 * space::XS, ROW_H),
                    );
                    let resp = ui.interact(r, egui::Id::unique(("ecad.newpage.kind", kr.word())), Sense::click());
                    resp.widget_info(|| egui::WidgetInfo::selected(egui::Role::MenuItem, true, j == si, format!("ecad.newpage.kind.{}", kr.word())));
                    if j == si || resp.hovered() {
                        p.rect_filled(r, 4.0, ecad_ui::color(if j == si { s.accent_soft } else { s.hover }));
                    }
                    if resp.clicked() {
                        chosen = Some((pk.hi, Some(j)));
                    }
                    p.text(
                        pos2(r.left() + space::M, r.center().y),
                        egui::Align2::LEFT_CENTER,
                        kr.label,
                        bar::font(ui, ui_type::BODY),
                        ecad_ui::color(s.text),
                    );
                    let right = if kr.in_book.is_empty() { pk.number.clone() } else { format!("in the Book · {}", kr.in_book.join(", ")) };
                    p.text(
                        pos2(r.right() - space::S, r.center().y),
                        egui::Align2::RIGHT_CENTER,
                        right,
                        bar::font(ui, ui_type::SMALL),
                        ecad_ui::color(s.text_3),
                    );
                }
            });
        }
        if let Some((i, sub)) = open_sub {
            pk.hi = i;
            pk.sub = sub.or(pk.sub.filter(|_| pk.hi == i)).or(Some(0));
        }
        // A click outside both lists closes the picker (not on the frame it opened).
        let outside = ctx.input(|i| {
            i.pointer.any_pressed() && i.pointer.interact_pos().is_some_and(|at| !list.contains(at) && sub_rect.is_none_or(|r| !r.contains(at)))
        });
        let _ = area;
        if let Some((row, kind)) = chosen {
            self.shell.new_page = Some(pk);
            self.add_from_picker(row, kind);
            return;
        }
        if outside && pk.frames > 1 {
            self.shell.new_page = None;
            return;
        }
        self.shell.new_page = Some(pk);
    }
}

/// Where the picker opens for a + at `rect`: centred on it, above it when it is in the lower half
/// of the window.
pub fn anchor_of(ctx: &egui::Context, rect: Rect) -> (Pos2, bool) {
    let above = rect.center().y > ctx.content_rect().center().y;
    (if above { rect.center_top() } else { rect.center_bottom() }, above)
}
