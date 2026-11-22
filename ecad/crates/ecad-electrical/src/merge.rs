//! The merge dialog's rules over the document (brief 020b, design 020 §5.2): what a value typed in a
//! conflict's Edit field is stored as. The engine's `merging` module runs the merge (brief 074
//! moved this rule here).
//!
//! This module excludes reading branches, planning and writing the merge.

use crate::CmdError;
use ecad_model::Document;
use ecad_model::merge::state::Obj;

/// Check a value typed in a conflict's Edit field as an ordinary property edit of that object
/// would be (design §5.2: the property's own rules from 018/019/022): `Ok(value)` as stored.
///
/// # Errors
/// The property's own refusal, or an empty value.
pub fn edit_rule(doc: &Document, obj: Obj, key: &str, value: &str) -> Result<String, CmdError> {
    let checked = match obj {
        Obj::Device(c) if crate::properties::commands::is_device_property(ecad_model::ObjectRef::Device(c), key) => {
            crate::devices::json::property_value(doc, key, Some(value))?
        }
        Obj::Wire(c) => match crate::wires::commands::wires::setprop_rule(doc, ecad_model::ObjectRef::Wire(c), key, Some(value))? {
            Some(v) => v,
            None => crate::devices::json::property_value(doc, "", Some(value))?,
        },
        Obj::Cable(c) => match crate::wires::commands::wires::setprop_rule(doc, ecad_model::ObjectRef::Cable(c), key, Some(value))? {
            Some(v) => v,
            None => crate::devices::json::property_value(doc, "", Some(value))?,
        },
        _ => crate::devices::json::property_value(doc, "", Some(value))?,
    };
    checked.ok_or(CmdError::Refused("An Edit value cannot be empty: type one, or keep one side's value.".into()))
}
