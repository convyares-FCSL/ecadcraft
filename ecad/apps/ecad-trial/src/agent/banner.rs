//! The agent banner (brief 035, design §7; styled by brief 043, U4d): its data contract, its
//! words, its styled strip in the shell's banner slot ([`show`]) and the plain one-line fallback
//! ([`show_line`]).
//!
//! # Semantics
//! - [`AgentBanner`] is the one value the trial keeps up to date ([`crate::Trial::agent_banner`]);
//!   the shell places it in its 32 px banner slot (spec §2) and styles it ([`show`]), keeping these
//!   fields, the controls ([`BannerAction`], through [`crate::Trial::agent_action`]) and the
//!   element names ([`ELEMENT`] and the `agent.banner.*` names).
//! - **Update map** (design §7.5): the banner changes only on `ecad.hello`, an accepted or refused
//!   agent call, the write flash running out, hold start and end, Pause/Resume, Disconnect,
//!   `ecad.bye`, the idle mark, access turned off, and Undo session; `steps` is recomputed only on
//!   those and on an undo, redo or branch switch by anyone. A person's ordinary edit changes
//!   nothing in it. Drawing it reads the value only (no project read).
//! - The top bar's agent pill hides and shows the banner of the session shown ([`hidden`],
//!   [`toggle_hidden`]); a new session shows it again.
//!
//! Excluded: ask-first mode, a chat, Agent access settings (042), several writing agents.

use std::time::Instant;

use ecad_ui::prim::{self, Kind, Tone};
use ecad_ui::tokens::{Color, line, space, ui_type};
use eframe::egui;

/// The banner's element name.
pub const ELEMENT: &str = "agent.banner";
/// Element names of its parts and controls.
pub const STATE: &str = "agent.banner.state";
pub const LAST: &str = "agent.banner.last";
pub const UNDO: &str = "agent.banner.undo";
pub const PAUSE: &str = "agent.banner.pause";
pub const DISCONNECT: &str = "agent.banner.disconnect";
pub const DISMISS: &str = "agent.banner.dismiss";
/// The Undo session button's tooltip when nothing is left to undo.
pub const NOTHING_TO_UNDO: &str = "Nothing of this session is left to undo";
/// The Undo session button's reason while the person is in the middle of a command.
pub const FINISH_FIRST: &str = "Finish the command first";
/// The banner's ground: its state's colour at this opacity, %, over the panel.
const GROUND_PERCENT: u8 = 8;

/// The banner's state (design §7.2).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BannerState {
    Connected,
    /// For 1 s after each accepted write (derived from [`AgentBanner::flash_until`]).
    Writing,
    /// A write waits for the person to finish (derived from [`AgentBanner::held`]).
    Held,
    Paused,
    /// Writes are off for this session (derived from [`AgentBanner::writes_off`]).
    ReadOnly,
    /// `ecad.bye`, idle, or disconnected.
    Finished,
    /// Agent access stopped.
    Error,
}

/// The session's last accepted change.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LastChange {
    pub seq: u64,
    pub label: String,
    /// Wall clock, ms since the epoch.
    pub time_ms: u64,
    /// The branch it was recorded on (hex id).
    pub branch: String,
}

/// The banner's data contract (design §7.1).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AgentBanner {
    /// The bridge session shown.
    pub session: String,
    /// The agent's name (`claude-code`).
    pub agent: String,
    /// The stored state; [`AgentBanner::shown_state`] adds the derived ones.
    pub state: BannerState,
    /// Why writes are off for this session, when they are.
    pub writes_off: Option<String>,
    pub last_change: Option<LastChange>,
    /// This session's transactions still on the log.
    pub steps: usize,
    /// The last refusal's first line, cleared by the next accepted call.
    pub last_refusal: Option<String>,
    /// Other connected sessions (read only).
    pub others: usize,
    /// What a held write waits for (`the designer is in the middle of WIRE`).
    pub held: Option<String>,
    /// The write flash runs until then.
    pub flash_until: Option<Instant>,
    /// Why access stopped (Error).
    pub error: Option<String>,
}

/// A banner control (design §7.3).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BannerAction {
    /// Open History at the last change.
    OpenLast,
    UndoSession,
    Pause,
    Resume,
    Disconnect,
    Dismiss,
}

impl AgentBanner {
    /// A banner for a session that just said hello.
    pub fn new(session: &str, agent: &str) -> Self {
        AgentBanner {
            session: session.to_string(),
            agent: agent.to_string(),
            state: BannerState::Connected,
            writes_off: None,
            last_change: None,
            steps: 0,
            last_refusal: None,
            others: 0,
            held: None,
            flash_until: None,
            error: None,
        }
    }

