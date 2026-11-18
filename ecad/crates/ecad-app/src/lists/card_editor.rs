//! Project ▸ PLC ▸ a card selected: the right panel is the card's editor (brief 056, owner's
//! change of 2026-11-14; board 2.03's right panel). Identity (type, ID, description, rack and
//! slot, function and location as 052 codes), Addresses (start, rule), Detailed page, and Parts with its
//! accessories.
//!
//! Built only from the `ecad_ui` components (`docs/design/components.md`): Type is a
//! `Segmented`, Rack a `Select`, ID, Description, Slot, Function, Location, Start, Maker and Part
//! number are `TextEntry`s, and values read from the drawing (a type it can't change, the address
//! rule) are plain `read_text`. No colour or size of its own.
//!
//! Every edit is one command, as typed at the command line: `plccardtype`, `setprop` (`tag`,
//! `description`, `function`, `location`, `maker`, `part_number`), `plcslot`, `plcaddress`,
//! `plcchannels {rest}`, `partline {add}`. A refusal shows under its field.
//!
//! **Update map.** The card's values are read from the project when the panel draws (one card, a
//! handful of properties). A field being edited keeps its text until it is committed or the card's
//! own value changes under it.

use std::collections::BTreeMap;

use ecad_cadcraft::plc::overview;
use ecad_cadcraft::plc::rack::{CardType, IO_TYPES, RackRef};
use ecad_model::{DeviceId, PageId};
use eframe::egui::{self, Rect, vec2};
use serde_json::json;

use ecad_ui::control::{Decor, Select, TextEntry, ValueBox, read_text};

use crate::App;
use crate::shell::right::widgets as w;

/// Element names.
pub const PANEL: &str = "ecad.plc.card.editor";
/// The greyed catalogue button's reason.
pub const CATALOGUE_COMES: &str = "The part catalogue comes later.";
/// The card's symbols read for the Function and Location hints (a card draws a body and channels).
const MAX_SYMBOLS: usize = 256;
/// The main part's note in Parts (board 2.03).
pub const MAIN_PART: &str = "main part";
/// The Rack choice for a card in no rack.
pub const NO_RACK: &str = "None";
/// The Detailed page note (board 2.03).
pub const PAGE_NOTE: &str = "A card can be drawn across several pages. Each channel remembers which.";

