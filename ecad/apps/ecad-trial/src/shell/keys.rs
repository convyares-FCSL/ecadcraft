//! The shell's keys (spec §5, "Keyboard that remains"): the tool group keys V W P C D B T,
//! Ctrl+1–4, Ctrl+K, Ctrl+/, PgUp/PgDn, F, Alt+Left/Right and Esc; and, from the action table
//! (brief 041, [`ecad_cadcraft::actions`]), Del, F2, R, X, Ctrl+C, Ctrl+V, Ctrl+D, Ctrl+Z and
//! Ctrl+Y. Undo and redo are keys only (owner answer 8).
//!
//! The keys are taken before CADCraft's drawing reads the frame's input, so Ctrl+1 opens no
//! CADCraft palette and a group letter starts no command. Single letters count only while the
//! pointer is over the main view and no text field has the keyboard (and never while a command
//! prompts: its keywords are typed letters).

use ecad_cadcraft::actions::{self, Key as ActionKey};
use eframe::egui::{self, Key, KeyboardShortcut, Modifiers};

use super::{Slots, Workspace};
use crate::Trial;

/// The egui key and modifiers of an action key.
pub fn shortcut(k: ActionKey) -> (Modifiers, Key) {
    match k {
        ActionKey::Delete => (Modifiers::NONE, Key::Delete),
        ActionKey::F2 => (Modifiers::NONE, Key::F2),
        ActionKey::R => (Modifiers::NONE, Key::R),
        ActionKey::X => (Modifiers::NONE, Key::X),
        ActionKey::CtrlC => (Modifiers::COMMAND, Key::C),
        ActionKey::CtrlV => (Modifiers::COMMAND, Key::V),
        ActionKey::CtrlD => (Modifiers::COMMAND, Key::D),
        ActionKey::CtrlZ => (Modifiers::COMMAND, Key::Z),
        ActionKey::CtrlY => (Modifiers::COMMAND, Key::Y),
    }
}

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
        // Search pages, tags, symbols and actions (brief 041).
        t.search.open = !t.search.open;
        t.search.focus = t.search.open;
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
    if draw && !t.shell.state.cad_chrome {
        action_keys(t, ctx, over_main, prompting);
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
        // Esc while idle: close the wheel or a flyout, clear the selection (CADCraft's command
        // line hears no idle keys in the shell).
        if ctx.input(|i| i.key_pressed(Key::Escape)) {
            t.shell.tools.flyout = None;
            if !t.cad.session.selection().is_empty() {
                t.cad.session.set_selection(Vec::new());
            }
        }
    }
}

/// The action table's keys. Undo and redo work while a tool runs (as CADCraft's shortcuts did:
/// inside LINE, Ctrl+Z takes back a segment); the others act on the selection with nothing
/// prompting, the single letters and Del only over the page.
fn action_keys(t: &mut Trial, ctx: &egui::Context, over_main: bool, prompting: bool) {
    let take = |m: Modifiers, k: Key| take_exact(ctx, m, k);
    // Ctrl+Shift+Z is redo too (CADCraft's own shortcut, kept); checked before Ctrl+Z.
    let redo = take(Modifiers::COMMAND | Modifiers::SHIFT, Key::Z);
    for k in [ActionKey::CtrlZ, ActionKey::CtrlY] {
        let (m, key) = shortcut(k);
        if let Some(a) = actions::on_key(k)
            && (take(m, key) || (k == ActionKey::CtrlY && redo))
        {
            t.cad.start(a.command);
            t.after_logic();
        }
    }
    if prompting || t.shell.tools.wheel.is_some() {
        return;
    }
    for k in [ActionKey::CtrlC, ActionKey::CtrlV, ActionKey::CtrlD, ActionKey::Delete, ActionKey::F2, ActionKey::R, ActionKey::X] {
        let Some(a) = actions::on_key(k) else { continue };
        let (m, key) = shortcut(k);
        let selection = !t.cad.session.selection().is_empty();
        // Paste needs no selection; the rest act on it. Plain keys only over the page.
        let wanted = (k == ActionKey::CtrlV || selection) && (m != Modifiers::NONE || over_main);
        // Backspace deletes too on macOS (as CADCraft's command line did).
        let mac_delete = k == ActionKey::Delete && ctx.os() == egui::os::OperatingSystem::Mac && wanted && take(Modifiers::NONE, Key::Backspace);
        if !(wanted && (take(m, key) || mac_delete)) {
            continue;
        }
        let target = t.target_from_selection();
        if let Err(why) = t.run_action(a, target.as_ref(), None) {
            t.refuse(a.label, why, target.as_ref());
        }
    }
}

/// Take `k` pressed with exactly modifiers `m` (Ctrl/Cmd, Shift and Alt as given), once.
///
/// egui's `consume_shortcut` matches logically: it ignores an extra Shift, so Ctrl+Z would also
/// take Ctrl+Shift+Z (redo), Ctrl+C CADCraft's Ctrl+Shift+C (copy with base point) and R a
/// Shift+R. This check keeps those chords for what they are (review of #79).
pub fn take_exact(ctx: &egui::Context, m: Modifiers, k: Key) -> bool {
    ctx.input_mut(|i| {
        let Some(at) = i.events.iter().position(|e| {
            matches!(e, egui::Event::Key { key, pressed: true, modifiers, .. }
                if *key == k && modifiers.command == m.command && modifiers.shift == m.shift && modifiers.alt == m.alt)
        }) else {
            return false;
        };
        i.events.remove(at);
        true
    })
}

/// Whether the running command's prompt would take `letter`: it accepts text, or one of its
/// keywords starts with it.
fn prompt_takes(t: &Trial, letter: char) -> bool {
    t.cad
        .session
        .current_prompt()
        .is_some_and(|p| p.accept.text || p.keywords.iter().any(|k| k.chars().next().is_some_and(|c| c.eq_ignore_ascii_case(&letter))))
}