    /// The state shown now: Error, Finished and Paused as stored; else Held while a write waits,
    /// Writing during the flash, ReadOnly when writes are off, Connected.
    pub fn shown_state(&self, now: Instant) -> BannerState {
        match self.state {
            BannerState::Error | BannerState::Finished | BannerState::Paused => self.state,
            _ if self.held.is_some() => BannerState::Held,
            _ if self.flash_until.is_some_and(|t| t > now) => BannerState::Writing,
            _ if self.writes_off.is_some() => BannerState::ReadOnly,
            _ => BannerState::Connected,
        }
    }

    /// The middle text for `state` (design §7.2).
    pub fn state_text(&self, state: BannerState) -> String {
        match state {
            BannerState::Connected | BannerState::Writing => "changing".into(),
            BannerState::Held => format!("waiting for you to finish{}", self.held.as_deref().map_or(String::new(), held_what)),
            BannerState::Paused => "paused — agent changes are refused".into(),
            BannerState::ReadOnly => format!("reading only: {}", self.writes_off.as_deref().unwrap_or("")),
            BannerState::Finished => format!("finished · {} steps", self.steps),
            BannerState::Error => format!("agent access stopped: {}", self.error.as_deref().unwrap_or("")),
        }
    }

    /// The last change's text: `last: #57 Wire W12 · 14:02:11`.
    pub fn last_text(&self) -> Option<String> {
        self.last_change.as_ref().map(|c| format!("last: #{} {} · {}", c.seq, c.label, clock(c.time_ms)))
    }

    /// The whole strip as one line (the minimal rendering; tests read it).
    pub fn text(&self, now: Instant) -> String {
        let state = self.shown_state(now);
        let mut parts = vec![format!("Agent {}", self.agent), self.state_text(state)];
        if !matches!(state, BannerState::Finished | BannerState::Error) {
            if let Some(l) = self.last_text() {
                parts.push(l);
            }
            parts.push(format!("{} steps", self.steps));
        }
        if let Some(r) = &self.last_refusal {
            parts.push(format!("refused: {r}"));
        }
        if self.others > 0 {
            parts.push(format!("+{} reading", self.others));
        }
        parts.join(" · ")
    }

    /// Which controls are shown: (undo, pause/resume, disconnect, dismiss).
    pub fn controls(&self, now: Instant) -> (bool, bool, bool, bool) {
        match self.shown_state(now) {
            BannerState::Finished => (true, false, false, true),
            BannerState::Error => (false, false, false, true),
            _ => (true, true, true, false),
        }
    }
}

/// ` WIRE` from `the designer is in the middle of WIRE`; other reasons in brackets.
fn held_what(said: &str) -> String {
    match said.strip_prefix("the designer is in the middle of ") {
        Some(id) => format!(" {id}"),
        None => format!(" ({said})"),
    }
}

/// `HH:MM:SS` local time of `ms` since the epoch.
fn clock(ms: u64) -> String {
    use chrono::{Local, TimeZone};
    i64::try_from(ms).ok().and_then(|ms| Local.timestamp_millis_opt(ms).single()).map_or_else(String::new, |t| t.format("%H:%M:%S").to_string())
}

fn named(r: &egui::Response, role: egui::Role, name: &str) {
    r.widget_info(|| egui::WidgetInfo::labeled(role, true, name));
}

/// The colour of a shown state (spec §2, design 035 §7.2): `agent` while it works, `warning`
/// while a write waits or writes are paused, `text_2` when reading only or finished, `error` when
/// access stopped.
pub fn state_color(state: BannerState) -> Color {
    let s = ecad_ui::SHELL;
    match state {
        BannerState::Connected | BannerState::Writing => s.agent,
        BannerState::Held | BannerState::Paused => s.warning,
        BannerState::ReadOnly | BannerState::Finished => s.text_2,
        BannerState::Error => s.error,
    }
}

/// The pill's tone for a shown state (the top bar's pill mirrors the banner).
pub fn state_tone(state: BannerState) -> Tone {
    match state {
        BannerState::Connected | BannerState::Writing => Tone::Agent,
        BannerState::Held | BannerState::Paused => Tone::Warning,
        BannerState::ReadOnly | BannerState::Finished => Tone::Neutral,
        BannerState::Error => Tone::Error,
    }
}

/// `connected since 14:02` from the wall clock of the hello (ms since the epoch).
pub fn since_text(since_ms: u64) -> String {
    let t = clock(since_ms);
    format!("connected since {}", t.get(..5).unwrap_or(&t))
}

/// The Undo session button's text: `Undo session (12)`.
pub fn undo_text(steps: usize) -> String {
    format!("Undo session ({steps})")
}

fn hidden_id() -> egui::Id {
    egui::Id::unique("ecad.agent.banner.hidden")
}

/// Whether the person hid the banner of `session` with the top bar's agent pill.
pub fn hidden(ctx: &egui::Context, session: &str) -> bool {
    ctx.data(|d| d.get_temp::<String>(hidden_id())).is_some_and(|s| s == session)
}

