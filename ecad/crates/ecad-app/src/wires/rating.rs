//! The Rating section of Project ▸ Wires and Cables (brief 057, design 057 §2 and §9; boards 2.05
//! and 2.06): the basis, the factors with where each came from, what the conductor carries
//! against its protective device, and for a cable its volt drop. It reads one kept result
//! ([`ecad_electrical::circuits::circuit::Rating`], copied by the tab's view) and writes through `rating`.
//!
//! **Update map.** Nothing of its own: it shows the result the view holds this frame.

use ecad_electrical::circuits::circuit::Rating;
use ecad_electrical::circuits::{self, RatedSubject};
use ecad_electrical::devices::ratings::dataset;
use ecad_model::Word;
use ecad_model::objects::wire::Basis;
use ecad_widgets::prim::{self, Tone};
use ecad_widgets::tokens::{line, space, ui_type};
use eframe::egui::{self, Stroke, pos2, vec2};
use serde_json::{Value, json};

/// Element names (design 057 §9).
pub const SECTION: &str = "ecad.project.detail.rating";

/// What the section shows.
#[derive(Debug, Clone, PartialEq)]
pub struct Panel {
    pub rating: Rating,
    /// The methods the project's rating table has rows for (all IEC names when none).
    pub methods: Vec<&'static str>,
    pub writable: bool,
}

/// One row: label, value, note, the note's tone, a tooltip, and the `rating` key it edits.
struct Row {
    label: &'static str,
    value: String,
    note: String,
    tone: Option<Tone>,
    tip: String,
    edit: Option<&'static str>,
    strong: bool,
    /// The location a panel temperature is written to (tester F1).
    loc: Option<String>,
}

fn row(label: &'static str, value: impl Into<String>, note: impl Into<String>) -> Row {
    Row { label, value: value.into(), note: note.into(), tone: None, tip: String::new(), edit: None, strong: false, loc: None }
}

/// The folded summary and its tone (design 057 §2.1): pass muted, fail in error.
pub fn summary(r: &Rating) -> (String, Option<Tone>) {
    let tone = if r.fails() { Some(Tone::Error) } else { None };
    (r.summary(), tone)
}

fn subject_key(s: RatedSubject) -> (&'static str, String) {
    match s {
        RatedSubject::Wire(c) => ("wire", c.hex()),
        RatedSubject::Cable(k) => ("cable", k.hex()),
    }
}

fn factor<'a>(r: &'a Rating, name: &str) -> Option<&'a circuits::circuit::Factor> {
    r.factors.iter().find(|f| f.name == name)
}

