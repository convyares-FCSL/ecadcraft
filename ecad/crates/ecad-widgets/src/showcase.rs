//! The primitives showcase (brief 037 scope 7): one page drawing every primitive in every state,
//! the shell colours and the type ramp, for screenshots beside the spec's Tokens board. The desktop app
//! hosts it as a hidden developer page, `ui.primitives`.

use {crate::tokens::Icon, crate::tokens::space, crate::tokens::ui_type};
use egui::{Rect, RichText, ScrollArea, Sense, Ui, vec2};

use crate::prim::{
    Button, Chip, Field, FieldKind, IconButton, Kind, Look, RowKind, RowSpec, Segmented, Tone, badge, keycap, pill, provisional, read_value, row,
};
use {crate::color, crate::fonts};

/// The page's accessible name and id.
pub const PAGE: &str = "ui.primitives";

/// What the showcase's live widgets edit.
#[derive(Debug, Clone)]
pub struct Showcase {
    pub text: String,
    pub search: String,
    pub number: String,
    pub invalid: String,
    pub read_only: String,
    pub workspace: usize,
    pub chip: bool,
    /// The gallery's selector and segmented choice.
    pub category: usize,
    pub basis: usize,
    /// Round 2's checkbox, radio choice and folding section (brief 063).
    pub checked: bool,
    pub radio: usize,
    pub open: bool,
}

impl Default for Showcase {
    fn default() -> Self {
        Self {
            text: "-K01".into(),
            search: String::new(),
            number: "2.5".into(),
            invalid: "2,5 mm".into(),
            read_only: "+CAB1".into(),
            workspace: 0,
            chip: true,
            category: 0,
            basis: 0,
            checked: true,
            radio: 0,
            open: true,
        }
    }
}

fn heading(ui: &mut Ui, text: &str) {
    ui.add_space(space::L);
    ui.label(RichText::new(text).font(fonts::font(ui.ctx(), ui_type::TITLE)).color(color(crate::SHELL.text)));
    ui.add_space(space::XS);
}

fn caption(ui: &mut Ui, text: &str) {
    ui.label(RichText::new(text).font(fonts::font(ui.ctx(), ui_type::CAPTION)).color(color(crate::SHELL.text_3)));
}

