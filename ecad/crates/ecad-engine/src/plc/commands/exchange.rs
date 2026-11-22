//! `plcimport` and `plcexport` (brief 056): reading and writing tag files. The formats and the
//! plan are [`crate::plc::exchange`] (bytes in, bytes out).

use cadcraft_engine::Session;
use ecad_electrical::CmdError;
use schemars::JsonSchema;
use serde::Deserialize;

use crate::commands::input::Command;
use crate::commands::{err, on_any_page, on_project};
use crate::plc::exchange::*;
use crate::plc::tagtable::{self, Read, TagFormat};
use serde_json::{Value, json};

/// The format named `id`, else the generic CSV.
fn format_named(id: Option<&str>) -> Result<&'static dyn TagFormat, CmdError> {
    match id {
        None => Ok(&tagtable::GenericCsv),
        Some(id) => tagtable::format(id).ok_or_else(|| CmdError::refused(format!("No tag table format \"{id}\"; the formats are: csv."))),
    }
}

/// The file name of `path` (no directory, house rule 10).
fn file_name(path: &str) -> String {
    std::path::Path::new(path).file_name().map_or_else(|| "a file".to_string(), |n| n.to_string_lossy().into_owned())
}

/// The bytes of `path` or `text` (at most [`tagtable::MAX_BYTES`]), and the source's name.
fn input_bytes(path: Option<&str>, text: Option<&str>, name: Option<&str>) -> Result<(Vec<u8>, String), CmdError> {
    if let Some(t) = text {
        if t.len() > tagtable::MAX_BYTES {
            return Err(CmdError::refused(format!("The text is {} MB; at most {} MB is read.", t.len() >> 20, tagtable::MAX_BYTES >> 20)));
        }
        let name = match name {
            None => "pasted text".to_string(),
            Some(n) if n.trim().is_empty() || n.chars().count() > MAX_NAME || n.chars().any(char::is_control) => {
                return Err(CmdError::refused(format!("`name` is 1 to {MAX_NAME} characters, with no control characters.")));
            }
            Some(n) => file_name(n),
        };
        return Ok((t.as_bytes().to_vec(), name));
    }
    let path = path.ok_or(CmdError::input("Give `path` (a file) or `text` (the tag table)."))?;
    read_file(path).map(|b| (b, file_name(path)))
}

fn read_file(path: &str) -> Result<Vec<u8>, CmdError> {
    use std::io::Read as _;
    let unread = |e: std::io::Error| CmdError::refused(format!("{} could not be read: {e}.", file_name(path)));
    let f = std::fs::File::open(path).map_err(unread)?;
    let mut buf = Vec::new();
    // One byte over the cap says the file is too large without reading all of it.
    let cap = u64::try_from(tagtable::MAX_BYTES).unwrap_or(u64::MAX).saturating_add(1);
    f.take(cap).read_to_end(&mut buf).map_err(unread)?;
    if buf.len() > tagtable::MAX_BYTES {
        return Err(CmdError::refused(format!(
            "{} is larger than {} MB; at most {} MB is read.",
            file_name(path),
            tagtable::MAX_BYTES >> 20,
            tagtable::MAX_BYTES >> 20
        )));
    }
    Ok(buf)
}

fn write_file(path: &str, bytes: &[u8]) -> Result<(), CmdError> {
    std::fs::write(path, bytes).map_err(|e| CmdError::refused(format!("{} could not be written: {e}.", file_name(path))))
}

/// [`read_tags`] of the file at `path` (at most [`tagtable::MAX_BYTES`]), and its file name.
///
/// # Errors
/// As [`read_tags`], or the file cannot be read.
pub fn read_file_tags(path: &str) -> Result<(Read, String), CmdError> {
    let bytes = read_file(path)?;
    Ok((read_tags(&bytes, None)?, file_name(path)))
}

/// Which columns `plcimport` fills.
#[derive(Deserialize, JsonSchema, Clone, Copy)]
#[serde(rename_all = "lowercase")]
enum WhatParam {
    /// Tags and descriptions.
    Both,
    /// Only the tags.
    Tags,
    /// Only the descriptions.
    Descriptions,
}

/// `plcimport {path | text, name?, format?, dry_run?, what?: both|tags|descriptions}`: fill the
/// channels' tags and descriptions from an I/O list, one way, in one transaction.
#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub(crate) struct PlcimportParams {
    /// The I/O list file.
    #[serde(default)]
    path: Option<String>,
    /// The I/O list as text (instead of `path`).
    #[serde(default)]
    text: Option<String>,
    /// With `text`: its file name (at most 255 characters).
    #[serde(default)]
    name: Option<String>,
    /// The format: csv (absent: read from the file).
    #[serde(default)]
    format: Option<String>,
    /// Say what it would set; write nothing.
    #[serde(default)]
    dry_run: Option<bool>,
    /// Which columns to fill.
    #[serde(default)]
    what: Option<WhatParam>,
}