fn rows(r: &Rating) -> Vec<Row> {
    let cable = matches!(r.subject, RatedSubject::Cable(_));
    let mut v = Vec::new();
    let temp_row = |v: &mut Vec<Row>, name: &'static str, label: &'static str| {
        if let Some(f) = factor(r, name) {
            let mut x = row(label, format!("\u{d7} {}", circuits::circuit::fmt_factor(f.factor)), f.input.clone());
            x.tip = format!("{} \u{b7} {}", f.input, f.from);
            if cable {
                x.edit = Some("ambient");
            } else {
                // Tester F1: the panel temperature is set here, on the wire's location (else the
                // project default).
                x.edit = Some("panel");
                x.loc = f.from.strip_prefix("location ").map(str::to_string).or_else(|| r.locations.first().cloned());
            }
            if f.from.contains("computed") {
                x.note = format!("{} \u{b7} computed", f.input);
            }
            v.push(x);
        }
    };
    let group_row = |v: &mut Vec<Row>| {
        let (name, label, key) = if cable { ("grouping", "Grouping", "grouped") } else { ("bundling", "Bundled", "bundling") };
        if let Some(f) = factor(r, name) {
            let mut x = row(label, format!("\u{d7} {}", circuits::circuit::fmt_factor(f.factor)), f.input.clone());
            if f.from.starts_with("not set") {
                x.note = f.from.clone();
                x.tone = Some(Tone::Warning);
            } else if f.from == "set here" {
                if f.input.trim().parse::<f64>().is_ok() {
                    x.note = "set here".into();
                }
            } else {
                x.note = format!("{} \u{b7} {}", f.input, f.from);
            }
            x.tip = f.from.clone();
            x.edit = Some(key);
            v.push(x);
        } else if cable {
            let mut x = row(label, "\u{d7} 1.00", "1 circuit");
            x.edit = Some(key);
            v.push(x);
        }
    };
    match r.basis {
        Basis::Datasheet => {
            if let Some((a, at)) = r.rated {
                v.push(row("Rated", format!("{} A", dataset::fmt_num(a)), format!("at {} \u{b0}C", dataset::fmt_num(at))));
                temp_row(&mut v, if cable { "ambient" } else { "panel" }, if cable { "Ambient" } else { "Panel" });
                group_row(&mut v);
            }
        }
        Basis::Table => {
            if let Some((a, size, method)) = r.table_value {
                let name = r.table.as_ref().map(|(n, _)| n.clone()).unwrap_or_default();
                let mut x =
                    row("Table", format!("{} A", dataset::fmt_num(a)), format!("{} mm\u{b2} \u{b7} {method} \u{b7} {name}", dataset::fmt_num(size)));
                x.tip = r
                    .table
                    .as_ref()
                    .map(|(n, src)| format!("Rating table {n}; source: {}", if src.is_empty() { "not given" } else { src }))
                    .unwrap_or_default();
                if !cable {
                    x.tip.push_str(". IEC B1; a panel datasheet usually allows more.");
                }
                v.push(x);
                temp_row(&mut v, if cable { "ambient" } else { "panel" }, if cable { "Ambient" } else { "Panel" });
                group_row(&mut v);
            }
        }
        Basis::Manual => {
            let mut x = row("Carries", r.iz.map(|x| format!("{} A", dataset::fmt_num(x))).unwrap_or_else(|| "\u{2014}".into()), "set by hand");
            x.edit = Some("manual");
            v.push(x);
        }
    }
    v
}

fn carries(r: &Rating) -> Row {
    let iz =
        r.iz.map(|x| format!("{} A", circuits::circuit::fmt_amps(x, r.device.as_ref().and_then(|d| d.amps)))).unwrap_or_else(|| "\u{2014}".into());
    let mut x = row("Carries", iz, "no protective device");
    x.strong = true;
    if let Some(d) = &r.device {
        match (d.amps, r.iz) {
            (Some(a), Some(iz)) => {
                let ok = iz >= a;
                x.note = format!("{} {} A {}", d.tag, dataset::fmt_num(a), if ok { "\u{2713}" } else { "\u{2717}" });
                x.tone = Some(if ok { Tone::Ok } else { Tone::Error });
                x.tip = format!(
                    "{} ({}). Ib \u{2264} In \u{2264} Iz; for circuit-breakers and gG fuses I2 \u{2264} 1.45 Iz then holds.",
                    d.tag,
                    d.how.word()
                );
            }
            (None, _) => x.note = format!("{}: rating has no current", d.tag),
            (Some(a), None) => x.note = format!("{} {} A", d.tag, dataset::fmt_num(a)),
        }
    }
    x
}

