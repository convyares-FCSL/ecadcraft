//! The JSON forms of brief 018's commands (design 018 §8): `place` with a starter symbol, `setprop`
//! on device properties (tag rules, merging), `symbols` and `devices` (read only).
//!
//! The interactive forms ([`crate::interact`]) commit through exactly these.

use cadcraft_engine::{EngineError, Session};
use ecad_model::tag::TAG_KEY;
use ecad_model::{Color, ComponentId, ObjectRef, Op, Placement, Pt, Style};
use serde_json::{Value, json};

use crate::commands::{with_any_page, with_project};
use crate::devices::{self, Target};
use crate::project::Project;

fn err(m: impl Into<String>) -> EngineError {
    EngineError::Other(m.into())
}

fn style(p: &Project) -> Style {
    Style { layer: p.default_layer, color: Color::ByLayer, linetype: "ByLayer".into(), lineweight: -1 }
}

fn hex_id<T: From<u128>>(v: &Value, key: &str) -> Result<Option<T>, String> {
    match v.get(key) {
        None | Some(Value::Null) => Ok(None),
        Some(x) => x
            .as_str()
            .and_then(|t| u128::from_str_radix(t.trim(), 16).ok())
            .map(|n| Some(T::from(n)))
            .ok_or_else(|| format!("`{key}` (hex id) is required")),
    }
}

/// Whether a `place` call is brief 018's form (a starter symbol), rather than the experiment's.
pub(crate) fn is_symbol_place(params: &Value) -> bool {
    ["symbol", "device", "tag"].iter().any(|k| params.get(*k).is_some_and(|v| !v.is_null()))
}

