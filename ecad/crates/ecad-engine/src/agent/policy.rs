//! What an agent may run through `execute`: an allowlist.
//!
//! # Semantics
//! - **Offered** ([`offered`]): ECAD's own commands ([`crate::commands::specs`]) and a short, reviewed
//!   list of CADCraft drawing commands ([`crate::agent::upstream`]), each marked [`Offered::Read`]
//!   or [`Offered::Write`]. Every other command is refused in every mode, before it reaches the
//!   engine.
//! - **Never** ([`NEVER_COMMANDS`]): the file, document and process commands, named so that they stay
//!   refused even if one were offered by mistake: opening, creating, closing and saving under
//!   another name stay with the person who started the server, nothing reads or writes other files,
//!   and no LISP runs. Every command CADCraft classifies as touching files is here. Saving the
//!   project is the `save` tool.
//! - **Writes off** (no `--write`, or the project opened read-only): only commands that read run
//!   ([`is_read_command`], and the reading forms of [`reads_only`]); a write is refused before it
//!   reaches the engine, so the head never moves.
//! - **Writes on:** an offered command runs through the engine exactly as for a person (ECAD's owner
//!   refuses or records it).
//! - Parameters naming a file ([`FILE_KEYS`], or a file key a generic command declares) are refused
//!   in every mode, whatever the command: a second fence behind the allowlist.

use std::collections::BTreeSet;
use std::sync::OnceLock;

use serde_json::Value;

use crate::agent::upstream;

/// What an offered command does to the project, as `list_commands` marks it (`"agent"`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Offered {
    /// It records nothing, whatever its parameters: it runs with writes off.
    Read,
    /// It may record a transaction: it runs only with writes on (some forms only read, see
    /// [`reads_only`]).
    Write,
}

impl Offered {
    /// The word `list_commands` shows: `read` or `write`.
    pub const fn word(self) -> &'static str {
        match self {
            Offered::Read => "read",
            Offered::Write => "write",
        }
    }
}

/// ECAD session commands that change nothing in the project or its files, allowed with writes
/// off. A generic command says so in its declaration instead, and an offered CADCraft command in
/// its entry ([`is_read_command`]).
pub(crate) const READ_COMMANDS: &[&str] = &[
    "ecadinspect",
    // Navigation between the project's pages: opens or shows a page, records nothing.
    "openpage",
    // Cable references: going to the other end opens or shows its page and selects
    // it, and records nothing.
    "cablegoto",
    // Cross-references: going to a reference opens or shows its page and records
    // nothing.
    "refgoto",
    // An issued revision opened read-only is a session view; nothing is recorded.
    "openrevision",
    "closerevision",
    // The History graph, steps, a point's details, the branch list, and a point
    // shown read-only, read the project or open a session view; nothing is recorded.
    "history",
    "steps",
    "pointinfo",
    "branches",
    "showpoint",
    "closepoint",
    // The reports and the printed book read the project and record nothing (a `path`
    // is refused in every mode, as for `plot` and `export`: `printbook` then returns the PDF as
    // base64 data); `reports` only asks the desktop app to open its window.
    "printbook",
    "reports",
    // The pre-release check reads the project and records nothing.
    "prerelease",
    // Show Hidden Text is a display state (like a cable view) and records nothing.
    "hiddentext",
];

/// Commands whose form with no fields only reads: `pageprops {page?}` and
/// `projectprops {}`: `numbering {}`: `textstyle {}`.
const READ_WITHOUT_FIELDS: &[(&str, &[&str])] = &[
    ("pageprops", &["page"]),
    ("projectprops", &[]),
    ("numbering", &[]),
    ("textstyle", &[]),
    ("standard", &[]),
    // `titleblock {id, sheet?}` reads one frame and its block.
    ("titleblock", &["id", "sheet"]),
    // `folderprops {folder}` reads a folder's start number.
    ("folderprops", &["folder"]),
    // `conflicts {}` lists the waiting conflicts.
    ("conflicts", &[]),
];

/// Commands whose form with `preview: true` only reads (the merge preview:
/// the Renumber and Finalise previews).
const READ_WITH_PREVIEW: &[&str] = &["merge", "renumber", "finalise"];

