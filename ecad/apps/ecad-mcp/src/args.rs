//! The command line of `ecad-mcp`.

use std::ffi::OsString;

use crate::Options;

/// Command-line usage.
pub const USAGE: &str = "usage: ecad-mcp [--write] [--agent <name>] [--create] [<project.ecad>]\n\n\
ECAD over MCP (JSON-RPC on stdin and stdout), headless: no window. Opens <project.ecad>; with no\n\
project, a new project that is never saved. Read-only unless --write is given.\n\
  --write          let the agent change the project through ECAD commands and save it\n\
  --agent <name>   record the agent's changes as `agent <name>` (default: the client's own name)\n\
  --create         create <project.ecad> (it must not exist yet) and save it before serving\n\
See ecad/docs/quick-start.md.";

/// What the command line asks for.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Invocation {
    /// Serve: open (or with `create`, create) `project` if given, else an unsaved project.
    Serve {
        project: Option<String>,
        create: bool,
        options: Options,
    },
    Help,
    Version,
}

/// Parse the command line (without the program name).
///
/// # Errors
/// A usage error, in words: an unknown option, `--agent` without a name, `--create` without a
/// project, more than one project, or a name that is not valid Unicode.
pub fn parse_args(args: impl IntoIterator<Item = OsString>) -> Result<Invocation, String> {
    let mut project: Option<String> = None;
    let mut create = false;
    let mut options = Options::default();
    let mut args = args.into_iter();
    while let Some(a) = args.next() {
        let Some(s) = a.to_str() else {
            return Err(format!("the argument {:?} is not valid Unicode", a.to_string_lossy()));
        };
        match s {
            "--write" => options.write = true,
            "--create" => create = true,
            "--agent" => match args.next().and_then(|n| n.into_string().ok()) {
                Some(n) if !n.trim().is_empty() => options.agent = Some(n),
                _ => return Err("--agent needs a name".into()),
            },
            "--help" | "-h" => return Ok(Invocation::Help),
            "--version" | "-V" => return Ok(Invocation::Version),
            other if other.starts_with('-') => return Err(format!("unknown argument {other:?}")),
            path if project.is_none() => project = Some(path.to_string()),
            extra => return Err(format!("only one project file can be opened ({extra:?})")),
        }
    }
    if create && project.is_none() {
        return Err("--create needs a project file".into());
    }
    Ok(Invocation::Serve { project, create, options })
}