fn after(r: &Rating) -> Vec<Row> {
    let mut v = Vec::new();
    if let Some(vd) = &r.voltdrop {
        let ok = vd.percent <= vd.limit;
        let mut x = row(
            "Volt drop",
            format!("{} %", circuits::circuit::fmt_percent(vd.percent)),
            format!("{} {} % {}", if ok { "\u{2264}" } else { ">" }, dataset::fmt_num(vd.limit), if ok { "\u{2713}" } else { "\u{2717}" }),
        );
        x.tone = Some(if ok { Tone::Ok } else { Tone::Warning });
        x.tip = format!("at {} A, {}; this cable only, not the drop from the origin", dataset::fmt_num(vd.amps), vd.why);
        x.edit = Some("vd_limit");
        v.push(x);
    }
    let cable = matches!(r.subject, RatedSubject::Cable(_));
    if let Some(l) = r.load {
        let mut x =
            row("Load", format!("{} A", dataset::fmt_num(l.amps)), if l.from == circuits::load::LoadFrom::Typed { "typed" } else { "from Power" });
        x.edit = Some("load");
        v.push(x);
    } else if cable {
        let mut x = row("Load", "\u{2014}", "not set");
        x.edit = Some("load");
        v.push(x);
    }
    v
}

/// The basis words of the segment (design 057 §2.1, §2.2; the owner: "Table" on cables too).
const BASIS_WORDS: [&str; 3] = ["Datasheet", "Table", "Manual"];

/// Draw the section's body. Returns the `rating` calls asked for.
pub fn show(ui: &mut egui::Ui, p: &Panel) -> Vec<(&'static str, Value)> {
    let s = ecad_widgets::SHELL;
    let r = &p.rating;
    let cable = matches!(r.subject, RatedSubject::Cable(_));
    let (what, id) = subject_key(r.subject);
    let mut out = Vec::new();
    // Basis.
    let mut basis = match r.basis {
        Basis::Datasheet => 0,
        Basis::Table => 1,
        Basis::Manual => 2,
    };
    let was = basis;
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = 0.0;
        label_w(ui, "Basis", BASIS_LABEL_W);
        prim::Segmented::new(&mut basis, &BASIS_WORDS)
            .read_only(!p.writable)
            .name("ecad.project.detail.rating.basis")
            .hover_text(if p.writable { "" } else { "This project is read-only here." })
            .show(ui);
    });
    if basis != was {
        let word = [Basis::Datasheet, Basis::Table, Basis::Manual].get(basis).map_or("datasheet", |b| b.word());
        out.push(("rating", json!({what: id, "basis": word})));
    }
    // Method (cables).
    if cable && r.basis == Basis::Table {
        let current = r.table_value.map(|t| t.2).unwrap_or("E");
        let words: Vec<String> = p.methods.iter().map(|m| format!("{m} \u{b7} {}", dataset::method_words(m))).collect();
        let options: Vec<&str> = words.iter().map(String::as_str).collect();
        let mut chosen = p.methods.iter().position(|m| *m == current).unwrap_or(usize::MAX);
        ecad_widgets::control::row(ui, "Method", |ui| {
            if ecad_widgets::control::Select::new(&mut chosen, &options, "ecad.project.detail.rating.method")
                .read_only(!p.writable)
                .show(ui)
                .changed()
                && let Some(m) = p.methods.get(chosen).filter(|m| **m != current)
            {
                out.push(("rating", json!({what: id, "method": m})));
            }
        });
    }
    let body = rows(r);
    if body.is_empty() {
        let msg = match r.basis {
            Basis::Table if r.table.is_none() => {
                "No rating table chosen. Choose one in Settings \u{25b8} Standards \u{25b8} Defaults \u{25b8} Rating."
            }
            _ if cable => "No rating data. Set Rated in the cable type, or choose Manual.",
            _ => "No rating data. Set Rated in the category, or choose Manual.",
        };
        ui.add(egui::Label::new(egui::RichText::new(msg).size(ui_type::SMALL.size).color(ecad_widgets::color(s.text_3))).wrap());
    }
    for x in &body {
        if let Some(call) = value_row(ui, x, &id, what, p.writable) {
            out.push(call);
        }
    }
    // A rule, then what it carries against its device.
    let (rr, _) = ui.allocate_exact_size(vec2(ui.available_width(), space::S), egui::Sense::hover());
    ui.painter().hline(rr.x_range(), rr.center().y, Stroke::new(line::HAIRLINE_PX, ecad_widgets::color(s.line)));
    let _ = value_row(ui, &carries(r), &id, what, p.writable);
    for x in after(r) {
        if let Some(call) = value_row(ui, &x, &id, what, p.writable) {
            out.push(call);
        }
    }
    out
}

