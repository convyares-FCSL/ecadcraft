//! The Library browser (design 048 §8.1; UI rev 3 board 3.01, Library ▸ Symbols): the Library's
//! tabs (Symbols, Snippets, Parts, Makers, Title blocks; only Symbols has data today, the others
//! say what comes) with the search and New symbol, the filters across the top, the category tree
//! at the left with the sources under it, the tiles in the centre grouped by category, and the
//! selected symbol in the right panel. Project symbols and drafts are listed with the built-ins;
//! every tile names its source; hovering a tile shows its connection points and text slots. A
//! lineage shows its newest version only.
//!
//! Filters that need data the symbols do not carry yet (poles, contact kind, the US standard,
//! the maker) are drawn greyed with their reason; Source (All, Built-in, Project) filters.

use std::collections::BTreeSet;

use ecad_cadcraft::symbols::{self, Family, Starter};
use ecad_model::{Definition, DefinitionId, Document, Primitive, Pt};
use ecad_ui::tokens::{space, ui_type};
use eframe::egui::{self, Align2, Color32, Pos2, Rect, Sense, Stroke, pos2, vec2};

use crate::shell::bar;

/// Element names (design 048 §8.1).
pub const BROWSER: &str = "library.browser";
pub const NEW: &str = "library.browser.new";
pub const SEARCH: &str = "library.browser.search";
pub const PLACE: &str = "library.browser.place";
pub const EDIT: &str = "library.browser.edit";
pub const RESUME: &str = "library.browser.resume";
pub const FORGET: &str = "library.browser.forget";
pub const PLACED_NEXT: &str = "library.browser.placed-next";
/// The Library's tabs: `library.tab.<word>`.
pub const TAB: &str = "library.tab";
/// A tile is `library.browser.tile.<lineage hex>` (a project symbol) or `.<key>` (a built-in).
pub const TILE: &str = "library.browser.tile";
/// The tree's width inside the main view (board 3.01).
pub const TREE_W: f32 = 236.0;
/// A tile's size (board 3.01).
pub const TILE_W: f32 = 164.0;
pub const TILE_H: f32 = 150.0;
/// The tab row's and the filter row's heights.
pub const TABS_H: f32 = 44.0;
pub const FILTERS_H: f32 = 40.0;

/// The Library's tabs (rev 3). Only Symbols has data today.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum LibraryTab {
    #[default]
    Symbols,
    Snippets,
    Parts,
    Makers,
    TitleBlocks,
}

impl LibraryTab {
    pub const ALL: [LibraryTab; 5] = [LibraryTab::Symbols, LibraryTab::Snippets, LibraryTab::Parts, LibraryTab::Makers, LibraryTab::TitleBlocks];

    pub fn label(self) -> &'static str {
        match self {
            LibraryTab::Symbols => "Symbols",
            LibraryTab::Snippets => "Snippets",
            LibraryTab::Parts => "Parts",
            LibraryTab::Makers => "Makers",
            LibraryTab::TitleBlocks => "Title blocks",
        }
    }

    pub fn name(self) -> String {
        format!("{TAB}.{}", self.label().to_lowercase().replace(' ', "_"))
    }

    /// What a tab without data says (a placeholder).
    pub fn later(self) -> &'static str {
        match self {
            LibraryTab::Symbols => "",
            LibraryTab::Snippets => "Snippets are small circuits you reuse: a motor starter, a safety relay with its contacts. They come later.",
            LibraryTab::Parts => "Parts hold manufacturer data: part numbers, ratings and the symbols each part is drawn with. They come later.",
            LibraryTab::Makers => "Makers build a device from a few answers, like the connector maker. They come later.",
            LibraryTab::TitleBlocks => "Title blocks and frames you can choose for a project's sheets. They come later.",
        }
    }
}

/// What a tile shows.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Tile {
    Project(DefinitionId),
    Builtin(&'static str),
    Draft(u64),
}

impl Tile {
    pub fn name(self) -> String {
        match self {
            Tile::Project(l) => format!("{TILE}.{}", l.hex()),
            Tile::Builtin(k) => format!("{TILE}.{k}"),
            Tile::Draft(n) => format!("{TILE}.draft.{n}"),
        }
    }
}

/// A category of the tree.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Default)]
pub enum Category {
    #[default]
    All,
    Project,
    Drafts,
    Builtin(u8),
}

/// The Source filter (board 3.01's filter row).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum SourceFilter {
    #[default]
    All,
    Builtin,
    Project,
}

/// The families in tree order, with their names.
pub const FAMILIES: [(Family, &str); 7] = [
    (Family::Basic, "Basic"),
    (Family::Contacts, "Contacts"),
    (Family::Protection, "Protection"),
    (Family::Sensors, "Sensors"),
    (Family::Actuators, "Actuators"),
    (Family::Connectors, "Connectors"),
    (Family::Plc, "PLC"),
];

