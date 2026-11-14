//! The Publish workspace (UI rev 3, board 4.01: pick, order, print settings). Publish has no
//! history; released revisions live in Draw's History panel.
//!
//! - **Left: output sets.** Today's single print is **Full booklet**; Panel builder pack,
//!   Customer approval and Site wiring are placeholders until output sets exist. Under them,
//!   Release (027's dialog, floating) for the pending revision.
//! - **Centre: every page** at a readable size, grouped as in the book, each with its place in
//!   the output. Tick to include: the ticked pages become Print Book's page list (all ticked:
//!   every page). Pages always print in book order today, so dragging to reorder is not offered;
//!   its place is said on hover.
//! - **Right: the print settings.** PDF is real (the file, colour or black, line weights; the
//!   paper is each sheet's own); Printer and DXF are placeholders, as are revision clouds, the
//!   watermark, bookmarks and numbering. The selected page as it will print. Export PDF runs
//!   `printbook` through the command path (one call, no transaction).
//!
//! Thumbnails are the page drawn from the model ([`ecad_cadcraft::preview`]), at most
//! [`THUMBS_PER_FRAME`] a frame, kept until a change touches their page.

use std::collections::{BTreeMap, BTreeSet};

use ecad_cadcraft::preview::{PreviewView, preview_drawing, rasterise};
use ecad_model::{FolderId, PageId};
use ecad_ui::tokens::{space, ui_type};
use eframe::egui::{self, Align2, Rect, Sense, Stroke, pos2, vec2};

use crate::Trial;
use crate::outputs_ui::ScopeKind;
use crate::shell::bar;

/// Element names.
pub const SETS: &str = "publish.sets";
pub const SET_FULL: &str = "publish.set.full_booklet";
/// A page tile: `publish.page.<hex>`; its tick `publish.page.<hex>.include`.
pub const PAGE: &str = "publish.page";
pub const SELECT_ALL: &str = "publish.select_all";
pub const EXPORT: &str = "publish.export";
pub const PRINT: &str = "publish.print";
pub const RELEASE: &str = "publish.release";
pub const PATH: &str = "ecad.printbook.path";
pub const COLOUR: &str = "publish.colour";
pub const LINEWEIGHTS: &str = "publish.lineweights";
pub const PREVIEW: &str = "publish.preview";
/// The output sets (board 4.01); only the first exists today.
pub const OUTPUT_SETS: [(&str, &str); 4] = [
    ("Full booklet", ""),
    ("Panel builder pack", "schematics, terminal plans, parts"),
    ("Customer approval", "schematics only, DRAFT watermark"),
    ("Site wiring", "field pages and cable list"),
];
/// The left column's width (board 4.01).
pub const SETS_W: f32 = 260.0;
/// A page tile's thumbnail.
pub const THUMB_W: f32 = 212.0;
pub const THUMB_H: f32 = 150.0;
/// Thumbnails drawn in one frame at most.
pub const THUMBS_PER_FRAME: usize = 3;

/// One page's thumbnail.
struct Thumb {
    texture: egui::TextureHandle,
}

impl std::fmt::Debug for Thumb {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("Thumb")
    }
}

/// Publish's state between frames.
#[derive(Debug, Default)]
pub struct Publish {
    /// Pages unticked (left out of the output).
    pub excluded: BTreeSet<PageId>,
    /// The page shown "as it will print".
    pub preview: Option<PageId>,
    thumbs: BTreeMap<PageId, Thumb>,
    /// The project serial and change count the thumbnails are good for.
    thumbs_at: Option<(u64, u64)>,
    /// The scope and change count the sheet count was taken at.
    counted: Option<(Option<(u64, u64)>, u64, u64)>,
    /// What the last Export said.
    pub note: Option<String>,
    /// Thumbnails drawn so far (tests: only when a change touched the page).
    pub drawn: u64,
    /// Pages whose thumbnail could not be drawn, kept until a change touches them (so they are
    /// not projected again every frame).
    failed: BTreeSet<PageId>,
    /// Bumped whenever `excluded` changes.
    excluded_rev: u64,
    /// The book as Publish lays it out, kept until the Pages rows or the ticks change.
    book: Book,
    /// How many times the book was worked out (tests: not every frame).
    pub books: u64,
}

