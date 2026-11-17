//! `setprop` (Set Property): a thin shim (brief 070). Its params struct reads the JSON; the rules
//! in [`ecad_electrical::cmd::prop`] decide, walking one table, `PROP_RULES`, for one object and
//! for several devices alike; the shim commits the ops and answers in JSON. A function or
//! location goes through [`crate::cmd::location`]'s set, as `setlocation` does.

use cadcraft_engine::Session;
use ecad_electrical::CmdError;
use ecad_electrical::cmd::location::{self as location_rules, MAX_DEVICES};
use ecad_electrical::cmd::prop::{self as rules, Build, Conflict, DeviceSet, TagSet};
use ecad_model::tag::TAG_KEY;
use ecad_model::{CableId, ConductorId, DeviceId, ObjectRef};
use schemars::JsonSchema;
use serde::Deserialize;
use serde_json::{Value, json};

use crate::access::Pages;
use crate::cmd::input::{Bounded, Command, OrNull, Typed, nullable};
use crate::cmd::{MAX_HANDLES, err, on_project};
use crate::project::Project;

/// The commands of this module with a typed input.
pub const TYPED: &[Typed] = &[Typed::of::<SetpropParams>("setprop")];

/// `setprop {component|conductor|cable, key, value, conflict?, merge?}` or `{components, key,
/// value}`: a property of one object, or of several devices in one transaction (not `tag`). A
/// device's or cable's tag: typing fixes it; `-K?` is the next provisional; a tag another device
/// or cable holds needs `conflict`. Key `tag_fixed`: "yes" or null.
#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub(crate) struct SetpropParams {
    /// A device (or any component).
    #[serde(default)]
    component: Option<DeviceId>,
    /// Several devices, in one transaction.
    #[serde(default)]
    devices: Option<Bounded<DeviceId, 0, MAX_HANDLES>>,
    /// A wire.
    #[serde(default)]
    conductor: Option<ConductorId>,
    /// A cable.
    #[serde(default)]
    cable: Option<CableId>,
    /// The property's key.
    key: String,
    /// The value, or null to clear the property.
    #[serde(deserialize_with = "nullable")]
    #[schemars(required, with = "OrNull<String>")]
    value: Option<Option<String>>,
    /// A tag another device or cable holds: swap, merge or next.
    #[serde(default)]
    conflict: Option<String>,
    /// A device tag another device holds: make this the same device (as conflict merge).
    #[serde(default)]
    merge: Option<bool>,
    /// With a null value: clear an older build's text setting on the project.
    #[serde(default)]
    project: Option<bool>,
}

impl Command for SetpropParams {
    fn run(self, s: &mut Session) -> cadcraft_engine::Result<Value> {
        let key = self.key;
        let typed = self.value.flatten();
        // Brief 050a (design 050 §3.4): an older build's text size, weight or font on the project is
        // cleared here (its Checks row goes); it is never written.
        if self.project == Some(true) && typed.is_none() && ecad_electrical::styles::checks::is_old_key(&key) {
            return on_project(s, |p, pg, _| {
                let Some(op) = rules::clear_old_key(p.ledger.document(), &key) else { return Ok(json!({"changed": false})) };
                p.apply_edit(pg, "setprop", "Clear an older build's text setting", vec![op])?;
                Ok(json!({"changed": true}))
            });
        }
        rules::reserved(&key).map_err(|e| err(e.to_string()))?;
        // Brief 052 (design 052 §5.9): a function or location (PROP_RULES' first row, which claims by
        // key alone) is stored as an entry key of the project's list, for all the devices at once.
        if (self.component.is_some() || self.devices.is_some())
            && let Some(prop) = location_rules::aspect_property(&key)
        {
            let objects = match self.devices {
                Some(cs) if cs.0.len() > MAX_DEVICES => {
                    return Err(err(format!("at most {MAX_DEVICES} entries in `devices` per call")));
                }
                Some(cs) => location_rules::objects(None, Some(cs.0)),
                None => location_rules::objects(self.component, None),
            };
            let objects = objects.map_err(|e| err(e.to_string()))?;
            return crate::cmd::location::apply(s, objects, prop, typed, "setprop");
        }
        // Several devices at once (brief 019, the Properties panel): one transaction for all.
        if let Some(cs) = self.devices {
            return on_project(s, |p, pg, _| {
                let ops = rules::plan_many(p.ledger.document(), &cs.0, &key, typed.as_deref())?;
                if ops.is_empty() {
                    return Ok(json!({"changed": false}));
                }
                let n = ops.len();
                p.apply_edit(pg, "setprop", "Set property", ops)?;
                Ok(json!({"changed": true, "devices": n}))
            });
        }
        let object = if let Some(c) = self.component {
            ObjectRef::Device(c)
        } else if let Some(c) = self.conductor {
            ObjectRef::Conductor(c)
        } else if let Some(c) = self.cable {
            ObjectRef::Cable(c)
        } else {
            return Err(err("`component`, `conductor` or `cable` (hex id) is required"));
        };
        let (word, merge) = (self.conflict, self.merge.unwrap_or(false));
        on_project(s, |p, pg, _| {
            let doc = p.ledger.document();
            // Pack re-run, bug 7: on a PLC card with channels, `channels` is its channel count.
            let key = match object {
                ObjectRef::Device(c) => rules::component_key(doc, c, &key),
                _ => key,
            };
            match rules::rule_for(doc, object, &key).build {
                Build::Tag => {
                    let conflict = rules::conflict(word.as_deref(), merge)?;
                    match (conflict, object) {
                        // Merging two devices keeps 018's path (labels, contacts, connection points).
                        (Some(Conflict::Merge), ObjectRef::Device(c)) => set_device(p, pg, c, TAG_KEY, typed.as_deref(), true),
                        _ => set_tag(p, pg, object, typed.as_deref(), conflict),
                    }
                }
                Build::Fixed => {
                    let done = rules::plan_fixed(doc, object, typed.as_deref())?;
                    let Some(op) = done.op else { return Ok(json!({"changed": false, "fixed": done.on})) };
                    p.apply_edit(pg, "setprop", if done.on { "Fix tag" } else { "Release tag" }, vec![op])?;
                    Ok(json!({"changed": true, "fixed": done.on}))
                }
                Build::Device(_) => match object {
                    ObjectRef::Device(c) => set_device(p, pg, c, &key, typed.as_deref(), merge),
                    _ => set_value(p, pg, object, &key, typed.as_deref()),
                },
                Build::Value(_) | Build::Aspect => set_value(p, pg, object, &key, typed.as_deref()),
                Build::Record(_) => {
                    let Some(op) = rules::plan_record(doc, object, &key, typed.as_deref())? else { return Ok(json!({"changed": false})) };
                    p.apply_edit(pg, "setprop", "Set property", vec![op])?;
                    Ok(json!({"changed": true}))
                }
            }
        })
    }
}

