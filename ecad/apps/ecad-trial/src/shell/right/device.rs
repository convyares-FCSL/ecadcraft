//! The Device tab (UI rev 3, board 1.05): a one-line status of this device's own checks, then
//! Identity, Parts, Where it is drawn, Connections, and Notes and attributes, each folding to a
//! one-line summary. With anything other than one device selected (nothing: the page; a wire, a
//! cable…) it shows that object's properties in the same folding sections.

use ecad_cadcraft::checks::{Object, Severity};
use ecad_cadcraft::props::{Row, Section};
use ecad_ui::tokens::{radius, space, ui_type};
use eframe::egui::{self, CornerRadius, Rect, Sense, pos2, vec2};

use super::widgets as w;
use super::{Cx, InspectorAction, fields};

/// Accessible names.
pub const OWN_CHECKS: &str = "ecad.inspector.own-checks";
pub const DRAWN_ROW: &str = "ecad.inspector.drawn.row";
pub const CONNECTION_ROW: &str = "ecad.inspector.connection.row";
/// What a button that has nothing behind it yet says.
pub const NOT_YET: &str = "Not available yet";

/// The props rows the Device tab places in its own sections; every other row of a device goes to
/// Notes and attributes (the Layout tab takes the contact table and the texts).
const PLACED: [&str; 11] = [
    "tag",
    "function",
    "location",
    "description",
    "maker",
    "part_number",
    "symbol",
    "where",
    "contactrows",
    "coil",
    ecad_cadcraft::xref::CONTACTS_KEY,
];

/// A row the Layout tab draws.
pub fn on_layout_tab(row: &Row) -> bool {
    row.key == ecad_cadcraft::xref::MIRROR_KEY || row.label.starts_with("Texts")
}

/// The device's own checks: about it or one of its connection points.
pub fn own_checks<'r>(cx: &Cx<'r>) -> Vec<&'r crate::checks_panel::Row> {
    let Some(d) = &cx.insp.device else { return Vec::new() };
    let Some(c) = d.component else { return Vec::new() };
    cx.checks
        .iter()
        .filter(|r| match r.check.object {
            Object::Component(x) => x == c,
            Object::Terminal(t) => d.terminals.contains(&t),
            _ => false,
        })
        .collect()
}

pub fn show(ui: &mut egui::Ui, cx: &mut Cx<'_>, out: &mut Vec<InspectorAction>) {
    let view = std::mem::take(&mut cx.sh.view);
    match (&cx.insp.device, view.sections.first()) {
        (Some(_), Some(s)) if s.kind == "device" => device(ui, cx, s, out),
        _ => {
            if view.sections.first().is_some_and(|s| s.kind == "page") {
                ui.add_space(space::S);
                ui.label(w::rich(ui, "Nothing selected: this page's details.", ui_type::SMALL, w::text_3()));
            }
            for s in &view.sections {
                generic(ui, cx, s);
            }
            if view.sections.is_empty() {
                ui.add_space(space::M);
                ui.label(w::rich(ui, "Nothing to show for this selection.", ui_type::SMALL, w::text_3()));
            }
            // A symbol that is not a device's main view still shows its symbol's update.
            cx.insp.show_symbol_update(ui, cx.writable, out);
        }
    }
    cx.sh.view = view;
}

/// The section title of a kind of object.
fn kind_title(kind: &str) -> &'static str {
    match kind {
        "page" => "This page",
        "wire" => "Wire",
        "cable" => "Cable",
        "reference" => "Reference",
        "box" => "Location box",
        "link" => "Link",
        "device" => "Devices",
        _ => "Selection",
    }
}

/// A text's name from its row's label (`Texts: connection point labels` → `Pin labels`).
pub fn text_label(label: &str) -> String {
    match label.trim_start_matches("Texts: ").trim_start_matches("Texts on this symbol: ") {
        "connection point labels" => "Pin labels".to_string(),
        l => {
            let mut c = l.chars();
            c.next().map(|f| f.to_uppercase().chain(c).collect()).unwrap_or_default()
        }
    }
}