/// What the editor shows of one card, read from the project.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Info {
    pub card: Option<DeviceId>,
    pub tag: String,
    pub ty: Option<CardType>,
    /// `Output card, 16 relay · rack 1, slot 2`.
    pub subtitle: String,
    pub description: String,
    pub function: (String, &'static str),
    pub location: (String, &'static str),
    pub rack: Option<String>,
    pub racks: Vec<String>,
    pub slot: String,
    /// The block's first address and whether it is pinned.
    pub start: String,
    pub pinned: bool,
    /// `IEC`, `byte.bit`, `%`.
    pub rule: String,
    /// The first page: (page, number, title).
    pub page: Option<(PageId, String, String)>,
    /// Used channels drawn, and used channels.
    pub drawn: (usize, usize),
    pub maker: String,
    pub part_number: String,
    pub parts: Vec<ecad_cadcraft::bom::PartSetRow>,
}

/// The editor's state: field buffers by key (`(model value, text)`), the accessory form, a refusal.
#[derive(Debug, Clone, Default)]
pub struct State {
    card: Option<DeviceId>,
    fields: BTreeMap<&'static str, (String, String)>,
    /// The `+ Accessory` form: description, part number, maker.
    accessory: Option<[String; 3]>,
    /// A refusal and the field it belongs to.
    error: Option<(&'static str, String)>,
    /// A type picked and waiting for **Change type** (the confirm step says what changes).
    pub pending_type: Option<CardType>,
    /// The value-with-hint field opened for typing (function or location), and whether its entry
    /// takes the focus on the next frame.
    editing: Option<&'static str>,
    focus: bool,
}

impl State {
    /// Bring field `key` in step with the card's `value`: an unedited field follows it.
    fn sync(&mut self, key: &'static str, value: &str) {
        let e = self.fields.entry(key).or_insert_with(|| (value.to_string(), value.to_string()));
        if e.0 != value {
            if e.1 == e.0 {
                e.1 = value.to_string();
            }
            e.0 = value.to_string();
        }
    }
}

/// The card shown by tile `i`, read from the project.
pub fn info(t: &mut App, i: usize) -> Option<Info> {
    let r = t.reports.as_ref()?;
    let tile = r.io.tiles.get(i)?;
    let c = tile.card()?;
    let rack_title = r.io.rack_title.to_lowercase();
    let p = t.project()?.clone();
    p.read(&mut t.cad.session, |p, _| {
        let doc = p.ledger.document();
        let plc = &p.outputs.as_ref()?.plc;
        let card = plc.cards.get(&c)?;
        let comp = doc.devices.get(&c)?;
        let own = |k: &str| comp.get(k).map(|v| v.trim().to_string()).filter(|v| !v.is_empty());
        let page = overview::pages_of(doc, c).first().copied();
        // Brief 052 (design 052 §9.1): function and location are entry keys stored on the device,
        // shown as their codes; nothing is inherited. The hint says where the stored code comes
        // from: the box, page or project whose code it is where one of the card's symbols is drawn
        // (a box first), else `set here`.
        let symbols: Vec<_> = doc.representations_of(ecad_model::Subject::Device(c)).map(|(_, r)| r).take(MAX_SYMBOLS).collect();
        let mut boxes = ecad_cadcraft::locations::BoxCache::default();
        let mut aspect = |a: ecad_cadcraft::designations::Aspect, l: ecad_cadcraft::locations::Located| -> (String, &'static str) {
            let mut hint = "set here";
            for r in &symbols {
                let x = ecad_cadcraft::locations::rep_context_in(doc, a, r, &mut boxes);
                if x.key.is_some() && x.key == l.key {
                    let h = source_hint(Some(&x.source));
                    if h == "from box" || hint == "set here" || (h == "from page" && hint == "from project") {
                        hint = h;
                    }
                }
            }
            match l.value {
                Some(code) => (code, hint),
                None if l.missing() => ("?".to_string(), "not in the list"),
                None => (String::new(), ""),
            }
        };
        let function = aspect(ecad_cadcraft::designations::Aspect::Function, ecad_cadcraft::locations::resolve_function(doc, c));
        let location = aspect(ecad_cadcraft::designations::Aspect::Location, ecad_cadcraft::locations::resolve(doc, c));
        let (rack, rule) = match card.rack {
            RackRef::Rack(r) => (
                plc.racks.get(&r).map(|x| x.tag.clone()),
                match ecad_cadcraft::plc::address::Style::of(doc, r) {
                    ecad_cadcraft::plc::address::Style::Iec => "IEC",
                    ecad_cadcraft::plc::address::Style::Percent => "%",
                    ecad_cadcraft::plc::address::Style::ByteBit => "byte.bit",
                    ecad_cadcraft::plc::address::Style::Slot => "slot",
                    ecad_cadcraft::plc::address::Style::None => "none",
                }
                .to_string(),
            ),
            _ => (None, String::new()),
        };
        let used: Vec<u32> = card.chans.iter().filter(|(_, x)| x.used || x.mark().is_some()).map(|(i, _)| *i).collect();
        let drawn = used.iter().filter(|i| card.chans.get(i).is_some_and(|x| !x.cells.is_empty())).count();
        let start = plc.automatic(doc, c, 0, ecad_cadcraft::plc::address::shows_output(card.ty)).unwrap_or_default();
        let slot_words = card.slot.map(|s| format!(", slot {s}")).unwrap_or_default();
        let kind = overview::kind_words(doc, card);
        let ty_name = overview::type_name(card.ty);
        let parts = part_rows(ecad_cadcraft::bom::part_set(doc, c), own("maker"), own("part_number"), format!("{ty_name}, {kind}"));
        Some(Info {
            card: Some(c),
            tag: card.tag.clone(),
            ty: Some(card.ty),
            subtitle: format!("{ty_name}, {kind} \u{b7} {rack_title}{slot_words}"),
            description: overview::description(doc, card).unwrap_or_default(),
            function,
            location,
            rack,
            racks: plc.racks_by_tag().iter().map(|r| r.tag.clone()).take(64).collect(),
            slot: card.slot.map(|s| s.to_string()).unwrap_or_default(),
            start,
            pinned: plc.blocks.get(&c).is_some_and(|b| b.pinned),
            rule,
            page: page
                .map(|pg| (pg, ecad_cadcraft::references::page_number(doc, pg), doc.pages.get(&pg).map(|x| x.title.to_string()).unwrap_or_default())),
            drawn: (drawn, used.len()),
            maker: own("maker").unwrap_or_default(),
            part_number: own("part_number").unwrap_or_default(),
            parts,
        })
    })
    .ok()
    .flatten()
}

/// The source hint of a stored function or location whose code is also the context's there.
fn source_hint(source: Option<&ecad_cadcraft::locations::Source>) -> &'static str {
    use ecad_cadcraft::locations::Source;
    match source {
        Some(Source::Box { .. }) => "from box",
        Some(Source::Page(_)) => "from page",
        Some(Source::Project) => "from project",
        _ => "set here",
    }
}

/// The Parts rows as board 2.03 shows them: the card's main part (named for what the card is, its
/// maker and part number, `main part`), then each accessory line, every quantity as `×1`. Empty
/// only when the card has no part at all (no maker, no part number, no lines).
pub fn part_rows(
    set: Vec<ecad_cadcraft::bom::PartSetRow>, maker: Option<String>, part_number: Option<String>, what: String,
) -> Vec<ecad_cadcraft::bom::PartSetRow> {
    if set.is_empty() && maker.is_none() && part_number.is_none() {
        return Vec::new();
    }
    let main = ecad_cadcraft::bom::PartSetRow {
        n: 0,
        qty: "1".into(),
        maker,
        part_number,
        description: Some(what),
        article_number: None,
        note: Some(MAIN_PART.into()),
    };
    std::iter::once(main)
        .chain(set.into_iter().filter(|r| r.n != 0))
        .map(|mut r| {
            r.qty = format!("\u{d7}{}", r.qty);
            r
        })
        .collect()
}

/// The byte a typed start names: `%Q4.0`, `Q4`, `IW64`, `4`; empty unpins.
pub fn start_byte(text: &str) -> Option<Option<u32>> {
    let t = text.trim();
    if t.is_empty() {
        return Some(None);
    }
    let digits: String = t.trim_start_matches(|c: char| !c.is_ascii_digit()).chars().take_while(char::is_ascii_digit).collect();
    digits.parse().ok().map(Some)
}

/// What a field's commit runs, or `None` when nothing changed.
pub fn field_line(c: DeviceId, key: &str, value: &str) -> Option<String> {
    let v = value.trim();
    let setprop = |k: &str| {
        format!("setprop {}", json!({"component": c.hex(), "key": k, "value": if v.is_empty() { serde_json::Value::Null } else { json!(v) }}))
    };
    Some(match key {
        "tag" => format!("setprop {}", json!({"component": c.hex(), "key": "tag", "value": v, "conflict": "swap"})),
        "description" | "function" | "location" | "maker" | "part_number" => setprop(key),
        "slot" => format!("plcslot {}", json!({"card": c.hex(), "slot": v.parse::<u32>().ok()?})),
        "start" => format!("plcaddress {}", json!({"card": c.hex(), "start": start_byte(v)?})),
        _ => return None,
    })
}

/// One field set here: its label, its input (with a grey hint at the right), its refusal under
/// it; a commit (Enter or leaving it) with a changed value asks for its command.
#[allow(clippy::too_many_arguments)]
fn field(
    ui: &mut egui::Ui, st: &mut State, lines: &mut Vec<(&'static str, String)>, c: DeviceId, writable: bool, key: &'static str, label: &str,
    placeholder: &str, hint: &str,
) {
    let invalid = st.error.as_ref().is_some_and(|(k, _)| *k == key);
    // Function and location are values with a hint (components.md): the code and where it comes
    // from, in a box that opens its entry when clicked.
    let boxed = matches!(key, "function" | "location");
    let name = format!("{PANEL}.{key}");
    if boxed && st.editing != Some(key) {
        let shown = st.fields.get(key).map(|e| e.0.clone()).unwrap_or_default();
        let clicked = w::row(ui, label, |ui| ui.add_enabled_ui(writable, |ui| ValueBox::new(&shown, &name).hint(hint).show(ui)).inner).clicked();
        if clicked && writable {
            st.editing = Some(key);
            st.focus = true;
        }
        error_line(ui, st, key);
        return;
    }
    let id = ui.make_persistent_id((PANEL, key));
    let focus = std::mem::take(&mut st.focus);
    let Some(entry) = st.fields.get_mut(key) else { return };
    let r = w::row(ui, label, |ui| {
        ui.add_enabled_ui(writable, |ui| {
            // The card's ID is its key identity value: bold (components.md).
            TextEntry::new(&mut entry.1, &name).id(id).placeholder(placeholder).hint(hint).bold(key == "tag").invalid(invalid.then_some("")).show(ui)
        })
        .inner
    });
    if focus {
        ui.memory_mut(|m| m.request_focus(id));
    }
    let done = r.lost_focus() || (r.has_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)));
    if boxed && done {
        st.editing = None;
    }
    if done
        && entry.1.trim() != entry.0.trim()
        && let Some(l) = field_line(c, key, &entry.1)
    {
        lines.push((key, l));
    }
    error_line(ui, st, key);
}

