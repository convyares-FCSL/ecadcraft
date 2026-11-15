//! `plcimport` and `plcexport` (brief 056): reading and writing tag files. The formats and the
//! plan are [`crate::plc::exchange`] (bytes in, bytes out).

use cadcraft_engine::Session;

use crate::commands::{err, opt_typed, with_any_page, with_project};
use crate::plc::exchange::*;
use crate::plc::tagtable::{self, Read, TagFormat};
use serde_json::{Value, json};

/// The format named by `format`, else the generic CSV.
fn format_param(params: &Value) -> Result<&'static dyn TagFormat, String> {
    match opt_typed(params, "format", "a format ID (csv)", Value::as_str)? {
        None => Ok(&tagtable::GenericCsv),
        Some(id) => tagtable::format(id).ok_or_else(|| format!("No tag table format \"{id}\"; the formats are: csv.")),
    }
}

/// The file name of `path` (no directory, house rule 10).
fn file_name(path: &str) -> String {
    std::path::Path::new(path).file_name().map_or_else(|| "a file".to_string(), |n| n.to_string_lossy().into_owned())
}

/// The bytes of `{path}` or `{text}` (at most [`tagtable::MAX_BYTES`]), and the source's name.
fn input_bytes(params: &Value) -> Result<(Vec<u8>, String), String> {
    if let Some(t) = opt_typed(params, "text", "the tag table as text", Value::as_str)? {
        if t.len() > tagtable::MAX_BYTES {
            return Err(format!("The text is {} MB; at most {} MB is read.", t.len() >> 20, tagtable::MAX_BYTES >> 20));
        }
        let name = match opt_typed(params, "name", "a file name", Value::as_str)? {
            None => "pasted text".to_string(),
            Some(n) if n.trim().is_empty() || n.chars().count() > MAX_NAME || n.chars().any(char::is_control) => {
                return Err(format!("`name` is 1 to {MAX_NAME} characters, with no control characters."));
            }
            Some(n) => file_name(n),
        };
        return Ok((t.as_bytes().to_vec(), name));
    }
    let path = opt_typed(params, "path", "a file path", Value::as_str)?.ok_or("Give `path` (a file) or `text` (the tag table).")?;
    read_file(path).map(|b| (b, file_name(path)))
}

#[cfg(not(target_arch = "wasm32"))]
fn read_file(path: &str) -> Result<Vec<u8>, String> {
    use std::io::Read as _;
    let f = std::fs::File::open(path).map_err(|e| format!("{} could not be read: {e}.", file_name(path)))?;
    let mut buf = Vec::new();
    // One byte over the cap says the file is too large without reading all of it.
    let cap = u64::try_from(tagtable::MAX_BYTES).unwrap_or(u64::MAX).saturating_add(1);
    f.take(cap).read_to_end(&mut buf).map_err(|e| format!("{} could not be read: {e}.", file_name(path)))?;
    if buf.len() > tagtable::MAX_BYTES {
        return Err(format!(
            "{} is larger than {} MB; at most {} MB is read.",
            file_name(path),
            tagtable::MAX_BYTES >> 20,
            tagtable::MAX_BYTES >> 20
        ));
    }
    Ok(buf)
}

#[cfg(target_arch = "wasm32")]
fn read_file(_path: &str) -> Result<Vec<u8>, String> {
    Err("The web version reads no files; paste the tag table as `text`.".into())
}

#[cfg(not(target_arch = "wasm32"))]
fn write_file(path: &str, bytes: &[u8]) -> Result<(), String> {
    std::fs::write(path, bytes).map_err(|e| format!("{} could not be written: {e}.", file_name(path)))
}

#[cfg(target_arch = "wasm32")]
fn write_file(_path: &str, _bytes: &[u8]) -> Result<(), String> {
    Err("The web version writes no files; leave out `path` to get the text.".into())
}

/// [`read_tags`] of the file at `path` (at most [`tagtable::MAX_BYTES`]), and its file name. The
/// web version reads no files.
///
/// # Errors
/// As [`read_tags`], or the file cannot be read.
pub fn read_file_tags(path: &str) -> Result<(Read, String), String> {
    let bytes = read_file(path)?;
    Ok((read_tags(&bytes, None)?, file_name(path)))
}