/// The browser's state between frames.
#[derive(Debug, Clone, Default)]
pub struct Browser {
    pub tab: LibraryTab,
    pub query: String,
    pub category: Category,
    pub source: SourceFilter,
    pub selected: Option<Tile>,
    /// After Save: "Placed next: <name>" with its Place button (design 048 §3.3 step 7).
    pub placed_next: Option<String>,
    pub collapsed: BTreeSet<u8>,
}

/// One tile's data, read from the project before drawing.
#[derive(Debug, Clone)]
pub struct Entry {
    pub tile: Tile,
    pub name: String,
    pub key: String,
    /// `Project · v3`, `Built-in · read-only`, `Draft`.
    pub source: String,
    pub def: Option<Definition>,
    pub family: Option<u8>,
    /// Placed copies (all versions of a project symbol).
    pub placed: usize,
    /// `v3 (12 placed) · v2 (1 placed)`.
    pub versions: Option<String>,
    /// A project copy of a built-in: `from Coil (built-in v1)`.
    pub from: Option<String>,
    /// A built-in's tag prefix (`-K`).
    pub prefix: Option<&'static str>,
}

/// What the browser shows: the project's symbols and drafts, then the built-ins.
#[derive(Debug, Clone, Default)]
pub struct Listing {
    pub entries: Vec<Entry>,
}

fn family_index(f: Family) -> u8 {
    FAMILIES.iter().position(|(x, _)| *x == f).map_or(0, |i| i as u8)
}

impl Listing {
    /// Read the listing from `doc` (`None`: no project, built-ins only) and the drafts.
    pub fn read(doc: Option<&Document>, drafts: &[super::Draft]) -> Self {
        let mut entries = Vec::new();
        if let Some(doc) = doc {
            for v in symbols::project_symbols(doc) {
                let versions = symbols::versions_of(doc, v.lineage);
                let placed: Vec<usize> = versions.iter().map(|x| doc.items_using(x.id).len()).collect();
                let words: Vec<String> = versions.iter().zip(&placed).rev().map(|(x, n)| format!("v{} ({n} placed)", x.seq)).collect();
                let from = v.def.derived_from.as_ref().and_then(symbols::starter_of_origin).map(|s| format!("from {} (built-in v1)", s.name));
                entries.push(Entry {
                    tile: Tile::Project(v.lineage),
                    name: v.def.name.clone(),
                    key: v.key().unwrap_or_default().to_string(),
                    source: format!("Project · v{}", v.seq),
                    def: Some(v.def.clone()),
                    family: None,
                    placed: placed.iter().sum(),
                    versions: Some(words.join(" · ")),
                    from,
                    prefix: None,
                });
            }
        }
        for d in drafts {
            entries.push(Entry {
                tile: Tile::Draft(d.id),
                name: d.shown_name().to_string(),
                key: d.key.clone(),
                source: "Draft · not saved".into(),
                def: Some(d.content().definition()),
                family: None,
                placed: 0,
                versions: None,
                from: None,
                prefix: None,
            });
        }
        for s in symbols::STARTERS {
            let placed = doc.and_then(|d| symbols::embedded(d, s).map(|id| d.items_using(id).len())).unwrap_or(0);
            entries.push(Entry {
                tile: Tile::Builtin(s.key),
                name: s.name.to_string(),
                key: s.key.to_string(),
                source: "IEC built-in".into(),
                def: s.definition().ok().cloned(),
                family: Some(family_index(s.family)),
                placed,
                versions: None,
                from: None,
                prefix: Some(s.prefix()),
            });
        }
        Listing { entries }
    }

    pub fn project_count(&self) -> usize {
        self.entries.iter().filter(|e| matches!(e.tile, Tile::Project(_))).count()
    }

    pub fn draft_count(&self) -> usize {
        self.entries.iter().filter(|e| matches!(e.tile, Tile::Draft(_))).count()
    }

    pub fn builtin_count(&self) -> usize {
        self.entries.iter().filter(|e| matches!(e.tile, Tile::Builtin(_))).count()
    }