impl crate::App {
    /// Act on a rating row's fix (design 057 §7).
    pub(crate) fn rating_fix(&mut self, f: ecad_electrical::checks::Fix) {
        use ecad_electrical::checks::Fix;
        let words = |s: RatedSubject| subject_key(s);
        match f {
            Fix::ShowRating(s) => self.show_rating(s),
            Fix::UseSize(c, size) => {
                let _ = self.run_value("setprop", json!({"conductor": c.hex(), "key": "size", "value": dataset::fmt_num(size)}));
            }
            Fix::PickCableType(..) => self.open_standards_at(ecad_model::StyleCatalogue::Cable),
            Fix::SetBasis(s, b) => {
                let (what, id) = words(s);
                let _ = self.run_value("rating", json!({what: id, "basis": b.word()}));
            }
            Fix::ClearProtectedBy(s) => {
                let (what, id) = words(s);
                let _ = self.run_value("rating", json!({what: id, "protected_by": null}));
            }
            Fix::ChooseRatingTable => {
                self.open_settings(crate::settings::Section::Standards);
                self.settings.standards.focus_key("rating.table");
            }
            Fix::ShowRatedWires => {
                self.shell.lists = crate::shell::ListsView::Wires;
                self.set_workspace(crate::shell::Workspace::Lists);
            }
            _ => {}
        }
    }

    /// Select subject `s` in Project ▸ Wires or Cables (its Rating section is open by default).
    pub(crate) fn show_rating(&mut self, s: RatedSubject) {
        self.shell.lists = match s {
            RatedSubject::Wire(_) => crate::shell::ListsView::Wires,
            RatedSubject::Cable(_) => crate::shell::ListsView::Cables,
        };
        self.set_workspace(crate::shell::Workspace::Lists);
        if let Some(r) = &mut self.reports {
            match s {
                RatedSubject::Wire(c) => {
                    if let Some(k) = r.wires.source.ids.iter().position(|x| *x == c) {
                        r.wires.table.selection.set([k]);
                        r.wires.table.reveal(&[k]);
                    }
                }
                RatedSubject::Cable(c) => {
                    if let Some(k) = r.cables.ids.iter().position(|x| *x == c) {
                        r.cables.table.selection.set([k]);
                        r.cables.table.reveal(&[k]);
                    }
                }
            }
        }
    }
}

