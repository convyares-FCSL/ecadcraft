//! Lane 072 B's rows of `setprop` (design 072 §2.1, brief 072 "Lanes"): a device's keys that became
//! [`ecad_model::Device`] fields. Each builds one `ModifyDevice`.

use ecad_model::text::TextError;
use ecad_model::{Device, DeviceId, Document, ObjectRef, Op, TextField};

use super::super::prop::{Build, NO_SUCH_OBJECT, PropRule};
use crate::error::CmdError;

/// This lane's rows, asked before [`super::super::prop::PROP_RULES`].
pub static RULES: &[PropRule] = &[PropRule {
    what: "device text fields (072 B)",
    claims: |_, o, k| matches!(o, ObjectRef::Device(_)) && TextField::of_key(k).is_some(),
    build: Build::Record(text_value),
}];

fn device_of(doc: &Document, o: ObjectRef) -> Result<(DeviceId, &Device), CmdError> {
    match o {
        ObjectRef::Device(c) => doc.devices.get(&c).map(|d| (c, d)).ok_or(CmdError::refused(NO_SUCH_OBJECT)),
        _ => Err(CmdError::refused(NO_SUCH_OBJECT)),
    }
}

/// The refusal of a text field's value, in words.
pub fn text_refusal(f: TextField, e: TextError) -> CmdError {
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
/// An item's description cleared ([`crate::bom::refuse_on_item`]); [`text_refusal`].
pub fn text_value(doc: &Document, o: ObjectRef, key: &str, value: Option<&str>) -> Result<Option<Op>, CmdError> {
    let (c, d) = device_of(doc, o)?;
    crate::bom::refuse_on_item(doc, c, key, value)?;
    let f = TextField::of_key(key).ok_or(CmdError::refused(NO_SUCH_OBJECT))?;
    let after = with_text(d, f, value)?;
    Ok((after != *d).then(|| Op::ModifyDevice { id: c, before: d.clone(), after }))
}
