//! `plcimport` and `plcexport`: reading and writing tag files. The formats and the
//! plan are [`crate::plc::exchange`] (bytes in, bytes out); both commands are generic
//! ([`crate::commands::run`]) and declare the file they read or write.

use schemars::JsonSchema;
use serde::Deserialize;
use serde_json::json;

use crate::commands::run::{Access, Cx, Generic, Io, Outcome, Pages, Read as FileRead};
use crate::error::CmdError;
use crate::plc::exchange::*;
use crate::plc::tagtable::{self, TagFormat};

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

/// The bytes of the file (read by the runner, at most [`tagtable::MAX_BYTES`]) or of `text`, and
/// the source's name.
fn input_bytes(cx: &Cx<'_>, path: Option<&str>, text: Option<&str>, name: Option<&str>) -> Result<(Vec<u8>, String), CmdError> {
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
    let path = path.map(str::trim).ok_or(CmdError::input("Give `path` (a file) or `text` (the tag table)."))?;
    let unread = |e: &dyn std::fmt::Display| CmdError::refused(format!("{} could not be read: {e}.", file_name(path)));
    match cx.read.get("path") {
        Some(Ok(crate::commands::run::Got::Bytes(b))) if b.len() > tagtable::MAX_BYTES => Err(CmdError::refused(format!(
            "{} is larger than {} MB; at most {} MB is read.",
            file_name(path),
            tagtable::MAX_BYTES >> 20,
            tagtable::MAX_BYTES >> 20
        ))),
        Some(Ok(crate::commands::run::Got::Bytes(b))) => Ok((b.clone(), file_name(path))),
        Some(Ok(_)) | None => Err(unread(&"it was not read")),
        Some(Err(e)) => Err(unread(&e.0)),
    }
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
pub struct PlcimportParams {
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

impl Generic for PlcimportParams {
    const ID: &'static str = "plcimport";
    const TITLE: &'static str = "Import an I/O List";
    const HELP: &'static str = "{path: file | text: the I/O list, name?: its file name (with text, at most 255 characters), format?: csv, dry_run?: bool, what?: both|tags|descriptions} (sets the tags and descriptions of the channels at the list's addresses, one way, in one transaction; dry_run writes nothing)";
    const PAGES: Pages = Pages::ReadAnywhere;
    const IO: Io = Io { reads: &[("path", FileRead::Bytes(tagtable::MAX_BYTES))], writes: &[] };

    fn run(self, cx: &mut Cx<'_>) -> Result<Outcome, CmdError> {
        let dry = self.dry_run.unwrap_or(false);
        let (tags, texts) = match self.what {
            None | Some(WhatParam::Both) => (true, true),
            Some(WhatParam::Tags) => (true, false),
            Some(WhatParam::Descriptions) => (false, true),
        };
        let (bytes, source) = input_bytes(cx, self.path.as_deref(), self.text.as_deref(), self.name.as_deref())?;
        let format = match self.format.as_deref() {
            Some(id) => Some(format_named(Some(id))?),
            None => None,
        };
        let read = read_tags(&bytes, format)?;
        let counts = json!({"not_io": read.not_io, "bad_address": read.bad, "bad_name": read.bad_name, "twice": read.twice});
        let doc = cx.doc;
        let fill = plan_fill(doc, &cx.derived.reports(doc).plc, &read.rows, tags, texts)?;
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
            return Ok(Outcome::answer(report(&fill, false)));
        }
        let answer = report(&fill, !fill.ops.is_empty());
        Ok(Outcome::edit_if_any(IMPORT_LABEL, fill.ops, answer).echo_message())
    }
}

/// `plcexport {format?, rack?, path?}`: the tag table of a rack (or every rack);
/// writes the file, or returns `{text, rows, left_out}` without `path`. Changes no project data.
#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct PlcexportParams {
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

impl Generic for PlcexportParams {
    const ID: &'static str = "plcexport";
    const TITLE: &'static str = "Export PLC Tags";
    const HELP: &'static str =
        "{format?: csv, rack?: id or tag (absent: every rack), path?: file} (without path: {text, rows, left_out}; changes nothing)";
    const PAGES: Pages = Pages::Any;
    const ACCESS: Access = Access::Reads;
    const IO: Io = Io { reads: &[], writes: &["path"] };

    fn run(self, cx: &mut Cx<'_>) -> Result<Outcome, CmdError> {
        let f = format_named(self.format.as_deref())?;
        let doc = cx.doc;
        let o = cx.derived.reports(doc);
        let rack = self.rack.as_deref().map(|r| crate::plc::rack::rack_named(doc, r)).transpose()?;
        let ex = export_rows(doc, &o.plc, rack, f);
        let bytes = f.write(&ex.rows)?;
        let scope = rack.map_or_else(|| "every rack".to_string(), |r| format!("rack {}", crate::devices::tags::tag_words(doc, r)));
        let n = ex.rows.len();
        let left = ex.left_out_words();
        let left_out = json!({"no_tag": ex.no_tag, "no_address": ex.no_address, "refused": ex.refused});
        let tags = if n == 1 { "tag" } else { "tags" };
        let tail = if left.is_empty() { ".".to_string() } else { format!("; {left}.") };
        match self.path.as_deref().map(str::trim) {
            Some(path) => {
                let message = format!("Wrote {n} {tags} of {scope} to {}{tail}", file_name(path));
                let mut out =
                    Outcome::answer(json!({"rows": n, "left_out": left_out, "file": file_name(path), "format": f.id(), "message": message}))
                        .echo_message();
                out.writes.push(("path", bytes));
                Ok(out)
            }
            None => {
                let message = format!("Made {n} {tags} of {scope}{tail}");
                let text = String::from_utf8(bytes).map_err(|_| CmdError::refused("The tag table is not text."))?;
                Ok(Outcome::answer(json!({"rows": n, "left_out": left_out, "text": text, "format": f.id(), "message": message})).echo_message())
            }
        }
    }
}