/// Standards ▸ Rating tables (design 057 §5.3): the licensing line, then the focused table's
/// source and rows (editable), Import CSV… and Export CSV; with no table focused, New. Standards ▸
/// Cable types with nothing focused: New. Returns whether it drew the detail (else the shared
/// style detail is drawn).
pub(crate) fn catalogue_ui(
    t: &mut crate::App, ui: &mut egui::Ui, writable: bool, word: ecad_model::StyleCatalogue, focus: Option<ecad_model::StyleRef>,
) -> bool {
    use ecad_electrical::styles::ratingtable;
    let s = ecad_widgets::SHELL;
    if word == ecad_model::StyleCatalogue::RatingTable {
        ui.add(
            egui::Label::new(
                egui::RichText::new("Enter only values you are licensed to use. ECAD ships no standard's tables.")
                    .size(ui_type::SMALL.size)
                    .color(ecad_widgets::color(s.text_3)),
            )
            .wrap(),
        );
        ui.add_space(space::S);
    }
    let new_id = egui::Id::unique(("ecad.standards.new", word));
    let msg_id = egui::Id::unique(("ecad.standards.message", word));
    let mut msg: Option<String> = ui.data(|d| d.get_temp(msg_id));
    let Some(id) = focus else {
        let mut name: String = ui.data(|d| d.get_temp(new_id)).unwrap_or_default();
        let one = if word == ecad_model::StyleCatalogue::RatingTable { "rating table" } else { "cable type" };
        ui.label(egui::RichText::new(format!("Choose a {one}, or make one.")).size(ui_type::SMALL.size).color(ecad_widgets::color(s.text_2)));
        ui.horizontal(|ui| {
            ecad_widgets::control::TextEntry::new(&mut name, "Name").placeholder("Name").read_only(!writable).width(180.0).show(ui);
            let new = format!("New {one}");
            let button = if writable && !name.trim().is_empty() {
                prim::Button::new(&new).small()
            } else {
                prim::Button::new(&new).small().disabled("Give it a name.")
            };
            if ui.add(button).clicked() {
                let r = if word == ecad_model::StyleCatalogue::RatingTable {
                    t.run_value("ratingtable", json!({"name": name.trim(), "csv": "# rows: kind,method/insulation,…\n"}))
                } else {
                    t.run_value("setstyle", json!({"catalogue": word, "new": true, "name": name.trim()}))
                };
                msg = r.err();
                name.clear();
            }
        });
        ui.data_mut(|d| d.insert_temp(new_id, name));
        show_msg(ui, msg_id, msg);
        return true;
    };
    if word != ecad_model::StyleCatalogue::RatingTable {
        return false;
    }
    // The rows are user data of up to 256 KiB (FP8): read and parsed once per change, not per frame.
    let cache_id = egui::Id::unique(("ecad.standards.ratingtable.read", id.text()));
    let count = t.project.clone().and_then(|p| p.read(&mut t.cad.session, |p, _| p.change_count()).ok()).unwrap_or(0);
    let cached: Option<(u64, std::sync::Arc<ratingtable::Table>)> = ui.data(|d| d.get_temp(cache_id));
    let table = match cached.filter(|(n, _)| *n == count) {
        Some((_, t)) => t,
        None => {
            let read = t.project.clone().and_then(|p| p.read(&mut t.cad.session, |p, _| ratingtable::read(p.ledger.document(), &id)).ok());
            let Some(table) = read else { return true };
            let table = std::sync::Arc::new(table);
            ui.data_mut(|d| d.insert_temp(cache_id, (count, table.clone())));
            table
        }
    };
    ui.label(egui::RichText::new(&table.name).size(ui_type::BODY_EMPHASIS.size).color(ecad_widgets::color(s.text)));
    ui.label(
        egui::RichText::new(format!(
            "{} row{} \u{b7} source: {}",
            table.data.rows.len(),
            if table.data.rows.len() == 1 { "" } else { "s" },
            if table.source.is_empty() { "not given" } else { table.source.as_str() }
        ))
        .size(ui_type::SMALL.size)
        .color(ecad_widgets::color(s.text_2)),
    );
    if let Some(odd) = table.data.odd() {
        ui.label(egui::RichText::new(odd).size(ui_type::SMALL.size).color(ecad_widgets::color(s.warning)));
    }
    ui.add_space(space::S);
    // The rows, as CSV, editable (Apply writes them; a bad row refuses the whole text).
    let rows_id = egui::Id::unique(("ecad.standards.ratingtable.rows", id.text()));
    // Shared, not copied each frame: the text may be 256 KiB.
    type Buf = std::sync::Arc<std::sync::Mutex<String>>;
    let buf: Buf = match ui.data(|d| d.get_temp::<Buf>(rows_id)) {
        Some(b) => b,
        None => {
            let b: Buf = std::sync::Arc::new(std::sync::Mutex::new(dataset::to_csv(&table.data)));
            ui.data_mut(|d| d.insert_temp(rows_id, b.clone()));
            b
        }
    };
    if let Ok(mut g) = buf.lock() {
        ui.add_enabled_ui(writable, |ui| ecad_widgets::control::TextArea::new(&mut g, "ecad.standards.ratingtable.rows").rows(12).show(ui));
    }
    let path_id = egui::Id::unique(("ecad.standards.ratingtable.path", id.text()));
    let mut path: String = ui.data(|d| d.get_temp(path_id)).unwrap_or_default();
    ui.horizontal(|ui| {
        let apply = if writable {
            prim::Button::new("Apply rows").small()
        } else {
            prim::Button::new("Apply rows").small().disabled("This project is read-only here.")
        };
        if ui.add(apply).clicked() {
            let text = buf.lock().map(|g| g.clone()).unwrap_or_default();
            msg = t.run_value("ratingtable", json!({"name": table.name, "csv": text})).err();
            ui.data_mut(|d| d.remove::<Buf>(rows_id));
        }
    });
    ui.horizontal(|ui| {
        ecad_widgets::control::TextEntry::new(&mut path, "CSV file path").placeholder("CSV file path").width(200.0).show(ui);
        let has = !path.trim().is_empty();
        let import = if writable && has {
            prim::Button::new("Import CSV\u{2026}").small()
        } else {
            prim::Button::new("Import CSV\u{2026}").small().disabled("Give a CSV file path.")
        };
        if ui.add(import).clicked() {
            msg = t.run_value("ratingtable", json!({"import": path.trim(), "name": table.name})).err();
            ui.data_mut(|d| d.remove::<Buf>(rows_id));
        }
        let export =
            if has { prim::Button::new("Export CSV").small() } else { prim::Button::new("Export CSV").small().disabled("Give a CSV file path.") };
        if ui.add(export).clicked() {
            msg = t.run_value("ratingtable", json!({"export": path.trim(), "name": table.name})).err();
        }
    });
    ui.data_mut(|d| d.insert_temp(path_id, path));
    show_msg(ui, msg_id, msg);
    true
}