/// Hide the banner of `session`, or show it again.
pub fn toggle_hidden(ctx: &egui::Context, session: &str) {
    let now = hidden(ctx, session);
    ctx.data_mut(|d| {
        if now {
            d.remove::<String>(hidden_id());
        } else {
            d.insert_temp(hidden_id(), session.to_string());
        }
    });
}

/// A text in a UI type style and colour.
fn text(ui: &mut egui::Ui, s: &str, style: ui_type::TypeStyle, c: Color) -> egui::Response {
    ui.label(egui::RichText::new(s).font(ecad_ui::fonts::font(ui.ctx(), style)).color(ecad_ui::color(c)))
}

/// Draw the banner styled (spec §2: 32 px under the top bar), filling `ui`'s rect: a bar in the
/// state's colour, the agent's name pill, "connected since", the state, the last write as a go-to
/// to History, the last refusal and other readers; on the right Undo session (with its step
/// count), Pause writes or Resume, Disconnect, and Dismiss once finished, as `ecad-ui` buttons.
/// `since_ms`: the session's hello, wall clock. Returns the control clicked.
pub fn show(ui: &mut egui::Ui, banner: &AgentBanner, busy: bool, since_ms: Option<u64>) -> Option<BannerAction> {
    let now = Instant::now();
    let state = banner.shown_state(now);
    let s = ecad_ui::SHELL;
    let rect = ui.max_rect();
    let ink = state_color(state);
    // A soft ground in the state's colour, its bar on the left, a hairline under the strip.
    let bar = egui::Rect::from_min_size(rect.min, egui::vec2(space::XS, rect.height()));
    ui.painter().rect_filled(rect, 0.0, ecad_ui::color(s.panel));
    ui.painter().rect_filled(rect, 0.0, ecad_ui::color(ink.at_percent(GROUND_PERCENT)));
    ui.painter().rect_filled(bar, 0.0, ecad_ui::color(ink));
    ui.painter().hline(rect.x_range(), rect.bottom() - 0.5, egui::Stroke::new(line::HAIRLINE_PX, ecad_ui::color(s.line)));
    let mut out = None;
    let inner = rect.shrink2(egui::vec2(space::L, 0.0));
    let mut child = ui.new_child(egui::UiBuilder::new().max_rect(inner).layout(egui::Layout::right_to_left(egui::Align::Center)));
    let ui = &mut child;
    let live = !matches!(state, BannerState::Finished | BannerState::Error);
    // The controls on the right first, so they never move when the text changes.
    ui.spacing_mut().item_spacing.x = space::S;
    let (undo, pause, disconnect, dismiss) = banner.controls(now);
    if dismiss {
        let b = ui.add(prim::Button::new("Dismiss").kind(Kind::Quiet).small());
        named(&b, egui::Role::Button, DISMISS);
        if b.clicked() {
            out = Some(BannerAction::Dismiss);
        }
    }
    if disconnect {
        let b = ui.add(prim::Button::new("Disconnect").kind(Kind::Quiet).small());
        named(&b, egui::Role::Button, DISCONNECT);
        if b.clicked() {
            out = Some(BannerAction::Disconnect);
        }
    }
    if pause {
        let paused = state == BannerState::Paused;
        let b = ui.add(prim::Button::new(if paused { "Resume" } else { "Pause writes" }).small());
        named(&b, egui::Role::Button, PAUSE);
        if b.clicked() {
            out = Some(if paused { BannerAction::Resume } else { BannerAction::Pause });
        }
    }
    if undo {
        let label = undo_text(banner.steps);
        let mut button = prim::Button::new(&label).kind(Kind::Danger).small();
        if banner.steps == 0 {
            button = button.disabled(NOTHING_TO_UNDO);
        } else if busy {
            button = button.disabled(FINISH_FIRST);
        }
        let b = ui.add(button);
        named(&b, egui::Role::Button, UNDO);
        if b.clicked() && banner.steps > 0 && !busy {
            out = Some(BannerAction::UndoSession);
        }
    }
    // The rest, left to right, in what is left.
    ui.with_layout(egui::Layout::left_to_right(egui::Align::Center), |ui| {
        ui.spacing_mut().item_spacing.x = space::M;
        let who = prim::pill(ui, &banner.agent, state_tone(state));
        named(&who, egui::Role::Label, ELEMENT);
        if let Some(ms) = since_ms.filter(|_| live) {
            text(ui, &since_text(ms), ui_type::SMALL, s.text_3);
        }
        let st = text(ui, &banner.state_text(state), ui_type::BODY_EMPHASIS, if live { s.text } else { ink });
        named(&st, egui::Role::Label, STATE);
        if let Some(l) = banner.last_text().filter(|_| live) {
            let last = prim::read_value(ui, &l).on_hover_text("Open History at this change");
            named(&last, egui::Role::Link, LAST);
            if last.clicked() {
                out = Some(BannerAction::OpenLast);
            }
        }
        if let Some(r) = &banner.last_refusal {
            text(ui, &format!("refused: {r}"), ui_type::SMALL, s.error);
        }
        if banner.others > 0 {
            prim::badge(ui, format!("+{} reading", banner.others), Tone::Neutral);
        }
    });
    out
}

