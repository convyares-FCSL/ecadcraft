//! Project-defined user fields (design 072 §2.8) and typed links between devices (§2.6).
//!
//! # Semantics
//! - The project lists its user fields ([`UserFieldDef`]: an id, a name unique ignoring case, and a
//!   kind). A device stores its values by field id ([`crate::Device::user`]); a value of the wrong
//!   kind, a choice not offered, or an id the project does not list is an invariant breach, so a
//!   file holding one does not open.
//! - A device link ([`DeviceLink`]) joins two different devices with a kind (a mechanical
//!   interlock, an auxiliary block). It lives at project level ([`crate::Document::device_links`])
//!   and changes only through [`crate::Op::AddDeviceLink`] and [`crate::Op::RemoveDeviceLink`]; a
//!   device with links cannot be deleted until they are removed.
//!
//! This module excludes the commands and how a value is shown in reports.
#![deny(clippy::wildcard_enum_match_arm)]

use serde::{Deserialize, Serialize};

use crate::naming::id::{DeviceId, UserFieldId};
use crate::naming::text::Text;

/// Most characters of a user field's name.
pub(crate) const MAX_FIELD_NAME: usize = 32;
/// Most characters of a number field's unit.
pub(crate) const MAX_UNIT: usize = 8;
/// Most characters of one choice.
pub(crate) const MAX_CHOICE: usize = 32;
/// Most choices one field offers.
pub const MAX_CHOICES: usize = 32;
/// Most characters of a text value.
pub(crate) const MAX_USER_TEXT: usize = 256;
/// Most user fields one project defines.
pub(crate) const MAX_USER_FIELDS: usize = 64;

/// A user field's name (`Panel row`).
pub type FieldName = Text<MAX_FIELD_NAME>;
/// A number field's unit (`mm`).
pub type FieldUnit = Text<MAX_UNIT>;
/// One choice of a choice field.
pub type Choice = Text<MAX_CHOICE>;
/// A text value.
pub(crate) type UserText = Text<MAX_USER_TEXT>;

/// What a user field holds.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", deny_unknown_fields)]
pub enum UserFieldKind {
    /// Free text.
    Text,
    /// A number, with an optional unit shown after it.
    Number {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        unit: Option<FieldUnit>,
    },
    /// Yes or no.
    Flag,
    /// One of a fixed list of choices.
    Choice { options: Vec<Choice> },
}

impl UserFieldKind {
    /// The kind's word (`text`, `number`, `flag`, `choice`).
    pub fn word(&self) -> &'static str {
        match self {
            UserFieldKind::Text => "text",
            UserFieldKind::Number { .. } => "number",
            UserFieldKind::Flag => "flag",
            UserFieldKind::Choice { .. } => "choice",
        }
    }
}

/// One project-defined field (design 072 §2.8).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct UserFieldDef {
    pub id: UserFieldId,
    pub name: FieldName,
    pub kind: UserFieldKind,
}

/// Why text does not read as a user field's value.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum UserValueError {
    #[error("{0}")]
    Text(crate::naming::text::TextError),
    #[error("\"{0}\" is not a number")]
    NotANumber(String),
    #[error("\"{0}\" is not yes or no")]
    NotAFlag(String),
    #[error("\"{text}\" is not one of {choices}")]
    NotAChoice { text: String, choices: String },
}

/// A device's value of one user field.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", deny_unknown_fields)]
pub enum UserValue {
    Text(UserText),
    Number(f64),
    Flag(bool),
    Choice(Choice),
}

impl UserValue {
    /// Whether the value is of `kind`: a finite number, a choice the field offers.
    pub fn fits(&self, kind: &UserFieldKind) -> bool {
        match (self, kind) {
            (UserValue::Text(_), UserFieldKind::Text) | (UserValue::Flag(_), UserFieldKind::Flag) => true,
            (UserValue::Number(n), UserFieldKind::Number { .. }) => n.is_finite(),
            (UserValue::Choice(c), UserFieldKind::Choice { options }) => options.contains(c),
            (UserValue::Text(_) | UserValue::Number(_) | UserValue::Flag(_) | UserValue::Choice(_), _) => false,
        }
    }