fn show_msg(ui: &mut egui::Ui, id: egui::Id, msg: Option<String>) {
    match msg {
        Some(m) => {
            ui.add(egui::Label::new(egui::RichText::new(&m).size(ui_type::SMALL.size).color(ecad_widgets::color(ecad_widgets::SHELL.error))).wrap());
            ui.data_mut(|d| d.insert_temp(id, m));
        }
        None => ui.data_mut(|d| d.remove::<String>(id)),
    }
}

const LABEL_W: f32 = 96.0;
const VALUE_W: f32 = 84.0;

/// The Basis row's label is narrower, so the three-word segment fits the panel.
const BASIS_LABEL_W: f32 = 64.0;

fn label_w(ui: &mut egui::Ui, text: &str, w: f32) {
    let s = ecad_widgets::SHELL;
    let (rect, _) = ui.allocate_exact_size(vec2(w, ecad_widgets::tokens::size::CONTROL), egui::Sense::hover());
    ui.painter().text(
        pos2(rect.left(), rect.center().y),
        egui::Align2::LEFT_CENTER,
        text,
        ecad_widgets::fonts::font(ui.ctx(), ui_type::SMALL),
        ecad_widgets::color(s.text_2),
    );
}

fn tone_colour(t: Tone) -> ecad_widgets::tokens::Color {
    let s = ecad_widgets::SHELL;
    match t {
        Tone::Ok => s.ok,
        Tone::Error => s.error,
        Tone::Warning => s.warning,
        _ => s.text_3,
    }
}

