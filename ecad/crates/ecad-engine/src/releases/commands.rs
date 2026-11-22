//! The release commands (`prerelease`, `release`); the release itself is [`crate::releases::release`].
//!
//! Both read their input through a params struct (brief 070), so a parameter that does not read is
//! refused at the edge in the uniform wording. The release (engine work: the checks, the reports,
//! the files it writes) still takes its options as JSON: the shim writes the read parameters back
//! (`serde_json::to_value`) and hands them over, and the release's own refusals keep their words.

use cadcraft_engine::{CommandSpec, Session};
use ecad_electrical::CmdError;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::commands::input::{Command, Typed};
use crate::commands::{err, on_any_page, on_project};
use crate::releases::release::{prerelease, release_issued, release_pending};

/// The commands of this module with a typed input.
pub const TYPED: &[Typed] = &[Typed::of::<PrereleaseParams>("prerelease"), Typed::of::<ReleaseParams>("release")];

/// The commands this module adds.
pub fn specs() -> Vec<CommandSpec> {
    vec![
        CommandSpec::new("prerelease", "Pre-release Check", crate::commands::input::typed::<PrereleaseParams>)
            .params("{rev?: an issued rev (default the pending one), limit?: rows per group (50, max 2000)} (read only)")
            .noundo(),
        CommandSpec::new("release", "Release", crate::commands::input::typed::<ReleaseParams>)
            .params(
                "{rev?: an issued rev to write again (records nothing; default: issue the pending one), folder?, reports?: [slot], \
accept?: \"errors\", provisional?: \"finalise\"|\"issue\", existing?: \"add-missing\"|\"replace\"|\"keep-both\", colour?: \"mono\"|\"screen\", \
lineweights?: bool}",
            )
            .noundo(),
    ]
}

/// The read parameters as the release takes them.
fn as_json<P: Serialize>(p: &P) -> cadcraft_engine::Result<Value> {
    serde_json::to_value(p).map_err(|e| err(e.to_string()))
}

/// `prerelease {rev?, limit?, accept?}` (read only): the pre-release check of the pending revision,
/// or of an issued one.
#[derive(Deserialize, Serialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub(crate) struct PrereleaseParams {
    /// An issued revision (default the pending one).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    rev: Option<String>,
    /// Rows per group (50, at most 2000).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    limit: Option<u64>,
    /// "errors": the check's errors are accepted.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    accept: Option<String>,
}

impl Command for PrereleaseParams {
    fn run(self, s: &mut Session) -> cadcraft_engine::Result<Value> {
        let params = as_json(&self)?;
        on_any_page(s, |p, _| prerelease(p, &params).map_err(CmdError::refused))
    }
}

/// `release {rev?, folder?, reports?, accept?, provisional?, existing?, colour?, lineweights?}`:
/// issue the pending revision and write its folder, or write an issued revision's folder again
/// (`rev`; records nothing).
#[derive(Deserialize, Serialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub(crate) struct ReleaseParams {
    /// An issued revision to write again (default: issue the pending one).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    rev: Option<String>,
    /// The folder to write (default the project's release folder for the revision).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    folder: Option<String>,
    /// The lists to write, by slot name (default all).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    reports: Option<Vec<String>>,
    /// "errors": release although the check found errors.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    accept: Option<String>,
    /// Provisional items: "finalise" (number them first) or "issue" (issue as they are).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    provisional: Option<String>,
    /// When the folder exists: "add-missing", "replace" or "keep-both".
    #[serde(default, skip_serializing_if = "Option::is_none")]
    existing: Option<String>,
    /// "mono" or "colour" (default the project's print setting).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    colour: Option<String>,
    /// Draw lines with their weights (default true).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    lineweights: Option<bool>,
}

impl Command for ReleaseParams {
    fn run(self, s: &mut Session) -> cadcraft_engine::Result<Value> {
        let params = as_json(&self)?;
        match self.rev {
            Some(r) => {
                let v = on_any_page(s, |p, _| release_issued(p, &params, &r, &mut |_, _| true).map_err(CmdError::refused))?;
                if let Some(m) = v.get("message").and_then(Value::as_str) {
                    s.echo(m.to_string());
                }
                Ok(v)
            }
            None => on_project(s, |p, pg, _| release_pending(p, pg, &params, &mut |_, _| true).map_err(CmdError::refused)),
        }
    }
}
