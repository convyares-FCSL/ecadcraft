//! The rows of `setprop` for a device's keys that became
//! [`ecad_model::Device`] fields. Each builds one `ModifyDevice`.

use ecad_model::naming::text::TextError;
use ecad_model::{Device, DeviceId, Document, ObjectRef, Op, TextField};

use crate::error::CmdError;
use crate::properties::commands::{Build, NO_SUCH_OBJECT, PropRule};

/// This lane's rows, asked before [`crate::properties::commands::PROP_RULES`].
pub(crate) static RULES: &[PropRule] = &[
    PropRule {
        what: "user fields (072 B)",
        claims: |_, o, k| matches!(o, ObjectRef::Device(_)) && k.starts_with(crate::devices::commands::device_links::USER_PREFIX),
        build: Build::Record(crate::devices::commands::device_links::user_value),
    },
    PropRule {
        what: "device text fields (072 B)",
        claims: |_, o, k| matches!(o, ObjectRef::Device(_)) && TextField::of_key(k).is_some(),
        build: Build::Record(text_value),
    },
    PropRule {
        what: "connector fields (072 B)",
        claims: |_, o, k| matches!(o, ObjectRef::Device(_)) && crate::connectors::is_field_key(k),
        build: Build::Record(connector_value),
    },
    PropRule {
        what: "contacts (brief 025, 072 B)",
        claims: |_, o, k| matches!(o, ObjectRef::Device(_)) && k == crate::references::devices::parse::CONTACTS_KEY,
        build: Build::Record(contacts_value),
    },
    PropRule {
        what: "contact table (brief 025, 072 B)",
        claims: |_, o, k| matches!(o, ObjectRef::Device(_)) && k == crate::references::devices::parse::MIRROR_KEY,
        build: Build::Record(table_value),
    },
    PropRule {
        what: "device rating (072 B)",
        claims: |doc, o, k| match o {
            ObjectRef::Device(c) => {
                k == ecad_model::objects::devices::RATING_KEY
                    || (k.starts_with(ecad_model::objects::devices::RATING_OF_PREFIX)
                        && doc.devices().get(&c).is_some_and(|d| {
                            matches!(d.device_type, ecad_model::DeviceType::Protection(_) | ecad_model::DeviceType::TerminalStrip(_))
                        }))
            }
            _ => false,
        },
        build: Build::Record(rating_value),
    },
];

/// A device's rating (its typed fields, or its note), a fuse bank's fuse rating or a strip's fuse
/// position rating, read by its group's reader ([`crate::devices::ratings`]): one `ModifyDevice`.
///
/// # Errors
/// The reader's refusals: ambiguous text (what is missing), a value the group does not have.
pub(crate) fn rating_value(doc: &Document, o: ObjectRef, key: &str, value: Option<&str>) -> Result<Option<Op>, CmdError> {
    let (c, d) = device_of(doc, o)?;
    let who = crate::devices::tags::tag_words(doc, c);
    let after = match key.strip_prefix(ecad_model::objects::devices::RATING_OF_PREFIX) {
        Some(p) if matches!(d.device_type, ecad_model::DeviceType::TerminalStrip(_)) => {
            crate::devices::ratings::with_position_rating(d, &who, &crate::terminal_strips::terminal_blocks::fuse_positions(doc, c), p, value)
        }
        Some(n) => crate::devices::ratings::with_output_rating(d, &who, &crate::symbols::rules::bank_outputs(doc, c), n, value),
        None => crate::devices::ratings::with_rating(d, &who, value),
    }?;
    Ok((after != *d).then(|| Op::ModifyDevice { id: c, before: d.clone(), after }))
}

/// The contacts a part offers: one `ModifyDevice` of its group's `contacts`.
///
/// # Errors
/// A device whose group has no contacts; an unreadable offer ([`crate::references::devices::parse::with_contacts`]).
pub(crate) fn contacts_value(doc: &Document, o: ObjectRef, _: &str, value: Option<&str>) -> Result<Option<Op>, CmdError> {
    let (c, d) = device_of(doc, o)?;
    let after = crate::references::devices::parse::with_contacts(d, &crate::devices::tags::tag_words(doc, c), value)?;
    Ok((after != *d).then(|| Op::ModifyDevice { id: c, before: d.clone(), after }))
}

/// Where a switching device's contact table goes: one `ModifyDevice` of its
/// `contact_table`.
///
/// # Errors
/// An unreadable placement; not a switching device ([`crate::references::devices::parse::with_table`]).
pub fn table_value(doc: &Document, o: ObjectRef, _: &str, value: Option<&str>) -> Result<Option<Op>, CmdError> {
    let (c, d) = device_of(doc, o)?;
    let table = crate::references::devices::parse::table_of_typed(value)?;
    let after = crate::references::devices::parse::with_table(d, &crate::devices::tags::tag_words(doc, c), table)?;
    Ok((after != *d).then(|| Op::ModifyDevice { id: c, before: d.clone(), after }))
}

/// A connector's pins, pin names, half parts or ignored rows: one `ModifyDevice`.
///
/// # Errors
/// [`crate::connectors::with_field`]'s refusals.
pub(crate) fn connector_value(doc: &Document, o: ObjectRef, key: &str, value: Option<&str>) -> Result<Option<Op>, CmdError> {
    let (c, d) = device_of(doc, o)?;
    let after = crate::connectors::with_field(d, &crate::devices::tags::tag_words(doc, c), key, value)?;
    Ok((after != *d).then(|| Op::ModifyDevice { id: c, before: d.clone(), after }))
}

fn device_of(doc: &Document, o: ObjectRef) -> Result<(DeviceId, &Device), CmdError> {
    match o {
        ObjectRef::Device(c) => doc.devices().get(&c).map(|d| (c, d)).ok_or(CmdError::refused(NO_SUCH_OBJECT)),
        _ => Err(CmdError::refused(NO_SUCH_OBJECT)),
    }
}

/// The refusal of a text field's value, in words.
pub(crate) fn text_refusal(f: TextField, e: TextError) -> CmdError {
    let max = f.max_chars();
    CmdError::refused(match e {
        TextError::TooLong { .. } => format!("`{}` can be at most {max} characters.", f.key()),
        TextError::Hidden { .. } | TextError::Lines => format!("`{}` is one line with no control or hidden characters.", f.key()),
        TextError::Empty => format!("`{}` is empty.", f.key()),
    })
}

/// The device `d` with text field `f` set to `value`, checked.
///
/// # Errors
/// [`text_refusal`]'s words.
pub fn with_text(d: &Device, f: TextField, value: Option<&str>) -> Result<Device, CmdError> {
    d.with_text(f, value).map_err(|e| text_refusal(f, e))
}

/// A device's description, part number, maker, article number or type text: one `ModifyDevice`.
///
/// # Errors
/// [`text_refusal`].
pub(crate) fn text_value(doc: &Document, o: ObjectRef, key: &str, value: Option<&str>) -> Result<Option<Op>, CmdError> {
    let (c, d) = device_of(doc, o)?;
    let f = TextField::of_key(key).ok_or(CmdError::refused(NO_SUCH_OBJECT))?;
    let after = with_text(d, f, value)?;
    Ok((after != *d).then(|| Op::ModifyDevice { id: c, before: d.clone(), after }))
}