/// Any section: its rows, note and buttons in one folding section; its texts' shown settings in
/// a second one, folded.
fn generic(ui: &mut egui::Ui, cx: &mut Cx<'_>, s: &Section) {
    // The page is named in the header already.
    let summary = if s.kind == "page" { String::new() } else { s.title.trim_start_matches("ECAD \u{b7} ").to_string() };
    let full = cx.full;
    let sh = &mut *cx.sh;
    let (texts, rows): (Vec<&Row>, Vec<&Row>) = s.rows.iter().partition(|r| r.label.starts_with("Texts"));
    w::section(ui, &mut cx.insp.folded, full, s.kind, kind_title(s.kind), &summary, false, |ui| {
        for row in rows {
            fields::prop(ui, sh, &s.target, s.kind, row, &row.label, if row.inherited { "from project" } else { "" });
        }
        if let Some(n) = &s.note {
            ui.label(w::rich(ui, n, ui_type::SMALL, w::text_3()));
        }
        fields::buttons(ui, sh, &s.buttons);
    });
    if !texts.is_empty() {
        let set = texts.iter().filter(|r| !r.inherited).count();
        let summary = if set == 0 { "default".to_string() } else { format!("{set} set") };
        let id = format!("{}-texts", s.kind);
        w::section(ui, &mut cx.insp.folded, full, &id, "Texts shown", &summary, true, |ui| {
            for row in texts {
                fields::prop(ui, sh, &s.target, s.kind, row, &text_label(&row.label), "");
            }
        });
    }
}

fn find<'s>(s: &'s Section, key: &str) -> Option<&'s Row> {
    s.rows.iter().find(|r| r.key == key)
}

