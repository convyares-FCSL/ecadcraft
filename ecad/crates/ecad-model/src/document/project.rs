//! The project's own data (design 072 §5.3): the location and function lists and the project's
//! default location and function.
//!
//! # Semantics
//! - The project holds one list of locations (`+CP1`) and one of functions (`=F1`). Each entry has
//!   a stable id, allocated when it is made and recorded in the op, a **code** unique (ignoring
//!   case) within its list and starting with its list's sign, an optional **tag** and any number
//!   of **descriptions** (description 1, description 2, …).
//! - Pages, the project and (lane B) devices refer to an entry by its id, so renaming a code is one
//!   edit of the list. An id with no entry (an entry a merge removed) shows `?`.
//! - The revision rows (design 019 §7.2) are [`Revision`]s in issue order, numbered from 1; every
//!   row but the last is issued, and the last is the pending one while it is not. The revision
//!   table's columns and marker stay registry values (they have a standard level).
//! - The whole project changes through one op, [`crate::Op::ModifyProject`].
//!
//! This module excludes what a code means for a device (ecad-electrical's `locations` and
//! `designations`) and the commands.
#![deny(clippy::wildcard_enum_match_arm)]

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::Word;
use crate::document::rev::{Rev, RevScheme};
use crate::naming::id::{FunctionId, LocationId};
use crate::naming::text::Text;

/// Most characters of a list code (with its sign) and of a tag.
pub const MAX_CODE: usize = 16;
/// Most characters of one description of a list entry.
pub const MAX_LIST_DESCRIPTION: usize = 128;
/// Most descriptions one entry holds.
pub const MAX_DESCRIPTIONS: usize = 8;

/// A list code (`+CP1`, `=F1`) or tag.
pub type ListCode = Text<MAX_CODE>;
/// One description of a list entry.
pub type ListDescription = Text<MAX_LIST_DESCRIPTION>;

/// One entry of the location or function list.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ListEntry<I> {
    pub id: I,
    pub code: ListCode,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tag: Option<ListCode>,
    /// Description 1, description 2, … in order.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub descriptions: Vec<ListDescription>,
}

impl<I> ListEntry<I> {
    /// An entry with `code` and nothing else.
    pub fn new(id: I, code: ListCode) -> Self {
        ListEntry { id, code, tag: None, descriptions: Vec::new() }
    }

    /// Description 1, if any.
    pub fn description(&self) -> Option<&ListDescription> {
        self.descriptions.first()
    }
}

/// A location (`+CP1`).
pub type Location = ListEntry<LocationId>;
/// A function (`=F1`).
pub type Function = ListEntry<FunctionId>;

/// The project's own data (design 072 §5.3). Every field is left out of records while empty.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Project {
    /// What the project is: title, drawing number, customer, site, status, the cover's notes.
    #[serde(default, skip_serializing_if = "ProjectInfo::is_empty")]
    pub info: ProjectInfo,
    /// The location list, in the order entries were made.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub locations: Vec<Location>,
    /// The function list, in the order entries were made.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub functions: Vec<Function>,
    /// The location of every page that sets none.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub location: Option<LocationId>,
    /// The function of every page that sets none.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub function: Option<FunctionId>,
    /// The revision rows, in issue order (row `n` is at `n - 1`).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub revisions: Vec<Revision>,
    /// The project-defined user fields (design 072 §2.8), in the order they were made.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub user_fields: Vec<crate::objects::user::UserFieldDef>,
    /// Which revs the project has and what follows each (S4, #37). Fixed once anything is issued.
    #[serde(default, skip_serializing_if = "RevScheme::is_default")]
    pub rev_scheme: RevScheme,
}

/// The project-property key a `keep_project_key` filter (Restore, merge) is asked about for the
/// revision rows: rows are history, kept as they are now unless the filter accepts this key.
pub const REVISIONS_KEY: &str = "rev.";

impl Project {
    /// Whether nothing is set.
    pub fn is_empty(&self) -> bool {
        *self == Project::default()
    }

    /// The location `id`, if the list has it.
    pub fn location(&self, id: LocationId) -> Option<&Location> {
        self.locations.iter().find(|e| e.id == id)
    }

    /// The function `id`, if the list has it.
    pub fn function(&self, id: FunctionId) -> Option<&Function> {
        self.functions.iter().find(|e| e.id == id)
    }

    /// Why the project is not well formed, if it is not: an id twice in a list, a code twice
    /// (ignoring case), a code without its list's sign (`+`, `=`), or too many descriptions.
    pub fn problem(&self) -> Option<String> {
        list_problem(&self.locations, '+', "location")
            .or_else(|| list_problem(&self.functions, '=', "function"))
            .or_else(|| revisions_problem(&self.revisions, &self.rev_scheme))
            .or_else(|| crate::objects::user::fields_problem(&self.user_fields))
    }

