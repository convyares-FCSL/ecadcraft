//! The primitives (brief 037, acceptance criterion 2): each primitive in each state is found by
//! its label; a disabled button reports its reason; a field shows the 2 px `accent_text` focus
//! ring; `read_value` has no frame and is clickable.

use ecad_tokens::{GRAPHITE, Icon, line};
use ecad_ui::color;
use ecad_ui::prim::{
    Button, Chip, Field, FieldKind, IconButton, Kind, Look, RowKind, RowSpec, Segmented, Tone, badge, keycap, pill, provisional, read_value, row,
};
use ecad_ui::showcase::{self, Showcase};
use egui::accesskit::{Role, Toggled};
use egui_kittest::kittest::{NodeT, Queryable};

use crate::common::{harness, strokes_in};

/// Every primitive of the showcase, in every state it draws, is found by its accessible label
/// (and role where it has one that matters).
#[test]
fn every_primitive_in_every_state_is_found_by_its_label() {
    let h = harness(|ui, s: &mut Showcase| showcase::show(ui, s), Showcase::default());
    for kind in ["Primary", "Secondary", "Danger", "Quiet"] {
        for state in ["normal", "hover", "pressed", "focus", "disabled"] {
            let label = format!("{kind} {state}");
            let node = h.get_by_role_and_label(Role::Button, &label);
            assert_eq!(node.accesskit_node().is_disabled(), state == "disabled", "{label}");
        }
    }
    for icon in Icon::ALL {
        h.get_by_role_and_label(Role::Button, icon.name());
    }
    assert_eq!(h.get_by_label("Wire tool").accesskit_node().toggled().map(|t| t == Toggled::True), Some(true));
    assert!(h.get_by_label("Box tool").accesskit_node().is_disabled());
    for tone in Tone::ALL {
        let label = format!("Pill {}", tone.name());
        let node = h.get_by_label(&label);
        assert_eq!(node.accesskit_node().description().as_deref(), Some(tone.name()));
    }
    for label in ["1", "2", "6", "IEC", "project", "Ctrl+K", "Alt+Left", "Esc"] {
        h.get_by_label(label);
    }
    for label in ["Chip", "Chip hover", "Chip selected", "Closable", "Remove Closable"] {
        h.get_by_role_and_label(Role::Button, label);
    }
    for label in ["Field text", "Field focused", "Field invalid", "Field read-only", "Field search", "Field number"] {
        h.get_by_role_and_label(Role::TextInput, label);
    }
    for (i, label) in ["Draw", "Lists", "Library", "Publish"].into_iter().enumerate() {
        let node = h.get_by_role_and_label(Role::RadioButton, label);
        assert_eq!(node.accesskit_node().toggled().map(|t| t == Toggled::True), Some(i == 0), "{label}");
    }
    for label in ["Group row", "List row", "List row hover", "List row selected", "Table row changed", "Table row"] {
        let node = h.get_by_role_and_label(Role::Row, label);
        assert_eq!(node.accesskit_node().toggled().map(|t| t == Toggled::True), Some(label == "List row selected"), "{label}");
    }
    h.get_by_role_and_label(Role::Link, "/3.4 (page 3, column 4)");
    assert_eq!(h.get_by_label("-K?1").accesskit_node().description().as_deref(), Some("provisional"));
    assert!(h.get_by_label("Field read-only").accesskit_node().is_disabled(), "read-only is not editable");
}

#[derive(Default)]
struct Clicks {
    enabled: u32,
    disabled: u32,
}

/// A disabled button reports its reason (accessible description, and a tooltip on hover) and
/// takes no click; an enabled one does.
#[test]
fn a_disabled_button_reports_its_reason() {
    const WHY: &str = "Nothing changed since revision B";
    let mut h = harness(
        |ui, c: &mut Clicks| {
            if ui.add(Button::new("Save").kind(Kind::Primary)).clicked() {
                c.enabled += 1;
            }
            if ui.add(Button::new("Release").disabled(WHY)).clicked() {
                c.disabled += 1;
            }
        },
        Clicks::default(),
    );
    let release = h.get_by_role_and_label(Role::Button, "Release");
    assert!(release.accesskit_node().is_disabled());
    assert_eq!(release.accesskit_node().description().as_deref(), Some(WHY));
    release.click();
    h.get_by_label("Save").click();
    h.run();
    assert_eq!((h.state().enabled, h.state().disabled), (1, 0));
    h.get_by_label("Release").hover();
    h.run();
    assert!(h.query_by_label(WHY).is_some(), "the reason is shown on hover");
}