/// Commands whose form naming an issued revision records nothing (`release {rev}`
/// writes that revision's folder again, beside the project, and never issues; it refuses a `rev`
/// that is not issued). Its `folder` is a file key, refused in every mode.
const READ_WITH_REV: &[&str] = &["release"];

/// Commands whose form with `dry_run: true` only reads (what an import would set).
const READ_WITH_DRY_RUN: &[&str] = &["plcimport"];

/// Whether command `id` only reads, whatever its parameters: a session command in
/// [`READ_COMMANDS`], a generic command that declares it, or an offered CADCraft command marked so.
pub fn is_read_command(id: &str) -> bool {
    READ_COMMANDS.contains(&id) || crate::commands::declared_read(id) || upstream::offered(id) == Some(Offered::Read)
}

/// Every command that only reads, whatever its parameters ([`is_read_command`]).
pub fn read_commands() -> Vec<&'static str> {
    let upstream_reads = upstream::UPSTREAM_COMMANDS.iter().filter(|(_, o)| *o == Offered::Read).map(|(c, _)| *c);
    READ_COMMANDS.iter().copied().chain(crate::commands::declared_reads()).chain(upstream_reads).collect()
}

/// Whether `id` is one of ECAD's own commands ([`crate::commands::specs`]).
pub fn is_ecad_command(id: &str) -> bool {
    static IDS: OnceLock<BTreeSet<&'static str>> = OnceLock::new();
    IDS.get_or_init(|| crate::commands::specs().iter().map(|c| c.id).collect()).contains(id)
}

/// What command `id` (its canonical id) does for an agent, or `None` when agents may not run it:
/// one of [`NEVER_COMMANDS`], or neither ECAD's nor an offered CADCraft command.
pub fn offered(id: &str) -> Option<Offered> {
    if NEVER_COMMANDS.contains(&id) {
        return None;
    }
    if is_ecad_command(id) {
        return Some(if is_read_command(id) { Offered::Read } else { Offered::Write });
    }
    upstream::offered(id)
}

/// Whether `id` with `params` only reads ([`READ_WITHOUT_FIELDS`], [`READ_WITH_PREVIEW`],
/// [`READ_WITH_REV`], [`READ_WITH_DRY_RUN`]).
pub fn reads_only(id: &str, params: &Value) -> bool {
    READ_WITHOUT_FIELDS.iter().any(|(c, keep)| *c == id && params.as_object().is_none_or(|o| o.keys().all(|k| keep.contains(&k.as_str()))))
        || (READ_WITH_PREVIEW.contains(&id) && params.get("preview").and_then(Value::as_bool) == Some(true))
        || (READ_WITH_DRY_RUN.contains(&id) && params.get("dry_run").and_then(Value::as_bool) == Some(true))
        || (READ_WITH_REV.contains(&id)
            && params.get("rev").is_some_and(Value::is_string)
            // Replace moves a folder on disk aside: a write, never with writes off.
            && params.get("existing").and_then(Value::as_str).is_none_or(|e| e.trim() != "replace"))
}

/// Commands an agent never runs: files, documents, LISP and the process. The project is opened by
/// the person who starts the server; saving is the `save` tool (only with `--write`). Every command
/// CADCraft lists as touching files (`file_access::commands::FILES`) or refuses under automation
/// roots (`REFUSED`) is here; a test checks it after each upstream sync.
pub const NEVER_COMMANDS: &[&str] = &[
    // Opening, creating, closing and saving documents.
    "open",
    "new",
    "close",
    "closeall",
    "qsave",
    "saveas",
    "recover",
    "drawingrecovery",
    "reinit",
    // Other files in or out: import, export, print, publish, compare, data links.
    "import",
    "export",
    "exportpdf",
    "plot",
    "plotwindow",
    "publish",
    "pdfbook",
    "pdfimport",
    "preview",
    "tableexport",
    "compare",
    "compareexport",
    "compareclose",
    "dataextraction",
    "datalink",
    "datalinkupdate",
    "eattext",
    "share",
    "layout",
    "layout.saveas",
    "pagesetup",
    "plotstyle",
    "plottermanager",
    "-plottermanager",
    "stylesmanager",
    // Blocks, references, images and patterns from files.
    "wblock",
    "-wblock",
    "insert",
    "minsert",
    "xref",
    "-xref",
    "xattach",
    "attach",
    "xopen",
    "xbind",
    "image",
    "imageattach",
    "blocklibrary",
    "blocklibraries",
    "blocklibimport",
    "hatch",
    "hatch.patterns",
    "linetype",
    "-linetype",
    // Scripts and LISP: they run anything, outside the commands ECAD records.
    "script",
    "lisp",
    "appload",
    // Settings of the person or the program; SETVAR also sets the path variables.
    "options",
    "aliasedit",
    "setvar",
    // AUDIT rewrites the drawing outside ECAD's commands; its log setting writes a file.
    "audit",
    // An agent never turns its own access on, widens it or moves the port.
    "agentaccess",
];

