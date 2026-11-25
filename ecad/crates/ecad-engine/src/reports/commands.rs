//! The commands that need the session: `printbook` (the PDF of the project's pages, through the
//! Print Book) and `reports` (opens the Reports window). The report lists (`bom`, `wirelist`,
//! `cablelist`) and the boxes (`locationbox`, `functionbox`) are generic commands in
//! [`ecad_electrical::reports::commands`].

use cadcraft_engine::{CommandSpec, Session};
use ecad_electrical::CmdError;
use ecad_model::FolderId;
use schemars::JsonSchema;
use serde::Deserialize;
use serde_json::{Value, json};

use crate::commands::input::{Command, Typed};
use crate::commands::{err, on_any_page};
use crate::export::printbook::{self, Options, Scope};
use crate::project::Project;

/// The commands of this module with a typed input.
pub(crate) const TYPED: &[Typed] = &[Typed::of::<PrintbookParams>("printbook"), Typed::of::<ReportsParams>("reports")];

/// The commands this module adds.
pub(crate) fn specs() -> Vec<CommandSpec> {
    vec![
        CommandSpec::new("printbook", "Print Book", crate::commands::input::typed::<PrintbookParams>).noundo(),
        CommandSpec::new("reports", "Reports", crate::commands::input::typed::<ReportsParams>).noundo(),
    ]
}

/// What `printbook` prints: "all", a folder or pages.
#[derive(Deserialize, JsonSchema)]
#[serde(untagged)]
enum ScopeParam {
    /// "all".
    Word(String),
    /// {folder: id} or {pages: "100, 105-110"}.
    Pick {
        /// A folder's pages.
        #[serde(default)]
        folder: Option<FolderId>,
        /// Page numbers and ranges.
        #[serde(default)]
        pages: Option<String>,
    },
}

/// The scope `printbook` prints.
fn scope_of(v: Option<ScopeParam>) -> Result<Scope, CmdError> {
    const WORDS: &str = "`scope` is \"all\", {folder: id} or {pages: \"100, 105-110\"}";
    match v {
        None => Ok(Scope::All),
        Some(ScopeParam::Word(t)) if t.trim() == "all" => Ok(Scope::All),
        Some(ScopeParam::Pick { folder: Some(f), .. }) => Ok(Scope::Folder(f)),
        Some(ScopeParam::Pick { folder: None, pages: Some(t) }) => Ok(Scope::Pages(t)),
        Some(_) => Err(CmdError::refused(WORDS)),
    }
}

/// Print `scope` of `p`: the printed result as JSON.
///
/// # Errors
/// No page matches, or a projection, plot or write failure (nothing written).
pub(crate) fn print_project(
    p: &mut Project, scope: &Scope, o: Options, path: Option<&str>, progress: &mut dyn FnMut(usize, usize) -> bool,
) -> Result<Value, CmdError> {
    if ecad_electrical::pages::generated::has_bom_page(p.ledger.document()) {
        p.reports();
    }
    // The book shows the pending marker as the screen does.
    p.refresh_pending();
    let o = Options { pending: p.frame.pending, ..o };
    let doc = p.ledger.document();
    let resolved = printbook::resolve_with(doc, scope, p.kept_reports()).map_err(CmdError::refused)?;
    let done =
        printbook::print(doc, &resolved.pages, &p.display.views, p.tolerance, p.kept_reports(), o, path, progress).map_err(CmdError::refused)?;
    let files: Vec<Value> = done
        .files
        .iter()
        .map(|(path, sheets, bytes, data)| {
            let mut f = json!({ "path": path, "sheets": sheets, "bytes": bytes });
            if let Some(d) = data {
                f["data"] = json!(printbook::base64(d));
            }
            f
        })
        .collect();
    let sheets = if done.sheets == 1 { "1 sheet".to_string() } else { format!("{} sheets", done.sheets) };
    let message = match done.files.iter().filter_map(|(p, _, b, _)| p.as_ref().map(|p| (p, b))).collect::<Vec<_>>().as_slice() {
        [] => format!("Printed {sheets} ({} generated).", done.generated),
        [(path, bytes)] => format!("Printed {sheets} ({} generated) to {path} ({} KB).", done.generated, (**bytes).div_ceil(1024)),
        many => format!(
            "Printed {sheets} ({} generated) to {} files: {}.",
            done.generated,
            many.len(),
            many.iter().map(|(p, _)| p.as_str()).collect::<Vec<_>>().join(", ")
        ),
    };
    Ok(json!({
        "files": files, "sheets": done.sheets, "generated": done.generated, "flagged": done.flagged,
        "unknown": resolved.unknown, "notes": resolved.notes.iter().chain(done.notes.iter()).collect::<Vec<_>>(), "message": message}))
}

/// The project's pages as a PDF.
#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub(crate) struct PrintbookParams {
    /// "all" (default), {folder: id} or {pages: "100, 105-110"}.
    #[serde(default)]
    scope: Option<ScopeParam>,
    /// The PDF file to write.
    #[serde(default)]
    path: Option<String>,
    /// Mono (black, the colour code in the wire label) or colour (wire colours; also `color` or
    /// `screen`); default the project's `print.colour`.
    #[serde(default)]
    colour: Option<ecad_electrical::commands::input::Said<ecad_electrical::defaults::print::PrintColour>>,
    /// Print lineweights (default true).
    #[serde(default)]
    lineweights: Option<bool>,
}

impl Command for PrintbookParams {
    fn run(self, s: &mut Session) -> cadcraft_engine::Result<Value> {
        let scope = scope_of(self.scope).map_err(|e| err(e.to_string()))?;
        let lineweights = self.lineweights.unwrap_or(true);
        let path = self.path;
        // No `colour` given prints as the project's `print.colour` says (mono by
        // default).
        let colour = self.colour.map(|c| c.0);
        let v = on_any_page(s, |p, _| {
            let mono = ecad_electrical::defaults::print::mono_of(p.ledger.document(), colour);
            print_project(p, &scope, Options { mono, lineweights, pending: false }, path.as_deref(), &mut |_, _| true)
        })?;
        if let Some(m) = v.get("message").and_then(Value::as_str) {
            s.echo(m.to_string());
        }
        for n in v.get("notes").and_then(Value::as_array).into_iter().flatten().filter_map(Value::as_str) {
            s.echo(n.to_string());
        }
        Ok(v)
    }
}

/// Opens the Reports window.
#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
#[schemars(extend("properties" = {}))]
pub(crate) struct ReportsParams {}

impl Command for ReportsParams {
    fn run(self, s: &mut Session) -> cadcraft_engine::Result<Value> {
        on_any_page(s, |p, _| {
            p.asks.open_reports = true;
            Ok(json!({"open": "reports"}))
        })
    }
}
