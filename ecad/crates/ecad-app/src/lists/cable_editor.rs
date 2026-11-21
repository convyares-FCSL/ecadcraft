//! Brief 059 (design 059 §5.2–§5.4): the selected cable's editable Details pane on Project ▸
//! Cables: Identity, Part and Ends edited in place with `ecad_ui` components, where it is drawn
//! (Go to links, or **Assign in the drawing…**), and board 2.06's footer (**Change cable type…**,
//! **Add a core**, **Remove**). Each box commits on Enter or focus loss as one `cableedit`; Esc
//! restores; a refusal shows the box invalid with the words under it, and nothing is written.
//!
//! **Update map.** The copy ([`Copy`]) is read with the cable copy's colours, when the selection
//! or the project's change count changed ([`super::cables::CablesView::sync_colours`]); never per
//! frame.

use std::collections::BTreeMap;

use ecad_engine::cable_cmds::{self, CableField, Shield, Side, Source};
use ecad_model::words::Word;
use ecad_model::{CableId, Document, PageId};
use ecad_ui::control::{self, Select, TextEntry};
use ecad_ui::panel;
use ecad_ui::prim::{self, Tone};
use eframe::egui;
use serde_json::json;

use super::cable_manager::ASSIGN_IN_DRAWING;

/// The shield segments, in [`Shield::ALL`] order.
pub const SHIELDS: [&str; 4] = ["None", "Overall", "Pairs", "Both"];
/// The pane's footer buttons (board 2.06).
pub const CHANGE_TYPE: &str = "Change cable type\u{2026}";
pub const ADD_CORE: &str = "Add a core";
pub const REMOVE: &str = "Remove";
/// Where it is drawn, when it is not.
pub const NOT_DRAWN_YET: &str = "Not drawn yet";
/// The read-only footer's reason (029).
pub const READ_ONLY: &str = "This project cannot be changed here (a release, or writes off).";

