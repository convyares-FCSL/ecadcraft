//! Revisions and versions as data (S4, #37): the typed [`Rev`], the project's [`RevScheme`] that
//! says which revs exist and what comes next, and [`PointName`], the key of a named point.
//!
//! # Semantics
//! - A [`Rev`] is a position in a scheme: the first issue (`–`), the nth letter, or the nth number.
//!   It carries no text; [`RevScheme::show`] spells it (`B`, `P03`) and [`RevScheme::parse`] reads
//!   typed text back, refusing what the scheme does not have and saying why.
//! - [`RevScheme::next`] is the only place the successor of a rev is worked out.
//! - A named point is a [`PointName`]: a user's version (`B`) or an issued revision (`B`), told
//!   apart by the variant, never by spelling. The two kinds never share a name.
//! - Order and equality compare positions within one scheme. There is no catch-all variant and no
//!   order across schemes: a project's scheme cannot change once anything is issued.
//!
//! This module excludes where revisions are stored ([`crate::Project`]) and the commands.

use serde::{Deserialize, Serialize};

use crate::naming::text::Text;

/// Most characters of a version's name.
pub const MAX_VERSION_NAME: usize = 100;
/// Most characters of a numbered scheme's prefix.
pub(crate) const MAX_PREFIX: usize = 4;
/// Most letters of a letter rev (`ZZZ`).
const MAX_LETTERS: usize = 3;
/// The en dash that spells the first issue.
pub const DASH: &str = "\u{2013}";

/// A user's version name.
pub type VersionName = Text<MAX_VERSION_NAME>;
/// The prefix of a numbered scheme (`P` in `P01`).
pub type RevPrefix = Text<MAX_PREFIX>;

/// A rev: a position in the project's [`RevScheme`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Rev {
    /// The first issue, before `A` (`–`).
    Initial,
    /// The nth letter, from 1: `A`, …, `Z`, `AA`.
    Letter(u32),
    /// The nth number of a numbered scheme.
    Number(u32),
}

/// The revs a project has and what follows each.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", deny_unknown_fields)]
pub enum RevScheme {
    /// `A`, `B`, … `Z`, `AA`; with `dash_first` the first issue is `–`.
    Letters { dash_first: bool },
    /// `P01`, `P02`, …: the prefix, digits at least `width`, and the first number.
    Numbered {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        prefix: Option<RevPrefix>,
        width: u8,
        start: u32,
    },
}

impl Default for RevScheme {
    /// `A`, `B`, … with no dash: the first issue is `A`.
    fn default() -> Self {
        RevScheme::Letters { dash_first: false }
    }
}

/// Why text is not a rev of the scheme.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum RevError {
    #[error("Rev is empty. Use {example}.")]
    Empty { example: String },
    #[error("\"{text}\" is not a rev of this project. Revs run {example}.")]
    NotInScheme { text: String, example: String },
    #[error("This project's revs start at {first}, not \u{2013}.")]
    NoDash { first: String },
    #[error("The scheme cannot change once a revision is issued.")]
    SchemeLocked,
}

impl RevScheme {
    /// Whether this is the default scheme (what records leave out).
    pub(crate) fn is_default(&self) -> bool {
        *self == RevScheme::default()
    }
}

impl Rev {
    /// The row key of a revision point (stable, scheme-free): `initial`, `letter:3`, `number:1`.
    pub fn key(&self) -> String {
        match self {
            Rev::Initial => "initial".into(),
            Rev::Letter(n) => format!("letter:{n}"),
            Rev::Number(n) => format!("number:{n}"),
        }
    }

    /// The rev a [`Rev::key`] names.
    pub fn from_key(key: &str) -> Option<Rev> {
        match key.split_once(':') {
            None => (key == "initial").then_some(Rev::Initial),
            Some(("letter", n)) => n.parse().ok().map(Rev::Letter),
            Some(("number", n)) => n.parse().ok().map(Rev::Number),
            Some(_) => None,
        }
    }
}

fn letters_of(mut n: u32) -> String {
    let mut out = Vec::new();
    while n > 0 {
        let r = u8::try_from((n - 1) % 26).unwrap_or(0);
        out.push(char::from(b'A' + r));
        n = (n - 1) / 26;
    }
    out.iter().rev().collect()
}

fn letters_value(text: &str) -> Option<u32> {
    if text.is_empty() || text.len() > MAX_LETTERS || !text.bytes().all(|b| b.is_ascii_uppercase()) {
        return None;
    }
    Some(text.bytes().fold(0u32, |acc, b| acc * 26 + u32::from(b - b'A') + 1))
}

impl RevScheme {
    /// The first rev of the scheme.
    pub fn first(&self) -> Rev {
        match self {
            RevScheme::Letters { dash_first: true } => Rev::Initial,
            RevScheme::Letters { dash_first: false } => Rev::Letter(1),
            RevScheme::Numbered { start, .. } => Rev::Number(*start),
        }
    }

    /// The rev after `last` (the first when nothing is issued).
    pub fn next(&self, last: Option<Rev>) -> Rev {
        match (self, last) {
            (_, None) => self.first(),
            (RevScheme::Letters { .. }, Some(Rev::Initial)) => Rev::Letter(1),
            (RevScheme::Letters { .. }, Some(Rev::Letter(n))) => Rev::Letter(n.saturating_add(1)),
            (RevScheme::Numbered { .. }, Some(Rev::Number(n))) => Rev::Number(n.saturating_add(1)),
            // A rev of another scheme cannot be in a project: `Project::problem` says so.
            (_, Some(_)) => self.first(),
        }
    }