/// `place {at, rotation?, mirror?, symbol, device?, tag?}` (design 018 §8).
pub(crate) fn run_place(s: &mut Session, params: &Value) -> cadcraft_engine::Result<Value> {
    if params.get("definition").is_some_and(|v| !v.is_null()) && params.get("symbol").is_some_and(|v| !v.is_null()) {
        return Err(err("`symbol` and `definition` are mutually exclusive"));
    }
    let key = params
        .get("symbol")
        .and_then(Value::as_str)
        .ok_or_else(|| err("`symbol` (a starter symbol key, see `symbols`) is required with `device` or `tag`"))?;
    let starter = devices::starter(key).map_err(err)?;
    let at = params.get("at").ok_or_else(|| err("`at` [x,y] is required"))?;
    let at = at.as_array().and_then(|a| Some(Pt::new(a.first()?.as_f64()?, a.get(1)?.as_f64()?))).ok_or_else(|| err("`at` [x,y] is required"))?;
    if !(at.is_finite() && at.x.abs() <= crate::interact::MAX_COORD && at.y.abs() <= crate::interact::MAX_COORD) {
        return Err(err(crate::interact::OUT_OF_RANGE));
    }
    let degrees = match params.get("rotation") {
        None | Some(Value::Null) => 0.0,
        Some(v) => v.as_f64().filter(|d| d.is_finite()).ok_or_else(|| err("`rotation` must be a finite number of degrees"))?,
    };
    let mirror = match params.get("mirror") {
        None | Some(Value::Null) => false,
        Some(v) => v.as_bool().ok_or_else(|| err("`mirror` must be true or false"))?,
    };
    // Brief 022: a junction or continuation is a tagless component of its own.
    if starter.kind == crate::symbols::Kind::Tagless {
        return crate::wire_cmds::place_tagless(s, params, starter, at, degrees, mirror);
    }
    // Brief 036: a level, a whole terminal, a disconnect or fuse terminal joins a strip as a position.
    if crate::terminal_types::is_level_starter(starter.key) {
        return crate::terminal_cmds::run_place_level(s, params, starter, at, degrees, mirror);
    }
    let device: Option<ComponentId> = hex_id(params, "device").map_err(err)?;
    let tag = match params.get("tag") {
        None | Some(Value::Null) => None,
        Some(Value::String(t)) => Some(t.clone()),
        Some(_) => return Err(err("`tag` must be a string")),
    };
    if device.is_some() && tag.is_some() {
        return Err(err("`tag` is for a new device; `device` joins an existing one"));
    }
    with_project(s, |p, pg, page| {
        let doc = p.ledger.document();
        let target = match (device, tag) {
            (Some(c), _) => {
                if !doc.components.contains_key(&c) {
                    return Err("No such device.".into());
                }
                Target::Join(c)
            }
            (None, Some(t)) => Target::New(Some(ecad_model::tag::normalise(&t, devices::padding(doc))?)),
            (None, None) => Target::New(None),
        };
        // Brief 028 (design 028 §3.1): a terminal clicked on a wire lands on it, splitting it.
        let landing = (starter.kind == crate::symbols::Kind::Terminal).then(|| crate::terminal_cmds::landing_point(doc, page, at)).flatten();
        let origin = landing.map_or(at, |on| on.at);
        let placement = Placement { origin, rotation: degrees.to_radians(), mirror, scale: 1.0 };
        let style = style(p);
        // Brief 025 (design 025 §4.3): a joined contact takes its part's next free offered labels.
        let offer_slot = match (&target, starter.kind) {
            (Target::Join(c), crate::symbols::Kind::Contact { make }) => {
                crate::xref::offer_labels(doc, *c, make, &devices::labels_of(doc, *c)).is_some()
            }
            _ => false,
        };
        let mut plan = devices::plan_place(&p.ledger, &mut p.ids, &mut p.tag_high, page, style, starter, placement, target)?;
        let land = match (landing, plan.terminals.first()) {
            (Some(on), Some((_, _, t))) => {
                let land = crate::terminal_cmds::plan_land(p.ledger.document(), &mut p.ids, page, on, *t)?;
                plan.ops.extend(land.ops.clone());
                Some((*t, land))
            }
            _ => None,
        };
        let label = if land.is_some() { "Place terminal".to_string() } else { plan.label.clone() };
        p.apply_edit(pg, "place", &label, plan.ops.clone())?;
        p.last_placed = Some((starter.key.to_string(), plan.component));
        let doc = p.ledger.document();
        let landed = land.map(|(t, l)| crate::terminal_cmds::land_result(doc, t, &l));
        if let (true, crate::symbols::Kind::Contact { make }) = (plan.joined, starter.kind) {
            let labels: Vec<String> = plan.terminals.iter().map(|(_, l, _)| l.clone()).collect();
            if let Some(line) = crate::xref::over_line(doc, plan.component, &labels, make) {
                pg.echo(line);
            }
        }
        let mut v = json!({
            "component": plan.component.hex(),
            "representation": plan.representation.hex(),
            "item": plan.item.hex(),
            "definition": plan.definition.hex(),
            "symbol": starter.key,
            "tag": plan.tag,
            "joined": plan.joined,
            "offer_slot": offer_slot,
            "pages": devices::page_numbers(doc, plan.component),
            "terminals": plan.terminals.iter().map(|(k, l, t)| json!({"key": k, "label": l, "terminal": t.hex()})).collect::<Vec<_>>(),
        });
        if let Some((fields, message)) = landed {
            if let (Some(o), Some(f)) = (v.as_object_mut(), fields.as_object()) {
                o.extend(f.clone());
            }
            v["message"] = json!(message);
            pg.echo(message);
        }
        Ok(v)
    })
}

/// Whether `setprop` on `object` and `key` follows design 018 §6.1's rules.
pub(crate) fn is_device_property(object: ObjectRef, key: &str) -> bool {
    matches!(object, ObjectRef::Component(_)) && devices::PROPERTIES.iter().any(|(k, _)| *k == key)
}

