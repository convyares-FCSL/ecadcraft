//! What **kind of device** something is comes from `Device.device_type` (typed data), never from
//! the symbol that draws it. The one way from a symbol to a kind is `Starter::device_type()`, and it
//! is `pub(crate)` in ecad-electrical, so the compiler keeps every other crate from asking a symbol.
//! Inside ecad-electrical this guard keeps the calls to the places that make a definition or a
//! device ([`SEED_FILES`]): there the kind is seeded from the symbol once and stored from then on.
//!
//! Looking a symbol up for a *drawing* question (which symbol a representation is, its labels, its
//! pieces) is not a kind question and is not checked.
//!
//! Textual, like the other guards: over ecad-electrical's `src/`, outside `//` comments and outside a
//! file's trailing `#[cfg(test)]` module; a function's own declaration is not a call.

use std::collections::BTreeMap;
use std::path::PathBuf;

use serde_json::Value;

/// The crate whose sources are read.
const ELECTRICAL_SRC: &str = "crates/ecad-electrical/src/";

/// The files that seed a kind from a symbol, and why. No counts: any other file that calls
/// `device_type()` fails, and a listed file that no longer does must leave the list.
#[rustfmt::skip]
const SEED_FILES: &[(&str, &str)] = &[
    ("crates/ecad-electrical/src/symbols/mod.rs", "a definition made from a starter records the type it declares"),
    ("crates/ecad-electrical/src/devices/placing.rs", "a device placed from a symbol"),
    ("crates/ecad-electrical/src/plc/channels.rs", "a PLC card made for a channel symbol"),
    ("crates/ecad-electrical/src/connectors/mod.rs", "a connector made from a connector symbol"),
    ("crates/ecad-electrical/src/terminal_strips/terminal_blocks.rs", "a terminal block made from a terminal symbol"),
    ("crates/ecad-electrical/src/plc/rack.rs", "the card type a card body symbol draws (which drawn piece it is)"),
];

/// Whether `line` calls `device_type()` (not declares it).
fn calls(line: &str) -> bool {
    !line.contains("fn device_type") && line.contains(".device_type()")
}

/// The guard's problems, if any.
pub fn violations(meta: &Value) -> Result<Vec<String>, String> {
    let root = PathBuf::from(meta["workspace_root"].as_str().ok_or("cargo metadata: no workspace root")?);
    let mut by_file: BTreeMap<String, Vec<usize>> = BTreeMap::new();
    let mut paths = Vec::new();
    super::rust_files(&root.join(ELECTRICAL_SRC), &mut paths);
    for f in paths {
        let text = std::fs::read_to_string(&f).map_err(|e| format!("{}: {e}", f.display()))?;
        let found: Vec<usize> = super::engine_guards::product_code(&text).lines().enumerate().filter(|(_, l)| calls(l)).map(|(i, _)| i + 1).collect();
        if !found.is_empty() {
            by_file.insert(f.strip_prefix(&root).unwrap_or(&f).to_string_lossy().replace('\\', "/"), found);
        }
    }
    let mut out = Vec::new();
    for (rel, lines) in &by_file {
        if !SEED_FILES.iter().any(|(p, _)| p == rel) {
            out.push(format!(
                "{rel}: asks a symbol what kind of device it is (`device_type()`); read the device's `device_type`, or seed it where a device is made (SEED_FILES)"
            ));
            out.extend(lines.iter().map(|l| format!("    {rel}:{l}")));
        }
    }
    for (rel, why) in SEED_FILES {
        if !by_file.contains_key(*rel) {
            out.push(format!("{rel}: in SEED_FILES ({why}) but no longer calls `device_type()`; remove the entry"));
        }
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_call_is_found_but_not_a_declaration_or_a_field() {
        assert!(calls("device_type: starter.device_type(),"));
        assert!(!calls("pub(crate) fn device_type(&self) -> DeviceType {"));
        assert!(!calls("let t = &device.device_type;"));
    }
}
