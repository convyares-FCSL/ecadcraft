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

/// Commands that change nothing in the project or its files, allowed with writes off.
pub const READ_COMMANDS: &[&str] = &[
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
];

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
];

/// Parameter names that name a file on disk.
pub const FILE_KEYS: &[&str] = &["path", "file", "patternFile", "template", "folder", "output"];

/// Commands whose `path` parameter is not a file (ARRAYPATH's path is a handle).
const PATH_IS_NOT_A_FILE: &[&str] = &["arraypath"];

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
    let back =
        id == "undo" && params.get("mode").and_then(Value::as_str).is_some_and(|m| m.eq_ignore_ascii_case("back"));
    let all = id == "mredo" && params.get("all").is_some_and(|v| !v.is_null() && v != &Value::Bool(false));
    counted || back || all
}

/// Whether SETVAR is asked to set AUDITCTL (which makes AUDIT write a log file).
fn sets_auditctl(id: &str, params: &Value) -> bool {
    id == "setvar"
        && params.get("name").and_then(Value::as_str).is_some_and(|n| n.trim().eq_ignore_ascii_case("auditctl"))
}

/// Said when a write is refused because the server was started without `--write`.
pub const WRITES_OFF: &str = "Refused: this ECAD MCP server is read-only (started without --write). \
Nothing was changed. Restart it with --write to let the agent edit and save the project.";

/// Said when the command is refused in every mode.
pub fn never(id: &str) -> String {
    format!(
        "Refused: {} is not available to agents. Opening, creating, closing and saving projects under another name, \
         and reading or writing other files, stay with the person who started the server; save the project with the \
         `save` tool. Nothing was changed.",
        id.to_ascii_uppercase()
    )
}

/// Said when the project opened read-only (`why` from the file layer).
pub fn project_read_only(why: &str) -> String {
    format!("Refused: the project opened read-only ({why}). Nothing was changed.")
}

/// Said when a parameter names a file.
pub fn file_key(id: &str, key: &str) -> String {
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
    if NEVER_COMMANDS.contains(&id) {
        return Err(never(id));
    }
    if sets_auditctl(id, params) {
        return Err(never("setvar auditctl"));
    }
    if READ_COMMANDS.contains(&id) {
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
        for (id, p) in
            [("undo", json!({"count": 1})), ("redo", json!({})), ("u", json!({})), ("mredo", json!({"all": false}))]
        {
            assert!(check(id, &p, true, None).is_ok(), "{id} {p}");
        }
        assert!(check("audit", &none, true, None).is_err());
        assert!(check("setvar", &json!({"name": "AuditCtl", "value": 1}), true, None).is_err());
        assert!(check("setvar", &json!({"name": "osmode", "value": 1}), true, None).is_ok());
        assert!(check("wire", &none, false, None).is_err());
        assert!(check("wire", &none, true, None).is_ok());
        assert!(check("wire", &none, true, Some("locked")).is_err());
        assert!(check("saveas", &none, true, None).is_err());
        assert!(check("line", &json!({"path": "x.dxf"}), true, None).is_err());
        assert!(check("arraypath", &json!({"path": "1A"}), true, None).is_ok());
        assert!(check("entities", &json!({"path": null}), false, None).is_ok());
    }
}