/// `setprop {component, key, value, merge?}` for a device property (design 018 §6, owner decisions
/// 2026-11-08): the value rules of §6.1; a tag another device holds is refused with the ways
/// forward, unless `merge` is true, which makes this device part of that one (one transaction).
pub(crate) fn run_setprop(s: &mut Session, params: &Value, c: ComponentId, key: String) -> cadcraft_engine::Result<Value> {
    let value = match params.get("value") {
        Some(Value::Null) => None,
        Some(Value::String(v)) => Some(v.clone()),
        Some(_) => return Err(err("`value` must be a string, or null to clear the property")),
        None => return Err(err("`value` is required (a string, or null to clear the property)")),
    };
    let merge = match params.get("merge") {
        None | Some(Value::Null) => false,
        Some(v) => v.as_bool().ok_or_else(|| err("`merge` must be true or false"))?,
    };
    with_project(s, |p, pg, _page| {
        let doc = p.ledger.document();
        let component = doc.components.get(&c).ok_or("no such object")?;
        let value = devices::property_value(doc, &key, value.as_deref())?;
        let before = component.properties.get(&key).cloned();
        if key == TAG_KEY
            && let Some(t) = &value
            && let Some(holder) = devices::holder_of(doc, t, Some(c))
        {
            if !merge {
                let next = devices::tag_of(doc, c)
                    .and_then(ecad_model::tag::parse)
                    .or_else(|| ecad_model::tag::parse(t))
                    .and_then(|(prefix, _)| p.tag_high.peek(doc, &prefix).ok());
                let way = match next {
                    Some(n) => format!("Set `merge` to true to make this the same device, or choose another tag (the next free one is {n})."),
                    None => "Set `merge` to true to make this the same device, or choose another tag.".into(),
                };
                return Err(devices::duplicate_words(doc, t, holder, &way));
            }
            let ops = devices::plan_merge(doc, &mut p.ids, c, holder)?;
            let into = devices::tag_words(doc, holder);
            p.apply_edit(pg, "setprop", &format!("Merge into {into}"), ops)?;
            let doc = p.ledger.document();
            return Ok(json!({
                "changed": true, "merged": true, "component": holder.hex(), "tag": into,
                "message": format!("{} is now part of {into} ({} symbols on {}).", before.unwrap_or_else(|| "The device".into()),
                    doc.representations_of(ecad_model::Subject::Component(holder)).count(), devices::pages_words(doc, holder)),
            }));
        }
        if before == value {
            return Ok(json!({"changed": false, "value": value}));
        }
        let label = if key == TAG_KEY { "Set tag" } else { "Set property" };
        let op = Op::SetProperty { object: ObjectRef::Component(c), key: key.clone(), before, after: value.clone() };
        p.apply_edit(pg, "setprop", label, vec![op])?;
        Ok(json!({"changed": true, "value": value, "component": c.hex()}))
    })
}

/// `symbols {query?}` (design 018 §8): read only.
pub(crate) fn run_symbols(s: &mut Session, params: &Value) -> cadcraft_engine::Result<Value> {
    let q = match params.get("query") {
        None | Some(Value::Null) => None,
        Some(Value::String(q)) => Some(q.clone()),
        Some(_) => return Err(err("`query` must be a string")),
    };
    with_any_page(s, |p, _| Ok(devices::symbols_json(p.ledger.document(), q.as_deref())))
}

/// `devices {tag?}` (design 018 §8): read only.
pub(crate) fn run_devices(s: &mut Session, params: &Value) -> cadcraft_engine::Result<Value> {
    let q = match params.get("tag") {
        None | Some(Value::Null) => None,
        Some(Value::String(q)) => Some(q.clone()),
        Some(_) => return Err(err("`tag` must be a string")),
    };
    with_any_page(s, |p, _| Ok(devices::devices_json(p.ledger.document(), q.as_deref())))
}

/// `deviceprops {component}`: the device's properties (read only); its interactive form changes them.
pub(crate) fn run_deviceprops(s: &mut Session, params: &Value) -> cadcraft_engine::Result<Value> {
    let c: ComponentId = hex_id(params, "component").map_err(err)?.ok_or_else(|| err("`component` (hex id) is required"))?;
    with_any_page(s, |p, _| {
        let doc = p.ledger.document();
        if !doc.components.contains_key(&c) {
            return Err("No such device.".into());
        }
        Ok(devices::device_json(doc, c))
    })
}

/// `devicedelete {component}`: the device and every symbol of it, as `ecaddelete` (design 018 §6.3).
pub(crate) fn run_devicedelete(s: &mut Session, params: &Value) -> cadcraft_engine::Result<Value> {
    let c: ComponentId = hex_id(params, "component").map_err(err)?.ok_or_else(|| err("`component` (hex id) is required"))?;
    let tag = with_any_page(s, |p, _| Ok(devices::tag_words(p.ledger.document(), c)))?;
    let r = crate::commands::run_delete(s, &json!({"component": c.hex()}))?;
    Ok(json!({"deleted": r.get("deleted").cloned().unwrap_or(Value::Bool(true)), "message": format!("Deleted {tag}.")}))
}

