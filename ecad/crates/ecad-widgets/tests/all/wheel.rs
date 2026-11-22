//! Brief 041 (U4b): the wheel widget: its geometry, press-move-release and click-click, greyed
//! slots and their reason, a slot's short list, and cancelling.

use crate::common::harness;
use ecad_tokens::{Icon, wheel as wt};
use ecad_ui::wheel::{self, Event, SlotSpec, Wheel, slot_at, slot_center};
use egui::{Modifiers, PointerButton, Pos2, pos2, vec2};
use egui_kittest::kittest::Queryable;

const C: Pos2 = Pos2 { x: 450.0, y: 350.0 };

#[test]
fn slots_are_43_degrees_with_2_degree_gaps_between_the_radii() {
    for i in 0..wt::SLOTS {
        assert_eq!(slot_at(C, slot_center(C, i)), Some(i), "slot {i}'s middle");
    }
    // UI rev 3 (board 1.14): about 190 px across.
    assert!((180.0..=200.0).contains(&(2.0 * wt::OUTER_RADIUS)));
    // Top is up; right is slot 2 (clockwise).
    assert_eq!(slot_at(C, C + vec2(0.0, -60.0)), Some(0));
    assert_eq!(slot_at(C, C + vec2(60.0, 0.0)), Some(2));
    assert_eq!(slot_at(C, C + vec2(0.0, 60.0)), Some(4));
    // The centre, beyond the rim and the gap at 22.5° are no slot.
    assert_eq!(slot_at(C, C + vec2(0.0, -20.0)), None);
    assert_eq!(slot_at(C, C + vec2(0.0, -110.0)), None);
    let gap = 22.5f32.to_radians();
    assert_eq!(slot_at(C, C + vec2(60.0 * gap.sin(), -60.0 * gap.cos())), None);
    let inside = 20.0f32.to_radians();
    assert_eq!(slot_at(C, C + vec2(60.0 * inside.sin(), -60.0 * inside.cos())), Some(0));
}

struct S {
    w: Option<Wheel>,
    slots: [Option<SlotSpec>; 8],
    events: Vec<Event>,
}

fn slots() -> [Option<SlotSpec>; 8] {
    let mut greyed = SlotSpec::new("Go to contacts", Icon::GoTo);
    greyed.greyed = Some("-K01 has no symbol on another page.".into());
    greyed.next = Some("Symbol".into());
    let mut list = SlotSpec::new("Set potential", Icon::Wire);
    list.list = Some(vec!["24V".into(), "0V".into(), "None".into()]);
    [
        Some(SlotSpec::new("Edit tag", Icon::Edit)),
        None,
        Some(greyed),
        None,
        Some(SlotSpec::new("Delete device", Icon::Delete)),
        Some(list),
        Some(SlotSpec::new("Rotate", Icon::Rotate)),
        None,
    ]
}

fn open(held: bool) -> egui_kittest::Harness<'static, S> {
    harness(
        |ui, s: &mut S| {
            if let Some(w) = &mut s.w {
                let ev = wheel::show(ui.ctx(), w, "-K01", &s.slots);
                if ev != Event::None {
                    s.events.push(ev);
                }
                if matches!(ev, Event::Run(_) | Event::Cancel | Event::Choose(..)) {
                    s.w = None;
                }
            }
        },
        S { w: Some(Wheel::new(C, held)), slots: slots(), events: Vec::new() },
    )
}

fn button(h: &mut egui_kittest::Harness<'_, S>, pos: Pos2, button: PointerButton, pressed: bool) {
    h.event(egui::Event::PointerMoved(pos));
    h.event(egui::Event::PointerButton { pos, button, pressed, modifiers: Modifiers::default() });
    h.run();
}

#[test]
fn the_wheel_shows_its_chip_and_slots_by_label() {
    let h = open(false);
    h.get_by_label("-K01");
    h.get_by_label("Wheel: -K01");
    h.get_by_label("Edit tag");
    h.get_by_label("Delete device");
}

#[test]
fn press_move_release_and_click_click_run_the_same_slot() {
    // Opened by a right press, released over Delete (bottom).
    let mut h = open(true);
    button(&mut h, slot_center(C, 4), PointerButton::Secondary, false);
    assert_eq!(h.state().events, [Event::Run(4)]);
    // Released in the centre: still open; then a click on Delete.
    let mut h = open(true);
    button(&mut h, C, PointerButton::Secondary, false);
    assert!(h.state().w.is_some() && h.state().events.is_empty());
    h.get_by_label("Delete device").click();
    h.run();
    assert_eq!(h.state().events, [Event::Run(4)]);
}

#[test]
fn releasing_or_clicking_outside_cancels_and_esc_too() {
    let mut h = open(true);
    button(&mut h, C + vec2(300.0, 0.0), PointerButton::Secondary, false);
    assert_eq!(h.state().events, [Event::Cancel]);
    let mut h = open(false);
    button(&mut h, pos2(20.0, 20.0), PointerButton::Primary, true);
    assert_eq!(h.state().events, [Event::Cancel]);
    let mut h = open(false);
    h.key_press(egui::Key::Escape);
    h.run();
    assert_eq!(h.state().events, [Event::Cancel]);
}

#[test]
fn a_greyed_slot_never_runs_and_says_why_with_its_next_step() {
    let mut h = open(false);
    h.get_by_label("Go to contacts").click();
    h.run();
    assert!(h.state().events.is_empty(), "greyed: nothing ran");
    h.get_by_label("-K01 has no symbol on another page.");
    h.get_by_label("Symbol").click();
    h.run();
    assert_eq!(h.state().events, [Event::Next(2)]);
}

#[test]
fn a_slot_opens_its_short_list() {
    let mut h = open(false);
    h.get_by_label("Set potential").click();
    h.run();
    assert!(h.state().events.is_empty());
    h.get_by_label("0V").click();
    h.run();
    assert_eq!(h.state().events, [Event::Choose(5, 1)]);
}
