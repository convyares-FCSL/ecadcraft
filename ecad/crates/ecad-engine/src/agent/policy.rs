//! What an agent may run through `execute` (brief 011, scope item 4).
//!
//! # Semantics
//! - **Writes off** (no `--write`, or the project opened read-only): only the commands in
//!   [`READ_COMMANDS`] run; every other command is refused before it reaches the engine, so the
//!   head never moves.
//! - **Writes on:** every command runs through the engine exactly as for a person (ECAD's owner
//!   refuses or records it), except the file and session commands in [`NEVER_COMMANDS`]: opening,
//!   creating, closing and saving under another name stay with the person who started the server,
//!   and nothing reads or writes other files. Saving the project is the `save` tool.
//! - Parameters naming a file ([`FILE_KEYS`]) are refused in every mode, whatever the command.

use serde_json::Value;

/// Session commands that change nothing in the project or its files, allowed with writes off. A
/// generic command says so in its declaration instead ([`is_read_command`]).
pub(crate) const READ_COMMANDS: &[&str] = &[
    "ecadinspect",
    "entities",
    "texts",
    "shapes",
    "usage",
    "loops",
    "drawing.inspect",
    "getvar",
    // Navigation between the project's pages: opens or shows a page, records nothing.
    "openpage",
    "document.switch",
    // Cable references (brief 010): going to the other end opens or shows its page and selects
    // it, and records nothing.
    "cablegoto",
    // Devices and starter symbols (brief 018): finding and listing them, and one device's
    // properties, read the project only.
    "symbols",
    "devices",
    "deviceprops",
    // Nets, wire numbers and checks (brief 022): listing them reads the project.
    "nets",
    "net",
    "checks",
    // Cross-references (brief 025): going to a reference opens or shows its page and records
    // nothing.
    "refgoto",
    // Brief 019: an issued revision opened read-only is a session view; nothing is recorded.
    "openrevision",
    "closerevision",
    // Brief 020a: the History graph, steps, a point's details, the branch list, and a point
    // shown read-only, read the project or open a session view; nothing is recorded.
    "history",
    "steps",
    "pointinfo",
    "branches",
    "showpoint",
    "closepoint",
    // Brief 024: the reports and the printed book read the project and record nothing (a `path`
    // is refused in every mode, as for `plot` and `export`: `printbook` then returns the PDF as
    // base64 data); `reports` only asks the desktop app to open its window.
    "printbook",
    "reports",
    // Brief 027: the pre-release check reads the project and records nothing.
    "prerelease",
    // Brief 031: Show Hidden Text is a display state (like a cable view) and records nothing.
    "hiddentext",
    // Brief 048: what an edit of a symbol would change on its placed copies reads the project.
    "symbolimpact",
];

/// Commands whose form with no fields only reads (brief 019): `pageprops {page?}` and
/// `projectprops {}`; brief 026: `numbering {}`; brief 031: `textstyle {}`.
const READ_WITHOUT_FIELDS: &[(&str, &[&str])] = &[
    ("pageprops", &["page"]),
    ("projectprops", &[]),
    ("numbering", &[]),
    ("textstyle", &[]),
    ("standard", &[]),
    // Brief 055: `titleblock {id, sheet?}` reads one frame and its block.
    ("titleblock", &["id", "sheet"]),
    // Brief 054: `folderprops {folder}` reads a folder's start number.
    ("folderprops", &["folder"]),
    // Brief 052d: `conflicts {}` lists the waiting conflicts.
    ("conflicts", &[]),
];

/// Commands whose form with `preview: true` only reads (brief 020b: the merge preview; brief 026:
/// the Renumber and Finalise previews).
const READ_WITH_PREVIEW: &[&str] = &["merge", "renumber", "finalise"];

/// Commands whose form naming an issued revision records nothing (brief 027: `release {rev}`
/// writes that revision's folder again, beside the project, and never issues; it refuses a `rev`
/// that is not issued). Its `folder` is a file key, refused in every mode.
const READ_WITH_REV: &[&str] = &["release"];

/// Commands whose form with `dry_run: true` only reads (brief 056: what an import would set).
const READ_WITH_DRY_RUN: &[&str] = &["plcimport"];

/// Whether command `id` only reads, whatever its parameters: a session command in
/// [`READ_COMMANDS`], or a generic command that declares it.
pub fn is_read_command(id: &str) -> bool {
    READ_COMMANDS.contains(&id) || crate::commands::declared_read(id)
}

/// Every command that only reads, whatever its parameters ([`is_read_command`]).
pub fn read_commands() -> Vec<&'static str> {
    READ_COMMANDS.iter().copied().chain(crate::commands::declared_reads()).collect()
}