    /// The tiles shown for the category, the source and the query.
    pub fn shown(&self, b: &Browser) -> Vec<&Entry> {
        let q = b.query.trim().to_lowercase();
        let starters: Option<Vec<&'static Starter>> = (!q.is_empty()).then(|| symbols::search(&q));
        self.entries
            .iter()
            .filter(|e| match b.category {
                Category::All => true,
                Category::Project => matches!(e.tile, Tile::Project(_)),
                Category::Drafts => matches!(e.tile, Tile::Draft(_)),
                Category::Builtin(f) => e.family == Some(f),
            })
            .filter(|e| match b.source {
                SourceFilter::All => true,
                SourceFilter::Builtin => matches!(e.tile, Tile::Builtin(_)),
                SourceFilter::Project => !matches!(e.tile, Tile::Builtin(_)),
            })
            .filter(|e| match (&starters, e.tile) {
                (None, _) => true,
                (Some(found), Tile::Builtin(k)) => found.iter().any(|s| s.key == k),
                (Some(_), _) => e.name.to_lowercase().contains(&q) || e.key.to_lowercase().contains(&q),
            })
            .collect()
    }

    pub fn get(&self, t: Tile) -> Option<&Entry> {
        self.entries.iter().find(|e| e.tile == t)
    }
}

/// The group a tile is listed under in the centre: This project, Drafts, then the families.
fn group_of(e: &Entry) -> (u8, &'static str) {
    match (e.tile, e.family) {
        (Tile::Project(_), _) => (0, "This project"),
        (Tile::Draft(_), _) => (1, "Drafts"),
        (Tile::Builtin(_), Some(f)) => (2 + f, FAMILIES.get(usize::from(f)).map_or("Other", |x| x.1)),
        (Tile::Builtin(_), None) => (100, "Other"),
    }
}

/// What the browser asks of the window.
#[derive(Debug, Clone, PartialEq)]
pub enum Act {
    New,
    Place(Tile),
    Edit(Tile),
    Resume(u64),
    Forget(u64),
}

fn c32(c: ecad_ui::tokens::Color) -> Color32 {
    ecad_ui::color(c)
}

/// Draw definition `d` fitted in `rect` in `ink`; `marks`: its points and slots too (hover).
pub fn paint_symbol(p: &egui::Painter, rect: Rect, d: &Definition, ink: Color32, marks: Option<(Color32, Color32)>) {
    let (lo, hi) = symbols::bounds(d);
    let (w, h) = ((hi.x - lo.x).max(5.0), (hi.y - lo.y).max(5.0));
    let s = ((rect.width() as f64) / w).min((rect.height() as f64) / h).min(8.0);
    if !s.is_finite() || s <= 0.0 {
        return;
    }
    let c = Pt::new((lo.x + hi.x) / 2.0, (lo.y + hi.y) / 2.0);
    let at = |q: Pt| pos2(rect.center().x + ((q.x - c.x) * s) as f32, rect.center().y - ((q.y - c.y) * s) as f32);
    let st = Stroke::new(((0.35 * s) as f32).clamp(1.0, 2.0), ink);
    for g in &d.graphics {
        match g {
            Primitive::Line { a, b } => {
                p.line_segment([at(*a), at(*b)], st);
            }
            Primitive::Polyline { vertices, closed } => {
                let pts: Vec<Pos2> = vertices.iter().map(|v| at(*v)).collect();
                p.add(if *closed { egui::Shape::closed_line(pts, st) } else { egui::Shape::line(pts, st) });
            }
            Primitive::Circle { center, radius } => {
                p.circle_stroke(at(*center), (*radius * s) as f32, st);
            }
            Primitive::Arc { center, radius, start, end } => {
                p.add(egui::Shape::line(super::builder::arc_points(*center, *radius, *start, *end).into_iter().map(at).collect(), st));
            }
            Primitive::Text { at: a, height, value, .. } => {
                p.text(at(*a), Align2::LEFT_BOTTOM, value, ecad_ui::style::ui_font(((*height * s) as f32 * 1.25).clamp(5.0, 14.0)), ink);
            }
            Primitive::Point { at: a } => {
                p.circle_filled(at(*a), 1.5, ink);
            }
        }
    }
    if let Some((ring, slot)) = marks {
        for t in &d.terminals {
            p.circle_stroke(at(t.at), 3.5, Stroke::new(1.25, ring));
            p.text(at(t.at) + vec2(5.0, -5.0), Align2::LEFT_BOTTOM, &t.key, ecad_ui::style::ui_font(ecad_ui::tokens::ui_type::MICRO.size), ring);
        }
        for sl in &d.texts {
            p.circle_filled(at(sl.at), 2.0, slot);
            p.text(
                at(sl.at) + vec2(4.0, 0.0),
                Align2::LEFT_CENTER,
                super::draft::slot_label(&sl.text),
                ecad_ui::style::ui_font(ecad_ui::tokens::ui_type::MICRO.size),
                slot,
            );
        }
    }
}

