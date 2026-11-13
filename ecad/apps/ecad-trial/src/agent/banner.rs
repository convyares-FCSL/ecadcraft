//! The agent banner (brief 035, design §7): its data contract, its words and a minimal,
//! unstyled rendering.
//!
//! # Semantics
//! - [`AgentBanner`] is the one value the trial keeps up to date ([`crate::Trial::agent_banner`]);
//!   the UI redesign (U2) places and styles it, keeping these fields, the controls
//!   ([`BannerAction`], through [`crate::Trial::agent_action`]) and the element names ([`ELEMENT`]
//!   and the `agent.banner.*` names).
//! - **Update map** (design §7.5): the banner changes only on `ecad.hello`, an accepted or refused
//!   agent call, the write flash running out, hold start and end, Pause/Resume, Disconnect,
//!   `ecad.bye`, the idle mark, access turned off, and Undo session; `steps` is recomputed only on
//!   those and on an undo, redo or branch switch by anyone. A person's ordinary edit changes
//!   nothing in it. Drawing it reads the value only.
//!
//! Excluded: the final look (colours, bar, placement) — the UI shell's.

use std::time::Instant;

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

/// Draw the banner minimally, unstyled, over the top of the drawing area (no new layout or
/// panel; the UI shell gives it its place). Returns the control clicked.
pub fn show(ctx: &egui::Context, banner: &AgentBanner, at: egui::Pos2, busy: bool) -> Option<BannerAction> {
    let now = Instant::now();
    let mut out = None;
    egui::Area::new(egui::Id::unique(ELEMENT)).fixed_pos(at).order(egui::Order::Foreground).show(ctx, |ui| {
        egui::Frame::popup(ui.style()).show(ui, |ui| {
            let state = banner.shown_state(now);
            // Two rows, the controls on the second, so they never move when the text changes.
            ui.vertical(|ui| {
                ui.horizontal(|ui| {
                    let strip = ui.label(format!("Agent {}", banner.agent));
                    named(&strip, egui::Role::Label, ELEMENT);
                    let st = ui.label(banner.state_text(state));
                    named(&st, egui::Role::Label, STATE);
                    if let Some(l) = banner.last_text().filter(|_| !matches!(state, BannerState::Finished | BannerState::Error)) {
                        let last = ui.link(l);
                        named(&last, egui::Role::Link, LAST);
                        if last.clicked() {
                            out = Some(BannerAction::OpenLast);
                        }
                    }
                    let rest = banner.text(now);
                    let tail: Vec<&str> =
                        rest.split(" · ").filter(|p| p.ends_with(" steps") || p.starts_with("refused: ") || p.ends_with(" reading")).collect();
                    if !tail.is_empty() {
                        ui.label(tail.join(" · "));
                    }
                });
                ui.horizontal(|ui| {
                    let (undo, pause, disconnect, dismiss) = banner.controls(now);
                    if undo {
                        let b = ui.add_enabled(banner.steps > 0 && !busy, egui::Button::new("Undo session"));
                        named(&b, egui::Role::Button, UNDO);
                        let b = if banner.steps == 0 { b.on_disabled_hover_text(NOTHING_TO_UNDO) } else { b };
                        if b.clicked() {
                            out = Some(BannerAction::UndoSession);
                        }
                    }
                    if pause {
                        let paused = state == BannerState::Paused;
                        let b = ui.button(if paused { "Resume" } else { "Pause" });
                        named(&b, egui::Role::Button, PAUSE);
                        if b.clicked() {
                            out = Some(if paused { BannerAction::Resume } else { BannerAction::Pause });
                        }
                    }
                    if disconnect {
                        let b = ui.button("Disconnect");
                        named(&b, egui::Role::Button, DISCONNECT);
                        if b.clicked() {
                            out = Some(BannerAction::Disconnect);
                        }
                    }
                    if dismiss {
                        let b = ui.button("Dismiss");
                        named(&b, egui::Role::Button, DISMISS);
                        if b.clicked() {
                            out = Some(BannerAction::Dismiss);
                        }
                    }
                });
            });
        });
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