/// Draw the showcase into `ui`.
pub fn show(ui: &mut Ui, state: &mut Showcase) {
    let s = crate::SHELL;
    let frame = egui::Frame::NONE.fill(color(s.panel)).inner_margin(space::PANEL_PADDING);
    frame.show(ui, |ui| {
        ScrollArea::vertical().id_salt(PAGE).auto_shrink([false, false]).show(ui, |ui| {
            ui.spacing_mut().item_spacing = vec2(space::M, space::M);
            heading(ui, "Shell colours");
            ui.horizontal_wrapped(|ui| {
                for (name, c) in s.rows() {
                    let (rect, r) = ui.allocate_exact_size(vec2(36.0, 24.0), Sense::hover());
                    ui.painter().rect_filled(rect, crate::style::corner(crate::tokens::radius::CHIP), color(c));
                    r.on_hover_text(name);
                }
            });
            heading(ui, "Type");
            for (name, style) in ui_type::ALL {
                ui.label(
                    RichText::new(format!("{name}  {} / {} px, {}", style.size, style.line, style.weight))
                        .font(fonts::font(ui.ctx(), style))
                        .color(color(s.text)),
                );
            }
            heading(ui, "Buttons");
            for kind in [Kind::Primary, Kind::Secondary, Kind::Danger, Kind::Quiet] {
                ui.horizontal(|ui| {
                    let name = format!("{kind:?}");
                    for (look, state) in [(Look::Auto, "normal"), (Look::Hover, "hover"), (Look::Pressed, "pressed"), (Look::Focus, "focus")] {
                        ui.add(Button::new(&format!("{name} {state}")).kind(kind).look(look));
                    }
                    ui.add(Button::new(&format!("{name} disabled")).kind(kind).disabled("Shown when hovered: why it is disabled"));
                });
            }
            ui.horizontal(|ui| {
                ui.add(Button::new("Small").small());
                caption(ui, "small: 24 px");
            });
            heading(ui, "Icon buttons");
            ui.horizontal_wrapped(|ui| {
                for icon in Icon::ALL {
                    ui.add(IconButton::new(icon, icon.name()));
                }
            });
            ui.horizontal(|ui| {
                ui.add(IconButton::new(Icon::Wire, "Wire tool").tool().selected(true));
                ui.add(IconButton::new(Icon::Place, "Place tool").tool().look(Look::Hover));
                ui.add(IconButton::new(Icon::Cable, "Cable tool").tool().look(Look::Pressed));
                ui.add(IconButton::new(Icon::Text, "Text tool").tool());
                ui.add(IconButton::new(Icon::Box, "Box tool").tool().disabled("Not on this page kind"));
                ui.add(IconButton::new(Icon::Search, "Search focused").look(Look::Focus));
                caption(ui, "tool 36 px: selected, hover, pressed, normal, disabled; 28 px focused");
            });
            heading(ui, "Pills and badges");
            ui.horizontal_wrapped(|ui| {
                for tone in Tone::ALL {
                    pill(ui, &format!("Pill {}", tone.name()), tone);
                }
            });
            ui.horizontal_wrapped(|ui| {
                for (i, tone) in Tone::ALL.into_iter().enumerate() {
                    badge(ui, i + 1, tone);
                }
                badge(ui, "IEC", Tone::Neutral);
                badge(ui, "project", Tone::Agent);
            });
            heading(ui, "Chips");
            ui.horizontal_wrapped(|ui| {
                ui.add(Chip::new("Chip"));
                ui.add(Chip::new("Chip hover").look(Look::Hover));
                let r = Chip::new("Chip selected").selected(state.chip).show(ui);
                if r.response.clicked() {
                    state.chip = !state.chip;
                }
                let _ = Chip::new("Closable").closable().show(ui);
            });
            heading(ui, "Fields");
            ui.horizontal_wrapped(|ui| {
                Field::new(&mut state.text, FieldKind::Text, "Field text").width(160.0).show(ui);
                Field::new(&mut state.text.clone(), FieldKind::Text, "Field focused").width(160.0).look(Look::Focus).show(ui);
                Field::new(&mut state.invalid, FieldKind::Number, "Field invalid").width(160.0).show(ui);
                Field::new(&mut state.read_only, FieldKind::Text, "Field read-only").width(160.0).read_only(true).show(ui);
            });
            ui.horizontal_wrapped(|ui| {
                Field::new(&mut state.search, FieldKind::Search, "Field search").placeholder("Search  Ctrl+K").width(240.0).show(ui);
                Field::new(&mut state.number, FieldKind::Number, "Field number").width(100.0).show(ui);
            });
            heading(ui, "Segmented control");
            ui.horizontal(|ui| {
                let items = ["Draw", "Lists", "Library", "Publish"];
                ui.add(Segmented::new(&mut state.workspace, &items).look(2, Look::Hover));
            });
            heading(ui, "Key caps");
            ui.horizontal(|ui| {
                for k in ["Ctrl+K", "Ctrl+1", "Esc", "W", "Alt+Left"] {
                    keycap(ui, k);
                }
            });
            heading(ui, "Rows");
            let rows = [
                ("Group row", RowSpec { kind: RowKind::Group, ..Default::default() }),
                ("List row", RowSpec::default()),
                ("List row hover", RowSpec { look: Look::Hover, ..Default::default() }),
                ("List row selected", RowSpec { selected: true, ..Default::default() }),
                ("Table row changed", RowSpec { kind: RowKind::Table, tint: 1.0, ..Default::default() }),
                ("Table row", RowSpec { kind: RowKind::Table, ..Default::default() }),
            ];
            for (label, spec) in rows {
                row(ui, spec, label, |ui| {
                    ui.label(RichText::new(label).font(fonts::font(ui.ctx(), ui_type::BODY)).color(color(s.text)));
                });
            }
            heading(ui, "Set and read");
            ui.horizontal(|ui| {
                caption(ui, "read:");
                read_value(ui, "/3.4 (page 3, column 4)");
                caption(ui, "provisional:");
                provisional(ui, "-K?1");
            });
            components(ui, state);
            // The bottom edge, so the last row is not under the scroll area's edge.
            let _ = ui.allocate_rect(Rect::from_min_size(ui.cursor().min, vec2(1.0, space::XL)), Sense::hover());
        });
    });
}

/// The states every value component is shown in, left to right.
pub const STATES: [&str; 6] = ["Editable", "Hover", "Focus", "Read-only", "Disabled", "Empty"];

/// One cell of the component table: the state's caption over the component, a column wide.
fn cell(ui: &mut Ui, add: impl FnOnce(&mut Ui)) {
    ui.allocate_ui(vec2(COLUMN, crate::tokens::size::CONTROL), |ui| {
        ui.set_width(COLUMN);
        add(ui);
    });
}