/// The pages in book order, their groups, their places in the output and Print Book's page list,
/// worked out from the Pages rows and the ticks only when either changed (review of #95).
#[derive(Debug, Default)]
struct Book {
    /// (the Pages rows' rebuild count, `excluded_rev`) they were worked out at.
    at: Option<(u64, u64)>,
    groups: Vec<Group>,
    order: Vec<PageId>,
    position: BTreeMap<PageId, usize>,
    /// Print Book's page list (`None`: every page).
    pages: Option<String>,
}

impl Publish {
    /// Tick or untick `page`.
    pub fn toggle(&mut self, page: PageId) {
        if !self.excluded.remove(&page) {
            self.excluded.insert(page);
        }
        self.excluded_rev += 1;
    }

    /// Tick every page.
    pub fn select_all(&mut self) {
        self.excluded.clear();
        self.excluded_rev += 1;
    }
}

/// One group of pages, as in the book.
#[derive(Debug)]
struct Group {
    name: String,
    pages: Vec<(PageId, String, String)>,
}

fn c32(c: ecad_ui::tokens::Color) -> egui::Color32 {
    ecad_ui::color(c)
}

impl Trial {
    /// The book's groups: a top-level run of pages sharing a folder (as the page well).
    fn publish_groups(&self) -> Vec<Group> {
        let mut names: BTreeMap<FolderId, String> = BTreeMap::new();
        for r in self.panel.tree() {
            if let ecad_cadcraft::TreeRow::Folder { id, name, .. } = r {
                names.insert(*id, name.clone());
            }
        }
        let mut groups: Vec<(Option<FolderId>, Group)> = Vec::new();
        for r in self.panel.rows() {
            match groups.last_mut() {
                Some((f, g)) if *f == r.folder => g.pages.push((r.id, r.number.clone(), r.title.clone())),
                _ => groups.push((
                    r.folder,
                    Group {
                        name: r.folder.and_then(|f| names.get(&f).cloned()).unwrap_or_else(|| "Pages".into()),
                        pages: vec![(r.id, r.number.clone(), r.title.clone())],
                    },
                )),
            }
        }
        groups.into_iter().map(|(_, g)| g).collect()
    }

    /// Keep the book (groups, order, places) and Print Book's scope in step with the Pages rows
    /// and the ticks, and the sheet count with the project: each worked out again only when what
    /// it reads changed.
    fn publish_sync(&mut self) {
        let at = (self.panel.rebuilds, self.publish.excluded_rev);
        if self.publish.book.at != Some(at) {
            let groups = self.publish_groups();
            let order: Vec<PageId> = groups.iter().flat_map(|g| g.pages.iter().map(|p| p.0)).collect();
            let present: BTreeSet<PageId> = order.iter().copied().collect();
            let before = self.publish.excluded.len();
            self.publish.excluded.retain(|p| present.contains(p));
            if self.publish.excluded.len() != before {
                self.publish.excluded_rev += 1;
            }
            let excluded = &self.publish.excluded;
            let mut position = BTreeMap::new();
            for p in order.iter().filter(|p| !excluded.contains(p)) {
                let n = position.len() + 1;
                position.insert(*p, n);
            }
            let pages = (!excluded.is_empty()).then(|| {
                groups.iter().flat_map(|g| g.pages.iter()).filter(|p| !excluded.contains(&p.0)).map(|p| p.1.as_str()).collect::<Vec<_>>().join(", ")
            });
            self.publish.books += 1;
            self.publish.book = Book { at: Some((self.panel.rebuilds, self.publish.excluded_rev)), groups, order, position, pages };
        }
        let Some(d) = &mut self.print_book else { return };
        d.hidden = true;
        // Print Book is opened again after an export: give it the ticks back.
        match &self.publish.book.pages {
            None => d.scope = ScopeKind::All,
            Some(list) => {
                d.scope = ScopeKind::Pages;
                if d.pages != *list {
                    d.pages.clone_from(list);
                }
            }
        }
        let Some(p) = self.project.clone().filter(|p| p.is_attached(&self.cad.session)) else { return };
        let Ok(count) = p.read(&mut self.cad.session, |p, _| p.change_count()) else { return };
        let counted = (self.publish.book.at, self.project_serial, count);
        if self.publish.counted.as_ref() == Some(&counted) {
            return;
        }
        let scope = match &self.publish.book.pages {
            Some(list) => ecad_cadcraft::printbook::Scope::Pages(list.clone()),
            None => ecad_cadcraft::printbook::Scope::All,
        };
        if let Ok(((sheets, unknown, flagged), notes)) =
            p.configure(&mut self.cad.session, |p| (ecad_cadcraft::printbook::count(p, &scope), ecad_cadcraft::printbook::notes(p, &scope)))
            && let Some(d) = &mut self.print_book
        {
            d.sheets = sheets;
            d.unknown = unknown;
            d.flagged = flagged;
            d.notes = notes;
        }
        self.publish.counted = Some(counted);
    }

