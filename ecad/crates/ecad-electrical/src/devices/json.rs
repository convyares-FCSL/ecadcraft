//! The JSON view of devices and symbols.

use super::*;

/// Whether device tag `t` matches a search `query`: `-k` and `k1` find `-K01`,
/// `-K10`…
pub(crate) fn tag_matches(t: &str, query: &str) -> bool {
    let q = query.trim().trim_start_matches('-').to_uppercase();
    if q.is_empty() {
        return true;
    }
    match (tag::parse(t), tag::parse(&q)) {
        (Some((p, n)), Some((qp, qn))) => p == qp && n.to_string().starts_with(&qn.to_string()),
        (Some((p, _)), None) => p.starts_with(&q) && q.chars().all(|c| c.is_ascii_alphabetic()),
        _ => tag::key(t).starts_with(&q),
    }
}

/// Every device (a component with a tag) whose tag matches `query` (or every one), ordered by
/// prefix, then number, then ID.
pub fn devices(doc: &Document, query: Option<&str>) -> Vec<DeviceId> {
    // Through the tag index: untagged components are never visited.
    let mut v: Vec<(String, u64, String, DeviceId)> = doc
        .tagged()
        .filter_map(|(_, id)| {
            let t = doc.devices().get(&id)?.tag()?;
            if query.is_some_and(|q| !tag_matches(t, q)) {
                return None;
            }
            let (p, n) = tag::parse(t).unwrap_or_else(|| (tag::key(t), 0));
            Some((p, n, t.to_string(), id))
        })
        .collect();
    v.sort();
    // A typed half tag (`-X10.S`, `-X10.P:3`) finds its
    // connector.
    if v.is_empty()
        && let Some(Ok((c, Some(_), _))) = query.and_then(|q| crate::connectors::resolve(doc, q))
    {
        return vec![c];
    }
    v.into_iter().map(|(_, _, _, id)| id).collect()
}

/// `symbols {query?}`'s result.
pub(crate) fn symbols_json(doc: &Document, query: Option<&str>) -> Value {
    let q = query.unwrap_or("");
    let symbols: Vec<Value> = symbols::project::search(q)
        .into_iter()
        .map(|s| {
            let (w, h) = s.size();
            json!({
                "key": s.key, "name": s.name, "aliases": s.aliases, "prefix": format!("-{}", prefix_for(doc, s)),
                "connection_points": s.labels(), "box": [w, h], "joinable": s.joinable(),
            })
        })
        .collect();
    let found = if q.trim().is_empty() { devices(doc, None) } else { devices(doc, Some(q)) };
    let more = found.len().saturating_sub(MAX_LISTED);
    let devices: Vec<Value> = found
        .into_iter()
        .take(MAX_LISTED)
        .map(|c| {
            json!({
                "component": c.hex(), "tag": tag_words(doc, c), "description": description(doc, c),
                "representations": doc.representations_of(Subject::Device(c)).count(),
                "pages": page_numbers(doc, c),
            })
        })
        .collect();
    let mut r = json!({ "symbols": symbols, "devices": devices });
    if more > 0 {
        r["more"] = json!(more);
    }
    r
}

/// `devices {tag?}`'s result.
pub(crate) fn devices_json(doc: &Document, query: Option<&str>) -> Value {
    let found = devices(doc, query);
    let more = found.len().saturating_sub(MAX_LISTED);
    let list: Vec<Value> = found.into_iter().take(MAX_LISTED).map(|c| device_json(doc, c)).collect();
    let mut r = json!({ "devices": list });
    if more > 0 {
        r["more"] = json!(more);
    }
    r
}

/// One device in JSON.
pub(crate) fn device_json(doc: &Document, c: DeviceId) -> Value {
    let comp = doc.devices().get(&c).cloned().unwrap_or_default();
    let order: BTreeMap<PageId, usize> = doc.page_order().iter().enumerate().map(|(i, p)| (*p, i + 1)).collect();
    let mut reps: Vec<(usize, RepresentationId, Value)> = doc
        .representations_of(Subject::Device(c))
        .map(|(rid, r)| {
            let labels: Vec<String> = shown_connection_points(doc, c, r).iter().map(|(t, _)| label_of(doc, *t)).collect();
            let n = order.get(&r.page).copied().unwrap_or(usize::MAX);
            (
                n,
                rid,
                json!({
                    "representation": rid.hex(), "page": r.page.hex(), "page_number": order.get(&r.page),
                    "role": r.role, "symbol": rep_starter(doc, r).map(|s| s.key), "connection_points": labels,
                }),
            )
        })
        .collect();
    reps.sort_by_key(|(n, rid, _)| (*n, *rid));
    let mut v = json!({ "component": c.hex() });
    for (key, _) in PROPERTIES {
        // A function or location shows its code; its stored key is `<key>_key`.
        let stored = comp.value(key);
        v[key] = stored.as_ref().map_or(Value::Null, |x| Value::String(crate::locations::designations::display(doc, key, x)));
        if crate::locations::designations::Aspect::of_property(key).is_some() {
            v[format!("{key}_key")] = stored.map_or(Value::Null, Value::String);
        }
    }
    // The location stored (never derived), and each symbol's own
    // location (overrides) and context (what a box or page there proposes).
    let mut boxes = crate::locations::BoxCache::default();
    let loc = crate::locations::resolve_with(doc, c, &mut boxes);
    v["location_shown"] = loc.value.map_or(Value::Null, Value::String);
    v["location_from"] = json!(loc.source.word());
    for (_, rid, x) in &mut reps {
        if let Some(r) = doc.representations().get(rid) {
            x["context"] = crate::locations::rep_context(doc, r, &mut boxes).value.map_or(Value::Null, Value::String);
            x["location"] = crate::locations::rep_location(doc, c, r).value.map_or(Value::Null, Value::String);
            x["type"] = json!(crate::symbols::reps::rep_type(doc, r).word());
        }
    }
    v["representations"] = Value::Array(reps.into_iter().map(|(_, _, x)| x).collect());
    // S6 #3: the tag with its state as one value (`{"state","tag"}`, the bare tag), and as drawn
    // (a provisional one with its `?`).
    v["tag"] = json!(comp.tag);
    v["tag_shown"] = json!(comp.tag_shown());
    v
}

/// The device a starter symbol `key` names.
///
/// # Errors
/// T4: no such symbol.
pub fn starter(key: &str) -> Result<&'static Starter, CmdError> {
    symbols::catalogue::by_key(key).ok_or_else(|| CmdError::refused(format!("No symbol \"{}\". Type List to see them.", key.trim())))
}

/// A device property value as stored: trimmed; `location` with a leading `+`,
/// `function` with a leading `=`; empty clears it. Tags go through [`ecad_model::naming::tag::normalise`].
///
/// # Errors
/// T6: too long or a control character.
pub(crate) fn property_value(doc: &Document, key: &str, value: Option<&str>) -> Result<Option<String>, CmdError> {
    let Some(v) = value.map(str::trim).filter(|v| !v.is_empty()) else { return Ok(None) };
    if key == TAG_KEY {
        return tag::normalise(v, crate::defaults::print::tag_style(doc)).map(Some).map_err(CmdError::from);
    }
    if v.chars().count() > MAX_PROPERTY_CHARS || v.chars().any(char::is_control) {
        return Err(CmdError::refused(format!("`{key}` can be at most {MAX_PROPERTY_CHARS} characters, with no control characters")));
    }
    Ok(Some(match key {
        "location" if !v.starts_with('+') => format!("+{v}"),
        "function" if !v.starts_with('=') => format!("={v}"),
        _ => v.to_string(),
    }))
}