/// The refusal under field `key`, if it has one.
fn error_line(ui: &mut egui::Ui, st: &State, key: &str) {
    if let Some((k, m)) = &st.error
        && *k == key
    {
        ui.label(w::rich(ui, m, ecad_ui::tokens::ui_type::SMALL, ecad_ui::color(ecad_ui::SHELL.error)));
    }
}

/// Draw the editor for tile `i` in `rect`; runs what it is asked to.
pub fn show(t: &mut App, ui: &mut egui::Ui, rect: Rect, i: usize) {
    let Some(info) = info(t, i) else { return };
    let Some(c) = info.card else { return };
    let writable = t.project_writable();
    let (mut st, mut folded) = match t.reports.as_mut() {
        Some(r) => (std::mem::take(&mut r.io.card_editor), std::mem::take(&mut r.io.card_folded)),
        None => return,
    };
    if st.card != Some(c) {
        st = State { card: Some(c), ..State::default() };
    }
    for (k, v) in [
        ("tag", info.tag.as_str()),
        ("description", info.description.as_str()),
        ("function", info.function.0.as_str()),
        ("location", info.location.0.as_str()),
        ("slot", info.slot.as_str()),
        ("start", info.start.as_str()),
        ("maker", info.maker.as_str()),
        ("part_number", info.part_number.as_str()),
    ] {
        st.sync(k, v);
    }
    let mut lines: Vec<(&'static str, String)> = Vec::new();
    let mut go: Option<PageId> = None;
    crate::shell::in_rect(ui, rect.shrink2(vec2(ecad_ui::tokens::space::L, ecad_ui::tokens::space::M)), PANEL, |ui| {
        egui::ScrollArea::vertical().id_salt("ecad.plc.card.scroll").auto_shrink([false, false]).show(ui, |ui| {
            let full = ui.max_rect().x_range();
            ui.label(w::rich(ui, &info.tag, ecad_ui::tokens::ui_type::OBJECT, w::text()));
            ui.label(w::rich(ui, &info.subtitle, ecad_ui::tokens::ui_type::SMALL, w::text_2()));
            // Identity (board 2.03, with the owner's editable type, ID, rack and slot).
            w::section(ui, &mut folded, full, "plc.identity", "Identity", "", false, |ui| {
                let types: Vec<&str> = IO_TYPES.iter().map(|t| t.code()).collect();
                let at = info.ty.and_then(|ty| IO_TYPES.iter().position(|t| *t == ty));
                let shown = st.pending_type.and_then(|p| IO_TYPES.iter().position(|t| *t == p)).or(at);
                w::row(ui, "Type", |ui| {
                    if at.is_some() {
                        let why = if writable { "" } else { super::plc::WRITES_OFF };
                        if let Some(k) = w::segmented(ui, &types, shown, writable, &format!("{PANEL}.type"), why)
                            && let Some(to) = IO_TYPES.get(k)
                        {
                            st.pending_type = (Some(*to) != info.ty).then_some(*to);
                            st.error = None;
                        }
                    } else {
                        read_text(ui, info.ty.map_or("", |t| overview::type_name(t)), Decor::default(), &format!("{PANEL}.type"));
                    }
                });
                // The confirm step (overseer's look check): what the change does to the signals.
                if let (Some(to), Some(from)) = (st.pending_type, info.ty) {
                    let words = format!(
                        "Change {} to {}? {}",
                        info.tag,
                        ecad_cadcraft::plc::rack::with_article(to),
                        ecad_cadcraft::plc::rack::change_words(from, to)
                    );
                    ui.add(egui::Label::new(w::rich(ui, words, ecad_ui::tokens::ui_type::SMALL, w::text_2())).wrap());
                    let (yes, no) = ui
                        .horizontal(|ui| {
                            let yes = ui.push_id(format!("{PANEL}.type.confirm"), |ui| w::button(ui, "Change type", true, None)).inner.clicked();
                            (yes, w::button(ui, "Cancel", false, None).clicked())
                        })
                        .inner;
                    if yes {
                        lines.push(("type", format!("plccardtype {}", json!({"card": c.hex(), "type": to.code()}))));
                    }
                    if yes || no {
                        st.pending_type = None;
                    }
                }
                if let Some(("type", m)) = st.error.as_ref().map(|(k, m)| (*k, m)) {
                    ui.label(w::rich(ui, m, ecad_ui::tokens::ui_type::SMALL, ecad_ui::color(ecad_ui::SHELL.error)));
                }
                field(ui, &mut st, &mut lines, c, writable, "tag", "ID", "-A03", "");
                field(ui, &mut st, &mut lines, c, writable, "description", "Description", "What the card is for", "");
                w::row(ui, "Rack", |ui| {
                    let options: Vec<&str> = info.racks.iter().map(String::as_str).chain([NO_RACK]).collect();
                    let current = info.rack.as_deref().unwrap_or(NO_RACK);
                    let at = options.iter().position(|o| *o == current).unwrap_or(options.len().saturating_sub(1));
                    let mut picked = at;
                    let r = ui.add_enabled_ui(writable, |ui| Select::new(&mut picked, &options, &format!("{PANEL}.rack")).show(ui)).inner;
                    if r.changed()
                        && picked != at
                        && let Some(to) = options.get(picked)
                    {
                        let rack = if *to == NO_RACK { serde_json::Value::Null } else { json!(to) };
                        lines.push(("rack", format!("plcslot {}", json!({"card": c.hex(), "rack": rack}))));
                    }
                });
                field(ui, &mut st, &mut lines, c, writable, "slot", "Slot", "", "");
                field(ui, &mut st, &mut lines, c, writable, "function", "Function", "", info.function.1);
                field(ui, &mut st, &mut lines, c, writable, "location", "Location", "", info.location.1);
            });
            // Addresses (board 2.03).
            w::section(ui, &mut folded, full, "plc.addresses", "Addresses", "", false, |ui| {
                field(ui, &mut st, &mut lines, c, writable, "start", "Start", "", if info.pinned { "pinned" } else { "automatic" });
                // The rule is the project's (Settings ▸ PLC), read here.
                w::row(ui, "Rule", |ui| {
                    read_text(ui, "Follow slot order", Decor { hint: &info.rule, ..Decor::default() }, &format!("{PANEL}.rule"));
                });
            });
            // Detailed page (board 2.03).
            let summary = info.page.as_ref().map(|(_, n, _)| format!("page {n}")).unwrap_or_default();
            w::section(ui, &mut folded, full, "plc.page", "Detailed page", &summary, false, |ui| {
                match &info.page {
                    Some((pg, n, title)) => {
                        let (d, u) = info.drawn;
                        // A page with no title of its own shows its number once.
                        let head = if title.trim().is_empty() || title.trim() == n.as_str() { n.clone() } else { format!("{n} \u{b7} {title}") };
                        ui.label(w::rich(ui, format!("{head} \u{b7} {d} of {u} channels drawn"), ecad_ui::tokens::ui_type::BODY, w::text()));
                        ui.horizontal(|ui| {
                            if w::button(ui, &format!("\u{2192} Go to page {n}"), false, None).clicked() {
                                go = Some(*pg);
                            }
                            if u > d {
                                let words = if u - d == 1 { "Draw the missing channel" } else { "Draw the missing channels" };
                                if w::button(ui, words, false, (!writable).then_some(super::plc::WRITES_OFF)).clicked() {
                                    lines.push(("page", format!("plcchannels {}", json!({"card": c.hex(), "rest": true}))));
                                }
                            }
                        });
                    }
                    None => {
                        ui.label(w::rich(ui, "Not drawn yet.", ecad_ui::tokens::ui_type::BODY, w::text_2()));
                        if w::button(ui, "Make its page", false, (!writable).then_some(super::plc::WRITES_OFF)).clicked() {
                            lines.push(("page", format!("plcpage {}", json!({"card": c.hex()}))));
                        }
                    }
                }
                ui.add(egui::Label::new(w::rich(ui, PAGE_NOTE, ecad_ui::tokens::ui_type::SMALL, w::text_3())).wrap());
            });
            // Parts, with the accessories (board 2.03; brief 047's part lines).
            let summary = match info.parts.len() {
                0 => "none yet".to_string(),
                1 => "1 line \u{b7} qty".to_string(),
                n => format!("{n} lines \u{b7} qty"),
            };
            w::section(ui, &mut folded, full, "plc.parts", "Parts", &summary, false, |ui| {
                for p in &info.parts {
                    crate::shell::right::device::part_card(ui, p);
                }
                field(ui, &mut st, &mut lines, c, writable, "maker", "Maker", "", "main part");
                field(ui, &mut st, &mut lines, c, writable, "part_number", "Part number", "", "main part");
                if let Some(mut form) = st.accessory.take() {
                    for (k, label) in ["Description", "Part number", "Maker"].iter().enumerate() {
                        if let Some(buf) = form.get_mut(k) {
                            w::row(ui, label, |ui| TextEntry::new(buf, &format!("{PANEL}.accessory.{k}")).show(ui));
                        }
                    }
                    let (add, cancel) =
                        ui.horizontal(|ui| (w::button(ui, "Add", true, None).clicked(), w::button(ui, "Cancel", false, None).clicked())).inner;
                    if add {
                        let [d, pn, m] = form.clone();
                        let mut add = serde_json::Map::new();
                        for (k, v) in [("description", d), ("part_number", pn), ("maker", m)] {
                            if !v.trim().is_empty() {
                                add.insert(k.into(), json!(v.trim()));
                            }
                        }
                        lines.push(("accessory", format!("partline {}", json!({"component": c.hex(), "add": add}))));
                    }
                    if !cancel {
                        st.accessory = Some(form);
                    }
                } else {
                    ui.horizontal(|ui| {
                        if w::button(ui, "+ Accessory", false, (!writable).then_some(super::plc::WRITES_OFF)).clicked() {
                            st.accessory = Some(Default::default());
                        }
                        w::button(ui, "From catalogue", false, Some(CATALOGUE_COMES));
                    });
                }
                if let Some(("accessory", m)) = st.error.as_ref().map(|(k, m)| (*k, m)) {
                    ui.label(w::rich(ui, m, ecad_ui::tokens::ui_type::SMALL, ecad_ui::color(ecad_ui::SHELL.error)));
                }
            });
        });
    });
    for (key, line) in lines {
        match t.run_line(&line) {
            Some(e) => st.error = Some((key, e)),
            None => {
                st.error = None;
                if key == "accessory" {
                    st.accessory = None;
                }
            }
        }
    }
    if let Some(r) = t.reports.as_mut() {
        r.io.card_editor = st;
        r.io.card_folded = folded;
    }
    if let Some(pg) = go {
        t.plc_action(super::ListAction::GoToCard(pg, Some(c)));
    }
}