    /// The user field `id`, if the project defines it.
    pub fn user_field(&self, id: crate::naming::id::UserFieldId) -> Option<&crate::objects::user::UserFieldDef> {
        self.user_fields.iter().find(|f| f.id == id)
    }
}

fn revisions_problem(rows: &[Revision], scheme: &RevScheme) -> Option<String> {
    if rows.len() > MAX_REVISIONS {
        return Some(format!("more than {MAX_REVISIONS} revisions"));
    }
    let mut seen = std::collections::BTreeSet::new();
    for (i, r) in rows.iter().enumerate() {
        if r.issued && r.rev.is_none() {
            return Some(format!("revision {} is issued without a rev", i + 1));
        }
        if let Some(rev) = r.rev {
            if !scheme.admits(rev) {
                return Some(format!("revision {} has rev {}, which the scheme does not have", i + 1, scheme.show(rev)));
            }
            if !seen.insert(rev) {
                return Some(format!("rev {} is on two rows", scheme.show(rev)));
            }
        }
    }
    let last = rows.len().saturating_sub(1);
    rows.iter().take(last).position(|r| !r.issued).map(|i| format!("revision {} is not issued but a later one exists", i + 1))
}

fn list_problem<I: Copy + Ord + std::fmt::Display>(list: &[ListEntry<I>], sign: char, what: &str) -> Option<String> {
    let mut ids = std::collections::BTreeSet::new();
    let mut codes = std::collections::BTreeSet::new();
    for e in list {
        if !ids.insert(e.id) {
            return Some(format!("{what} {} is listed twice", e.id));
        }
        if !codes.insert(e.code.as_str().to_uppercase()) {
            return Some(format!("{what} code {} is listed twice", e.code));
        }
        if !e.code.as_str().starts_with(sign) || e.code.as_str().len() == sign.len_utf8() {
            return Some(format!("{what} code {} does not start with {sign} and a name", e.code));
        }
        if e.descriptions.len() > MAX_DESCRIPTIONS {
            return Some(format!("{what} {} has more than {MAX_DESCRIPTIONS} descriptions", e.code));
        }
    }
    None
}

// ---------------------------------------------------------------- revisions (design 072 §5.3)

/// Most characters of a revision field's value.
pub const MAX_REV_VALUE: usize = 200;
/// Most characters of a custom revision field's name.
pub const MAX_REV_FIELD: usize = 20;
/// Most characters of a release folder as recorded.
pub const MAX_RELEASED: usize = 1000;
/// Most revisions a project holds (a bound on hostile files).
pub const MAX_REVISIONS: usize = 10_000;

/// One revision field's value.
pub type RevValue = Text<MAX_REV_VALUE>;

crate::word_enum! {
    /// A built-in revision field, in table order (design 019 §7.2).
    #[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
    pub enum RevField {
        Rev,
        Date,
        Description,
        Drawn,
        Checked,
        Approved,
        Ecr,
    }
}

/// The name of a custom revision field (`ecr`): 1 to 20 of `a-z 0-9 _`, not a built-in field and
/// not `issued` or `released`.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize)]
#[serde(transparent)]
pub struct CustomField(Text<MAX_REV_FIELD>);

impl CustomField {
    /// `name`, if it may name a custom field.
    pub fn new(name: &str) -> Option<CustomField> {
        let ok = (1..=MAX_REV_FIELD).contains(&name.len())
            && name.bytes().all(|b| matches!(b, b'a'..=b'z' | b'0'..=b'9' | b'_'))
            && RevField::parse(name).is_none()
            && name != "issued"
            && name != "released";
        ok.then(|| Text::new(name).ok().map(CustomField)).flatten()
    }

    /// The name.
    pub fn as_str(&self) -> &str {
        self.0.as_str()
    }
}

impl<'de> Deserialize<'de> for CustomField {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        let text = String::deserialize(d)?;
        CustomField::new(&text).ok_or_else(|| serde::de::Error::custom(format!("\"{text}\" is not a custom revision field")))
    }
}

/// One revision row (design 019 §7.2, 033 §7.2; typed in S4). Rows are numbered by their place in
/// [`Project::revisions`], from 1; the last row is the pending one while it is not issued.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Revision {
    /// Its rev in the project's scheme.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub rev: Option<Rev>,
    /// The date, written `YYYY-MM-DD` in records.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub date: Option<chrono::NaiveDate>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<RevValue>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub drawn: Option<RevValue>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub checked: Option<RevValue>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub approved: Option<RevValue>,
    /// The engineering change request it answers.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ecr: Option<RevValue>,
    /// The project's own extra columns (brief 033), by name.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub custom: BTreeMap<CustomField, RevValue>,
    /// Whether it was issued; the issue is the revision point `Revision(rev)` in the document's versions.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub issued: bool,
    /// The release folder, when a release issued it (brief 027), relative to the project file's
    /// folder.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub released: Option<Text<MAX_RELEASED>>,
}

