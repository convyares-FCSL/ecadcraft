//! The shell's keys (spec §5, "Keyboard that remains"), those that exist today: the tool group
//! keys V W P C D B T, Ctrl+1–4, Ctrl+K, Ctrl+/, PgUp/PgDn, F, Alt+Left/Right, Esc and Del
//! (erase the selection; CADCraft's command line did it, and hears no idle keys in the shell).
//! Ctrl+Z and Ctrl+Y stay CADCraft's shortcuts (they reach the command path as before).
//!
//! The keys are taken before CADCraft's drawing reads the frame's input, so Ctrl+1 opens no
//! CADCraft palette and a group letter starts no command. Single letters count only while the
//! pointer is over the main view and no text field has the keyboard (and never while a command
//! prompts: its keywords are typed letters).

use eframe::egui::{self, Key, KeyboardShortcut, Modifiers};

use super::{Slots, Workspace};
use crate::Trial;

/// Take the shell's keys from this frame's input and act on them.
pub fn handle(t: &mut Trial, ctx: &egui::Context, slots: &Slots) {
    let typing = ctx.egui_wants_keyboard_input();
    let take = |m: Modifiers, k: Key| ctx.input_mut(|i| i.consume_shortcut(&KeyboardShortcut::new(m, k)));
    for (i, k) in [Key::Num1, Key::Num2, Key::Num3, Key::Num4].into_iter().enumerate() {
        if take(Modifiers::COMMAND, k)
            && let Some(w) = Workspace::from_index(i)
        {
            t.set_workspace(w);
        }
    }
    if take(Modifiers::COMMAND, Key::Slash) {
        t.toggle_text_panel();
    }
    if take(Modifiers::COMMAND, Key::K) {
        // Search: today's symbol search, for now (spec §5's search over everything is U4b).
        t.search.open = !t.search.open || t.shell.state.workspace == Workspace::Library;
    }
    if take(Modifiers::ALT, Key::ArrowLeft) {
        t.go_back(false);
    }
    if take(Modifiers::ALT, Key::ArrowRight) {
        t.go_back(true);
    }
    if typing {
        return;
    }
    let draw = t.shell.state.workspace == Workspace::Draw;
    if draw && take(Modifiers::NONE, Key::PageUp) {
        t.step_page(false);
    }
    if draw && take(Modifiers::NONE, Key::PageDown) {
        t.step_page(true);
    }
    let over_main = ctx.pointer_hover_pos().is_some_and(|p| slots.main.contains(p));
    let prompting = t.cad.session.prompting();
    // Del (and plain Backspace on macOS) erases the selection while no command runs, as
    // CADCraft's command line does; with idle typing off (US1) it no longer hears these keys.
    if draw && !prompting && !t.shell.state.cad_chrome && !t.cad.session.selection().is_empty() {
        let mac = ctx.os() == egui::os::OperatingSystem::Mac;
        if take(Modifiers::NONE, Key::Delete) || (mac && take(Modifiers::NONE, Key::Backspace)) {
            let _ = t.cad.run("erase.selection", serde_json::json!({}));
        }
    }
    if !draw || !over_main || t.shell.state.cad_chrome {
        return;
    }
    if prompting {
        // Pressed again while its group's tool prompts: the next tool of the group, unless the
        // prompt takes that letter (typed text, or a keyword starting with it).
        for (gi, g) in super::tool_strip::GROUPS.iter().enumerate() {
            if t.group_tool_running(gi) && !prompt_takes(t, g.letter) && take(Modifiers::NONE, g.key) {
                ctx.input_mut(|i| i.events.retain(|e| !matches!(e, egui::Event::Text(x) if x.eq_ignore_ascii_case(&g.letter.to_string()))));
                t.group_key(g.letter);
            }
        }
    } else {
        for g in super::tool_strip::GROUPS.iter() {
            if take(Modifiers::NONE, g.key) {
                t.group_key(g.letter);
            }
        }
        if take(Modifiers::NONE, Key::F) {
            t.zoom_to_drawing();
        }
        // Esc while idle: close a flyout, clear the selection (CADCraft's command line hears no
        // idle keys in the shell).
        if ctx.input(|i| i.key_pressed(Key::Escape)) {
            t.shell.tools.flyout = None;
            if !t.cad.session.selection().is_empty() {
                t.cad.session.set_selection(Vec::new());
            }
        }
    }
}

/// Whether the running command's prompt would take `letter`: it accepts text, or one of its
/// keywords starts with it.
fn prompt_takes(t: &Trial, letter: char) -> bool {
    t.cad
        .session
        .current_prompt()
        .is_some_and(|p| p.accept.text || p.keywords.iter().any(|k| k.chars().next().is_some_and(|c| c.eq_ignore_ascii_case(&letter))))
}
