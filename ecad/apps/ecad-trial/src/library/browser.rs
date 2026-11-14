//! The Library browser (design 048 §8.1, screen i): a category tree and a preview grid in the
//! main view, the selected symbol's details in the right panel. Project symbols are listed with
//! the built-ins, in a **This project** group at the top; every tile names its source
//! (`Project · v3`, `Built-in · read-only`); hovering a tile shows its connection points and text
//! slots. A lineage shows its newest version only. New symbol is in the header.

use std::collections::BTreeSet;

use ecad_cadcraft::symbols::{self, Family, Starter};
use ecad_model::{Definition, DefinitionId, Document, Primitive, Pt};
use ecad_ui::tokens::{size, space};
use eframe::egui::{self, Align2, Color32, FontId, Pos2, Rect, Sense, Stroke, pos2, vec2};

/// Element names (design 048 §8.1).
pub const BROWSER: &str = "library.browser";
pub const NEW: &str = "library.browser.new";
pub const SEARCH: &str = "library.browser.search";
pub const PLACE: &str = "library.browser.place";
pub const EDIT: &str = "library.browser.edit";
pub const RESUME: &str = "library.browser.resume";
pub const FORGET: &str = "library.browser.forget";
pub const PLACED_NEXT: &str = "library.browser.placed-next";
/// A tile is `library.browser.tile.<lineage hex>` (a project symbol) or `.<key>` (a built-in).
pub const TILE: &str = "library.browser.tile";
/// The tree's width inside the main view.
pub const TREE_W: f32 = 260.0;
/// A tile's size.
pub const TILE_W: f32 = 168.0;
pub const TILE_H: f32 = 150.0;

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
    pub query: String,
    pub category: Category,
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
            });
        }
        for s in symbols::STARTERS {
            let placed = doc.and_then(|d| symbols::embedded(d, s).map(|id| d.items_using(id).len())).unwrap_or(0);
            entries.push(Entry {
                tile: Tile::Builtin(s.key),
                name: s.name.to_string(),
                key: s.key.to_string(),
                source: "Built-in · read-only".into(),
                def: s.definition().ok().cloned(),
                family: Some(family_index(s.family)),
                placed,
                versions: None,
                from: None,
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

    /// The tiles shown for the category and the query.
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
                p.text(at(*a), Align2::LEFT_BOTTOM, value, FontId::proportional(((*height * s) as f32 * 1.25).clamp(5.0, 14.0)), ink);
            }
            Primitive::Point { at: a } => {
                p.circle_filled(at(*a), 1.5, ink);
            }
        }
    }
    if let Some((ring, slot)) = marks {
        for t in &d.terminals {
            p.circle_stroke(at(t.at), 3.5, Stroke::new(1.25, ring));
            p.text(at(t.at) + vec2(0.0, -5.0), Align2::CENTER_BOTTOM, &t.key, FontId::proportional(9.0), ring);
        }
        for sl in &d.texts {
            p.circle_filled(at(sl.at), 2.0, slot);
            p.text(at(sl.at) + vec2(4.0, 0.0), Align2::LEFT_CENTER, super::draft::slot_label(&sl.text), FontId::proportional(9.0), slot);
        }
    }
}