/// A chip in the filter row: on (accent), off, or greyed with its reason.
fn chip(ui: &mut egui::Ui, text: &str, on: bool, enabled: bool, reason: &str) -> bool {
    let s = ecad_ui::SHELL;
    let font = bar::font(ui, ui_type::SMALL);
    let w = bar::text_width(ui, text, ui_type::SMALL) + 2.0 * space::S;
    let (r, resp) = ui.allocate_exact_size(vec2(w, 22.0), if enabled { Sense::click() } else { Sense::hover() });
    let fill = if on {
        Some(s.accent_soft)
    } else if resp.hovered() && enabled {
        Some(s.hover)
    } else {
        Some(s.raised)
    };
    if let Some(f) = fill {
        ui.painter().rect_filled(r, ecad_ui::style::corner(4.0), c32(f));
    }
    let ink = if on {
        s.accent_text
    } else if enabled {
        s.text_2
    } else {
        s.text_3
    };
    ui.painter().text(r.center(), Align2::CENTER_CENTER, text, font, c32(ink));
    let label = format!("library.filter.{}", text.to_lowercase().replace(' ', "_"));
    resp.widget_info(|| egui::WidgetInfo::selected(egui::Role::Button, enabled, on, &label));
    let clicked = resp.clicked();
    if !enabled {
        resp.on_hover_text(reason);
    }
    clicked && enabled
}

fn filter_label(ui: &mut egui::Ui, text: &str) {
    ui.label(egui::RichText::new(text).font(bar::font(ui, ui_type::SMALL)).color(c32(ecad_ui::SHELL.text_3)));
}

fn divider(ui: &mut egui::Ui) {
    let (r, _) = ui.allocate_exact_size(vec2(space::M, 18.0), Sense::hover());
    ui.painter().vline(r.center().x, r.y_range(), Stroke::new(1.0, c32(ecad_ui::SHELL.line)));
}