fn device(ui: &mut egui::Ui, cx: &mut Cx<'_>, s: &Section, out: &mut Vec<InspectorAction>) {
    let Some(d) = cx.insp.device.clone() else { return };
    let full = cx.full;
    // The status of this device's own checks (never another device's).
    ui.add_space(space::M);
    let own = own_checks(cx);
    if own.is_empty() {
        w::status_pill(ui, true, &format!("No checks on {}", d.tag), "only this device's own", OWN_CHECKS);
    } else {
        let e = own.iter().filter(|r| r.check.severity() == Severity::Error).count();
        let words = super::checks_tab::count_words(e, own.len().saturating_sub(e));
        w::status_pill(ui, false, &format!("{words} on {}", d.tag), "only this device's own", OWN_CHECKS);
        for r in own {
            super::checks_tab::card(ui, r, cx.writable, &mut cx.panel_out);
        }
    }
    ui.add_space(space::XS);
    w::rule(ui, full);

    let target = &s.target;
    let sh = &mut *cx.sh;
    let insp = &mut *cx.insp;
    // Identity.
    let summary = if d.description.is_empty() { d.tag.clone() } else { format!("{} \u{b7} {}", d.tag, d.description) };
    let mut folded = std::mem::take(&mut insp.folded);
    w::section(ui, &mut folded, full, "identity", "Identity", &summary, false, |ui| {
        let fixed = find(s, ecad_model::tag::FIXED_KEY).is_some_and(|r| r.value == "yes");
        for (key, label) in [("tag", "Tag"), ("function", "Function"), ("location", "Location"), ("description", "Description")] {
            let Some(row) = find(s, key) else { continue };
            let hint = match key {
                "tag" if fixed => "fixed",
                "tag" => "auto",
                "location" | "function" if row.inherited || row.value.is_empty() => "from page",
                _ => "",
            };
            fields::prop(ui, sh, target, "device", row, label, hint);
        }
        w::row(ui, "Symbol", |ui| {
            ui.label(w::rich(ui, &d.symbol_name, ui_type::BODY, w::text()));
            ui.label(w::rich(ui, format!("\u{b7} {}", d.standard), ui_type::SMALL, w::text_3()));
        });
        insp.show_symbol_update(ui, cx.writable, out);
        // The symbol's tag prefix and other defaults about what it is (read, with their badge).
        insp.show_rows(ui, cx.writable, out, |r| r.key.ends_with(".prefix"));
    });
    // Parts.
    let parts = insp.parts.clone();
    let summary =
        if parts.is_empty() { "none yet".to_string() } else { format!("{} line{} \u{b7} qty", parts.len(), if parts.len() == 1 { "" } else { "s" }) };
    w::section(ui, &mut folded, full, "parts", "Parts", &summary, false, |ui| {
        if !parts.is_empty() {
            ui.scope(|ui| {
                for p in &parts {
                    part_card(ui, p);
                }
            })
            .response
            .widget_info(|| egui::WidgetInfo::labeled(egui::Role::Label, true, super::PARTS));
        }
        for (key, label) in [("maker", "Maker"), ("part_number", "Part number")] {
            if let Some(row) = find(s, key) {
                fields::prop(ui, sh, target, "device", row, label, if parts.is_empty() { "" } else { "main part" });
            }
        }
        ui.horizontal(|ui| {
            w::button(ui, "+ Accessory", false, Some(NOT_YET));
            w::button(ui, "From catalogue", false, Some(NOT_YET));
        });
    });
    // Where it is drawn.
    let summary = format!("{} of {}", d.drawn_count, d.drawn.len());
    w::section(ui, &mut folded, full, "drawn", "Where it is drawn", &summary, false, |ui| {
        for (i, r) in d.drawn.iter().enumerate() {
            if let Some(rep) = drawn_row(ui, r, i) {
                out.push(InspectorAction::Placements(vec![rep]));
            }
        }
        if let Some(row) = find(s, ecad_cadcraft::xref::CONTACTS_KEY) {
            ui.add_space(space::XS);
            fields::prop(ui, sh, target, "device", row, "Contacts offered", "");
        }
        insp.show_placements(ui, out);
    });
    // Connections.
    let connected = d.connections.iter().filter(|c| c.wire != "not connected").count();
    w::section(ui, &mut folded, full, "connections", "Connections", &connected.to_string(), false, |ui| {
        if d.connections.is_empty() {
            ui.label(w::rich(ui, "No connection points.", ui_type::SMALL, w::text_3()));
        }
        for (i, c) in d.connections.iter().enumerate() {
            connection_row(ui, c, i);
        }
    });
    // Notes and attributes: everything else about the device.
    let rest: Vec<&Row> =
        s.rows.iter().filter(|r| !PLACED.contains(&r.key.as_str()) && !on_layout_tab(r) && r.key != ecad_model::tag::FIXED_KEY).collect();
    let fixed = find(s, ecad_model::tag::FIXED_KEY);
    let buttons: Vec<String> = s.buttons.iter().filter(|b| !b.starts_with("refgoto:")).cloned().collect();
    let n = rest.len() + insp.connector.len();
    let summary = if n == 0 { "none".to_string() } else { n.to_string() };
    w::section(ui, &mut folded, full, "notes", "Notes and attributes", &summary, true, |ui| {
        if let Some(row) = fixed {
            fields::prop(ui, sh, target, "device", row, "Tag fixed", "");
        }
        for row in rest {
            fields::prop(ui, sh, target, "device", row, &row.label, if row.inherited { "from project" } else { "" });
        }
        if let Some(n) = &s.note {
            ui.label(w::rich(ui, n, ui_type::SMALL, w::text_3()));
        }
        fields::buttons(ui, sh, &buttons);
        insp.show_connector(ui, cx.writable, out);
    });
    insp.folded = folded;
}

/// A part line as a card: a dot (blue for the main part), its words, its maker and number, and
/// the quantity in a box.
fn part_card(ui: &mut egui::Ui, p: &ecad_cadcraft::bom::PartSetRow) {
    let s = ecad_ui::SHELL;
    let width = ui.available_width();
    let (rect, resp) = ui.allocate_exact_size(vec2(width, 46.0), Sense::hover());
    let name = format!("{}.row.{}", super::PARTS, p.n);
    resp.widget_info(|| egui::WidgetInfo::labeled(egui::Role::Label, true, &name));
    let painter = ui.painter();
    painter.rect_filled(rect, CornerRadius::same(radius::CARD as u8 / 2 + 1), ecad_ui::color(s.raised));
    let dot = if p.n == 0 { ecad_ui::color(s.accent_text) } else { w::text_3() };
    painter.circle_filled(pos2(rect.left() + 13.0, rect.top() + 15.0), 3.5, dot);
    let title = p.description.clone().or_else(|| p.part_number.clone()).unwrap_or_else(|| "Part".into());
    painter.text(
        pos2(rect.left() + 26.0, rect.top() + 15.0),
        egui::Align2::LEFT_CENTER,
        w::ellipsis(&title, 34),
        w::font(ui, ui_type::BODY_EMPHASIS),
        w::text(),
    );
    let mut second = Vec::new();
    if let Some(m) = &p.maker {
        second.push(m.clone());
    }
    second.push(p.part_number.clone().unwrap_or_else(|| "?".into()));
    if let Some(n) = &p.note {
        second.push(n.clone());
    }
    let colour = if p.part_number.is_none() { ecad_ui::color(s.warning) } else { w::text_2() };
    painter.text(
        pos2(rect.left() + 26.0, rect.top() + 33.0),
        egui::Align2::LEFT_CENTER,
        w::ellipsis(&second.join(" \u{b7} "), 40),
        w::font(ui, ui_type::SMALL),
        colour,
    );
    let q = Rect::from_center_size(pos2(rect.right() - 20.0, rect.top() + 15.0), vec2(24.0, 22.0));
    painter.rect_stroke(
        q,
        CornerRadius::same(4),
        egui::Stroke::new(ecad_ui::tokens::line::HAIRLINE_PX, ecad_ui::color(s.line_strong)),
        egui::StrokeKind::Inside,
    );
    painter.text(q.center(), egui::Align2::CENTER_CENTER, &p.qty, w::font(ui, ui_type::BODY), w::text());
    ui.add_space(space::XS);
}

