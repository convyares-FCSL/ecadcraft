//! A representation's role: a device symbol's part in its device, or one
//! of the fixed roles of the other representations.
//!
//! # Semantics
//! - A device's symbol carries only its part ([`PartRole`]); which symbol it is comes from its
//!   definition, never from the role. The part is set when the symbol is placed, from the
//!   declaring symbol ([`crate::Definition::part_role`]), and stored.
//! - Every role is stored as one word (`head`, `contact`, `route`, `box`, `fan-anchor`…); an
//!   unknown word is a read error.
//!
//! Excludes: which representation is a device's head or child (`ecad_electrical::symbols::reps`), and a
//! channel point's role ([`crate::ChannelRole`]).

use crate::naming::words::Word;

crate::word_enum! {
    /// A symbol's part in its device: stored, set from the declaring symbol when it is placed.
    #[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Default)]
    pub enum PartRole {
        /// The device's exclusive symbol: coil, actuator, breaker body, PLC card body.
        Head,
        /// A contact: NO, NC, change-over, the power pole sets.
        Contact,
        /// A further pole or part of a multi-part device.
        Pole,
        /// A strip position or level; a device's terminal.
        TerminalBlock,
        /// A PLC channel cell.
        Channel,
        /// A whole connector half (socket or plug).
        Half,
        /// A connector pin (socket pin, plug pin, pin pair).
        Pin,
        /// Anything else: a tagless symbol, no starter.
        #[default]
        Other,
    }
}

impl PartRole {
    /// Whether this is [`PartRole::Other`] (left out of a definition's file form).
    pub fn is_other(&self) -> bool {
        *self == PartRole::Other
    }

    /// Whether a representation of this part hangs under its device's head.
    pub fn is_child(self) -> bool {
        matches!(self, PartRole::Contact | PartRole::Pole | PartRole::Channel)
    }

    /// The numbering rank: head 0, everything else 1. A device is numbered
    /// by its head's position, else by its first symbol in drawing order.
    pub fn numbering_rank(self) -> u8 {
        match self {
            PartRole::Head => 0,
            PartRole::Contact | PartRole::Pole | PartRole::TerminalBlock | PartRole::Channel | PartRole::Half | PartRole::Pin | PartRole::Other => 1,
        }
    }
}

/// What a representation is for its subject.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Role {
    /// A device's symbol: its part in the device.
    Part(PartRole),
    /// A wire's drawn route (`route`).
    Route,
    /// A cable's slash across its conductors (`slash`).
    Slash,
    /// A cable's reference end (`reference`).
    Reference,
    /// A location box (`box`).
    LocationBox,
    /// A cable's hidden fan anchor (`fan-anchor`).
    FanAnchor,
    /// A terminal level's fuse (`terminalblockfuse`).
    FuseTerm,
    /// A converted drawing (`drawn`).
    Drawn,
}

impl Default for Role {
    fn default() -> Self {
        Role::Part(PartRole::Other)
    }
}

impl Role {
    /// The part, for a device's symbol.
    pub fn part(self) -> Option<PartRole> {
        match self {
            Role::Part(p) => Some(p),
            Role::Route | Role::Slash | Role::Reference | Role::LocationBox | Role::FanAnchor | Role::FuseTerm | Role::Drawn => None,
        }
    }
}

impl Word for Role {
    const ALL: &'static [Self] = &[
        Role::Part(PartRole::Head),
        Role::Part(PartRole::Contact),
        Role::Part(PartRole::Pole),
        Role::Part(PartRole::TerminalBlock),
        Role::Part(PartRole::Channel),
        Role::Part(PartRole::Half),
        Role::Part(PartRole::Pin),
        Role::Part(PartRole::Other),
        Role::Route,
        Role::Slash,
        Role::Reference,
        Role::LocationBox,
        Role::FanAnchor,
        Role::FuseTerm,
        Role::Drawn,
    ];
    const WORDS: &'static [&'static str] = &[
        "head",
        "contact",
        "pole",
        "terminal_block",
        "channel",
        "half",
        "pin",
        "other",
        "route",
        "slash",
        "reference",
        "box",
        "fan-anchor",
        "terminalblockfuse",
        "drawn",
    ];

    fn word(self) -> &'static str {
        match self {
            Role::Part(p) => p.word(),
            Role::Route => "route",
            Role::Slash => "slash",
            Role::Reference => "reference",
            Role::LocationBox => "box",
            Role::FanAnchor => "fan-anchor",
            Role::FuseTerm => "terminalblockfuse",
            Role::Drawn => "drawn",
        }
    }

    fn parse(text: &str) -> Option<Self> {
        let t = text.trim();
        Self::ALL.iter().copied().find(|r| r.word().eq_ignore_ascii_case(t))
    }
}

impl std::fmt::Display for Role {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.word())
    }
}

impl serde::Serialize for Role {
    fn serialize<S: serde::Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        s.serialize_str(self.word())
    }
}

impl<'de> serde::Deserialize<'de> for Role {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        let text = String::deserialize(d)?;
        Role::parse(&text)
            .ok_or_else(|| <D::Error as serde::de::Error>::custom(crate::naming::words::unknown_word_message(&text, "Role", Role::WORDS)))
    }
}

crate::__word_schema!(Role);

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_role_reads_back_its_own_word() {
        assert_eq!(Role::ALL.len(), Role::WORDS.len());
        for (r, w) in Role::ALL.iter().zip(Role::WORDS) {
            assert_eq!(r.word(), *w);
            assert_eq!(Role::parse(w), Some(*r));
        }
        assert_eq!(Role::parse("coil"), None);
        assert_eq!(Role::parse("main"), None);
    }
}
