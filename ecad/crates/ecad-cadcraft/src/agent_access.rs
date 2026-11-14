//! The agent access preference (brief 035, design §3): whether the ECAD window lets an agent
//! connect over its loopback port, read only or to change the project, and on which port.
//!
//! # Semantics
//! - Stored per user in `agent-access.json` in the ECAD settings folder (the folder
//!   `cable-views.json` uses, [`Settings`]), format `ecad-agent-access/1`:
//!   `{"format": "ecad-agent-access/1", "mode": "off"|"read"|"write", "port": 1024..=65535}`.
//! - Missing reads as Off with the default port. Unreadable, over 4 KiB, not JSON, or an unknown
//!   format, mode or port reads as **Off** and says so ([`Loaded::warning`]).
//! - Written atomically (write a temporary file, then rename), never into a project.
//! - `agentaccess {mode?, port?}` reads the preference (no fields) or asks the window to set it
//!   ([`Project::agent_access_request`]); the window applies it and reports
//!   ([`Project::agent_access_status`]). Agents never run it (`ecad-mcp`'s policy refuses it).
//!
//! Excluded: the listener, the gate and the banner (the trial), and anything stored in a project.

use std::io::Read;
use std::path::PathBuf;

use cadcraft_engine::{CommandSpec, Session};
use serde_json::{Value, json};

use crate::commands::{err, opt_typed, with_any_page};
use crate::project::Project;
use crate::views::Settings;

/// The preference file's name in the settings folder.
pub const FILE_NAME: &str = "agent-access.json";
/// The preference file's format.
pub const FORMAT: &str = "ecad-agent-access/1";
/// The port when none is set (design 035 §17, 1). A per-user window setting, not a drawing default:
/// the project-scoped defaults registry does not hold it.
pub const DEFAULT_PORT: u16 = 47823;
/// The lowest port accepted (below are the system's).
pub const MIN_PORT: u16 = 1024;
/// Largest preference file read, in bytes; a larger one reads as Off.
pub const MAX_FILE_BYTES: u64 = 4 * 1024;
/// Said when the file cannot be used.
pub const UNREADABLE: &str = "Agent access settings could not be read; agent access is off.";
/// Said when a port is out of range.
pub const PORT_RANGE: &str = "1024–65535";

/// Off, Read only, or Read and change.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum Mode {
    #[default]
    Off,
    Read,
    Write,
}

impl Mode {
    /// The stored word.
    pub fn word(self) -> &'static str {
        match self {
            Mode::Off => "off",
            Mode::Read => "read",
            Mode::Write => "write",
        }
    }

    /// The words the person sees.
    pub fn label(self) -> &'static str {
        match self {
            Mode::Off => "Off",
            Mode::Read => "Read only",
            Mode::Write => "Read and change",
        }
    }

    /// A stored or typed word (`off`, `read`, `write`, also the labels), without case.
    pub fn parse(s: &str) -> Option<Mode> {
        match s.trim().to_ascii_lowercase().as_str() {
            "off" => Some(Mode::Off),
            "read" | "read only" | "readonly" => Some(Mode::Read),
            "write" | "read and change" => Some(Mode::Write),
            _ => None,
        }
    }
}

/// The preference.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Preference {
    pub mode: Mode,
    pub port: u16,
}

impl Default for Preference {
    fn default() -> Self {
        Preference { mode: Mode::Off, port: DEFAULT_PORT }
    }
}

/// A port typed or stored, when it is in range.
pub fn parse_port(s: &str) -> Option<u16> {
    s.trim().parse::<u16>().ok().filter(|p| *p >= MIN_PORT)
}

/// What reading the file found.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Loaded {
    pub preference: Preference,
    /// [`UNREADABLE`] when the file exists but could not be used.
    pub warning: Option<&'static str>,
}

/// The preference file in `settings`, if there is a folder.
pub fn file(settings: &Settings) -> Option<PathBuf> {
    settings.dir().map(|d| d.join(FILE_NAME))
}

/// Read the preference. Never fails: anything unusable reads as Off (with [`Loaded::warning`]).
pub fn load(settings: &Settings) -> Loaded {
    let bad = Loaded { preference: Preference::default(), warning: Some(UNREADABLE) };
    let Some(path) = file(settings) else { return Loaded::default() };
    let f = match std::fs::File::open(&path) {
        Ok(f) => f,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Loaded::default(),
        Err(_) => return bad,
    };
    let mut bytes = Vec::new();
    if f.take(MAX_FILE_BYTES + 1).read_to_end(&mut bytes).is_err() || bytes.len() as u64 > MAX_FILE_BYTES {
        return bad;
    }
    let Ok(Value::Object(top)) = serde_json::from_slice::<Value>(&bytes) else { return bad };
    if top.get("format").and_then(Value::as_str) != Some(FORMAT) {
        return bad;
    }
    let mode = top.get("mode").and_then(Value::as_str).and_then(|m| match m {
        "off" => Some(Mode::Off),
        "read" => Some(Mode::Read),
        "write" => Some(Mode::Write),
        _ => None,
    });
    let port = match top.get("port") {
        None | Some(Value::Null) => Some(DEFAULT_PORT),
        Some(v) => v.as_u64().and_then(|p| u16::try_from(p).ok()).filter(|p| *p >= MIN_PORT),
    };
    match (mode, port) {
        (Some(mode), Some(port)) => Loaded { preference: Preference { mode, port }, warning: None },
        _ => bad,
    }
}