    /// Draw up to [`THUMBS_PER_FRAME`] missing thumbnails of `want`; drop the ones a change
    /// touched.
    fn publish_thumbs(&mut self, ctx: &egui::Context, want: &[PageId]) {
        let Some(p) = self.project.clone().filter(|p| p.is_attached(&self.cad.session)) else {
            self.publish.thumbs.clear();
            self.publish.thumbs_at = None;
            return;
        };
        let serial = self.project_serial;
        let since = self.publish.thumbs_at.filter(|a| a.0 == serial).map(|a| a.1);
        let read = p.read(&mut self.cad.session, |p, _| (p.change_count(), since.map(|n| p.changes_since(n))));
        let Ok((count, touched)) = read else { return };
        match touched {
            None | Some(None) => {
                self.publish.thumbs.clear();
                self.publish.failed.clear();
            }
            Some(Some(pages)) => {
                self.publish.thumbs.retain(|k, _| !pages.contains(k));
                self.publish.failed.retain(|k| !pages.contains(k));
            }
        }
        self.publish.thumbs_at = Some((serial, count));
        let sheet = crate::canvas::Sheet::Paper.tokens().sheet;
        let bg = [sheet.r, sheet.g, sheet.b];
        let (w, h) = (THUMB_W as u32 * 2, THUMB_H as u32 * 2);
        let mut drawn = 0;
        for page in want {
            if drawn >= THUMBS_PER_FRAME {
                ctx.request_repaint();
                break;
            }
            if self.publish.thumbs.contains_key(page) || self.publish.failed.contains(page) {
                continue;
            }
            drawn += 1;
            let d = match p.read(&mut self.cad.session, |p, _| preview_drawing(p, *page)) {
                Ok(Ok(d)) => d,
                // The page cannot be drawn: remember it until a change touches it.
                Ok(Err(_)) => {
                    self.publish.failed.insert(*page);
                    continue;
                }
                // The project is busy: try again next frame.
                Err(_) => {
                    ctx.request_repaint();
                    break;
                }
            };
            let view = PreviewView::extents(&d, w, h);
            match rasterise(&d, view, w, h, bg, 0.0) {
                Ok(img) => {
                    let image = egui::ColorImage::from_rgba_premultiplied([img.width as usize, img.height as usize], &img.rgba);
                    let texture = ctx.load_texture(format!("publish.thumb.{}", page.hex()), image, egui::TextureOptions::LINEAR);
                    self.publish.thumbs.insert(*page, Thumb { texture });
                    self.publish.drawn += 1;
                }
                Err(_) => {
                    self.publish.failed.insert(*page);
                }
            }
        }
    }