/// Said when an agent asks for `agentaccess`.
pub const AGENT_ACCESS: &str = "Refused: Agent access is set by the person in the ECAD window. Nothing was changed.";

/// Parameter names that name a file on disk.
pub(crate) const FILE_KEYS: &[&str] = &["path", "file", "patternFile", "template", "folder", "output"];

/// Commands whose file-named parameter is not a file.
const PATH_IS_NOT_A_FILE: &[&str] = &[
    // `folder` names a folder of pages, never a file.
    "newpage",
    "movepage",
    "newfolder",
    "renamefolder",
    "movefolder",
    "deletefolder",
    // A folder's start number, and Renumber pages in one folder.
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

/// Said when the command is neither ECAD's nor an offered CADCraft command.
pub fn not_offered(id: &str) -> String {
    format!(
        "Refused: {} is not available to agents. Agents run ECAD's commands and a short list of drawing commands; \
         list_commands shows them, each marked read or write. Nothing was changed.",
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
    let Some(offer) = offered(id) else { return Err(not_offered(id)) };
    if offer == Offered::Read || reads_only(id, params) {
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
        assert_eq!(check("setvar", &json!({"name": "osmode", "value": 1}), true, None), Err(never("setvar")));
        for id in ["lisp", "appload", "blocklibraries", "insert", "hatch"] {
            assert_eq!(check(id, &none, true, None), Err(never(id)), "{id}");
        }
        // Commands nobody offered are refused in every mode, as are names that are no command.
        for id in ["3dorbit", "layer", "pastetable", "nosuch"] {
            assert_eq!(check(id, &none, true, None), Err(not_offered(id)), "{id}");
            assert_eq!(check(id, &none, false, None), Err(not_offered(id)), "{id}");
        }
        assert!(check("entities", &none, false, Some("locked")).is_ok());
        assert_eq!(check("line", &none, false, None), Err(WRITES_OFF.into()));
        assert!(check("wire", &none, false, None).is_err());
        assert!(check("wire", &none, true, None).is_ok());
        assert!(check("wire", &none, true, Some("locked")).is_err());
        assert!(check("saveas", &none, true, None).is_err());
        assert_eq!(check("agentaccess", &json!({"mode": "write"}), true, None), Err(AGENT_ACCESS.into()));
        assert!(check("line", &json!({"path": "x.dxf"}), true, None).is_err());
        assert_eq!(check("newfolder", &json!({"folder": "A"}), true, None), Ok(()));
        assert!(check("entities", &json!({"path": null}), false, None).is_ok());
        // The check reads; `release` issues only without `rev`; no folder of its own.
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

    /// Every command the read lists name is offered, and every special form belongs to one of
    /// ECAD's own commands.
    #[test]
    fn every_read_command_is_offered() {
        for id in read_commands() {
            assert_eq!(offered(id), Some(Offered::Read), "{id}");
        }
        let forms = [READ_WITH_PREVIEW, READ_WITH_REV, READ_WITH_DRY_RUN, PATH_IS_NOT_A_FILE].concat();
        for id in forms.into_iter().chain(READ_WITHOUT_FIELDS.iter().map(|(c, _)| *c)) {
            assert!(is_ecad_command(id), "{id}");
        }
    }
}
