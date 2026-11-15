//! The release commands (`prerelease`, `release`); the release itself is [`crate::release`].

use cadcraft_engine::{CommandSpec, Session};
use serde_json::Value;

use crate::cmd::{err, with_any_page, with_project};
use crate::release::{prerelease, release_issued, release_pending};

/// The commands this module adds.
pub fn specs() -> Vec<CommandSpec> {
    vec![
        CommandSpec::new("prerelease", "Pre-release Check", run_prerelease)
            .params("{rev?: an issued rev (default the pending one), limit?: rows per group (50, max 2000)} (read only)")
            .noundo(),
        CommandSpec::new("release", "Release", run_release)
            .params(
                "{rev?: an issued rev to write again (records nothing; default: issue the pending one), folder?, outputs?: [slot], \
accept?: \"errors\", provisional?: \"finalise\"|\"issue\", existing?: \"add-missing\"|\"replace\"|\"keep-both\", colour?: \"mono\"|\"screen\", \
lineweights?: bool}",
            )
            .noundo(),
    ]
}

fn run_prerelease(s: &mut Session, params: &Value) -> cadcraft_engine::Result<Value> {
    with_any_page(s, |p, _| prerelease(p, params))
}

fn run_release(s: &mut Session, params: &Value) -> cadcraft_engine::Result<Value> {
    let rev = params
        .get("rev")
        .filter(|v| !v.is_null())
        .map(|v| v.as_str().map(str::to_string).ok_or_else(|| err("`rev` is text such as \"A\"")))
        .transpose()?;
    match rev {
        Some(r) => {
            let v = with_any_page(s, |p, _| release_issued(p, params, &r, &mut |_, _| true))?;
            if let Some(m) = v.get("message").and_then(Value::as_str) {
                s.echo(m.to_string());
            }
            Ok(v)
        }
        None => with_project(s, |p, pg, _| release_pending(p, pg, params, &mut |_, _| true)),
    }
}