    /// Whether `rev` is a rev of this scheme.
    pub fn admits(&self, rev: Rev) -> bool {
        match (self, rev) {
            (RevScheme::Letters { dash_first }, Rev::Initial) => *dash_first,
            (RevScheme::Letters { .. }, Rev::Letter(n)) => n >= 1 && letters_of(n).len() <= MAX_LETTERS,
            (RevScheme::Numbered { start, .. }, Rev::Number(n)) => n >= *start,
            (_, _) => false,
        }
    }

    /// How `rev` is written (`–`, `B`, `P03`).
    pub fn show(&self, rev: Rev) -> String {
        match (self, rev) {
            (_, Rev::Initial) => DASH.into(),
            (_, Rev::Letter(n)) => letters_of(n),
            (RevScheme::Numbered { prefix, width, .. }, Rev::Number(n)) => {
                format!("{}{n:0w$}", prefix.as_ref().map_or("", |p| p.as_str()), w = usize::from(*width))
            }
            (RevScheme::Letters { .. }, Rev::Number(n)) => n.to_string(),
        }
    }

    /// What the scheme's revs look like, for messages (`–, A, B, …`).
    pub fn example(&self) -> String {
        let first = self.first();
        let second = self.next(Some(first));
        let third = self.next(Some(second));
        format!("{}, {}, {}, \u{2026}", self.show(first), self.show(second), self.show(third))
    }

    /// The rev `text` spells in this scheme.
    ///
    /// # Errors
    /// [`RevError`] says what is wrong: empty, not of the scheme, or a dash where the scheme has none.
    pub fn parse(&self, text: &str) -> Result<Rev, RevError> {
        let t = text.trim();
        if t.is_empty() {
            return Err(RevError::Empty { example: self.example() });
        }
        let not = || RevError::NotInScheme { text: t.to_string(), example: self.example() };
        match self {
            RevScheme::Letters { dash_first } => {
                if t == DASH || t == "-" {
                    return if *dash_first { Ok(Rev::Initial) } else { Err(RevError::NoDash { first: "A".into() }) };
                }
                letters_value(t).map(Rev::Letter).ok_or_else(not)
            }
            RevScheme::Numbered { prefix, start, .. } => {
                let digits = t.strip_prefix(prefix.as_ref().map_or("", |p| p.as_str())).ok_or_else(not)?;
                let n: u32 = digits.parse().map_err(|_| not())?;
                let rev = Rev::Number(n);
                // Spelled exactly as the scheme writes it (`P03`, not `P3`), and not before the start.
                (n >= *start && self.show(rev) == t).then_some(rev).ok_or_else(not)
            }
        }
    }
}

/// Which kind a named point is.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PointKind {
    /// A name the user gave.
    Version,
    /// An issued revision.
    Revision,
}

/// The name of a named point: versions and revisions are keyed by kind and name.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PointName {
    Version(VersionName),
    Revision(Rev),
}

impl PointName {
    /// The version `name`.
    ///
    /// # Errors
    /// Why the text is not a version name.
    pub fn version(name: &str) -> Result<PointName, crate::naming::text::TextError> {
        VersionName::new(name).map(PointName::Version)
    }

    /// Which kind it is.
    pub fn kind(&self) -> PointKind {
        match self {
            PointName::Version(_) => PointKind::Version,
            PointName::Revision(_) => PointKind::Revision,
        }
    }

    /// The row key within its kind: the version's text, or [`Rev::key`].
    pub fn key(&self) -> String {
        match self {
            PointName::Version(v) => v.as_str().to_string(),
            PointName::Revision(r) => r.key(),
        }
    }

    /// The point a row of `kind` with key `key` names.
    pub fn from_row(kind: PointKind, key: &str) -> Option<PointName> {
        match kind {
            PointKind::Version => VersionName::new(key).ok().map(PointName::Version),
            PointKind::Revision => Rev::from_key(key).map(PointName::Revision),
        }
    }

    /// How it is shown: a version's text, or `Rev B` as `scheme` writes it.
    pub fn label(&self, scheme: &RevScheme) -> String {
        match self {
            PointName::Version(v) => v.to_string(),
            PointName::Revision(r) => format!("Rev {}", scheme.show(*r)),
        }
    }

    /// The revision, if it is one.
    pub fn rev(&self) -> Option<Rev> {
        match self {
            PointName::Revision(r) => Some(*r),
            PointName::Version(_) => None,
        }
    }
}

/// The named points of a document as a list of `{point, at}` entries (JSON keys cannot be typed
/// points): the form in a snapshot of a [`crate::Document`] and in the project chunk.
pub mod as_points {
    use std::collections::BTreeMap;

    use serde::{Deserialize, Deserializer, Serialize, Serializer};

    use super::PointName;

    #[derive(Serialize, Deserialize)]
    #[serde(deny_unknown_fields)]
    struct PointAt {
        point: PointName,
        at: u64,
    }

    /// Write the points as a list.
    ///
    /// # Errors
    /// The serializer's.
    pub fn serialize<S: Serializer>(points: &BTreeMap<PointName, u64>, s: S) -> Result<S::Ok, S::Error> {
        s.collect_seq(points.iter().map(|(point, at)| PointAt { point: point.clone(), at: *at }))
    }

    /// Read the points from a list; a point named twice is refused.
    ///
    /// # Errors
    /// The deserializer's, or a point named twice.
    pub fn deserialize<'de, D: Deserializer<'de>>(d: D) -> Result<BTreeMap<PointName, u64>, D::Error> {
        let mut out = BTreeMap::new();
        for p in Vec::<PointAt>::deserialize(d)? {
            if out.insert(p.point.clone(), p.at).is_some() {
                return Err(serde::de::Error::custom(format!("point {:?} is named twice", p.point)));
            }
        }
        Ok(out)
    }
}