impl Command for PlcimportParams {
    fn run(self, s: &mut Session) -> cadcraft_engine::Result<Value> {
        let dry = self.dry_run.unwrap_or(false);
        let (tags, texts) = match self.what {
            None | Some(WhatParam::Both) => (true, true),
            Some(WhatParam::Tags) => (true, false),
            Some(WhatParam::Descriptions) => (false, true),
        };
        let (bytes, source) = input_bytes(self.path.as_deref(), self.text.as_deref(), self.name.as_deref()).map_err(|e| err(e.to_string()))?;
        let format = match self.format.as_deref() {
            Some(id) => Some(format_named(Some(id)).map_err(|e| err(e.to_string()))?),
            None => None,
        };
        let read = read_tags(&bytes, format).map_err(err)?;
        let counts = json!({"not_io": read.not_io, "bad_address": read.bad, "bad_name": read.bad_name, "twice": read.twice});
        let plan = |p: &mut crate::project::Project| -> Result<Fill, CmdError> {
            p.outputs();
            let o = p.kept_reports().ok_or(CmdError::refused("The PLC model is not available."))?;
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
            let fill = on_any_page(s, |p, _| plan(p))?;
            return Ok(report(&fill, false));
        }
        let v = on_project(s, |p, pg, _| {
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
}

/// `plcexport {format?, rack?, path?}` (design 056 §6.6): the tag table of a rack (or every rack);
/// writes the file, or returns `{text, rows, left_out}` without `path`. Changes no project data.
#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub(crate) struct PlcexportParams {
    /// The format: csv.
    #[serde(default)]
    format: Option<String>,
    /// Only this rack: an ID or a tag (absent: every rack).
    #[serde(default)]
    rack: Option<String>,
    /// The file to write (absent: the text is returned).
    #[serde(default)]
    path: Option<String>,
}

impl Command for PlcexportParams {
    fn run(self, s: &mut Session) -> cadcraft_engine::Result<Value> {
        let f = format_named(self.format.as_deref()).map_err(|e| err(e.to_string()))?;
        let path = self.path;
        let v = on_any_page(s, |p, _| {
            p.outputs();
            let o = p.kept_reports().ok_or(CmdError::refused("The PLC model is not available."))?;
            let doc = p.ledger.document();
            let rack = self.rack.as_deref().map(|r| crate::plc::rack::rack_named(doc, r)).transpose()?;
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
                    let text = String::from_utf8(bytes).map_err(|_| CmdError::refused("The tag table is not text."))?;
                    Ok(json!({"rows": n, "left_out": left_out, "text": text, "format": f.id(), "message": message}))
                }
            }
        })?;
        if let Some(m) = v.get("message").and_then(Value::as_str) {
            s.echo(m.to_string());
        }
        Ok(v)
    }
}

/// The commands of brief 056.
pub fn specs() -> Vec<cadcraft_engine::CommandSpec> {
    use crate::commands::input::typed;
    use cadcraft_engine::CommandSpec;
    vec![
        CommandSpec::new("plcmaketags", "Make PLC Tags", typed::<crate::plc::commands::signals::PlcmaketagsParams>)
            .params("{card: id or tag} or {rack: id or tag} (fill the empty tags of used and reserved channels from their descriptions)")
            .noundo(),
        CommandSpec::new("plcexport", "Export PLC Tags", typed::<PlcexportParams>)
            .params("{format?: csv, rack?: id or tag (absent: every rack), path?: file} (without path: {text, rows, left_out}; changes nothing)")
            .noundo(),
        CommandSpec::new("plcimport", "Import an I/O List", typed::<PlcimportParams>)
            .params(
                "{path: file | text: the I/O list, name?: its file name (with text, at most 255 characters), format?: csv, dry_run?: bool, \
what?: both|tags|descriptions} (sets the tags and descriptions of the channels at the list's addresses, one way, in one transaction; \
dry_run writes nothing)",
            )
            .noundo(),
        CommandSpec::new("plccardtype", "Card Type", typed::<crate::plc::commands::rack::PlccardtypeParams>)
            .params("{card: id or tag, type: DI|DQ|AI|AO|RTD|IOL} (changes what an I/O card is: its body symbol, one transaction)")
            .noundo(),
    ]
}