/// The browser in the main view.
pub fn show(ui: &mut egui::Ui, main: Rect, b: &mut Browser, list: &Listing, sheet: &ecad_ui::tokens::Sheet) -> Vec<Act> {
    let s = ecad_ui::SHELL;
    let line = Stroke::new(ecad_ui::tokens::line::HAIRLINE_PX, c32(s.line));
    let mut acts = Vec::new();
    ui.painter().rect_filled(main, 0.0, c32(s.bg));

    // The tab row: the tabs, the search, Open an .ecadlib and New symbol.
    let head = Rect::from_min_size(main.min, vec2(main.width(), TABS_H));
    ui.painter().rect_filled(head, 0.0, c32(s.panel));
    ui.painter().hline(head.x_range(), head.bottom() - 0.5, line);
    let total = list.entries.len();
    ui.scope_builder(egui::UiBuilder::new().max_rect(head.shrink2(vec2(space::M, space::S))), |ui| {
        ui.horizontal_centered(|ui| {
            // The tabs, one pill.
            let labels: Vec<String> = LibraryTab::ALL
                .iter()
                .map(|t| match t {
                    LibraryTab::Symbols => format!("{} {}", t.label(), list.project_count() + list.builtin_count()),
                    _ => t.label().to_string(),
                })
                .collect();
            let widths: Vec<f32> = labels.iter().map(|l| bar::text_width(ui, l, ui_type::BODY) + 2.0 * space::M).collect();
            let (pill, _) = ui.allocate_exact_size(vec2(widths.iter().sum::<f32>() + 2.0 * space::XS, 28.0), Sense::hover());
            bar::pill(ui, pill);
            let mut x = pill.left() + space::XS;
            for ((tab, label), w) in LibraryTab::ALL.iter().zip(&labels).zip(&widths) {
                let r = Rect::from_min_size(pos2(x, pill.top() + 3.0), vec2(*w, 22.0));
                let on = b.tab == *tab;
                let resp = bar::text_button(ui, r, label, &tab.name(), on, false);
                resp.widget_info(|| egui::WidgetInfo::selected(egui::Role::Tab, true, on, tab.name()));
                if resp.clicked() {
                    b.tab = *tab;
                }
                x += w;
            }
            ui.add_space(space::M);
            let f = ecad_ui::prim::Field::new(&mut b.query, ecad_ui::prim::FieldKind::Search, SEARCH)
                .placeholder("Find a symbol by name, tag letter or pin")
                .width(420.0)
                .show(ui);
            let _ = f;
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                let n = ui.add(ecad_ui::prim::Button::new("+  New symbol").kind(ecad_ui::prim::Kind::Primary).small());
                n.widget_info(|| egui::WidgetInfo::labeled(egui::Role::Button, true, NEW));
                if n.clicked() {
                    acts.push(Act::New);
                }
                ui.add(ecad_ui::prim::Button::new("Open an .ecadlib").small().disabled("Shared libraries come later"));
            });
        });
    });

    if b.tab != LibraryTab::Symbols {
        let body = Rect::from_min_max(pos2(main.left(), head.bottom()), main.max);
        ui.scope_builder(egui::UiBuilder::new().max_rect(body.shrink(space::XL)), |ui| {
            ui.add_space(space::XL);
            ui.vertical_centered(|ui| {
                ui.label(egui::RichText::new(b.tab.label()).font(bar::font(ui, ui_type::TITLE)).color(c32(s.text)));
                ui.add_space(space::S);
                ui.label(egui::RichText::new(b.tab.later()).color(c32(s.text_2)));
            });
        });
        return acts;
    }

    // The filter row.
    let shown = list.shown(b);
    let filters = Rect::from_min_size(pos2(main.left(), head.bottom()), vec2(main.width(), FILTERS_H));
    ui.painter().rect_filled(filters, 0.0, c32(s.panel));
    ui.painter().hline(filters.x_range(), filters.bottom() - 0.5, line);
    let later = "Needs data the symbols do not carry yet";
    ui.scope_builder(egui::UiBuilder::new().max_rect(filters.shrink2(vec2(space::M, space::XS))), |ui| {
        ui.horizontal_centered(|ui| {
            ui.spacing_mut().item_spacing.x = space::XS;
            filter_label(ui, "Poles");
            chip(ui, "Any", true, false, later);
            for n in ["1", "2", "3", "4"] {
                chip(ui, n, false, false, later);
            }
            divider(ui);
            filter_label(ui, "Contact");
            for n in ["NO", "NC", "Change-over"] {
                chip(ui, n, false, false, later);
            }
            divider(ui);
            filter_label(ui, "Standard");
            chip(ui, "IEC", true, false, "Every symbol is IEC today");
            chip(ui, "US", false, false, "US symbols come with the US standard");
            divider(ui);
            filter_label(ui, "Source");
            let project = list.project_count() + list.draft_count();
            for (f, text) in [
                (SourceFilter::All, "All".to_string()),
                (SourceFilter::Builtin, format!("Built-in {}", list.builtin_count())),
                (SourceFilter::Project, format!("Project {project}")),
            ] {
                if chip(ui, &text, b.source == f, true, "") {
                    b.source = f;
                }
            }
            divider(ui);
            chip(ui, "Maker: any", false, false, "Makers come with parts");
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                ui.label(egui::RichText::new(format!("{} of {total} shown", shown.len())).font(bar::font(ui, ui_type::SMALL)).color(c32(s.text_3)));
            });
        });
    });

    // The tree, with the sources at its foot.
    let tree = Rect::from_min_max(pos2(main.left(), filters.bottom()), pos2(main.left() + TREE_W, main.bottom()));
    ui.painter().rect_filled(tree, 0.0, c32(s.panel));
    ui.painter().vline(tree.right() - 0.5, tree.y_range(), line);
    let sources_h = 112.0;
    let tree_top = Rect::from_min_max(tree.min, pos2(tree.right(), tree.bottom() - sources_h));
    ui.scope_builder(egui::UiBuilder::new().max_rect(tree_top.shrink2(vec2(0.0, space::S))), |ui| {
        let row = |ui: &mut egui::Ui, b: &mut Browser, c: Category, label: &str, n: usize| {
            let (r, resp) = ui.allocate_exact_size(vec2(ui.available_width(), 28.0), Sense::click());
            let on = b.category == c;
            resp.widget_info(|| egui::WidgetInfo::selected(egui::Role::Button, true, on, format!("library.category.{}", label.to_lowercase())));
            if on {
                ui.painter().rect_filled(r, 0.0, c32(s.accent_soft));
            } else if resp.hovered() {
                ui.painter().rect_filled(r, 0.0, c32(s.hover));
            }
            bar::text_at(ui, r.left() + space::L, r.center().y, label, ui_type::BODY, c32(if on { s.text } else { s.text_2 }));
            let f = bar::font(ui, ui_type::SMALL);
            ui.painter().text(pos2(r.right() - space::M, r.center().y), Align2::RIGHT_CENTER, n.to_string(), f, c32(s.text_3));
            if resp.clicked() {
                b.category = c;
            }
        };
        ui.spacing_mut().item_spacing.y = 0.0;
        row(ui, b, Category::All, "All symbols", total);
        if list.project_count() > 0 {
            row(ui, b, Category::Project, "This project", list.project_count());
        }
        if list.draft_count() > 0 {
            row(ui, b, Category::Drafts, "Drafts", list.draft_count());
        }
        for (i, (_, name)) in FAMILIES.iter().enumerate() {
            let n = list.entries.iter().filter(|e| e.family == Some(i as u8)).count();
            row(ui, b, Category::Builtin(i as u8), name, n);
        }
    });
    let src = Rect::from_min_max(pos2(tree.left(), tree.bottom() - sources_h), tree.max);
    ui.painter().hline(src.x_range(), src.top() + 0.5, line);
    let mut y = src.top() + space::M + 6.0;
    bar::text_at(ui, src.left() + space::L, y, "Sources", ui_type::CAPTION, c32(s.text_3));
    y += 22.0;
    let f = bar::font(ui, ui_type::SMALL);
    for (name, n) in [("IEC built-in", format!("{} · read-only", list.builtin_count())), ("This project", list.project_count().to_string())] {
        bar::text_at(ui, src.left() + space::L, y, name, ui_type::SMALL, c32(s.text));
        ui.painter().text(pos2(src.right() - space::M, y), Align2::RIGHT_CENTER, n, f.clone(), c32(s.text_3));
        y += 20.0;
    }
    if list.draft_count() > 0 {
        bar::text_at(ui, src.left() + space::L, y, "Drafts", ui_type::SMALL, c32(s.text));
        ui.painter().text(pos2(src.right() - space::M, y), Align2::RIGHT_CENTER, list.draft_count().to_string(), f.clone(), c32(s.warning));
    }

    // The tiles, grouped by category.
    let grid = Rect::from_min_max(pos2(tree.right(), filters.bottom()), main.max);
    let footer_h = if b.placed_next.is_some() { 40.0 } else { 0.0 };
    let body = Rect::from_min_max(grid.min, pos2(grid.right(), grid.bottom() - footer_h));
    let mut groups: Vec<((u8, &'static str), Vec<&Entry>)> = Vec::new();
    for e in &shown {
        let g = group_of(e);
        match groups.iter_mut().find(|(k, _)| *k == g) {
            Some((_, v)) => v.push(e),
            None => groups.push((g, vec![e])),
        }
    }
    groups.sort_by_key(|(k, _)| k.0);
    ui.scope_builder(egui::UiBuilder::new().max_rect(body.shrink2(vec2(space::XL, space::L))), |ui| {
        egui::ScrollArea::vertical().id_salt("library.browser.scroll").auto_shrink([false, false]).show(ui, |ui| {
            if shown.is_empty() {
                ui.label(egui::RichText::new(format!("Nothing matches \"{}\".", b.query.trim())).color(c32(s.text_3)));
            }
            for ((_, name), entries) in &groups {
                ui.horizontal(|ui| {
                    ui.label(egui::RichText::new(*name).font(bar::font(ui, ui_type::TITLE)).color(c32(s.text)));
                    let n = entries.len();
                    ui.label(egui::RichText::new(format!("{n} {}", if n == 1 { "symbol" } else { "symbols" })).color(c32(s.text_3)));
                });
                ui.add_space(space::S);
                ui.horizontal_wrapped(|ui| {
                    ui.spacing_mut().item_spacing = vec2(space::M, space::M);
                    for e in entries {
                        tile(ui, b, e, sheet, &mut acts);
                    }
                });
                ui.add_space(space::L);
            }
        });
    });
    if let Some(name) = b.placed_next.clone() {
        let f = Rect::from_min_max(pos2(grid.left(), grid.bottom() - footer_h), grid.max);
        ui.painter().rect_filled(f, 0.0, c32(s.panel));
        ui.painter().hline(f.x_range(), f.top() + 0.5, line);
        ui.scope_builder(egui::UiBuilder::new().max_rect(f.shrink2(vec2(space::L, space::XS))), |ui| {
            ui.horizontal_centered(|ui| {
                ui.label(format!("Placed next: {name}")).widget_info(|| egui::WidgetInfo::labeled(egui::Role::Label, true, PLACED_NEXT));
                if let Some(t) = b.selected {
                    let r = ui.add(ecad_ui::prim::Button::new("Place").kind(ecad_ui::prim::Kind::Primary).small());
                    if r.clicked() {
                        acts.push(Act::Place(t));
                    }
                }
            });
        });
    }
    acts
}

/// One tile: the symbol on paper, its name and its source; hover names its points and slots.
fn tile(ui: &mut egui::Ui, b: &mut Browser, e: &Entry, sheet: &ecad_ui::tokens::Sheet, acts: &mut Vec<Act>) {
    let s = ecad_ui::SHELL;
    let (r, resp) = ui.allocate_exact_size(vec2(TILE_W, TILE_H), Sense::click());
    resp.widget_info(|| egui::WidgetInfo::selected(egui::Role::Button, true, b.selected == Some(e.tile), e.tile.name()));
    let p = ui.painter_at(r);
    let chosen = b.selected == Some(e.tile);
    p.rect_filled(r, 6.0, c32(if chosen { s.accent_soft } else { s.panel }));
    p.rect_stroke(
        r,
        6.0,
        Stroke::new(
            if chosen { 2.0 } else { 1.0 },
            c32(if chosen {
                s.accent
            } else if resp.hovered() {
                s.line_strong
            } else {
                s.line
            }),
        ),
        egui::StrokeKind::Inside,
    );
    let pic = Rect::from_min_size(r.min + vec2(8.0, 8.0), vec2(r.width() - 16.0, 84.0));
    p.rect_filled(pic, 3.0, c32(sheet.sheet));
    if let Some(d) = &e.def {
        let marks = resp.hovered().then(|| (c32(sheet.connection_point), c32(sheet.placement_preview)));
        paint_symbol(&p, pic.shrink(10.0), d, c32(sheet.ink), marks);
    }
    let name = p.layout(e.name.clone(), ecad_ui::style::ui_font(ecad_ui::tokens::ui_type::BODY.size), c32(s.text), r.width() - 20.0);
    let name_h = name.size().y.min(36.0);
    p.with_clip_rect(Rect::from_min_size(pos2(r.left() + 10.0, pic.bottom() + 6.0), vec2(r.width() - 20.0, 36.0))).galley(
        pos2(r.left() + 10.0, pic.bottom() + 6.0),
        name,
        c32(s.text),
    );
    let tone = if matches!(e.tile, Tile::Draft(_)) { s.warning } else { s.text_3 };
    let src = match e.tile {
        Tile::Project(_) => "Project",
        _ => e.source.as_str(),
    };
    p.text(
        pos2(r.left() + 10.0, (pic.bottom() + 10.0 + name_h).max(r.bottom() - 22.0)),
        Align2::LEFT_TOP,
        src,
        ecad_ui::style::ui_font(ecad_ui::tokens::ui_type::CAPTION.size),
        c32(tone),
    );
    if resp.clicked() {
        b.selected = Some(e.tile);
    }
    if resp.double_clicked() {
        acts.push(match e.tile {
            Tile::Draft(n) => Act::Resume(n),
            t => Act::Place(t),
        });
    }
    if let Some(d) = &e.def {
        resp.on_hover_ui(|ui| {
            ui.label(egui::RichText::new(&e.name).strong());
            let keys: Vec<&str> = d.terminals.iter().map(|t| t.key.as_str()).collect();
            let n = keys.len();
            ui.label(
                egui::RichText::new(format!("{n} connection {}: {}", if n == 1 { "point" } else { "points" }, keys.join(", ")))
                    .color(c32(ecad_ui::SHELL.connect)),
            );
            if !d.texts.is_empty() {
                let slots: Vec<String> = d.texts.iter().map(|sl| super::draft::slot_label(&sl.text).to_string()).collect();
                ui.label(format!("{} text {}: {}", slots.len(), if slots.len() == 1 { "slot" } else { "slots" }, slots.join(", ")));
            }
            if let Some(px) = e.prefix {
                ui.label(egui::RichText::new(format!("Tag letter {}", px.trim_start_matches('-'))).color(c32(ecad_ui::SHELL.text_2)));
            }
        });
    }
}

/// The selected symbol in the right panel (board 3.01's right panel).
pub fn details(ui: &mut egui::Ui, b: &Browser, list: &Listing, writable: bool, page: Option<&str>) -> Vec<Act> {
    let s = ecad_ui::SHELL;
    let mut acts = Vec::new();
    let Some(e) = b.selected.and_then(|t| list.get(t)) else {
        ui.add_space(space::L);
        ui.label(
            egui::RichText::new("Select a symbol to see its connection points, text slots, source and where it is used.")
                .small()
                .color(c32(s.text_3)),
        );
        return acts;
    };
    if let Some(d) = &e.def {
        let (r, _) = ui.allocate_exact_size(vec2(ui.available_width(), 170.0), Sense::hover());
        let sheet = crate::canvas::Sheet::Paper.tokens();
        ui.painter().rect_filled(r, 4.0, c32(sheet.sheet));
        paint_symbol(ui.painter(), r.shrink(24.0), d, c32(sheet.ink), Some((c32(sheet.connection_point), c32(sheet.placement_preview))));
    }
    ui.add_space(space::M);
    ui.label(egui::RichText::new(&e.name).font(bar::font(ui, ui_type::TITLE)).color(c32(s.text)));
    ui.horizontal_wrapped(|ui| {
        let (pill, rest) = match e.tile {
            Tile::Builtin(_) => ("IEC built-in", "read-only · editing makes a project copy"),
            Tile::Project(_) => ("project", ""),
            Tile::Draft(_) => ("draft", "not saved"),
        };
        ecad_ui::prim::pill(ui, pill, ecad_ui::prim::Tone::Neutral);
        if !rest.is_empty() {
            ui.label(egui::RichText::new(rest).small().color(c32(s.text_2)));
        }
    });
    if let Some(f) = &e.from {
        ui.label(egui::RichText::new(format!("Project · {f}")).small().color(c32(s.text_2)));
    }
    ui.add_space(space::M);
    let fold = |ui: &mut egui::Ui, title: &str, add: &mut dyn FnMut(&mut egui::Ui)| {
        egui::CollapsingHeader::new(egui::RichText::new(title).strong().color(c32(s.text)))
            .id_salt(("library.details", title))
            .default_open(true)
            .show(ui, add);
    };
    let kv = |ui: &mut egui::Ui, k: &str, v: &str| {
        ui.horizontal(|ui| {
            let (r, _) = ui.allocate_exact_size(vec2(100.0, 22.0), Sense::hover());
            ui.painter().text(pos2(r.left(), r.center().y), Align2::LEFT_CENTER, k, bar::font(ui, ui_type::SMALL), c32(s.text_2));
            ui.label(egui::RichText::new(v).color(c32(s.text)));
        });
    };
    fold(ui, "About this symbol", &mut |ui| {
        let category = match (e.tile, e.family) {
            (Tile::Builtin(_), Some(f)) => FAMILIES.get(usize::from(f)).map_or("", |x| x.1).to_string(),
            (Tile::Project(_), _) => "This project".into(),
            _ => "Drafts".into(),
        };
        kv(ui, "Category", &category);
        if let Some(px) = e.prefix {
            kv(ui, "Tag letter", px.trim_start_matches('-'));
        }
        if !e.key.is_empty() {
            kv(ui, "Key", &e.key);
        }
        if let Some(d) = &e.def {
            let keys: Vec<&str> = d.terminals.iter().map(|t| t.key.as_str()).collect();
            kv(ui, "Connects at", &if keys.is_empty() { "none".to_string() } else { keys.join(", ") });
            let slots: Vec<String> = d.texts.iter().map(|sl| super::draft::slot_label(&sl.text).to_string()).collect();
            kv(ui, "Text slots", &if slots.is_empty() { "none".to_string() } else { slots.join(", ") });
        }
        if let Some(v) = &e.versions {
            kv(ui, "Versions", v);
        }
    });
    if !matches!(e.tile, Tile::Draft(_)) {
        let n = e.placed;
        fold(ui, &format!("Used {n} {} in this project", if n == 1 { "time" } else { "times" }), &mut |ui| {
            if n == 0 {
                ui.label(egui::RichText::new("Not placed yet.").small().color(c32(s.text_3)));
            } else {
                ui.label(egui::RichText::new("Its devices are listed in Project ▸ Devices.").small().color(c32(s.text_3)));
            }
        });
    }
    ui.add_space(space::M);
    ui.horizontal(|ui| match e.tile {
        Tile::Draft(n) => {
            let r = ui.add(ecad_ui::prim::Button::new("Resume").kind(ecad_ui::prim::Kind::Primary).small());
            r.widget_info(|| egui::WidgetInfo::labeled(egui::Role::Button, true, RESUME));
            if r.clicked() {
                acts.push(Act::Resume(n));
            }
            let f = ui.add(ecad_ui::prim::Button::new("Discard").kind(ecad_ui::prim::Kind::Danger).small());
            f.widget_info(|| egui::WidgetInfo::labeled(egui::Role::Button, true, FORGET));
            if f.clicked() {
                acts.push(Act::Forget(n));
            }
        }
        t => {
            let words = page.map_or_else(|| "Place".to_string(), |p| format!("Place on page {p}"));
            let r = ui.add(ecad_ui::prim::Button::new(&words).kind(ecad_ui::prim::Kind::Primary).small());
            r.widget_info(|| egui::WidgetInfo::labeled(egui::Role::Button, true, PLACE));
            if r.clicked() {
                acts.push(Act::Place(t));
            }
            let label = if matches!(t, Tile::Builtin(_)) { "Edit a copy" } else { "Edit symbol" };
            let r = ui.add(ecad_ui::prim::Button::new(label).small());
            r.widget_info(|| egui::WidgetInfo::labeled(egui::Role::Button, true, EDIT));
            if r.clicked() {
                acts.push(Act::Edit(t));
            }
        }
    });
    if !matches!(e.tile, Tile::Draft(_)) {
        ui.label(egui::RichText::new("Place switches to Draw with this symbol on the cursor.").small().color(c32(s.text_3)));
    }
    let _ = writable;
    acts
}