/// Whether `id` with `params` only reads ([`READ_WITHOUT_FIELDS`], [`READ_WITH_PREVIEW`],
/// [`READ_WITH_REV`], [`READ_WITH_DRY_RUN`]).
pub fn reads_only(id: &str, params: &Value) -> bool {
    READ_WITHOUT_FIELDS.iter().any(|(c, keep)| *c == id && params.as_object().is_none_or(|o| o.keys().all(|k| keep.contains(&k.as_str()))))
        || (READ_WITH_PREVIEW.contains(&id) && params.get("preview").and_then(Value::as_bool) == Some(true))
        || (READ_WITH_DRY_RUN.contains(&id) && params.get("dry_run").and_then(Value::as_bool) == Some(true))
        || (READ_WITH_REV.contains(&id)
            && params.get("rev").is_some_and(Value::is_string)
            // Replace moves a folder on disk aside: a write, never with writes off (review of PR #54, 1).
            && params.get("existing").and_then(Value::as_str).is_none_or(|e| e.trim() != "replace"))
}

/// Commands an agent never runs: files, documents and the process. The project is opened by the
/// person who starts the server; saving is the `save` tool (only with `--write`).
pub const NEVER_COMMANDS: &[&str] = &[
    "open",
    "new",
    "close",
    "closeall",
    "qsave",
    "saveas",
    "recover",
    "drawingrecovery",
    "import",
    "export",
    "exportpdf",
    "plot",
    "plotwindow",
    "publish",
    "pdfbook",
    "preview",
    "wblock",
    "xref",
    "-xref",
    "xattach",
    "attach",
    "xopen",
    "xbind",
    "layout.saveas",
    "tableexport",
    "compare",
    "compareexport",
    "compareclose",
    "dataextraction",
    "datalink",
    "datalinkupdate",
    "eattext",
    "script",
    "options",
    "aliasedit",
    "reinit",
    "plottermanager",
    "-plottermanager",
    "stylesmanager",
    "share",
    // AUDIT rewrites the drawing outside ECAD's commands; its log setting writes a file.
    "audit",
    // Brief 035: an agent never turns its own access on, widens it or moves the port.
    "agentaccess",
];

/// Said when an agent asks for `agentaccess` (brief 035, design §3.3).
pub const AGENT_ACCESS: &str = "Refused: Agent access is set by the person in the ECAD window. Nothing was changed.";

/// Parameter names that name a file on disk.
pub(crate) const FILE_KEYS: &[&str] = &["path", "file", "patternFile", "template", "folder", "output"];

/// Commands whose `path` parameter is not a file (ARRAYPATH's path is a handle).
const PATH_IS_NOT_A_FILE: &[&str] = &[
    "arraypath",
    // Brief 019: `folder` names a folder of pages, never a file.
    "newpage",
    "movepage",
    "newfolder",
    "renamefolder",
    "movefolder",
    "deletefolder",
    // Brief 054: a folder's start number, and Renumber pages in one folder.
    "folderprops",
    "renumberpages",
];

/// Said when one call asks for several undo or redo steps.
pub fn one_step(id: &str) -> String {
    format!(
        "Refused: {} takes one step per call here (no count above 1, no `mode: back`, no `all`): each call may record \
         at most one transaction. Call it once per step. Nothing was changed.",
        id.to_ascii_uppercase()
    )
}

/// Whether `params` asks UNDO, REDO or MREDO for more than one step (a count above 1, or one
/// that is not a plain number; UNDO's `back` mode; MREDO's `all`).
fn several_steps(id: &str, params: &Value) -> bool {
    if !matches!(id, "undo" | "redo" | "mredo" | "u") {
        return false;
    }
    let count = params.get("count").filter(|v| !v.is_null());
    let counted = count.is_some_and(|c| c.as_f64().is_none_or(|n| n.is_nan() || n > 1.0));
    let back = id == "undo" && params.get("mode").and_then(Value::as_str).is_some_and(|m| m.eq_ignore_ascii_case("back"));
    let all = id == "mredo" && params.get("all").is_some_and(|v| !v.is_null() && v != &Value::Bool(false));
    counted || back || all
}

/// Whether SETVAR is asked to set AUDITCTL (which makes AUDIT write a log file).
fn sets_auditctl(id: &str, params: &Value) -> bool {
    id == "setvar" && params.get("name").and_then(Value::as_str).is_some_and(|n| n.trim().eq_ignore_ascii_case("auditctl"))
}

/// Said when a write is refused because the server was started without `--write`.
pub const WRITES_OFF: &str = "Refused: this ECAD MCP server is read-only (started without --write). \
Nothing was changed. Restart it with --write to let the agent edit and save the project.";

/// Said when the command is refused in every mode.
pub fn never(id: &str) -> String {
    if id == "agentaccess" {
        return AGENT_ACCESS.into();
    }
    format!(
        "Refused: {} is not available to agents. Opening, creating, closing and saving projects under another name, \
         and reading or writing other files, stay with the person who started the server; save the project with the \
         `save` tool. Nothing was changed.",
        id.to_ascii_uppercase()
    )
}