/// A `Value` row's change on one object.
fn set_value(p: &mut Project, pg: &mut Pages<'_>, object: ObjectRef, key: &str, typed: Option<&str>) -> Result<Value, CmdError> {
    let done = rules::plan_value(p.ledger.document(), object, key, typed)?;
    let Some(op) = done.op else { return Ok(json!({"changed": false})) };
    p.apply_edit(pg, "setprop", "Set property", vec![op])?;
    Ok(match done.note {
        Some(m) => {
            pg.echo(m.clone());
            json!({"changed": true, "message": m})
        }
        None => json!({"changed": true}),
    })
}

/// A device property (design 018 §6), merging with `merge`.
fn set_device(p: &mut Project, pg: &mut Pages<'_>, c: DeviceId, key: &str, typed: Option<&str>, merge: bool) -> Result<Value, CmdError> {
    match rules::plan_device(p.ledger.document(), &mut p.ids, &p.tag_high, c, key, typed, merge)? {
        DeviceSet::Unchanged(value) => Ok(json!({"changed": false, "value": value})),
        DeviceSet::Set { op, label, value } => {
            p.apply_edit(pg, "setprop", label, vec![*op])?;
            Ok(json!({"changed": true, "value": value, "component": c.hex()}))
        }
        DeviceSet::Merged { ops, holder, into, before } => {
            p.apply_edit(pg, "setprop", &format!("Merge into {into}"), ops)?;
            let message = rules::merged_device_words(p.ledger.document(), before.as_deref(), holder, &into);
            Ok(json!({"changed": true, "merged": true, "component": holder.hex(), "tag": into, "message": message}))
        }
    }
}

/// The tag of a device or cable (design 026 §4, §7).
fn set_tag(p: &mut Project, pg: &mut Pages<'_>, o: ObjectRef, typed: Option<&str>, conflict: Option<Conflict>) -> Result<Value, CmdError> {
    p.tag_high.catch_up(&p.ledger);
    match rules::plan_tag(p.ledger.document(), &mut p.tag_high, o, typed, conflict)? {
        TagSet::Unchanged(value) => Ok(json!({"changed": false, "value": value})),
        TagSet::Set { ops, value, fixed } => {
            p.apply_edit(pg, "setprop", "Set tag", ops)?;
            Ok(match fixed {
                Some(f) => json!({"changed": true, "value": value, "fixed": f}),
                None => json!({"changed": true, "value": value}),
            })
        }
        TagSet::Swapped { ops, value, other, other_value, line } => {
            p.apply_edit(pg, "setprop", "Swap tags", ops)?;
            pg.echo(line.clone());
            Ok(json!({"changed": true, "swapped": true, "value": value, "other": object_json(other), "other_value": other_value, "message": line}))
        }
        TagSet::MergedCable { ops, into, tag, before } => {
            p.apply_edit(pg, "setprop", &format!("Merge into {tag}"), ops)?;
            let line = rules::merged_cable_words(p.ledger.document(), before.as_deref(), &tag, into);
            pg.echo(line.clone());
            Ok(json!({"changed": true, "merged": true, "cable": into.hex(), "tag": tag, "message": line}))
        }
        TagSet::Next { ops, value, fixed, line } => {
            p.apply_edit(pg, "setprop", "Set tag", ops)?;
            pg.echo(line.clone());
            Ok(json!({"changed": true, "value": value, "fixed": fixed, "message": line}))
        }
    }
}

fn object_json(o: ObjectRef) -> Value {
    match o {
        ObjectRef::Device(c) => json!({"component": c.hex()}),
        ObjectRef::Cable(k) => json!({"cable": k.hex()}),
        _ => Value::Null,
    }
}