/// A focused field draws the 2 px `accent_text` ring; unfocused, it does not.
#[test]
fn a_focused_field_shows_the_focus_ring() {
    let ring = color(GRAPHITE.accent_text);
    let mut h = harness(|ui, text: &mut String| drop(Field::new(text, FieldKind::Text, "Tag").show(ui)), String::from("-K01"));
    assert!(!strokes_in(&h, ring).contains(&line::FOCUS_RING_PX), "no ring before focus");
    h.get_by_label("Tag").focus();
    h.run();
    assert!(strokes_in(&h, ring).contains(&line::FOCUS_RING_PX), "a 2 px accent_text ring while focused");
    h.get_by_label("Tag").type_text("A");
    h.run();
    assert_eq!(h.state(), "-K01A");
}

/// A number field that does not hold a number is invalid: an error line, and the message as its
/// description; a given message marks any field invalid.
#[test]
fn an_invalid_field_says_why() {
    let mut h = harness(
        |ui, s: &mut (String, String)| {
            Field::new(&mut s.0, FieldKind::Number, "Height").show(ui);
            Field::new(&mut s.1, FieldKind::Text, "Tag").invalid("Tag already used").show(ui);
        },
        ("2,5".to_string(), "-K01".to_string()),
    );
    let error = color(GRAPHITE.error);
    assert_eq!(strokes_in(&h, error).iter().filter(|w| **w == line::FOCUS_RING_PX).count(), 2);
    assert_eq!(h.get_by_label("Height").accesskit_node().description().as_deref(), Some("Not a number"));
    assert_eq!(h.get_by_label("Tag").accesskit_node().description().as_deref(), Some("Tag already used"));
    h.state_mut().0 = "2.5".into();
    h.run();
    assert_eq!(h.get_by_label("Height").accesskit_node().description(), None);
}

/// `read_value` draws text only (no frame: no rectangle, no stroke) and is a clickable go-to.
#[test]
fn read_value_has_no_frame_and_is_clickable() {
    let mut h = harness(
        |ui, s: &mut (u32, egui::Rect)| {
            // Room around it, so the harness's own panel ground is not taken for its box.
            ui.allocate_space(egui::vec2(400.0, 100.0));
            let r = read_value(ui, "-K01 (page 3)");
            s.1 = r.rect;
            if r.clicked() {
                s.0 += 1;
            }
        },
        (0u32, egui::Rect::NOTHING),
    );
    let at = h.state().1;
    let boxes = h
        .output()
        .shapes
        .iter()
        .filter(|c| matches!(&c.shape, egui::Shape::Rect(r) if r.rect.intersects(at) && r.rect.area() < 10.0 * at.area()))
        .count();
    assert_eq!(boxes, 0, "a read value has no box");
    h.get_by_role_and_label(Role::Link, "-K01 (page 3)").click();
    h.run();
    assert_eq!(h.state().0, 1);
}

/// The segmented control changes its selection on a click; the chip's close button reports.
#[test]
fn segmented_and_chip_report_what_happened() {
    #[derive(Default)]
    struct S {
        selected: usize,
        changed: u32,
        closed: u32,
    }
    let mut h = harness(
        |ui, s: &mut S| {
            let items = ["Draw", "Lists"];
            if ui.add(Segmented::new(&mut s.selected, &items)).changed() {
                s.changed += 1;
            }
            if Chip::new("+CAB1").closable().show(ui).closed {
                s.closed += 1;
            }
        },
        S::default(),
    );
    h.get_by_role_and_label(Role::RadioButton, "Lists").click();
    h.run();
    h.get_by_label("Remove +CAB1").click();
    h.run();
    assert_eq!((h.state().selected, h.state().changed, h.state().closed), (1, 1, 1));
    assert_eq!(h.get_by_label("Lists").accesskit_node().toggled().map(|t| t == Toggled::True), Some(true));
}

/// The primitives outside the showcase's fixed set: rows by kind, icon buttons, labels.
#[test]
fn primitives_take_their_sizes_from_the_tokens() {
    let h = harness(
        |ui, _: &mut ()| {
            let r = row(ui, RowSpec { kind: RowKind::Group, ..Default::default() }, "Group", |_| ());
            assert_eq!(r.response.rect.height(), ecad_tokens::size::ROW_GROUP);
            let b = ui.add(Button::new("Ok"));
            assert_eq!(b.rect.height(), ecad_tokens::size::CONTROL);
            let i = ui.add(IconButton::new(Icon::Wire, "Wire").tool());
            assert_eq!(i.rect.size(), egui::vec2(ecad_tokens::size::TOOL_BUTTON, ecad_tokens::size::TOOL_BUTTON));
            let i = ui.add(IconButton::new(Icon::Close, "Close").look(Look::Hover));
            assert_eq!(i.rect.width(), ecad_tokens::size::ICON_BUTTON);
            pill(ui, "Saved", Tone::Ok);
            badge(ui, 3, Tone::Error);
            keycap(ui, "Ctrl++");
            provisional(ui, "24V-?");
        },
        (),
    );
    h.get_by_label("Ctrl++");
    h.get_by_label("Saved");
}