/// Draw the banner as one plain line with egui's own widgets (035's first rendering, kept as the
/// fallback; the shell draws [`show`]): the controls first, so they never move when the text changes, then
/// the agent, its state, the last change (a link to History) and the rest. Returns the control
/// clicked.
pub fn show_line(ui: &mut egui::Ui, banner: &AgentBanner, busy: bool) -> Option<BannerAction> {
    let now = Instant::now();
    let state = banner.shown_state(now);
    let mut out = None;
    ui.horizontal_wrapped(|ui| {
        let (undo, pause, disconnect, dismiss) = banner.controls(now);
        if undo {
            let b = ui.add_enabled(banner.steps > 0 && !busy, egui::Button::new("Undo session").small());
            named(&b, egui::Role::Button, UNDO);
            let b = if banner.steps == 0 { b.on_disabled_hover_text(NOTHING_TO_UNDO) } else { b };
            if b.clicked() {
                out = Some(BannerAction::UndoSession);
            }
        }
        if pause {
            let paused = state == BannerState::Paused;
            let b = ui.small_button(if paused { "Resume" } else { "Pause" });
            named(&b, egui::Role::Button, PAUSE);
            if b.clicked() {
                out = Some(if paused { BannerAction::Resume } else { BannerAction::Pause });
            }
        }
        if disconnect {
            let b = ui.small_button("Disconnect");
            named(&b, egui::Role::Button, DISCONNECT);
            if b.clicked() {
                out = Some(BannerAction::Disconnect);
            }
        }
        if dismiss {
            let b = ui.small_button("Dismiss");
            named(&b, egui::Role::Button, DISMISS);
            if b.clicked() {
                out = Some(BannerAction::Dismiss);
            }
        }
        let who = ui.label(egui::RichText::new(format!("Agent {}", banner.agent)).small());
        named(&who, egui::Role::Label, ELEMENT);
        let st = ui.label(egui::RichText::new(banner.state_text(state)).small());
        named(&st, egui::Role::Label, STATE);
        if let Some(l) = banner.last_text().filter(|_| !matches!(state, BannerState::Finished | BannerState::Error)) {
            let last = ui.link(egui::RichText::new(l).small());
            named(&last, egui::Role::Link, LAST);
            if last.clicked() {
                out = Some(BannerAction::OpenLast);
            }
        }
        let rest = banner.text(now);
        let tail: Vec<&str> = rest.split(" · ").filter(|p| p.ends_with(" steps") || p.starts_with("refused: ") || p.ends_with(" reading")).collect();
        if !tail.is_empty() {
            ui.label(egui::RichText::new(tail.join(" · ")).small());
        }
    });
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    #[test]
    fn the_states_and_their_words() {
        let now = Instant::now();
        let mut b = AgentBanner::new("0011223344556677", "claude-code");
        assert_eq!(b.shown_state(now), BannerState::Connected);
        assert!(b.text(now).starts_with("Agent claude-code · changing · 0 steps"), "{}", b.text(now));
        b.last_change = Some(LastChange { seq: 57, label: "Wire W12".into(), time_ms: 0, branch: String::new() });
        b.steps = 12;
        b.flash_until = Some(now + Duration::from_secs(1));
        assert_eq!(b.shown_state(now), BannerState::Writing);
        assert!(b.text(now).contains("last: #57 Wire W12"));
        assert!(b.text(now).contains("12 steps"));
        b.held = Some("the designer is in the middle of WIRE".into());
        assert_eq!(b.state_text(b.shown_state(now)), "waiting for you to finish WIRE");
        b.held = None;
        b.last_refusal = Some("one call may record at most 1 transaction".into());
        b.others = 1;
        assert!(b.text(now).ends_with("refused: one call may record at most 1 transaction · +1 reading"), "{}", b.text(now));
        b.state = BannerState::Paused;
        assert_eq!(b.state_text(b.shown_state(now)), "paused — agent changes are refused");
        b.state = BannerState::Finished;
        assert_eq!(b.state_text(b.shown_state(now)), "finished · 12 steps");
        assert_eq!(b.controls(now), (true, false, false, true));
        b.state = BannerState::Connected;
        b.flash_until = None;
        b.writes_off = Some("Agent access in the ECAD window is Read only.".into());
        assert_eq!(b.shown_state(now), BannerState::ReadOnly);
    }
}