/// Text that is not a date: only `YYYY-MM-DD` is one.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("\"{0}\" is not a date. Write it as YYYY-MM-DD, such as 2026-11-22.")]
pub struct NotADate(pub String);

/// The date `text` spells.
///
/// # Errors
/// [`NotADate`] says what is wrong with it: only `YYYY-MM-DD` is a date.
pub fn parse_date(text: &str) -> Result<chrono::NaiveDate, NotADate> {
    let t = text.trim();
    chrono::NaiveDate::parse_from_str(t, "%Y-%m-%d").ok().filter(|d| d.format("%Y-%m-%d").to_string() == t).ok_or_else(|| NotADate(t.to_string()))
}

impl Revision {
    /// The text of built-in field `f` (the rev as `scheme` writes it, the date as `YYYY-MM-DD`).
    pub fn text(&self, f: RevField, scheme: &RevScheme) -> Option<String> {
        let t = |v: &Option<RevValue>| v.as_ref().map(|t| t.as_str().to_string());
        match f {
            RevField::Rev => self.rev.map(|r| scheme.show(r)),
            RevField::Date => self.date.map(|d| d.format("%Y-%m-%d").to_string()),
            RevField::Description => t(&self.description),
            RevField::Drawn => t(&self.drawn),
            RevField::Checked => t(&self.checked),
            RevField::Approved => t(&self.approved),
            RevField::Ecr => t(&self.ecr),
        }
    }

    /// Set a text field (`None` clears it). `false` for `rev` and `date`, which are typed values
    /// set through their own fields.
    pub fn set_text(&mut self, f: RevField, v: Option<RevValue>) -> bool {
        match f {
            RevField::Rev | RevField::Date => return false,
            RevField::Description => self.description = v,
            RevField::Drawn => self.drawn = v,
            RevField::Checked => self.checked = v,
            RevField::Approved => self.approved = v,
            RevField::Ecr => self.ecr = v,
        }
        true
    }

    /// Whether nothing is set.
    pub fn is_empty(&self) -> bool {
        *self == Revision::default()
    }
}

// ---------------------------------------------------------------- project info (design 072 §5.3)

/// Most characters of a one-line project info field.
pub const MAX_INFO: usize = 200;
/// Most notes on the cover, and characters of one note.
pub const MAX_NOTES: usize = 40;
pub const MAX_NOTE_CHARS: usize = 400;

/// A one-line project info field.
pub type InfoText = Text<MAX_INFO>;
/// The cover's notes: one per line.
pub type Notes = crate::naming::text::LongText<{ MAX_NOTES * (MAX_NOTE_CHARS + 1) }>;

crate::word_enum! {
    /// A field of [`ProjectInfo`], by the project property it was (`title`, `cover.notes`).
    #[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
    pub enum InfoField {
        Title,
        #[strum(serialize = "drawing_number")]
        DrawingNumber,
        Customer,
        Site,
        Status,
        #[strum(serialize = "cover.notes")]
        Notes,
    }
}

/// What the project is (design 072 §5.3): what the title block and the cover show. Every field is
/// left out of records while unset.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ProjectInfo {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub title: Option<InfoText>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub drawing_number: Option<InfoText>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub customer: Option<InfoText>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub site: Option<InfoText>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub status: Option<InfoText>,
    /// The cover's notes, one per line.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub notes: Option<Notes>,
}

impl ProjectInfo {
    /// Whether nothing is set.
    pub fn is_empty(&self) -> bool {
        *self == ProjectInfo::default()
    }

    /// The value of field `f`.
    pub fn get(&self, f: InfoField) -> Option<&str> {
        match f {
            InfoField::Title => self.title.as_ref().map(Text::as_str),
            InfoField::DrawingNumber => self.drawing_number.as_ref().map(Text::as_str),
            InfoField::Customer => self.customer.as_ref().map(Text::as_str),
            InfoField::Site => self.site.as_ref().map(Text::as_str),
            InfoField::Status => self.status.as_ref().map(Text::as_str),
            InfoField::Notes => self.notes.as_ref().map(crate::naming::text::LongText::as_str),
        }
    }

    /// `self` with field `f` set to `value` (`None`, or blank, clears it).
    ///
    /// # Errors
    /// Why the text was refused.
    pub fn with(&self, f: InfoField, value: Option<&str>) -> Result<ProjectInfo, crate::naming::text::TextError> {
        let mut out = self.clone();
        let v = value.map(str::trim).filter(|v| !v.is_empty());
        let line = |v: Option<&str>| v.map(InfoText::new).transpose();
        match f {
            InfoField::Title => out.title = line(v)?,
            InfoField::DrawingNumber => out.drawing_number = line(v)?,
            InfoField::Customer => out.customer = line(v)?,
            InfoField::Site => out.site = line(v)?,
            InfoField::Status => out.status = line(v)?,
            InfoField::Notes => out.notes = v.map(Notes::new).transpose()?,
        }
        Ok(out)
    }
}