/// Said when the project opened read-only (`why` from the file layer).
pub(crate) fn project_read_only(why: &str) -> String {
    format!("Refused: the project opened read-only ({why}). Nothing was changed.")
}

/// Said when a parameter names a file.
pub(crate) fn file_key(id: &str, key: &str) -> String {
    format!(
        "Refused: `{key}` names a file; agents cannot read or write files through {} (save the project with the `save` \
         tool). Nothing was changed.",
        id.to_ascii_uppercase()
    )
}

/// Whether the agent may run command `id` (its canonical id) with `params`. `writes` says whether
/// writes are on; `read_only` is why the project cannot be written, if it cannot.
///
/// # Errors
/// The refusal, in words, when the command must not run.
pub fn check(id: &str, params: &Value, writes: bool, read_only: Option<&str>) -> Result<(), String> {
    if let Some(o) = params.as_object()
        && !PATH_IS_NOT_A_FILE.contains(&id)
        && let Some(k) = FILE_KEYS.iter().find(|k| o.get(**k).is_some_and(|v| !v.is_null()))
    {
        return Err(file_key(id, k));
    }
    if let Some(k) = crate::commands::declared_file_keys(id).find(|k| params.get(*k).is_some_and(|v| !v.is_null())) {
        return Err(file_key(id, k));
    }
    if NEVER_COMMANDS.contains(&id) {
        return Err(never(id));
    }
    if sets_auditctl(id, params) {
        return Err(never("setvar auditctl"));
    }
    if is_read_command(id) || reads_only(id, params) {
        return Ok(());
    }
    if !writes {
        return Err(WRITES_OFF.into());
    }
    if let Some(why) = read_only {
        return Err(project_read_only(why));
    }
    if several_steps(id, params) {
        return Err(one_step(id));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn the_rules() {
        let none = json!({});
        assert!(check("ecadinspect", &none, false, None).is_ok());
        for id in ["cablereferences", "cablegoto", "openpage"] {
            assert!(check(id, &json!({"reference": "1"}), false, None).is_ok(), "{id} reads");
            assert!(check(id, &json!({}), false, Some("locked")).is_ok(), "{id} reads a read-only project");
        }
        for id in ["cablereference", "cablecontinue", "cablecore", "cablefan", "renamepage", "movepage"] {
            assert_eq!(check(id, &none, false, None), Err(WRITES_OFF.into()), "{id} writes");
        }
        for (id, p) in [
            ("undo", json!({"count": 3})),
            ("undo", json!({"count": "2"})),
            ("undo", json!({"mode": "back"})),
            ("redo", json!({"count": 2})),
            ("mredo", json!({"count": 5})),
            ("mredo", json!({"all": true})),
        ] {
            assert_eq!(check(id, &p, true, None), Err(one_step(id)), "{id} {p}");
        }
        for (id, p) in [("undo", json!({"count": 1})), ("redo", json!({})), ("u", json!({})), ("mredo", json!({"all": false}))] {
            assert!(check(id, &p, true, None).is_ok(), "{id} {p}");
        }
        assert!(check("audit", &none, true, None).is_err());
        assert!(check("setvar", &json!({"name": "AuditCtl", "value": 1}), true, None).is_err());
        assert!(check("setvar", &json!({"name": "osmode", "value": 1}), true, None).is_ok());
        assert!(check("wire", &none, false, None).is_err());
        assert!(check("wire", &none, true, None).is_ok());
        assert!(check("wire", &none, true, Some("locked")).is_err());
        assert!(check("saveas", &none, true, None).is_err());
        assert_eq!(check("agentaccess", &json!({"mode": "write"}), true, None), Err(AGENT_ACCESS.into()));
        assert!(check("line", &json!({"path": "x.dxf"}), true, None).is_err());
        assert!(check("arraypath", &json!({"path": "1A"}), true, None).is_ok());
        assert!(check("entities", &json!({"path": null}), false, None).is_ok());
        // Brief 027: the check reads; `release` issues only without `rev`; no folder of its own.
        assert!(check("prerelease", &none, false, Some("locked")).is_ok());
        assert!(check("release", &json!({"rev": "A"}), false, Some("locked")).is_ok());
        assert_eq!(check("release", &none, false, None), Err(WRITES_OFF.into()));
        assert_eq!(check("release", &json!({"rev": null}), false, None), Err(WRITES_OFF.into()));
        assert_eq!(check("release", &json!({"rev": "A", "existing": "replace"}), false, None), Err(WRITES_OFF.into()));
        assert!(check("release", &json!({"rev": "A", "existing": "keep-both"}), false, None).is_ok());
        assert!(check("release", &json!({"rev": "A", "existing": "replace"}), true, None).is_ok());
        assert!(check("release", &none, true, None).is_ok());
        assert_eq!(check("release", &json!({"folder": "x"}), true, None), Err(file_key("release", "folder")));
    }
}
