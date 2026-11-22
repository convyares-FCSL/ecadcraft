//! The contacts a part offers and where its contact table goes (design 072 §2.2, design 025 FP1
//! and FP2): typed fields of the groups that physically have contacts.
//!
//! # Semantics
//! - A [`ContactSet`] is the offer as its person wrote it, one [`ContactEntry`] per entry: a count
//!   of poles (`3P`, `1NO`) or one pole with its own labels (`13/14 NO`). An empty set offers
//!   nothing. [`fmt::Display`] writes it in the offer grammar (`3P+1NO+13/14 NC`).
//! - Switching ([`crate::objects::devices::Switching`]), protection with auxiliaries ([`crate::objects::devices::Breaker`],
//!   [`crate::objects::devices::Rcd`], [`crate::objects::devices::Rcbo`]) and sensors ([`crate::objects::devices::Sensor`]) carry one.
//! - A [`ContactTable`] says where a switching device's contact table goes; none takes the
//!   project's default.
//!
//! This module excludes reading the typed offer text and numbering the poles it offers
//! (`ecad-electrical`'s `reference`), and drawing the table.
#![deny(clippy::wildcard_enum_match_arm)]

use std::fmt;
use std::num::NonZeroU8;

use serde::{Deserialize, Serialize};

/// Most poles an offer lists.
pub const MAX_POLES: usize = 32;
/// Most characters of one explicit pole label.
pub const MAX_LABEL_CHARS: usize = 16;
/// Most millimetres a moved table is from its coil, in each direction.
pub const MAX_OFFSET_MM: f64 = 2_000.0;

/// One label of an explicit pole (`13`, `L1`).
pub type ContactLabel = crate::naming::text::Text<MAX_LABEL_CHARS>;

crate::word_enum! {
    /// Normally open or normally closed.
    #[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
    pub enum PoleKind {
        No,
        Nc,
    }
}

impl PoleKind {
    /// `NO` or `NC`.
    pub fn upper(self) -> &'static str {
        match self {
            PoleKind::No => "NO",
            PoleKind::Nc => "NC",
        }
    }
}

/// The poles of one entry: a count, numbered when read, or one pole with its two labels.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ContactPoles {
    Count(NonZeroU8),
    Labels([ContactLabel; 2]),
}

/// One entry of an offer: `3P`, `1NO`, `13/14 NC`, `1/2 PNC`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ContactEntry {
    pub poles: ContactPoles,
    pub kind: PoleKind,
    /// Power (main) poles rather than auxiliary contacts.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub power: bool,
}

impl ContactEntry {
    /// How many poles it offers.
    pub fn len(&self) -> usize {
        match &self.poles {
            ContactPoles::Count(n) => usize::from(n.get()),
            ContactPoles::Labels(_) => 1,
        }
    }

    /// Whether it offers no pole (never: a count is at least one).
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    fn suffix(&self) -> &'static str {
        match (self.power, self.kind) {
            (true, PoleKind::No) => "P",
            (true, PoleKind::Nc) => "PNC",
            (false, PoleKind::No) => "NO",
            (false, PoleKind::Nc) => "NC",
        }
    }
}

impl fmt::Display for ContactEntry {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match &self.poles {
            ContactPoles::Count(n) => write!(f, "{n}{}", self.suffix()),
            ContactPoles::Labels([a, b]) => write!(f, "{a}/{b} {}", self.suffix()),
        }
    }
}

/// The contacts a part offers (design 025 FP1), in written order; empty offers nothing.
///
/// # Example
/// ```
/// use ecad_model::objects::devices::{ContactEntry, ContactPoles, ContactSet, PoleKind};
/// let n = |n| ContactPoles::Count(std::num::NonZeroU8::new(n).unwrap());
/// let s = ContactSet::new(vec![
///     ContactEntry { poles: n(3), kind: PoleKind::No, power: true },
///     ContactEntry { poles: n(1), kind: PoleKind::Nc, power: false },
/// ]);
/// assert_eq!(s.to_string(), "3P+1NC");
/// assert_eq!(s.poles(), 4);
/// ```
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(transparent)]
pub struct ContactSet(Vec<ContactEntry>);

impl ContactSet {
    /// The set of `entries`, in written order.
    pub fn new(entries: Vec<ContactEntry>) -> ContactSet {
        ContactSet(entries)
    }

    /// Its entries, in written order.
    pub fn entries(&self) -> &[ContactEntry] {
        &self.0
    }

    /// Whether it offers nothing.
    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    /// How many poles it offers.
    pub fn poles(&self) -> usize {
        self.0.iter().map(ContactEntry::len).sum()
    }

    /// Why it is not well formed, if it is not: more than [`MAX_POLES`] poles, or a label that is
    /// not letters, digits and dots.
    pub fn problem(&self) -> Option<String> {
        if self.poles() > MAX_POLES {
            return Some(format!("a part offers at most {MAX_POLES} contacts"));
        }
        let ok = |l: &ContactLabel| !l.as_str().is_empty() && l.as_str().chars().all(|c| c.is_ascii_alphanumeric() || c == '.');
        self.0
            .iter()
            .find(|e| match &e.poles {
                ContactPoles::Labels(ls) => !ls.iter().all(ok),
                ContactPoles::Count(_) => false,
            })
            .map(|e| format!("\"{e}\" is not a contact"))
    }
}

impl fmt::Display for ContactSet {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        for (i, e) in self.0.iter().enumerate() {
            if i > 0 {
                f.write_str("+")?;
            }
            write!(f, "{e}")?;
        }
        Ok(())
    }
}

/// Where a switching device's contact table goes (design 025 FP2).
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ContactTable {
    Auto,
    Below,
    Right,
    Off,
    /// Millimetres from the head symbol's insertion point (Move Mirror Table).
    At {
        dx: f64,
        dy: f64,
    },
}

impl ContactTable {
    /// Why it is not well formed, if it is not: an offset not finite or beyond [`MAX_OFFSET_MM`].
    pub fn problem(&self) -> Option<String> {
        match *self {
            ContactTable::At { dx, dy } if !(dx.is_finite() && dy.is_finite() && dx.abs() <= MAX_OFFSET_MM && dy.abs() <= MAX_OFFSET_MM) => {
                Some(format!("a contact table is at most {MAX_OFFSET_MM} mm from its coil"))
            }
            ContactTable::At { .. } | ContactTable::Auto | ContactTable::Below | ContactTable::Right | ContactTable::Off => None,
        }
    }
}