/// What the editor shows for the selected cable, read from the document when the selection or the
/// project changed (never per frame).
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Copy {
    pub cable: Option<CableId>,
    pub tag: String,
    pub drawn: bool,
    /// Each text field's value and hint (`set here`, `from type`, `from notation`).
    pub values: BTreeMap<CableField, (String, &'static str)>,
    /// The cable types (ID text, name); the Select's first item is `None`.
    pub types: Vec<(String, String)>,
    pub type_index: usize,
    /// The project's location and function codes; each planned Select's first item is `—`.
    pub locations: Vec<String>,
    pub functions: Vec<String>,
    pub plans: BTreeMap<CableField, usize>,
    pub shield: usize,
    pub shield_hint: &'static str,
    /// `4 · 3 assigned`.
    pub cores_line: String,
    /// The offer **Add a core** writes.
    pub next_offer: String,
    /// The pages it is drawn on (`2/D1`).
    pub pages: Vec<(PageId, String)>,
    /// Each end drawn away from that end's plan: `planned +FIELD`.
    pub off_plan: [Option<String>; 2],
    /// `=PUMP +CAB1 → =PUMP +FIELD`, the planned ends.
    pub runs: String,
}

/// Read cable `k`'s editor copy.
pub fn copy(doc: &Document, k: CableId) -> Copy {
    use ecad_engine::designations::{self, Aspect};
    let mut c = Copy { cable: Some(k), tag: ecad_engine::views::cable_title(doc, k), drawn: cable_cmds::is_drawn(doc, k), ..Copy::default() };
    c.values.insert(CableField::Tag, (c.tag.clone(), ""));
    if let Some(v) = cable_cmds::read(doc, k, CableField::Type) {
        c.values.insert(CableField::Type, (v, Source::Here.hint()));
    }
    if let Some((v, s)) = cable_cmds::cores_shown(doc, k) {
        c.values.insert(CableField::Cores, (v, s.hint()));
    }
    for f in [CableField::Maker, CableField::PartNumber, CableField::Description] {
        if let Some((v, s)) = cable_cmds::part_value(doc, k, f) {
            c.values.insert(f, (v, s.hint()));
        }
    }
    if let Some(v) = cable_cmds::read(doc, k, CableField::Length) {
        c.values.insert(CableField::Length, (v.trim().trim_end_matches('m').trim().to_string(), ""));
    }
    for side in Side::BOTH {
        if let Some((g, s)) = cable_cmds::gland(doc, k, side) {
            c.values.insert(CableField::Gland(side), (g, s.hint()));
        }
    }
    let catalogue = ecad_engine::styles::catalogue(ecad_model::StyleCatalogue::Cable);
    c.types = ecad_engine::styles::rows(doc, catalogue).into_iter().map(|r| (r.id.text(), r.name)).collect();
    let own_type = ecad_engine::styles::cable::of(doc, k).map(|id| id.text());
    c.type_index = own_type.and_then(|t| c.types.iter().position(|(id, _)| *id == t)).map_or(0, |i| i + 1);
    c.locations = designations::entries(doc, Aspect::Location).into_iter().map(|e| e.code).collect();
    c.functions = designations::entries(doc, Aspect::Function).into_iter().map(|e| e.code).collect();
    for side in Side::BOTH {
        for (aspect, field) in [(Aspect::Location, CableField::EndLocation(side)), (Aspect::Function, CableField::EndFunction(side))] {
            let list = if aspect == Aspect::Location { &c.locations } else { &c.functions };
            let at = cable_cmds::planned(doc, k, side, aspect).and_then(|code| list.iter().position(|x| *x == code)).map_or(0, |i| i + 1);
            c.plans.insert(field, at);
        }
    }
    let (sh, src) = cable_cmds::shield(doc, k).unwrap_or((Shield::None, Source::Here));
    c.shield = Shield::ALL.iter().position(|s| *s == sh).unwrap_or(0);
    c.shield_hint = match (cable_cmds::read(doc, k, CableField::Shield).is_some(), src) {
        (true, _) => Source::Here.hint(),
        (false, Source::Type) => Source::Type.hint(),
        _ => "",
    };
    let members = doc.cables.get(&k).map_or(0, |x| x.cores.len());
    c.cores_line = match ecad_engine::strips::cable_cores(doc, k).and_then(|n| n.offer) {
        Some(o) => format!("{} \u{b7} {members} assigned", o.len()),
        None => format!("{members} assigned"),
    };
    c.next_offer = ecad_electrical::cmd::cable::next_offer(doc, k);
    c.pages = cable_cmds::drawn_pages(doc, k);
    for o in cable_cmds::off_plan(doc, k) {
        if let Some(slot) = c.off_plan.get_mut(usize::from(o.side == Side::To)) {
            *slot = cable_cmds::plan_words(doc, k, o.side).map(|p| format!("planned {p}"));
        }
    }
    let plan = |s: Side| cable_cmds::plan_words(doc, k, s).unwrap_or_else(|| super::report::DASH.to_string());
    c.runs = format!("{} \u{2192} {}", plan(Side::From), plan(Side::To));
    c
}

/// The editor's state: the copy, the boxes' text being typed, a refusal to show.
#[derive(Debug, Default)]
pub struct Editor {
    pub copy: Copy,
    pub bufs: BTreeMap<CableField, String>,
    /// The last edit refused, under its row.
    pub error: Option<(CableField, String)>,
    /// A drawn cable's Remove refused (Q1): the words and its pages (each a Go to).
    pub refused: Option<(String, Vec<(PageId, String)>)>,
    /// Put focus in the Tag box (after + Cable).
    pub focus_tag: bool,
}

impl Editor {
    /// Take a new copy: the boxes show its values again; a refusal about another cable is dropped.
    pub fn set_copy(&mut self, c: Copy) {
        if c.cable != self.copy.cable {
            self.error = None;
            self.refused = None;
        } else if c != self.copy {
            // The cable changed since: an older refusal no longer applies.
            self.error = None;
        }
        self.bufs = c.values.iter().map(|(f, (v, _))| (*f, v.clone())).collect();
        self.copy = c;
    }

    fn value(&self, f: CableField) -> (String, &'static str) {
        self.copy.values.get(&f).map_or((String::new(), ""), |(v, h)| (v.clone(), *h))
    }
}

/// What the editor asks for.
#[derive(Debug, Clone, PartialEq)]
pub enum Ask {
    /// `cableedit {cable, set}`, and the field it changes (for its error).
    Edit(CableField, serde_json::Value),
    Remove,
    Assign,
    Go(PageId),
}

/// The JSON `set` of one field's new text (empty clears).
pub fn set_of(field: CableField, text: &str) -> serde_json::Value {
    let v = if text.trim().is_empty() { serde_json::Value::Null } else { json!(text.trim()) };
    match field {
        CableField::EndLocation(s) => json!({s.word(): {"location": v}}),
        CableField::EndFunction(s) => json!({s.word(): {"function": v}}),
        CableField::Gland(s) => json!({s.word(): {"gland": v}}),
        f => json!({ f.key(): v }),
    }
}

/// One text box row; commits on Enter or focus loss, Esc restores.
pub fn entry(ui: &mut egui::Ui, ed: &mut Editor, field: CableField, label: &str, name: &str, writable: bool, out: &mut Vec<Ask>) {
    let (value, hint) = ed.value(field);
    let error = ed.error.as_ref().filter(|(f, _)| *f == field).map(|(_, m)| m.clone());
    let focus = ed.focus_tag && field == CableField::Tag;
    let buf = ed.bufs.entry(field).or_insert_with(|| value.clone());
    control::row(ui, label, |ui| {
        let mut te = TextEntry::new(buf, name).hint(hint).read_only(!writable).invalid(error.as_ref().map(|_| ""));
        if field == CableField::Tag {
            te = te.bold(true);
        }
        if field == CableField::Length {
            te = te.number("m");
        }
        let r = te.show(ui);
        if focus {
            r.request_focus();
        }
        if r.lost_focus() {
            if ui.input(|i| i.key_pressed(egui::Key::Escape)) {
                *buf = value.clone();
            } else if buf.trim() != value.trim() {
                out.push(Ask::Edit(field, set_of(field, buf)));
            }
        }
    });
    if focus {
        ed.focus_tag = false;
    }
    if let Some(m) = error {
        panel::status_pill(ui, Tone::Error, &m, &m);
    }
}

/// A planned function or location row: a Select over the project's codes.
fn plan(ui: &mut egui::Ui, ed: &Editor, field: CableField, label: &str, name: &str, writable: bool, out: &mut Vec<Ask>) {
    let list = match field {
        CableField::EndLocation(_) => &ed.copy.locations,
        _ => &ed.copy.functions,
    };
    let mut opts: Vec<&str> = vec![super::report::DASH];
    opts.extend(list.iter().map(String::as_str));
    let mut at = ed.copy.plans.get(&field).copied().unwrap_or(0);
    let before = at;
    control::row(ui, label, |ui| Select::new(&mut at, &opts, name).read_only(!writable).show(ui));
    if at != before {
        let code = if at == 0 { String::new() } else { opts.get(at).map(|s| (*s).to_string()).unwrap_or_default() };
        out.push(Ask::Edit(field, set_of(field, &code)));
    }
}

fn nonempty(s: &str) -> String {
    if s.trim().is_empty() || s == "?" { super::report::DASH.into() } else { s.to_string() }
}

/// The JSON value choosing cable type item `i` of the Select (`0`: none).
fn type_value(c: &Copy, i: usize) -> serde_json::Value {
    i.checked_sub(1).and_then(|j| c.types.get(j)).map_or(serde_json::Value::Null, |(id, _)| json!(id))
}

/// The cable's editable sections and footer (board 2.06), in the Details pane. `ends` are its
/// drawn ends (`+CAB1 -X01`); `middle` draws the sections that go between Part and Ends (057's
/// Rating). Returns what was asked for; the caller runs it.
pub fn show(
    ui: &mut egui::Ui, ed: &mut Editor, ends: [String; 2], writable: bool, rule: &dyn Fn(&mut egui::Ui), middle: &mut dyn FnMut(&mut egui::Ui),
) -> Vec<Ask> {
    let mut out = Vec::new();
    let s = ecad_ui::SHELL;
    // Identity.
    panel::section_header(ui, true, "Identity", "set here");
    entry(ui, ed, CableField::Tag, "Tag", "ecad.project.detail.cable.tag", writable, &mut out);
    let runs = if ed.copy.drawn { format!("{} \u{2192} {}", nonempty(&ends[0]), nonempty(&ends[1])) } else { ed.copy.runs.clone() };
    control::row(ui, "Runs", |ui| control::read_text(ui, &runs, control::Decor::default(), "ecad.project.detail.cable.runs"));
    rule(ui);
    // Part.
    panel::section_header(ui, true, "Part", "sets the cores");
    let names: Vec<String> = std::iter::once("None".to_string()).chain(ed.copy.types.iter().map(|(_, n)| n.clone())).collect();
    let opts: Vec<&str> = names.iter().map(String::as_str).collect();
    let mut ti = ed.copy.type_index;
    control::row(ui, "Cable type", |ui| Select::new(&mut ti, &opts, "ecad.project.detail.cable.cable_type").read_only(!writable).show(ui));
    if ti != ed.copy.type_index {
        out.push(Ask::Edit(CableField::CableType, json!({"cable_type": type_value(&ed.copy, ti)})));
    }
    entry(ui, ed, CableField::Type, "Type", "ecad.project.detail.cable.type", writable, &mut out);
    entry(ui, ed, CableField::Cores, "Cores", "ecad.project.detail.cable.cores", writable, &mut out);
    let line = ed.copy.cores_line.clone();
    control::row(ui, "", |ui| control::read_text(ui, &line, control::Decor::default(), "ecad.project.detail.cable.cores_used"));
    let mut sh = ed.copy.shield;
    let hint = ed.copy.shield_hint;
    control::row(ui, "Shield", |ui| {
        prim::Segmented::new(&mut sh, &SHIELDS).read_only(!writable).name("ecad.project.detail.cable.shield").show(ui);
        if !hint.is_empty() {
            ui.label(egui::RichText::new(hint).size(ecad_ui::tokens::ui_type::SMALL.size).color(ecad_ui::color(s.text_3)));
        }
    });
    if sh != ed.copy.shield
        && let Some(w) = Shield::ALL.get(sh)
    {
        out.push(Ask::Edit(CableField::Shield, json!({"shield": w.word()})));
    }
    entry(ui, ed, CableField::Maker, "Maker", "ecad.project.detail.cable.maker", writable, &mut out);
    entry(ui, ed, CableField::PartNumber, "Part number", "ecad.project.detail.cable.part_number", writable, &mut out);
    entry(ui, ed, CableField::Description, "Description", "ecad.project.detail.cable.description", writable, &mut out);
    entry(ui, ed, CableField::Length, "Length", "ecad.project.detail.cable.length", writable, &mut out);
    rule(ui);
    // 057's Rating section (board 2.06: after Part).
    middle(ui);
    // Ends: each end's drawn end, then its planned function and location (Q4).
    panel::section_header(ui, true, "Ends", "planned and drawn");
    for (i, side) in Side::BOTH.into_iter().enumerate() {
        let (label, name, f, l) = match side {
            Side::From => (
                "Cabinet end",
                "ecad.project.detail.cable.end.from",
                "ecad.project.detail.cable.end.from.function",
                "ecad.project.detail.cable.end.from.location",
            ),
            Side::To => (
                "Field end",
                "ecad.project.detail.cable.end.to",
                "ecad.project.detail.cable.end.to.function",
                "ecad.project.detail.cable.end.to.location",
            ),
        };
        let drawn = if ed.copy.drawn { nonempty(ends.get(i).map_or("", String::as_str)) } else { super::report::DASH.to_string() };
        let warn = ed.copy.off_plan.get(i).cloned().flatten().unwrap_or_default();
        control::row(ui, label, |ui| control::read_text(ui, &drawn, control::Decor { hint: &warn, ..control::Decor::default() }, name));
        plan(ui, ed, CableField::EndFunction(side), "Function", f, writable, &mut out);
        plan(ui, ed, CableField::EndLocation(side), "Location", l, writable, &mut out);
        let g = match side {
            Side::From => "ecad.project.detail.cable.gland.from",
            Side::To => "ecad.project.detail.cable.gland.to",
        };
        entry(ui, ed, CableField::Gland(side), "Gland", g, writable, &mut out);
    }
    rule(ui);
    // Where it is drawn.
    panel::section_header(ui, true, "Where it is drawn", "");
    if ed.copy.pages.is_empty() {
        control::row(ui, "", |ui| {
            control::read_text(ui, NOT_DRAWN_YET, control::Decor::default(), "ecad.project.detail.cable.drawn");
            if writable && control::link(ui, ASSIGN_IN_DRAWING).clicked() {
                out.push(Ask::Assign);
            }
        });
    } else {
        ui.horizontal_wrapped(|ui| {
            for (page, words) in &ed.copy.pages {
                if control::link(ui, &format!("\u{2192} {words}")).clicked() {
                    out.push(Ask::Go(*page));
                }
            }
        });
    }
    // A drawn cable's Remove refused (Q1), with a Go to per page.
    if let Some((words, pages)) = ed.refused.clone() {
        panel::status_pill(ui, Tone::Warning, &words, "ecad.lists.cables.remove.refused");
        ui.horizontal_wrapped(|ui| {
            for (page, w) in &pages {
                if control::link(ui, &format!("\u{2192} {w}")).clicked() {
                    out.push(Ask::Go(*page));
                }
            }
        });
    }
    rule(ui);
    // The footer (board 2.06).
    ui.horizontal(|ui| {
        let button = |label| match writable {
            true => prim::Button::new(label).small(),
            false => prim::Button::new(label).small().disabled(READ_ONLY),
        };
        let r = ui.add(button(CHANGE_TYPE));
        egui::Popup::menu(&r).show(|ui| {
            for (i, n) in opts.iter().enumerate() {
                if ui.selectable_label(i == ed.copy.type_index, *n).clicked() && i != ed.copy.type_index {
                    out.push(Ask::Edit(CableField::CableType, json!({"cable_type": type_value(&ed.copy, i)})));
                }
            }
        });
        if ui.add(button(ADD_CORE)).clicked() {
            out.push(Ask::Edit(CableField::Cores, json!({"cores": ed.copy.next_offer})));
        }
        if ui.add(button(REMOVE).kind(prim::Kind::Danger)).clicked() {
            out.push(Ask::Remove);
        }
    });
    out
}

impl crate::App {
    /// Run what the cable editor asked for, on cable `k`.
    pub fn cable_ask(&mut self, k: CableId, a: Ask) {
        match a {
            Ask::Edit(field, set) => {
                let res = self.run_value("cableedit", json!({"cable": k.hex(), "set": set}));
                if let Some(r) = self.reports.as_mut() {
                    r.cables.edit.error = res.err().map(|e| (field, e));
                }
            }
            Ask::Remove => {
                if let Some(r) = self.reports.as_mut() {
                    r.cables.edit.refused = None;
                }
                let Some(p) = self.project.clone() else { return };
                let check = p.read(&mut self.cad.session, |p, _| cable_cmds::drawn(p.ledger.document(), k));
                match check {
                    Ok(Some(d)) => {
                        if let Some(r) = self.reports.as_mut() {
                            r.cables.edit.refused = Some((d.words(), d.pages));
                        }
                    }
                    _ => {
                        let _ = self.run_value("cableremove", json!({"cable": k.hex()}));
                    }
                }
            }
            Ask::Assign => self.assign_in_drawing(k),
            Ask::Go(page) => {
                let _ = self.show_page(page);
            }
        }
    }

    /// **+ Cable** (design 059 §5.1): a new cable at once, its row selected and its details open,
    /// focus in its Tag box. No dialog.
    pub(crate) fn new_cable(&mut self) {
        let Ok(v) = self.run_value("cablenew", json!({})) else { return };
        let Some(k) = v["cable"].as_str().and_then(CableId::parse) else { return };
        if let Some(r) = self.reports.as_mut() {
            r.cables.pending_select = Some(k);
            r.cables.details = true;
            r.cables.edit.focus_tag = true;
        }
    }
}