    /// The value read from `text` as a value of `kind`.
    ///
    /// # Errors
    /// Text too long or with hidden characters, a number that does not read, a flag that is not
    /// yes or no, a choice the field does not offer ([`UserValueError`]).
    pub fn parse(kind: &UserFieldKind, text: &str) -> Result<UserValue, UserValueError> {
        let t = text.trim();
        match kind {
            UserFieldKind::Text => UserText::new(t).map(UserValue::Text).map_err(UserValueError::Text),
            UserFieldKind::Number { unit } => {
                let bare = unit.as_ref().and_then(|u| t.strip_suffix(u.as_str())).unwrap_or(t).trim();
                match bare.parse::<f64>() {
                    Ok(n) if n.is_finite() => Ok(UserValue::Number(n)),
                    _ => Err(UserValueError::NotANumber(t.to_string())),
                }
            }
            UserFieldKind::Flag => crate::naming::words::flag(t).map(UserValue::Flag).ok_or_else(|| UserValueError::NotAFlag(t.to_string())),
            UserFieldKind::Choice { options } => {
                options.iter().find(|o| o.as_str().eq_ignore_ascii_case(t)).map(|o| UserValue::Choice(o.clone())).ok_or_else(|| {
                    let list: Vec<&str> = options.iter().map(|o| o.as_str()).collect();
                    UserValueError::NotAChoice { text: t.to_string(), choices: list.join(", ") }
                })
            }
        }
    }

    /// The value as shown (`12.5 mm`, `yes`, `Left`).
    pub fn show(&self, kind: &UserFieldKind) -> String {
        match self {
            UserValue::Text(t) => t.as_str().to_string(),
            UserValue::Choice(c) => c.as_str().to_string(),
            UserValue::Flag(b) => crate::naming::words::flag_word(*b).to_string(),
            UserValue::Number(n) => match kind {
                UserFieldKind::Number { unit: Some(u) } => format!("{n} {}", u.as_str()),
                UserFieldKind::Number { unit: None } | UserFieldKind::Text | UserFieldKind::Flag | UserFieldKind::Choice { .. } => n.to_string(),
            },
        }
    }
}

/// Why the project's user fields are not well formed, if they are not: too many, an id or a name
/// (ignoring case) twice, a choice field with no choices, too many, or one twice.
pub fn fields_problem(fields: &[UserFieldDef]) -> Option<String> {
    if fields.len() > MAX_USER_FIELDS {
        return Some(format!("more than {MAX_USER_FIELDS} user fields"));
    }
    for (i, f) in fields.iter().enumerate() {
        let earlier = fields.get(..i).unwrap_or_default();
        if earlier.iter().any(|g| g.id == f.id) {
            return Some(format!("user field {} is listed twice", f.id));
        }
        if f.name.as_str().trim().is_empty() {
            return Some(format!("user field {} has no name", f.id));
        }
        if earlier.iter().any(|g| g.name.as_str().eq_ignore_ascii_case(f.name.as_str())) {
            return Some(format!("user field name \"{}\" is used twice", f.name.as_str()));
        }
        if let UserFieldKind::Choice { options } = &f.kind {
            if options.is_empty() || options.len() > MAX_CHOICES {
                return Some(format!("choice field \"{}\" offers {} choices (1 to {MAX_CHOICES})", f.name.as_str(), options.len()));
            }
            for (j, o) in options.iter().enumerate() {
                if options.get(..j).unwrap_or_default().iter().any(|p| p.as_str().eq_ignore_ascii_case(o.as_str())) {
                    return Some(format!("choice field \"{}\" offers \"{}\" twice", f.name.as_str(), o.as_str()));
                }
            }
        }
    }
    None
}

crate::word_enum! {
    /// What joins two devices (design 072 §2.6).
    #[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
    pub enum LinkKind {
        /// Two contactors that must not close together (a reversing pair).
        MechanicalInterlock,
        /// An auxiliary contact block fitted to a device.
        AuxiliaryBlock,
    }
}

/// A typed link between two different devices (design 072 §2.6).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DeviceLink {
    pub kind: LinkKind,
    pub a: DeviceId,
    pub b: DeviceId,
}

impl DeviceLink {
    /// Whether the link names device `d`.
    pub fn names(&self, d: DeviceId) -> bool {
        self.a == d || self.b == d
    }

    /// Whether `other` joins the same two devices (in either order) with the same kind.
    pub fn same_as(&self, other: &DeviceLink) -> bool {
        self.kind == other.kind && ((self.a, self.b) == (other.a, other.b) || (self.a, self.b) == (other.b, other.a))
    }
}