/// One value row; an editable row's note is an input (Enter applies). Returns a `rating` call.
fn value_row(ui: &mut egui::Ui, x: &Row, id: &str, what: &'static str, writable: bool) -> Option<(&'static str, Value)> {
    let s = ecad_widgets::SHELL;
    let h = ecad_widgets::tokens::size::CONTROL;
    let (rect, resp) = ui.allocate_exact_size(vec2(ui.available_width(), h), egui::Sense::hover());
    let small = ecad_widgets::fonts::font(ui.ctx(), ui_type::SMALL);
    let body = ecad_widgets::fonts::font(ui.ctx(), if x.strong { ui_type::BODY_EMPHASIS } else { ui_type::BODY });
    let p = ui.painter();
    let label_font = if x.strong { ecad_widgets::fonts::font(ui.ctx(), ui_type::BODY_EMPHASIS) } else { small.clone() };
    p.text(
        pos2(rect.left(), rect.center().y),
        egui::Align2::LEFT_CENTER,
        x.label,
        label_font,
        ecad_widgets::color(if x.strong { s.text } else { s.text_2 }),
    );
    p.text(pos2(rect.left() + LABEL_W, rect.center().y), egui::Align2::LEFT_CENTER, &x.value, body, ecad_widgets::color(s.text));
    let note_x = rect.left() + LABEL_W + VALUE_W;
    let name = format!("{SECTION}.{}", x.label.to_ascii_lowercase().replace(' ', "_"));
    let info = format!("{}: {} {}", x.label, x.value, x.note);
    resp.widget_info(|| egui::WidgetInfo::labeled(egui::Role::Label, true, &info));
    let _ = name;
    let mut call = None;
    match x.edit.filter(|_| writable) {
        Some(key) => {
            // An editable value: the shared entry box (docs/design/components.md).
            let note_rect = egui::Rect::from_min_max(pos2(note_x, rect.top()), rect.max);
            let buf_id = egui::Id::unique(("ecad.rating.edit", id, key));
            let mut text = ui.data(|d| d.get_temp::<String>(buf_id)).unwrap_or_else(|| x.note.clone());
            let edit_name = format!("{SECTION}.{key}");
            let edit = ui
                .scope_builder(egui::UiBuilder::new().max_rect(note_rect), |ui| {
                    ecad_widgets::control::TextEntry::new(&mut text, &edit_name).id(buf_id.with("entry")).width(note_rect.width()).show(ui)
                })
                .inner;
            if edit.changed() {
                ui.data_mut(|d| d.insert_temp(buf_id, text.clone()));
            }
            if edit.lost_focus() {
                if ui.input(|i| i.key_pressed(egui::Key::Enter)) && text.trim() != x.note.trim() {
                    let t = text.trim();
                    let clear = t.is_empty() || t == "\u{2014}" || t == "not set";
                    let v = if clear { Value::Null } else { json!(t) };
                    call = Some(match (key, &x.loc) {
                        // A typed °C goes to the location, or to the project when the wire has none.
                        ("panel", Some(loc)) => (
                            "rating",
                            json!({"location": loc, "temperature": ecad_electrical::devices::ratings::parse::ambient(t).map_or(v, |n| json!(n))}),
                        ),
                        ("panel", None) if clear => ("setdefault", json!({"key": "rating.panel_temperature", "level": "project", "reset": true})),
                        ("panel", None) => (
                            "setdefault",
                            json!({"key": "rating.panel_temperature", "level": "project", "value": ecad_electrical::devices::ratings::parse::ambient(t).map_or(v, |n| json!(n.to_string()))}),
                        ),
                        _ => ("rating", json!({what: id, key: v})),
                    });
                }
                ui.data_mut(|d| d.remove::<String>(buf_id));
            }
            if !x.tip.is_empty() {
                edit.on_hover_text(&x.tip);
            }
        }
        None => {
            let colour = ecad_widgets::color(x.tone.map_or(s.text_3, tone_colour));
            let g = ecad_widgets::table::cells::fitted(ui, &x.note, ui_type::SMALL, colour, (rect.right() - note_x).max(0.0));
            ui.painter().galley(pos2(note_x, rect.center().y - g.size().y / 2.0), g, colour);
            if !x.tip.is_empty() {
                resp.on_hover_text(&x.tip);
            }
        }
    }
    call
}