/// A component table column's width: an inspector value column.
const COLUMN: f32 = crate::tokens::size::LABEL_COLUMN * 2.0;

/// The value components (`crate::control`, `crate::panel`), each in every state (design
/// `docs/design/components.md`): the gallery the style guide's screenshots come from.
pub fn components(ui: &mut Ui, state: &mut Showcase) {
    use crate::control::{Decor, Select, TextEntry, Toggle, ValueBox, link, read_text};
    heading(ui, "Value components");
    caption(ui, "Anything the person can change looks editable at a glance; a value read from the drawing never looks like an input.");
    ui.horizontal(|ui| {
        ui.add_space(crate::tokens::size::LABEL_COLUMN);
        for s in STATES {
            cell(ui, |ui| caption(ui, s));
        }
    });
    let looks = [Look::Auto, Look::Hover, Look::Focus];
    let line = |ui: &mut Ui, name: &str, each: &mut dyn FnMut(&mut Ui, usize)| {
        ui.horizontal(|ui| {
            crate::panel::label(ui, name);
            for i in 0..STATES.len() {
                cell(ui, |ui| each(ui, i));
            }
        });
    };
    let swatch = Some((ecad_drawing_style::sheet::PAPER.ink, None));
    line(ui, "Text entry", &mut |ui, i| {
        let mut t = if i == 5 { String::new() } else { state.text.clone() };
        let e = TextEntry::new(&mut t, "Text entry").hint("auto").bold(true);
        match i {
            0..=2 => {
                let e = e.look(looks.get(i).copied().unwrap_or_default());
                if i == 0 {
                    TextEntry::new(&mut state.text, "Text entry").hint("auto").bold(true).show(ui);
                } else {
                    e.show(ui);
                }
            }
            3 => {
                e.read_only(true).show(ui);
            }
            4 => {
                ui.add_enabled_ui(false, |ui| e.show(ui));
            }
            _ => {
                e.placeholder("Not set").show(ui);
            }
        }
    });
    line(ui, "Number + unit", &mut |ui, i| {
        let mut t = if i == 5 { String::new() } else { state.number.clone() };
        let e = TextEntry::new(&mut t, "Number with unit").number("mm\u{b2}").hint("from category");
        match i {
            0 => {
                TextEntry::new(&mut state.number, "Number with unit").number("mm\u{b2}").hint("from category").show(ui);
            }
            1 | 2 => {
                e.look(looks.get(i).copied().unwrap_or_default()).show(ui);
            }
            3 => {
                e.read_only(true).show(ui);
            }
            4 => {
                ui.add_enabled_ui(false, |ui| e.show(ui));
            }
            _ => {
                e.show(ui);
            }
        }
    });
    let options = ["Power L / N", "Control 24 V", "PE"];
    line(ui, "Selector", &mut |ui, i| {
        let mut k = if i == 5 { usize::MAX } else { state.category };
        fn sel<'a>(k: &'a mut usize, options: &'a [&'a str]) -> Select<'a> {
            Select::new(k, options, "Selector").placeholder("Choose…")
        }
        match i {
            0 => {
                Select::new(&mut state.category, &options, "Selector").show(ui);
            }
            1 | 2 => {
                sel(&mut k, &options).look(looks.get(i).copied().unwrap_or_default()).show(ui);
            }
            3 => {
                sel(&mut k, &options).read_only(true).show(ui);
            }
            4 => {
                ui.add_enabled_ui(false, |ui| sel(&mut k, &options).show(ui));
            }
            _ => {
                sel(&mut k, &options).show(ui);
            }
        }
    });
    line(ui, "Colour", &mut |ui, i| {
        let text = if i == 5 { "" } else { "BK" };
        let d = Decor { hint: "from category", swatch: if i == 5 { None } else { swatch }, ..Decor::default() };
        match i {
            3 => {
                read_text(ui, text, d, "Colour");
            }
            4 => {
                ui.add_enabled_ui(false, |ui| ValueBox::new(text, "Colour").decor(d).show(ui));
            }
            _ => {
                ValueBox::new(text, "Colour").decor(d).look(looks.get(i).copied().unwrap_or_default()).show(ui);
            }
        }
    });
    line(ui, "Value + hint", &mut |ui, i| {
        let text = if i == 5 { "" } else { "L2" };
        let d = Decor { hint: "from the drawing", ..Decor::default() };
        match i {
            3 => {
                read_text(ui, text, d, "Value with hint");
            }
            4 => {
                ui.add_enabled_ui(false, |ui| ValueBox::new(text, "Value with hint").decor(d).show(ui));
            }
            _ => {
                ValueBox::new(text, "Value with hint").decor(d).look(looks.get(i).copied().unwrap_or_default()).show(ui);
            }
        }
    });
    line(ui, "Read value", &mut |ui, i| {
        let text = if i == 5 { "" } else { "-K01:4 \u{b7} 2/C1" };
        read_text(ui, text, Decor::default(), "Read value");
    });
    line(ui, "Toggle", &mut |ui, i| {
        let mut on = i != 5 && state.chip;
        let t = Toggle::new(&mut on, "Toggle");
        match i {
            0 => {
                Toggle::new(&mut state.chip, "Toggle").show(ui);
            }
            1 | 2 => {
                t.look(looks.get(i).copied().unwrap_or_default()).show(ui);
            }
            3 => {
                t.read_only(true).show(ui);
            }
            4 => {
                ui.add_enabled_ui(false, |ui| t.show(ui));
            }
            _ => {
                t.show(ui);
            }
        }
    });
    line(ui, "Segmented", &mut |ui, i| {
        let items = ["Datasheet", "Table", "Manual"];
        let mut k = if i == 5 { usize::MAX } else { state.basis };
        match i {
            0 => {
                ui.add(Segmented::new(&mut state.basis, &items));
            }
            1 => {
                ui.add(Segmented::new(&mut k, &items).look(1, Look::Hover));
            }
            2 => {
                ui.add(Segmented::new(&mut k, &items).look(1, Look::Focus));
            }
            3 => {
                ui.add(Segmented::new(&mut k, &items).read_only(true));
            }
            4 => {
                ui.add_enabled_ui(false, |ui| ui.add(Segmented::new(&mut k, &items)));
            }
            _ => {
                ui.add(Segmented::new(&mut k, &items));
            }
        }
    });
    line(ui, "Link", &mut |ui, i| {
        if i == 0 || i == 1 {
            link(ui, "\u{2192} 2/C1");
        }
    });
    line(ui, "Status pill", &mut |ui, i| {
        let (tone, words) = match i {
            0 => (Tone::Ok, "No checks on L2-03"),
            1 => (Tone::Warning, "1 warning"),
            2 => (Tone::Error, "1 error"),
            _ => return,
        };
        crate::panel::status_pill(ui, tone, words, words);
    });
    line(ui, "Buttons", &mut |ui, i| {
        let (label, kind) = match i {
            0 => ("Primary", Kind::Primary),
            1 => ("Secondary", Kind::Secondary),
            2 => ("Quiet", Kind::Quiet),
            3 => ("Danger", Kind::Danger),
            4 => {
                ui.add(Button::new("Disabled").disabled("not yet"));
                return;
            }
            _ => return,
        };
        ui.add(Button::new(label).kind(kind).small());
    });
    // Brief 057's shared pieces: several lines of text, a table's colour and toggle cells, and the
    // splitter between two stacked regions.
    ui.horizontal(|ui| {
        crate::panel::label(ui, "Text area");
        let mut rows = "Several lines\ntyped here".to_string();
        ui.allocate_ui(vec2(space::XXL * 12.0, 0.0), |ui| crate::control::TextArea::new(&mut rows, "Text area").rows(3).show(ui));
    });
    ui.horizontal(|ui| {
        crate::panel::label(ui, "Table cells");
        for cell in [
            crate::table::Cell::Colour(vec![ecad_drawing_style::sheet::PAPER.ink], "BK".into()),
            crate::table::Cell::Colour(vec![crate::tokens::Color::hex(0x2e7d32), crate::tokens::Color::hex(0xf9d825)], "GNYE".into()),
            crate::table::Cell::Toggle(false),
            crate::table::Cell::Toggle(true),
        ] {
            let (r, _) = ui.allocate_exact_size(vec2(space::XXL * 3.0, crate::tokens::size::CONTROL), Sense::hover());
            let _ = crate::table::cells::draw(ui, r, &cell, crate::table::Align::Left, false);
        }
    });
    ui.horizontal(|ui| {
        crate::panel::label(ui, "Splitter");
        ui.allocate_ui(vec2(space::XXL * 12.0, crate::tokens::size::CONTROL), |ui| crate::splitter::horizontal(ui, "Splitter"));
    });
    ui.add_space(space::M);
    let mut open = true;
    let _ = crate::panel::section_header(ui, open, "Section header", "18.8 A \u{2265} 4 A");
    open = !open;
    let _ = crate::panel::section_header(ui, open, "Folded section", "2/D1");
    round_two(ui, state);
}