/// `plcimport {path | text, name?, format?, dry_run?, what?: both|tags|descriptions}`: fill the
/// channels' tags and descriptions from an I/O list, one way, in one transaction.
pub(crate) fn run_plcimport(s: &mut Session, params: &Value) -> cadcraft_engine::Result<Value> {
    let dry = opt_typed(params, "dry_run", "true or false", Value::as_bool).map_err(err)?.unwrap_or(false);
    let (tags, texts) = match opt_typed(params, "what", "both, tags or descriptions", Value::as_str).map_err(err)? {
        None | Some("both") => (true, true),
        Some("tags") => (true, false),
        Some("descriptions") => (false, true),
        Some(_) => return Err(err("`what` is both, tags or descriptions")),
    };
    let (bytes, source) = input_bytes(params).map_err(err)?;
    let format = match params.get("format").filter(|v| !v.is_null()) {
        Some(_) => Some(format_param(params).map_err(err)?),
        None => None,
    };
    let read = read_tags(&bytes, format).map_err(err)?;
    let counts = json!({"not_io": read.not_io, "bad_address": read.bad, "bad_name": read.bad_name, "twice": read.twice});
    let plan = |p: &mut crate::project::Project| -> Result<Fill, String> {
        p.outputs();
        let o = p.outputs.as_ref().ok_or("The PLC model is not available.")?;
        plan_fill(p.ledger.document(), &o.plc, &read.rows, tags, texts)
    };
    let report = |fill: &Fill, written: bool| {
        let lead = if written { format!("Imported {source}") } else { format!("{source} would set") };
        let words = if written { fill.words() } else { fill.words().replacen(" set on ", " on ", 1) };
        json!({
            "read": read.total,
            "io": read.rows.len(),
            "skipped": counts,
            "tags": fill.tags,
            "descriptions": fill.descriptions,
            "unchanged": fill.unchanged,
            "no_card": fill.no_card.iter().map(|a| tagtable::percent(a)).collect::<Vec<_>>(),
            "written": written,
            "message": format!("{lead}: {words} {}", read.words()),
        })
    };
    if dry {
        let fill = with_any_page(s, |p, _| plan(p))?;
        return Ok(report(&fill, false));
    }
    let v = with_project(s, |p, pg, _| {
        let fill = plan(p)?;
        if !fill.ops.is_empty() {
            p.apply_edit(pg, "plcimport", IMPORT_LABEL, fill.ops.clone())?;
        }
        Ok(report(&fill, !fill.ops.is_empty()))
    })?;
    if let Some(m) = v.get("message").and_then(Value::as_str) {
        s.echo(m.to_string());
    }
    Ok(v)
}

/// `plcexport {format?, rack?, path?}` (design 056 §6.6): the tag table of a rack (or every rack);
/// writes the file, or returns `{text, rows, left_out}` without `path`. Changes no project data.
pub(crate) fn run_plcexport(s: &mut Session, params: &Value) -> cadcraft_engine::Result<Value> {
    let f = format_param(params).map_err(err)?;
    let path = opt_typed(params, "path", "a file path", Value::as_str).map_err(err)?.map(str::to_string);
    let v = with_any_page(s, |p, _| {
        p.outputs();
        let o = p.outputs.as_ref().ok_or("The PLC model is not available.")?;
        let doc = p.ledger.document();
        let rack = match params.get("rack").filter(|v| !v.is_null()) {
            Some(r) => Some(crate::plc::rack::rack_param(doc, r)?),
            None => None,
        };
        let ex = export_rows(doc, &o.plc, rack, f);
        let bytes = f.write(&ex.rows)?;
        let scope = rack.map_or_else(|| "every rack".to_string(), |r| format!("rack {}", crate::devices::tag_words(doc, r)));
        let n = ex.rows.len();
        let left = ex.left_out_words();
        let left_out = json!({"no_tag": ex.no_tag, "no_address": ex.no_address, "refused": ex.refused});
        let tags = if n == 1 { "tag" } else { "tags" };
        let tail = if left.is_empty() { ".".to_string() } else { format!("; {left}.") };
        match &path {
            Some(path) => {
                write_file(path, &bytes)?;
                let message = format!("Wrote {n} {tags} of {scope} to {}{tail}", file_name(path));
                Ok(json!({"rows": n, "left_out": left_out, "file": file_name(path), "format": f.id(), "message": message}))
            }
            None => {
                let message = format!("Made {n} {tags} of {scope}{tail}");
                let text = String::from_utf8(bytes).map_err(|_| "The tag table is not text.".to_string())?;
                Ok(json!({"rows": n, "left_out": left_out, "text": text, "format": f.id(), "message": message}))
            }
        }
    })?;
    if let Some(m) = v.get("message").and_then(Value::as_str) {
        s.echo(m.to_string());
    }
    Ok(v)
}

/// The commands of brief 056.
pub fn specs() -> Vec<cadcraft_engine::CommandSpec> {
    use cadcraft_engine::CommandSpec;
    vec![
        CommandSpec::new("plcmaketags", "Make PLC Tags", super::signals::run_plcmaketags)
            .params("{card: id or tag} or {rack: id or tag} (fill the empty tags of used and reserved channels from their descriptions)")
            .noundo(),
        CommandSpec::new("plcexport", "Export PLC Tags", run_plcexport)
            .params("{format?: csv, rack?: id or tag (absent: every rack), path?: file} (without path: {text, rows, left_out}; changes nothing)")
            .noundo(),
        CommandSpec::new("plcimport", "Import an I/O List", run_plcimport)
            .params(
                "{path: file | text: the I/O list, name?: its file name (with text, at most 255 characters), format?: csv, dry_run?: bool, \
what?: both|tags|descriptions} (sets the tags and descriptions of the channels at the list's addresses, one way, in one transaction; \
dry_run writes nothing)",
            )
            .noundo(),
        CommandSpec::new("plccardtype", "Card Type", super::rack::run_plccardtype)
            .params("{card: id or tag, type: DI|DQ|AI|AO|RTD|IOL} (changes what an I/O card is: its body symbol, one transaction)")
            .noundo(),
    ]
}