/// The browser in the main view.
pub fn show(ui: &mut egui::Ui, main: Rect, b: &mut Browser, list: &Listing, sheet: &ecad_ui::tokens::Sheet) -> Vec<Act> {
    let s = ecad_ui::SHELL;
    let mut acts = Vec::new();
    ui.painter().rect_filled(main, 0.0, c32(s.bg));
    // Header: the counts, the search, New symbol.
    let head = Rect::from_min_size(main.min, vec2(main.width(), size::PANEL_HEADER + space::S));
    ui.painter().rect_filled(head, 0.0, c32(s.panel));
    let cats = FAMILIES.len() + 1;
    let sources = 1 + usize::from(list.project_count() > 0);
    let total = list.project_count() + list.builtin_count();
    ui.scope_builder(egui::UiBuilder::new().max_rect(head.shrink2(vec2(space::L, space::XS))), |ui| {
        ui.horizontal_centered(|ui| {
            ui.label(egui::RichText::new("Library").strong());
            ui.label(egui::RichText::new("found by browsing as well as by search").small().color(c32(s.text_3)));
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                let n = ui.add(ecad_ui::prim::Button::new("New symbol").kind(ecad_ui::prim::Kind::Primary).small());
                n.widget_info(|| egui::WidgetInfo::labeled(egui::Role::Button, true, NEW));
                if n.clicked() {
                    acts.push(Act::New);
                }
                let f = ui.add(egui::TextEdit::singleline(&mut b.query).hint_text("Search symbols").desired_width(240.0));
                f.widget_info(|| egui::WidgetInfo::labeled(egui::Role::SearchInput, true, SEARCH));
            });
        });
    });

    // The tree.
    let tree = Rect::from_min_max(pos2(main.left(), head.bottom()), pos2(main.left() + TREE_W, main.bottom()));
    ui.painter().rect_filled(tree, 0.0, c32(s.panel));
    ui.painter().vline(tree.right() - 0.5, tree.y_range(), Stroke::new(1.0, c32(s.line)));
    ui.scope_builder(egui::UiBuilder::new().max_rect(tree.shrink(space::M)), |ui| {
        let row = |ui: &mut egui::Ui, b: &mut Browser, c: Category, label: String, right: String, indent: f32| {
            ui.horizontal(|ui| {
                ui.add_space(indent);
                let r = ui.selectable_label(b.category == c, label);
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    ui.label(egui::RichText::new(right).small().color(c32(s.text_3)));
                });
                if r.clicked() {
                    b.category = c;
                }
            });
        };
        // Screen i's three counts.
        ui.horizontal(|ui| {
            for (n, what) in [(total, "symbols"), (cats, "categories"), (sources, "sources")] {
                egui::Frame::new()
                    .fill(c32(s.raised))
                    .corner_radius(6.0)
                    .inner_margin(egui::Margin::symmetric(6, 6))
                    .show(ui, |ui| {
                        ui.set_width(60.0);
                        ui.vertical(|ui| {
                            ui.label(egui::RichText::new(n.to_string()).size(20.0).strong());
                            ui.label(egui::RichText::new(what).small().color(c32(s.text_3)));
                        });
                    })
                    .response
                    .widget_info(|| egui::WidgetInfo::labeled(egui::Role::Label, true, format!("library.browser.count.{what}")));
            }
        });
        ui.add_space(space::S);
        row(ui, b, Category::All, "All symbols".into(), total.to_string(), 0.0);
        ui.add_space(space::S);
        ecad_ui::prim::read_value(ui, "SOURCES");
        row(ui, b, Category::Project, "This project".into(), list.project_count().to_string(), 0.0);
        if list.draft_count() > 0 {
            row(ui, b, Category::Drafts, "Drafts".into(), list.draft_count().to_string(), 0.0);
        }
        ui.horizontal(|ui| {
            ui.label("IEC built-in");
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                ui.label(egui::RichText::new(format!("{} · read-only", list.builtin_count())).small().color(c32(s.text_3)));
            });
        });
        for (i, (_, name)) in FAMILIES.iter().enumerate() {
            let n = list.entries.iter().filter(|e| e.family == Some(i as u8)).count();
            row(ui, b, Category::Builtin(i as u8), (*name).to_string(), n.to_string(), space::L);
        }
    });

    // The grid.
    let grid = Rect::from_min_max(pos2(tree.right(), head.bottom()), main.max);
    let shown = list.shown(b);
    let footer_h = if b.placed_next.is_some() { 40.0 } else { 0.0 };
    let body = Rect::from_min_max(grid.min, pos2(grid.right(), grid.bottom() - footer_h));
    ui.scope_builder(egui::UiBuilder::new().max_rect(body.shrink(space::L)), |ui| {
        egui::ScrollArea::vertical().id_salt("library.browser.scroll").auto_shrink([false, false]).show(ui, |ui| {
            if shown.is_empty() {
                ui.label(egui::RichText::new(format!("Nothing matches \"{}\".", b.query.trim())).color(c32(s.text_3)));
            }
            ui.horizontal_wrapped(|ui| {
                ui.spacing_mut().item_spacing = vec2(space::M, space::M);
                for e in &shown {
                    let (r, resp) = ui.allocate_exact_size(vec2(TILE_W, TILE_H), Sense::click());
                    resp.widget_info(|| egui::WidgetInfo::selected(egui::Role::Button, true, b.selected == Some(e.tile), e.tile.name()));
                    let p = ui.painter_at(r);
                    let chosen = b.selected == Some(e.tile);
                    p.rect_filled(r, 6.0, c32(s.panel));
                    p.rect_stroke(
                        r,
                        6.0,
                        Stroke::new(if chosen { 2.0 } else { 1.0 }, c32(if chosen { s.accent } else { s.line })),
                        egui::StrokeKind::Inside,
                    );
                    let pic = Rect::from_min_size(r.min + vec2(8.0, 8.0), vec2(r.width() - 16.0, 88.0));
                    p.rect_filled(pic, 4.0, c32(sheet.sheet));
                    if let Some(d) = &e.def {
                        let marks = resp.hovered().then(|| (c32(sheet.connection_point), c32(sheet.placement_preview)));
                        paint_symbol(&p, pic.shrink(10.0), d, c32(sheet.ink), marks);
                    }
                    p.text(pos2(r.left() + 10.0, pic.bottom() + 8.0), Align2::LEFT_TOP, &e.name, FontId::proportional(13.0), c32(s.text));
                    let tone = if matches!(e.tile, Tile::Draft(_)) { s.warning } else { s.text_3 };
                    p.text(pos2(r.left() + 10.0, pic.bottom() + 28.0), Align2::LEFT_TOP, &e.source, FontId::proportional(11.0), c32(tone));
                    if resp.clicked() {
                        b.selected = Some(e.tile);
                    }
                    if resp.double_clicked() {
                        acts.push(match e.tile {
                            Tile::Draft(n) => Act::Resume(n),
                            t => Act::Place(t),
                        });
                    }
                }
            });
        });
    });
    if let Some(name) = b.placed_next.clone() {
        let f = Rect::from_min_max(pos2(grid.left(), grid.bottom() - footer_h), grid.max);
        ui.painter().rect_filled(f, 0.0, c32(s.panel));
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

/// The selected symbol's details in the right panel (screen i's right panel).
pub fn details(ui: &mut egui::Ui, b: &Browser, list: &Listing, writable: bool) -> Vec<Act> {
    let s = ecad_ui::SHELL;
    let mut acts = Vec::new();
    let Some(e) = b.selected.and_then(|t| list.get(t)) else {
        ui.label(
            egui::RichText::new("Select a symbol to see its connection points, text slots, source and where it is used.")
                .small()
                .color(c32(s.text_3)),
        );
        return acts;
    };
    ui.label(egui::RichText::new(&e.name).strong());
    ui.label(egui::RichText::new(&e.source).small().color(c32(s.text_2)));
    if let Some(f) = &e.from {
        ui.label(egui::RichText::new(format!("Project · {f}")).small().color(c32(s.text_2)));
    }
    if !e.key.is_empty() {
        ui.label(egui::RichText::new(format!("Key: {}", e.key)).small());
    }
    if let Some(v) = &e.versions {
        ui.label(egui::RichText::new(v).small().color(c32(s.text_3)));
    }
    if !matches!(e.tile, Tile::Draft(_)) {
        ui.label(egui::RichText::new(format!("Used {} {} in this project", e.placed, if e.placed == 1 { "time" } else { "times" })).small());
    }
    if let Some(d) = &e.def {
        let (r, _) = ui.allocate_exact_size(vec2(ui.available_width(), 140.0), Sense::hover());
        let sheet = crate::canvas::Sheet::Paper.tokens();
        ui.painter().rect_filled(r, 4.0, c32(sheet.sheet));
        paint_symbol(ui.painter(), r.shrink(16.0), d, c32(sheet.ink), Some((c32(sheet.connection_point), c32(sheet.placement_preview))));
        ui.add_space(space::S);
        ecad_ui::prim::read_value(ui, "CONNECTION POINTS");
        for t in &d.terminals {
            ui.label(egui::RichText::new(format!("{}  {}", t.key, t.label.as_deref().unwrap_or(""))).small().monospace());
        }
        if !d.texts.is_empty() {
            ui.add_space(space::S);
            ecad_ui::prim::read_value(ui, "TEXT SLOTS");
            for sl in &d.texts {
                ui.label(egui::RichText::new(super::draft::slot_label(&sl.text)).small());
            }
        }
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
            let r = ui.add(ecad_ui::prim::Button::new("Place").kind(ecad_ui::prim::Kind::Primary).small());
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
    if matches!(e.tile, Tile::Builtin(_)) {
        ui.label(egui::RichText::new("Editing a built-in makes a project copy that remembers where it came from.").small().color(c32(s.text_3)));
    }
    let _ = writable;
    acts
}