/// Round 2's pieces (brief 063): section header, property grid, list rows, text styles,
/// checkbox, radio, search field and a dialog's footer, each in its states.
fn round_two(ui: &mut Ui, state: &mut Showcase) {
    use crate::prim::{Checkbox, ListRow, PropertyGrid, Radio, SearchField, SectionHeader};
    heading(ui, "Shared pieces (round 2)");
    let mut folded = false;
    SectionHeader::new(&mut state.open, "Folding section").summary("3 rows").show(ui);
    SectionHeader::new(&mut folded, "Folding section, folded").summary("2/D1").show(ui);
    ui.add_space(space::S);
    let mut name = "Spare cores".to_string();
    PropertyGrid::new().show(ui, |g| {
        g.row("Property row", |ui| crate::control::TextEntry::new(&mut name, "Property row").width(COLUMN).show(ui));
        g.row("Read row", |ui| crate::control::read_text(ui, "-W12 \u{b7} 2/C1", crate::control::Decor::default(), "Read row"));
    });
    ui.add_space(space::S);
    ui.allocate_ui(vec2(COLUMN * 2.0, 0.0), |ui| {
        ListRow::new("List row, plain").show(ui);
        ListRow::new("List row, hovered").look(Look::Hover).show(ui);
        ListRow::new("List row, selected").selected(true).show_with(ui, |ui| {
            crate::prim::caption(ui, "page 4");
        });
        ListRow::new("List row, dim (archived)").dim(true).show(ui);
    });
    ui.add_space(space::S);
    let line = |ui: &mut Ui, name: &str, add: &mut dyn FnMut(&mut Ui)| {
        ui.horizontal(|ui| {
            crate::panel::label(ui, name);
            add(ui);
        });
    };
    line(ui, "Text", &mut |ui| {
        crate::prim::heading(ui, "Heading");
        crate::prim::body(ui, "Body", false);
        crate::prim::body(ui, "Secondary", true);
        crate::prim::caption(ui, "CAPTION");
    });
    line(ui, "Notes", &mut |ui| {
        crate::prim::note(ui, "A note", Tone::Neutral);
        crate::prim::note(ui, "A warning", Tone::Warning);
        crate::prim::note(ui, "An error", Tone::Error);
    });
    line(ui, "Checkbox", &mut |ui| {
        Checkbox::new(&mut state.checked, "Live").show(ui);
        let (mut on, mut off) = (true, false);
        Checkbox::new(&mut off, "Hover").look(Look::Hover).show(ui);
        Checkbox::new(&mut on, "Focus").look(Look::Focus).show(ui);
        ui.add_enabled_ui(false, |ui| Checkbox::new(&mut on, "Disabled").show(ui));
    });
    line(ui, "Radio", &mut |ui| {
        for (i, label) in ["Keep main", "Keep branch"].into_iter().enumerate() {
            if Radio::new(state.radio == i, label).show(ui).clicked() {
                state.radio = i;
            }
        }
        Radio::new(false, "Hover").look(Look::Hover).show(ui);
        ui.add_enabled_ui(false, |ui| Radio::new(true, "Disabled").show(ui));
    });
    line(ui, "Search field", &mut |ui| {
        SearchField::new(&mut state.search, "Search field").placeholder("Search pages").width(COLUMN).show(ui);
        let mut t = "K01".to_string();
        SearchField::new(&mut t, "Search field, focused").width(COLUMN).look(Look::Focus).show(ui);
    });
    line(ui, "Dialog footer", &mut |ui| {
        let size = vec2(COLUMN, crate::tokens::size::CONTROL);
        ui.allocate_ui_with_layout(size, egui::Layout::right_to_left(egui::Align::Center), |ui| {
            crate::dialog::action(ui, "Merge", Kind::Primary, "Dialog primary", None);
            crate::dialog::action(ui, "Cancel", Kind::Secondary, "Dialog secondary", None);
        });
        ui.allocate_ui_with_layout(size, egui::Layout::right_to_left(egui::Align::Center), |ui| {
            crate::dialog::action(ui, "Delete", Kind::Danger, "Dialog danger", None);
            crate::dialog::action(ui, "Cancel", Kind::Secondary, "Dialog cancel", None);
        });
    });
}