/// `tagsettings {padding?, prefixes?}` (brief 018, owner decision 2026-11-08): how many digits
/// automatic tag numbers have, and the project's own prefix per starter symbol (`{"coil": "KA"}`;
/// `null` goes back to the built-in one). One `SetProjectSettings` transaction; with nothing to
/// change, or no parameters, it records nothing and reports the settings.
/// The tag settings `params` asks for under `padding_key` and `prefixes_key`: the padding, and per
/// starter symbol key the prefix (`None`: back to the built-in one).
///
/// # Errors
/// A padding out of range, an unknown symbol, a prefix that is not 1 to 3 letters.
pub(crate) fn tag_settings_params(
    params: &Value, padding_key: &str, prefixes_key: &str,
) -> Result<(Option<u8>, Vec<(String, Option<String>)>), String> {
    let padding = match params.get(padding_key) {
        None | Some(Value::Null) => None,
        Some(v) => Some(
            v.as_u64()
                .filter(|n| (1..=u64::from(ecad_model::tag::MAX_PADDING)).contains(n))
                .and_then(|n| u8::try_from(n).ok())
                .ok_or_else(|| format!("`{padding_key}` must be a whole number from 1 to {}", ecad_model::tag::MAX_PADDING))?,
        ),
    };
    let mut changes: Vec<(String, Option<String>)> = Vec::new();
    match params.get(prefixes_key) {
        None | Some(Value::Null) => {}
        Some(Value::Object(m)) => {
            for (k, v) in m {
                let symbol = devices::starter(k)?;
                let prefix = match v {
                    Value::Null => None,
                    Value::String(t) => Some(
                        devices::valid_prefix(t)
                            .ok_or_else(|| format!("A prefix is 1 to {} letters, for example K or SR.", devices::MAX_PREFIX_LETTERS))?,
                    ),
                    _ => return Err("each prefix must be a string, or null for the built-in one".into()),
                };
                changes.push((symbol.key.to_string(), prefix));
            }
        }
        Some(_) => return Err(format!("`{prefixes_key}` must be an object: {{\"<symbol>\": \"<prefix>\" | null}}")),
    }
    Ok((padding, changes))
}

/// Apply tag settings read by [`tag_settings_params`] to `after`.
pub(crate) fn apply_tag_settings(after: &mut ecad_model::ProjectSettings, padding: Option<u8>, changes: Vec<(String, Option<String>)>) {
    if let Some(n) = padding {
        after.tag_padding = n;
    }
    for (k, v) in changes {
        match v {
            Some(px) if crate::symbols::by_key(&k).is_some_and(|s| s.prefix() == px) => after.tag_prefixes.remove(&k),
            Some(px) => after.tag_prefixes.insert(k, px),
            None => after.tag_prefixes.remove(&k),
        };
    }
}

/// Every starter symbol's prefix as this project uses it (`-K`).
pub(crate) fn prefixes_json(doc: &ecad_model::Document) -> serde_json::Map<String, Value> {
    crate::symbols::STARTERS
        .iter()
        // Junctions and continuations have no tag (brief 022).
        .filter(|s| s.kind != crate::symbols::Kind::Tagless)
        .map(|s| (s.key.to_string(), Value::String(format!("-{}", devices::prefix_for(doc, s)))))
        .collect()
}

pub(crate) fn run_tagsettings(s: &mut Session, params: &Value) -> cadcraft_engine::Result<Value> {
    let (padding, changes) = tag_settings_params(params, "padding", "prefixes").map_err(err)?;
    with_project(s, |p, pg, _| {
        let before = p.ledger.document().settings.clone();
        let mut after = before.clone();
        apply_tag_settings(&mut after, padding, changes);
        let changed = after != before;
        if changed {
            p.apply_edit(pg, "tagsettings", "Tag settings", vec![Op::SetProjectSettings { before, after: after.clone() }])?;
        }
        let doc = p.ledger.document();
        Ok(json!({"changed": changed, "padding": devices::padding(doc), "prefixes": prefixes_json(doc)}))
    })
}
