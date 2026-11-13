//! The primitives showcase (brief 037 scope 7): one page drawing every primitive in every state,
//! the shell colours and the type ramp, for screenshots beside the spec's Tokens board. The trial
//! hosts it as a hidden developer page, `ui.primitives`.

use ecad_tokens::{GRAPHITE, Icon, space, ui_type};
use egui::{Rect, RichText, ScrollArea, Sense, Ui, vec2};

use crate::prim::{
    Button, Chip, Field, FieldKind, IconButton, Kind, Look, RowKind, RowSpec, Segmented, Tone, badge, keycap, pill, provisional, read_value, row,
};
use crate::{color, fonts};

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
        }
    }
}

fn heading(ui: &mut Ui, text: &str) {
    ui.add_space(space::L);
    ui.label(RichText::new(text).font(fonts::font(ui.ctx(), ui_type::TITLE)).color(color(GRAPHITE.text)));
    ui.add_space(space::XS);
}

fn caption(ui: &mut Ui, text: &str) {
    ui.label(RichText::new(text).font(fonts::font(ui.ctx(), ui_type::CAPTION)).color(color(GRAPHITE.text_3)));
}

/// Draw the showcase into `ui`.
pub fn show(ui: &mut Ui, state: &mut Showcase) {
    let s = GRAPHITE;
    let frame = egui::Frame::NONE.fill(color(s.panel)).inner_margin(space::PANEL_PADDING);
    frame.show(ui, |ui| {
        ScrollArea::vertical().id_salt(PAGE).auto_shrink([false, false]).show(ui, |ui| {
            ui.spacing_mut().item_spacing = vec2(space::M, space::M);
            heading(ui, "Shell colours");
            ui.horizontal_wrapped(|ui| {
                for (name, c) in s.rows() {
                    let (rect, r) = ui.allocate_exact_size(vec2(36.0, 24.0), Sense::hover());
                    ui.painter().rect_filled(rect, crate::style::corner(ecad_tokens::radius::CHIP), color(c));
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
            // The bottom edge, so the last row is not under the scroll area's edge.
            let _ = ui.allocate_rect(Rect::from_min_size(ui.cursor().min, vec2(1.0, space::XL)), Sense::hover());
        });
    });
}