/// A row of "Where it is drawn"; returns the symbol to go to when clicked.
fn drawn_row(ui: &mut egui::Ui, r: &super::details::DrawnRow, i: usize) -> Option<ecad_model::RepresentationId> {
    let s = ecad_ui::SHELL;
    let width = ui.available_width();
    let sense = if r.rep.is_some() { Sense::click() } else { Sense::hover() };
    let (rect, resp) = ui.allocate_exact_size(vec2(width, 26.0), sense);
    let name = format!("{DRAWN_ROW}.{i}");
    resp.widget_info(|| egui::WidgetInfo::labeled(egui::Role::Button, r.rep.is_some(), &name));
    let p = ui.painter();
    if r.this {
        p.rect_filled(rect, CornerRadius::same(4), ecad_ui::color(s.accent_soft));
    } else if resp.hovered() && r.rep.is_some() {
        p.rect_filled(rect, CornerRadius::same(4), ecad_ui::color(s.hover));
    }
    let spare = r.place.is_empty();
    let ink = if spare { w::text_3() } else { w::text() };
    p.text(pos2(rect.left() + space::M, rect.center().y), egui::Align2::LEFT_CENTER, w::ellipsis(&r.pins, 14), w::font(ui, ui_type::BODY), ink);
    p.text(
        pos2(rect.left() + 106.0, rect.center().y),
        egui::Align2::LEFT_CENTER,
        w::ellipsis(&r.what, 20),
        w::font(ui, ui_type::BODY),
        if spare { w::text_3() } else { w::text_2() },
    );
    p.text(pos2(rect.right() - space::M, rect.center().y), egui::Align2::RIGHT_CENTER, &r.place, w::font(ui, ui_type::BODY), w::text_2());
    let resp = if r.rep.is_some() { resp.on_hover_text("Go to it") } else { resp };
    if resp.clicked() { r.rep } else { None }
}

fn connection_row(ui: &mut egui::Ui, c: &super::details::ConnRow, i: usize) {
    let width = ui.available_width();
    let (rect, resp) = ui.allocate_exact_size(vec2(width, 26.0), Sense::hover());
    let name = format!("{CONNECTION_ROW}.{i}");
    resp.widget_info(|| egui::WidgetInfo::labeled(egui::Role::Label, true, &name));
    let p = ui.painter();
    let open = c.wire == "not connected";
    p.text(
        pos2(rect.left() + space::M, rect.center().y),
        egui::Align2::LEFT_CENTER,
        w::ellipsis(&c.pin, 8),
        w::font(ui, ui_type::BODY_EMPHASIS),
        w::text(),
    );
    p.text(
        pos2(rect.left() + 56.0, rect.center().y),
        egui::Align2::LEFT_CENTER,
        w::ellipsis(&c.wire, 16),
        w::font(ui, ui_type::BODY),
        if open { w::text_3() } else { w::text() },
    );
    p.text(
        pos2(rect.right() - space::M, rect.center().y),
        egui::Align2::RIGHT_CENTER,
        w::ellipsis(&c.others, 22),
        w::font(ui, ui_type::BODY),
        w::text_2(),
    );
}