/// Write the preference (temporary file, then rename).
///
/// # Errors
/// No settings folder, or the folder or file cannot be written; in words.
pub fn store(settings: &Settings, p: &Preference) -> Result<(), String> {
    let (Some(dir), Some(path)) = (settings.dir(), file(settings)) else { return Err("no settings folder".into()) };
    let text = format!("{}\n", json!({"format": FORMAT, "mode": p.mode.word(), "port": p.port}));
    std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
    let tmp = dir.join(format!("{FILE_NAME}.{}.tmp", crate::session_seed()));
    let written = std::fs::write(&tmp, text.as_bytes()).and_then(|()| std::fs::rename(&tmp, &path));
    if let Err(e) = written {
        let _ = std::fs::remove_file(&tmp);
        return Err(e.to_string());
    }
    Ok(())
}

/// The window's agent access state, as `agentaccess` reports it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Status {
    pub preference: Preference,
    /// Something listens on the port now.
    pub listening: bool,
    /// The status text the person sees (`Off`, `● Listening on 127.0.0.1:47823`, …).
    pub text: String,
    /// Set by `--control` for this run (the preference is not changed by it).
    pub this_run: bool,
}

pub fn specs() -> Vec<CommandSpec> {
    vec![
        CommandSpec::new("agentaccess", "Agent Access", run_agentaccess)
            .params("{mode?: \"off\"|\"read\"|\"write\", port?: 1024–65535} (no fields: reads; set by the person only)")
            .noundo(),
    ]
}

fn reply(st: Option<&Status>, pref: Preference) -> Value {
    json!({
        "mode": pref.mode.word(),
        "port": pref.port,
        "listening": st.is_some_and(|s| s.listening),
        "status": st.map_or_else(|| "Off".to_string(), |s| s.text.clone()),
        "thisRun": st.is_some_and(|s| s.this_run),
    })
}

fn run_agentaccess(s: &mut Session, params: &Value) -> cadcraft_engine::Result<Value> {
    let mode = opt_typed(params, "mode", "off, read or write", |v| v.as_str().and_then(Mode::parse)).map_err(err)?;
    let port = opt_typed(params, "port", "a port from 1024 to 65535", |v| v.as_u64().and_then(|p| u16::try_from(p).ok()).filter(|p| *p >= MIN_PORT))
        .map_err(err)?;
    with_any_page(s, |p: &mut Project, _| {
        let current = p.agent_access_status.as_ref().map_or_else(|| load(&Settings::from_env()).preference, |s| s.preference);
        if mode.is_none() && port.is_none() {
            return Ok(reply(p.agent_access_status.as_ref(), current));
        }
        if p.agent_access_status.as_ref().is_some_and(|s| s.this_run) {
            return Err("Agent access is set by --control for this run; restart ECAD without it to change the preference.".into());
        }
        let want = Preference { mode: mode.unwrap_or(current.mode), port: port.unwrap_or(current.port) };
        p.agent_access_request = Some(want);
        Ok(reply(p.agent_access_status.as_ref(), want))
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_file_round_trips_and_anything_unusable_reads_as_off() {
        let dir = std::env::temp_dir().join(format!("ecad-agent-access-{}", crate::session_seed()));
        let settings = Settings::at(&dir);
        assert_eq!(load(&settings), Loaded::default(), "missing: off, no warning");
        let p = Preference { mode: Mode::Write, port: 50000 };
        store(&settings, &p).unwrap();
        assert_eq!(load(&settings), Loaded { preference: p, warning: None });
        let path = file(&settings).unwrap();
        for bad in [
            "not json",
            r#"{"format":"ecad-agent-access/2","mode":"write","port":50000}"#,
            r#"{"format":"ecad-agent-access/1","mode":"all","port":50000}"#,
            r#"{"format":"ecad-agent-access/1","mode":"write","port":80}"#,
            r#"{"format":"ecad-agent-access/1","mode":"write","port":70000}"#,
        ] {
            std::fs::write(&path, bad).unwrap();
            let l = load(&settings);
            assert_eq!(l.preference.mode, Mode::Off, "{bad}");
            assert_eq!(l.warning, Some(UNREADABLE), "{bad}");
        }
        std::fs::write(&path, vec![b' '; 5000]).unwrap();
        assert_eq!(load(&settings).warning, Some(UNREADABLE), "over 4 KiB");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn modes_and_ports_parse() {
        assert_eq!(Mode::parse("Read and change"), Some(Mode::Write));
        assert_eq!(Mode::parse("read"), Some(Mode::Read));
        assert_eq!(Mode::parse("x"), None);
        assert_eq!(parse_port("47823"), Some(47823));
        assert_eq!(parse_port("80"), None);
        assert_eq!(parse_port("x"), None);
    }
}
