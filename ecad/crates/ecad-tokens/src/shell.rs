//! Shell colours: the Graphite set (spec §4, "Shell colours") and the light set (UI rev 3, board
//! 1.17). The person picks one with the top bar's light/dark switch; neither follows the operating
//! system's setting.

use crate::{Color, WHITE};

/// The shell's colours. Field names are the spec's token names (`line-strong` → `line_strong`).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Shell {
    /// Window ground, the desk behind the sheet.
    pub bg: Color,
    /// Panels, top bar, status bar.
    pub panel: Color,
    /// Fields, buttons, group rows, chips.
    pub raised: Color,
    /// Hovered row or button.
    pub hover: Color,
    /// Pressed button, active mode, active side panel.
    pub active: Color,
    /// 1 px hairlines between regions and rows.
    pub line: Color,
    /// Popover borders, key caps, badges.
    pub line_strong: Color,
    /// Primary text.
    pub text: Color,
    /// Secondary text.
    pub text_2: Color,
    /// Field labels, placeholders.
    pub text_3: Color,
    /// Primary button, active tool.
    pub accent: Color,
    /// Selected outline, focus ring, links.
    pub accent_text: Color,
    /// Selected row: the accent at 18 %.
    pub accent_soft: Color,
    /// Saved, check passed.
    pub ok: Color,
    /// Warning.
    pub warning: Color,
    /// Error, delete.
    pub error: Color,
    /// Provisional and pending values.
    pub provisional: Color,
    /// Agent presence and agent-made changes.
    pub agent: Color,
    /// Cable names in lists.
    pub cable: Color,
    /// Links and connection points in lists.
    pub connect: Color,
}

const ACCENT: Color = Color::hex(0x2F6BE0);

/// Opacity of a soft fill, %: the selected row (`accent_soft`) and the tinted ground of a pill in
/// a status tone.
pub const SOFT_PERCENT: u8 = 18;

/// The Graphite shell.
pub const GRAPHITE: Shell = Shell {
    bg: Color::hex(0x131416),
    panel: Color::hex(0x1C1D20),
    raised: Color::hex(0x25272B),
    hover: Color::hex(0x2D3035),
    active: Color::hex(0x353941),
    line: Color::hex(0x32353B),
    line_strong: Color::hex(0x464A52),
    text: Color::hex(0xE8E9EB),
    text_2: Color::hex(0xA9ADB5),
    text_3: Color::hex(0x8A8F98),
    accent: ACCENT,
    accent_text: Color::hex(0x7FB0FF),
    accent_soft: ACCENT.at_percent(SOFT_PERCENT),
    ok: Color::hex(0x55D68B),
    warning: Color::hex(0xF2B84B),
    error: Color::hex(0xFF6B5E),
    provisional: Color::hex(0xFF7AB8),
    agent: Color::hex(0xB9E34A),
    cable: Color::hex(0xB48CFF),
    connect: Color::hex(0x45CFD3),
};

/// The light shell (UI rev 3, board 1.17): the same meanings as [`GRAPHITE`] on a light ground.
/// The status and list colours are darker than Graphite's so that text in them keeps 4.5:1 on
/// every light ground ([`Shell::text_pairs`]).
pub const LIGHT: Shell = Shell {
    bg: Color::hex(0xE6E8EB),
    panel: Color::hex(0xF7F8F9),
    raised: WHITE,
    hover: Color::hex(0xEDEFF2),
    active: Color::hex(0xE1E5EB),
    line: Color::hex(0xD5D9DF),
    line_strong: Color::hex(0xBCC3CC),
    text: Color::hex(0x16191D),
    text_2: Color::hex(0x474D56),
    text_3: Color::hex(0x5F6670),
    accent: ACCENT,
    accent_text: Color::hex(0x1F5FCC),
    accent_soft: ACCENT.at_percent(LIGHT_SOFT_PERCENT),
    ok: Color::hex(0x116E3E),
    warning: Color::hex(0x8A5200),
    error: Color::hex(0xC4231A),
    provisional: Color::hex(0xB0186A),
    agent: Color::hex(0x4A7200),
    cable: Color::hex(0x6A35C8),
    connect: Color::hex(0x066A6E),
};

/// Opacity of the light shell's soft fill, %: 18 % reads too strong on a light ground (board 1.17's
/// selected row is the accent at about 12 %).
pub const LIGHT_SOFT_PERCENT: u8 = 12;

impl Shell {
    /// Every token as `(name, colour)`, in the spec's order; names as in `tokens.md` (`shell.text_2`).
    pub fn rows(&self) -> [(&'static str, Color); 20] {
        [
            ("shell.bg", self.bg),
            ("shell.panel", self.panel),
            ("shell.raised", self.raised),
            ("shell.hover", self.hover),
            ("shell.active", self.active),
            ("shell.line", self.line),
            ("shell.line_strong", self.line_strong),
            ("shell.text", self.text),
            ("shell.text_2", self.text_2),
            ("shell.text_3", self.text_3),
            ("shell.accent", self.accent),
            ("shell.accent_text", self.accent_text),
            ("shell.accent_soft", self.accent_soft),
            ("shell.ok", self.ok),
            ("shell.warning", self.warning),
            ("shell.error", self.error),
            ("shell.provisional", self.provisional),
            ("shell.agent", self.agent),
            ("shell.cable", self.cable),
            ("shell.connect", self.connect),
        ]
    }

    /// The text colours and the backgrounds each is used on, as `(text, background)` names and
    /// colours. Every pair must reach 4.5:1 (WCAG AA for body text); a token not listed for a
    /// background is not used as text on it (for example `text_3` is never text on `hover` or
    /// `active`, where it would fall below 4.5:1).
    pub fn text_pairs(&self) -> Vec<(&'static str, Color, &'static str, Color)> {
        let grounds = [("bg", self.bg), ("panel", self.panel), ("raised", self.raised), ("hover", self.hover), ("active", self.active)];
        let mut out = Vec::new();
        // Text on every ground.
        let everywhere = [("text", self.text), ("text_2", self.text_2), ("accent_text", self.accent_text)];
        // Labels and placeholders sit on panels and in fields, never on a hovered or pressed control.
        let resting = [("text_3", self.text_3)];
        // Status and list colours: never on a pressed control.
        let status = [
            ("ok", self.ok),
            ("warning", self.warning),
            ("error", self.error),
            ("provisional", self.provisional),
            ("agent", self.agent),
            ("cable", self.cable),
            ("connect", self.connect),
        ];
        for (t, tc) in everywhere {
            out.extend(grounds.iter().map(|(g, gc)| (t, tc, *g, *gc)));
        }
        for (t, tc) in resting {
            out.extend(grounds.iter().take(3).map(|(g, gc)| (t, tc, *g, *gc)));
        }
        for (t, tc) in status {
            out.extend(grounds.iter().take(4).map(|(g, gc)| (t, tc, *g, *gc)));
        }
        // The primary button's label.
        out.push(("white", WHITE, "accent", self.accent));
        out
    }
}