    /// Publish's main view: the output sets at the left, the pages in the centre.
    pub(crate) fn publish_main(&mut self, ui: &mut egui::Ui, main: Rect) {
        let s = ecad_ui::SHELL;
        let line = Stroke::new(ecad_ui::tokens::line::HAIRLINE_PX, c32(s.line));
        ui.painter().rect_filled(main, 0.0, c32(s.bg));
        let open = self.project_open();
        if open {
            self.publish_sync();
        }
        let total = self.publish.book.order.len();
        let included = total.saturating_sub(self.publish.excluded.len());

        // Left: the output sets.
        let sets = Rect::from_min_max(main.min, pos2(main.left() + SETS_W, main.bottom()));
        ui.painter().rect_filled(sets, 0.0, c32(s.panel));
        ui.painter().vline(sets.right() - 0.5, sets.y_range(), line);
        let head = Rect::from_min_size(sets.min, vec2(sets.width(), 44.0));
        bar::text_at(ui, head.left() + space::L, head.center().y, "Output sets", ui_type::BODY_EMPHASIS, c32(s.text));
        let plus = Rect::from_center_size(pos2(head.right() - space::L - 8.0, head.center().y), vec2(24.0, 24.0));
        bar::icon_button(ui, plus, ecad_ui::tokens::Icon::Plus, "New output set", 14.0, false).on_hover_text("Output sets come later");
        ui.painter().hline(head.x_range(), head.bottom() - 0.5, line);
        let mut y = head.bottom() + space::S;
        for (i, (name, about)) in OUTPUT_SETS.iter().enumerate() {
            let r = Rect::from_min_size(pos2(sets.left() + space::S, y), vec2(sets.width() - 2.0 * space::S, 50.0));
            let on = i == 0;
            let resp = ui.interact(r, egui::Id::unique(("publish.set", i)), Sense::hover());
            let label = if on { SET_FULL.to_string() } else { format!("publish.set.{}", name.to_lowercase().replace(' ', "_")) };
            resp.widget_info(|| egui::WidgetInfo::selected(egui::Role::Button, on, on, &label));
            if on {
                ui.painter().rect_filled(r, ecad_ui::style::corner(6.0), c32(s.accent_soft));
                ui.painter().vline(r.left() + 1.5, r.y_range(), Stroke::new(3.0, c32(s.accent)));
            } else {
                resp.on_hover_text("Output sets come later: each remembers its pages, order and settings.");
            }
            bar::text_at(ui, r.left() + space::M + 2.0, r.top() + 16.0, name, ui_type::BODY_EMPHASIS, c32(if on { s.text } else { s.text_2 }));
            let sub = if on { format!("{included} of {total} pages · PDF") } else { (*about).to_string() };
            bar::text_at(ui, r.left() + space::M + 2.0, r.top() + 35.0, &sub, ui_type::SMALL, c32(s.text_3));
            y = r.bottom() + space::XS;
        }
        // The foot: Release (027's dialog) for the pending revision.
        let foot = Rect::from_min_max(pos2(sets.left(), sets.bottom() - 96.0), sets.max);
        ui.painter().hline(foot.x_range(), foot.top() + 0.5, line);
        let pending = if open {
            self.project.clone().and_then(|p| p.read(&mut self.cad.session, |p, _| ecad_cadcraft::revisions::pending_rev(p.ledger.document())).ok())
        } else {
            None
        };
        let mut release = false;
        crate::shell::in_rect(ui, foot.shrink2(vec2(space::L, space::M)), "publish.foot", |ui| {
            ui.label(egui::RichText::new("Release").font(bar::font(ui, ui_type::CAPTION)).color(c32(s.text_3)));
            ui.label(egui::RichText::new("Released revisions are in Draw ▸ History.").small().color(c32(s.text_2)));
            let words = pending.as_ref().map_or_else(|| "Release…".to_string(), |r| format!("Release Rev {r}…"));
            let b = ui.add_enabled(open, ecad_ui::prim::Button::new(&words).small());
            b.widget_info(|| egui::WidgetInfo::labeled(egui::Role::Button, open, RELEASE));
            release = b.clicked();
        });
        if release {
            self.open_release(None);
        }

        // Centre: the pages.
        let centre = Rect::from_min_max(pos2(sets.right(), main.top()), main.max);
        let chead = Rect::from_min_size(centre.min, vec2(centre.width(), 44.0));
        ui.painter().rect_filled(chead, 0.0, c32(s.panel));
        ui.painter().hline(chead.x_range(), chead.bottom() - 0.5, line);
        let mut select_all = false;
        crate::shell::in_rect(ui, chead.shrink2(vec2(space::L, space::S)), "publish.head", |ui| {
            ui.horizontal_centered(|ui| {
                ui.label(egui::RichText::new("Full booklet").strong().color(c32(s.text)));
                ui.label(egui::RichText::new(format!("· {included} pages in book order · untick to leave out")).color(c32(s.text_2)));
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    let b = ui.add_enabled(open && included < total, ecad_ui::prim::Button::new("Select all").small());
                    b.widget_info(|| egui::WidgetInfo::labeled(egui::Role::Button, open, SELECT_ALL));
                    select_all = b.clicked();
                    ui.add_space(space::S);
                    ui.add(ecad_ui::prim::Button::new("Changed since the last release").small().disabled("Comes with output sets"));
                    ui.add(ecad_ui::prim::Button::new("Book order").small().disabled("Pages print in book order"));
                });
            });
        });
        if select_all {
            self.publish.select_all();
        }
        let body = Rect::from_min_max(pos2(centre.left(), chead.bottom()), centre.max);
        if !open {
            crate::shell::in_rect(ui, body.shrink(space::XL), "publish.empty", |ui| {
                ecad_ui::prim::read_value(ui, "No project is open.");
            });
            return;
        }
        let book = &self.publish.book;
        let in_book = |p: &PageId| book.position.contains_key(p) || self.publish.excluded.contains(p);
        if self.publish.preview.is_none_or(|p| !in_book(&p)) {
            self.publish.preview = self.panel.active.filter(in_book).or(book.order.first().copied());
        }
        let mut toggles = Vec::new();
        let mut chosen = None;
        let mut visible = Vec::new();
        let sheet = crate::canvas::Sheet::Paper.tokens();
        let thumbs = &self.publish.thumbs;
        let preview = self.publish.preview;
        let excluded = &self.publish.excluded;
        let tile = vec2(THUMB_W + 8.0, THUMB_H + 34.0);
        let gap = vec2(space::XL, space::L);
        let group_head = 30.0;
        crate::shell::in_rect(ui, body.shrink2(vec2(space::XL, space::L)), "publish.pages", |ui| {
            let per_row = (((ui.available_width() + gap.x) / (tile.x + gap.x)).floor() as usize).max(1);
            // Each group's top, in the content's own coordinates.
            let mut tops = Vec::with_capacity(book.groups.len());
            let mut y = 0.0;
            for g in &book.groups {
                tops.push(y);
                let rows = g.pages.len().div_ceil(per_row);
                y += group_head + rows as f32 * (tile.y + gap.y) + space::M;
            }
            let height = y;
            egui::ScrollArea::vertical().id_salt("publish.pages.scroll").auto_shrink([false, false]).show_viewport(ui, |ui, view| {
                ui.set_height(height);
                let origin = ui.max_rect().min;
                for (g, top) in book.groups.iter().zip(&tops) {
                    let rows = g.pages.len().div_ceil(per_row);
                    let bottom = top + group_head + rows as f32 * (tile.y + gap.y);
                    if bottom < view.top() || *top > view.bottom() {
                        continue;
                    }
                    bar::text_at(ui, origin.x, origin.y + top + 10.0, &g.name, ui_type::BODY_EMPHASIS, c32(s.text_2));
                    // Only the rows in view are laid out.
                    let first = (((view.top() - top - group_head) / (tile.y + gap.y)).floor().max(0.0)) as usize;
                    let last = (((view.bottom() - top - group_head) / (tile.y + gap.y)).ceil().max(0.0) as usize).min(rows);
                    for row in first..last {
                        for col in 0..per_row {
                            let Some((id, number, title)) = g.pages.get(row * per_row + col) else { break };
                            let at = origin + vec2(col as f32 * (tile.x + gap.x), top + group_head + row as f32 * (tile.y + gap.y));
                            let r = Rect::from_min_size(at, tile);
                            let resp = ui.interact(r, egui::Id::unique(("publish.page", *id)), Sense::click());
                            let name = format!("{PAGE}.{}", id.hex());
                            let on = !excluded.contains(id);
                            resp.widget_info(|| egui::WidgetInfo::selected(egui::Role::Button, true, preview == Some(*id), &name));
                            visible.push(*id);
                            let pic = Rect::from_min_size(r.min + vec2(4.0, 4.0), vec2(THUMB_W, THUMB_H));
                            let p = ui.painter();
                            if preview == Some(*id) {
                                p.rect_filled(r, ecad_ui::style::corner(6.0), c32(s.accent_soft));
                                p.rect_stroke(r, ecad_ui::style::corner(6.0), Stroke::new(1.5, c32(s.accent)), egui::StrokeKind::Inside);
                            }
                            match thumbs.get(id) {
                                Some(t) => {
                                    p.image(t.texture.id(), pic, Rect::from_min_max(pos2(0.0, 0.0), pos2(1.0, 1.0)), egui::Color32::WHITE);
                                }
                                None => {
                                    p.rect_filled(pic, 0.0, c32(sheet.sheet));
                                }
                            }
                            p.rect_stroke(pic, 0.0, Stroke::new(1.0, c32(s.line_strong)), egui::StrokeKind::Inside);
                            if !on {
                                p.rect_filled(pic, 0.0, egui::Color32::from_black_alpha(150));
                            }
                            // Tick (include) at the top left, the place in the output at the top right.
                            let tick = Rect::from_min_size(pic.min + vec2(6.0, 6.0), vec2(16.0, 16.0));
                            let tr = ui.interact(tick.expand(3.0), egui::Id::unique(("publish.tick", *id)), Sense::click());
                            tr.widget_info(|| egui::WidgetInfo::selected(egui::Role::CheckBox, true, on, format!("{name}.include")));
                            let p = ui.painter();
                            p.rect_filled(tick, ecad_ui::style::corner(3.0), c32(if on { s.accent } else { s.raised }));
                            p.rect_stroke(
                                tick,
                                ecad_ui::style::corner(3.0),
                                Stroke::new(1.0, c32(if on { s.accent } else { s.line_strong })),
                                egui::StrokeKind::Inside,
                            );
                            if on {
                                let c = tick.center();
                                p.line_segment([c + vec2(-4.0, 0.0), c + vec2(-1.0, 3.0)], Stroke::new(1.8, egui::Color32::WHITE));
                                p.line_segment([c + vec2(-1.0, 3.0), c + vec2(4.5, -3.5)], Stroke::new(1.8, egui::Color32::WHITE));
                            }
                            if tr.clicked() {
                                toggles.push(*id);
                            }
                            if let Some(n) = book.position.get(id) {
                                let at = pos2(pic.right() - 16.0, pic.top() + 16.0);
                                p.circle_filled(at, 11.0, c32(s.accent));
                                p.text(at, Align2::CENTER_CENTER, n.to_string(), bar::font(ui, ui_type::CAPTION), egui::Color32::WHITE);
                            }
                            let label_y = pic.bottom() + 14.0;
                            let ink = c32(if on { s.text } else { s.text_3 });
                            let x = bar::text_at(ui, pic.left() + 2.0, label_y, &format!("{number:0>2}"), ui_type::SMALL, c32(s.text_3));
                            let end = bar::text_at(ui, x + space::S, label_y, title, ui_type::BODY, ink);
                            if !on {
                                ui.painter().hline((pic.left() + 2.0)..=end, label_y, Stroke::new(1.0, c32(s.text_3)));
                            }
                            if resp.clicked() {
                                chosen = Some(*id);
                            }
                            resp.on_hover_text("Click to preview it at the right. Pages print in book order: reordering comes with output sets.");
                        }
                    }
                }
            });
        });
        for id in toggles {
            self.publish.toggle(id);
        }
        if let Some(p) = chosen {
            self.publish.preview = Some(p);
        }
        if let Some(p) = self.publish.preview
            && !visible.contains(&p)
        {
            visible.insert(0, p);
        }
        let ctx = ui.ctx().clone();
        self.publish_thumbs(&ctx, &visible);
    }

    /// Publish's right column: PDF, Printer or DXF, the paper, what goes on every page, the
    /// selected page as it will print, and Export PDF.
    pub(crate) fn publish_right(&mut self, ui: &mut egui::Ui, rect: Rect) {
        let s = ecad_ui::SHELL;
        let line = Stroke::new(ecad_ui::tokens::line::HAIRLINE_PX, c32(s.line));
        ui.painter().rect_filled(rect, 0.0, c32(s.panel));
        ui.painter().vline(rect.left() + 0.5, rect.y_range(), line);
        let head = Rect::from_min_size(rect.min, vec2(rect.width(), 44.0));
        ui.painter().hline(head.x_range(), head.bottom() - 0.5, line);
        crate::shell::in_rect(ui, head.shrink2(vec2(space::L, space::S)), "publish.right.head", |ui| {
            ui.horizontal_centered(|ui| {
                ui.label(egui::RichText::new("Settings").strong().color(c32(s.text)));
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    ui.add(ecad_ui::prim::Button::new("DXF").small().disabled("DXF export comes later"));
                    ui.add(ecad_ui::prim::Button::new("Printer").small().disabled("Printing to a printer comes later"));
                    let _ = ui.add(ecad_ui::prim::Button::new("PDF").kind(ecad_ui::prim::Kind::Primary).small());
                });
            });
        });
        let pending =
            self.project.clone().and_then(|p| p.read(&mut self.cad.session, |p, _| ecad_cadcraft::revisions::pending_rev(p.ledger.document())).ok());
        let preview = self.publish.preview;
        let preview_words = preview.and_then(|p| self.panel.rows().iter().find(|r| r.id == p)).map(|r| format!("page {:0>2}", r.number));
        let foot_h = 84.0;
        let body = Rect::from_min_max(pos2(rect.left(), head.bottom()), pos2(rect.right(), rect.bottom() - foot_h));
        let foot = Rect::from_min_max(pos2(rect.left(), rect.bottom() - foot_h), rect.max);
        let thumb = preview.and_then(|p| self.publish.thumbs.get(&p)).map(|t| t.texture.id());
        let Some(d) = &mut self.print_book else {
            crate::shell::in_rect(ui, body.shrink(space::L), "publish.right.empty", |ui| ecad_ui::prim::read_value(ui, "No project is open."));
            return;
        };
        let kv = |ui: &mut egui::Ui, k: &str, add: &mut dyn FnMut(&mut egui::Ui)| {
            ui.horizontal(|ui| {
                let (r, _) = ui.allocate_exact_size(vec2(84.0, 26.0), Sense::hover());
                ui.painter().text(pos2(r.left(), r.center().y), Align2::LEFT_CENTER, k, bar::font(ui, ui_type::SMALL), c32(s.text_2));
                add(ui);
            });
        };
        crate::shell::in_rect(ui, body.shrink2(vec2(space::L, space::M)), "publish.right.body", |ui| {
            egui::ScrollArea::vertical().id_salt("publish.right.scroll").auto_shrink([false, false]).show(ui, |ui| {
                egui::CollapsingHeader::new(egui::RichText::new("Paper").strong().color(c32(s.text))).default_open(true).show(ui, |ui| {
                    kv(ui, "Size", &mut |ui| {
                        ui.label(egui::RichText::new("As drawn").color(c32(s.text_2)));
                    });
                    kv(ui, "Scale", &mut |ui| {
                        ui.label(egui::RichText::new("1 : 1").color(c32(s.text_2)));
                    });
                    kv(ui, "Colour", &mut |ui| {
                        // Print Book's two: as on screen, or black on white (grey comes later).
                        let mut i = usize::from(d.mono);
                        let r = ecad_ui::prim::segmented(ui, &mut i, &["Colour", "Black"]);
                        r.widget_info(|| egui::WidgetInfo::labeled(egui::Role::RadioGroup, true, COLOUR));
                        d.mono = i == 1;
                    });
                    kv(ui, "", &mut |ui| {
                        ui.checkbox(&mut d.lineweights, "Line weights")
                            .widget_info(|| egui::WidgetInfo::labeled(egui::Role::CheckBox, true, LINEWEIGHTS));
                    });
                    kv(ui, "File", &mut |ui| {
                        let r = ui.add(egui::TextEdit::singleline(&mut d.path).desired_width(ui.available_width()));
                        r.widget_info(|| egui::WidgetInfo::labeled(egui::Role::TextInput, true, PATH));
                    });
                });
                ui.add_space(space::S);
                egui::CollapsingHeader::new(egui::RichText::new("On every page").strong().color(c32(s.text))).default_open(true).show(ui, |ui| {
                    kv(ui, "Revision", &mut |ui| {
                        let words = pending.as_ref().map_or_else(|| "none pending".to_string(), |r| format!("{r} · pending"));
                        ui.label(egui::RichText::new(words).color(c32(s.text_2)));
                    });
                    ui.add_enabled_ui(false, |ui| {
                        let mut off = false;
                        ui.checkbox(&mut off, "Revision clouds since the last release").on_disabled_hover_text("Comes later");
                        ui.checkbox(&mut off, "Watermark").on_disabled_hover_text("Comes later");
                        ui.checkbox(&mut off, "Bookmarks and cross-reference links").on_disabled_hover_text("Comes later");
                    });
                    kv(ui, "Numbering", &mut |ui| {
                        ui.label(egui::RichText::new("Book").color(c32(s.text_2)));
                    });
                });
                ui.add_space(space::S);
                let title = format!("As it will print{}", preview_words.as_ref().map_or_else(String::new, |w| format!(" · {w}")));
                egui::CollapsingHeader::new(egui::RichText::new(title).strong().color(c32(s.text)))
                    .id_salt("publish.as_printed")
                    .default_open(true)
                    .show(ui, |ui| {
                        let w = ui.available_width();
                        let (r, resp) = ui.allocate_exact_size(vec2(w, w * THUMB_H / THUMB_W), Sense::hover());
                        resp.widget_info(|| egui::WidgetInfo::labeled(egui::Role::Image, true, PREVIEW));
                        match thumb {
                            Some(t) => {
                                ui.painter().image(t, r, Rect::from_min_max(pos2(0.0, 0.0), pos2(1.0, 1.0)), egui::Color32::WHITE);
                            }
                            None => {
                                ui.painter().rect_filled(r, 0.0, c32(crate::canvas::Sheet::Paper.tokens().sheet));
                            }
                        }
                        ui.label(egui::RichText::new("Click any page in the grid to preview it here.").small().color(c32(s.text_3)));
                    });
                for n in &d.notes {
                    ui.label(egui::RichText::new(n).small().color(c32(s.warning)))
                        .widget_info(|| egui::WidgetInfo::labeled(egui::Role::Label, true, "ecad.printbook.note"));
                }
                if d.flagged > 0 {
                    let n = if d.flagged == 1 { "1 value".to_string() } else { format!("{} values", d.flagged) };
                    ui.label(egui::RichText::new(format!("{n} marked ? on generated pages.")).small().color(c32(s.text_3)));
                }
            });
        });
        let mut export = false;
        let note = self.publish.note.clone();
        crate::shell::in_rect(ui, foot.shrink2(vec2(space::L, space::M)), "publish.right.foot", |ui| {
            ui.horizontal(|ui| {
                let n = d.sheets;
                ui.label(egui::RichText::new(format!("{n} {}", if n == 1 { "sheet" } else { "sheets" })).small().color(c32(s.text_2)));
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if let Some(e) = &d.error {
                        ui.label(egui::RichText::new(e).small().color(c32(s.error)));
                    } else if n == 0 {
                        ui.label(egui::RichText::new("No page matches.").small().color(c32(s.warning)));
                    } else if let Some(note) = &note {
                        ui.label(egui::RichText::new(note).small().color(c32(s.ok)));
                    }
                });
            });
            ui.add_space(space::S);
            ui.horizontal(|ui| {
                let can = d.sheets > 0 && !d.path.trim().is_empty();
                let pr = ui.add(ecad_ui::prim::Button::new("Print…").disabled("Printing to a printer comes later"));
                pr.widget_info(|| egui::WidgetInfo::labeled(egui::Role::Button, false, PRINT));
                let w = ui.available_width();
                let b = ui.add_enabled(can, ecad_ui::prim::Button::new("Export PDF…").kind(ecad_ui::prim::Kind::Primary));
                let _ = w;
                b.widget_info(|| egui::WidgetInfo::labeled(egui::Role::Button, can, EXPORT));
                export = b.clicked();
            });
        });
        if export {
            self.publish.note = match self.print_from_dialog() {
                Ok(v) => Some(format!("Wrote {}", v["path"].as_str().unwrap_or("the PDF"))),
                Err(_) => None,
            };
            self.publish.counted = None;
        }
    }
}
