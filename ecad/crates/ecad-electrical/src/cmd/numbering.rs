//! The rules of brief 026's `renumber` (design 026 §3, §13; brief 028 §9.5 for terminals): what a
//! renumber covers, read from its typed input into an [`ecad_model::numbering::Request`]. The plan
//! itself is [`ecad_model::numbering`]'s; the engine crate's shim plans it against the project's
//! tag marks, commits and answers in JSON.

use std::collections::BTreeSet;

use ecad_model::numbering::{self, Kind, Request, Scope};
use ecad_model::{CableId, DeviceId, Document, FolderId, Noun, PageId, TerminalId, WireId};

use crate::error::CmdError;

/// The kinds `kind` names (absent: devices); `all` is devices, cables and wires.
///
/// # Errors
/// Any other word.
pub fn kinds(word: Option<&str>) -> Result<BTreeSet<Kind>, CmdError> {
    let k = word.unwrap_or("devices").trim().to_ascii_lowercase();
    Ok(match k.as_str() {
        "devices" | "device" | "dev" => [Kind::Devices].into(),
        "cables" | "cable" | "cbl" => [Kind::Cables].into(),
        "wires" | "wire" => [Kind::Wires].into(),
        // Brief 028: terminal labels per strip ("all" keeps its 026 meaning: tags and wires).
        "terminals" | "terminal" | "trm" => [Kind::Terminals].into(),
        "all" => [Kind::Devices, Kind::Cables, Kind::Wires].into(),
        other => {
            return Err(CmdError::refused(format!("`kind` must be \"devices\", \"cables\", \"wires\", \"terminals\" or \"all\", not {other:?}")));
        }
    })
}

/// A renumber's `scope` as given: a word, or an object. Each of the object's entries: absent
/// (`None`), null (`Some(None)`) or given.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ScopeInput {
    /// `all`, `selection`, `provisional` (or `unnumbered`).
    Word(String),
    /// `{prefix}`, `{folder: id}` or `{pages: [id]}`, read in that order.
    Pick { prefix: Option<Option<String>>, folder: Option<Option<FolderId>>, pages: Option<Option<Vec<PageId>>> },
}

/// What `renumber` is given. `objects` and `strips` are hex IDs as written (a device, cable or
/// wire; a strip or one of its terminals); `Some(None)`: given as null.
#[derive(Debug, Clone, Default)]
pub struct RenumberInput {
    pub kind: Option<String>,
    pub scope: Option<ScopeInput>,
    pub objects: Option<Option<Vec<String>>>,
    pub strips: Option<Option<Vec<String>>>,
    pub start: Option<u64>,
    pub digits: Option<u64>,
}

/// Most pages a `{pages}` scope reads.
const MAX_PAGES: usize = 100_000;

/// The ID hex `t` writes, else the refusal naming it in `list`.
fn hex(t: &str, list: &str) -> Result<u128, CmdError> {
    u128::from_str_radix(t.trim(), 16).map_err(|_| CmdError::input(format!("`{t:?}` in `{list}` is not a valid hex id")))
}

/// The scope of a renumber.
///
/// # Errors
/// An unknown word, a selection without `objects`, an object that is none of the three, or an
/// ID the project does not have.
pub fn scope(doc: &Document, input: &RenumberInput, max_objects: usize) -> Result<Scope, CmdError> {
    match &input.scope {
        None => Ok(Scope::All),
        Some(ScopeInput::Word(w)) => match w.trim().to_ascii_lowercase().as_str() {
            "all" => Ok(Scope::All),
            "provisional" | "unnumbered" => Ok(Scope::Provisional),
            // Brief 028: a terminal renumber may name its strips in `strips` alone.
            "selection" if input.objects.is_none() && input.strips.is_some() => {
                Ok(Scope::Selection { devices: BTreeSet::new(), cables: BTreeSet::new(), conductors: BTreeSet::new() })
            }
            "selection" => {
                let list = input
                    .objects
                    .as_ref()
                    .and_then(Option::as_ref)
                    .ok_or(CmdError::input("`objects` (array of hex ids) is required with scope \"selection\""))?;
                if list.len() > max_objects {
                    return Err(CmdError::input(format!("at most {max_objects} entries in `objects` per call")));
                }
                let (mut devices, mut cables, mut conductors) = (BTreeSet::new(), BTreeSet::new(), BTreeSet::new());
                for t in list {
                    let n = hex(t, "objects")?;
                    if doc.devices.contains_key(&DeviceId(n)) {
                        devices.insert(DeviceId(n));
                    } else if doc.cables.contains_key(&CableId(n)) {
                        cables.insert(CableId(n));
                    } else if doc.wires.contains_key(&WireId(n)) {
                        conductors.insert(WireId(n));
                    } else {
                        return Err(CmdError::refused(format!("{t} in `objects` is not a device, cable or wire.")));
                    }
                }
                Ok(Scope::Selection { devices, cables, conductors })
            }
            other => Err(CmdError::refused(format!(
                "`scope` must be \"all\", \"selection\", \"provisional\", {{prefix}}, {{folder}} or {{pages}}, not {other:?}"
            ))),
        },
        Some(ScopeInput::Pick { prefix, folder, pages }) => {
            if let Some(p) = prefix {
                let p = p
                    .as_deref()
                    .map(str::trim)
                    .filter(|p| !p.is_empty())
                    .ok_or(CmdError::refused("`prefix` must be letters, such as \"-K\", or a potential"))?;
                return Ok(Scope::Prefix(p.to_string()));
            }
            if let Some(f) = folder {
                let f = f.ok_or(CmdError::input("`folder` (hex id) is required"))?;
                if !doc.folders.contains_key(&f) {
                    return Err(CmdError::NotFound(Noun::Folder));
                }
                return Ok(Scope::Folder(f));
            }
            if let Some(ps) = pages {
                let a = ps.as_ref().ok_or(CmdError::input("`pages` must be an array of hex ids"))?;
                let mut set = BTreeSet::new();
                for p in a.iter().take(MAX_PAGES) {
                    if !doc.pages.contains_key(p) {
                        return Err(CmdError::refused(format!("No page {}.", p.hex())));
                    }
                    set.insert(*p);
                }
                return Ok(Scope::Pages(set));
            }
            Err(CmdError::input("`scope` as an object is {prefix}, {folder: id} or {pages: [id]}"))
        }
    }
}

/// Brief 028 (design 028 §9.5): the strips a terminal renumber covers, from the scope: every
/// strip (all, folder, pages; the planner keeps those on the pages), the strips with that tag
/// prefix, or the strips named in `strips` or `objects` (selection; a terminal ID names its strip).
///
/// # Errors
/// A named strip that is not a terminal strip or one of its terminals.
pub fn strips(doc: &Document, named: &[String], scope: &Scope) -> Result<BTreeSet<DeviceId>, CmdError> {
    let all = || doc.devices.keys().copied().filter(|c| crate::strips::strip_of(doc, *c).is_some());
    Ok(match scope {
        Scope::All | Scope::Folder(_) | Scope::Pages(_) => all().collect(),
        Scope::Prefix(p) => {
            let p = p.trim_start_matches('-').to_ascii_uppercase();
            all().filter(|c| crate::devices::tag_of(doc, *c).and_then(ecad_model::tag::parse).is_some_and(|(q, _)| q == p)).collect()
        }
        Scope::Selection { devices, .. } => {
            let mut out: BTreeSet<DeviceId> = devices.iter().copied().filter(|c| crate::strips::strip_of(doc, *c).is_some()).collect();
            for t in named {
                let n = hex(t, "strips")?;
                let c = match doc.terminals.get(&TerminalId(n)) {
                    Some(t) => t.device(),
                    None => Some(DeviceId(n)),
                };
                let Some(c) = c.filter(|c| crate::strips::strip_of(doc, *c).is_some()) else {
                    return Err(CmdError::refused(format!("{t} in `strips` is not a terminal strip or one of its terminals.")));
                };
                out.insert(c);
            }
            out
        }
        Scope::Provisional => BTreeSet::new(),
    })
}

/// The request a `renumber` input makes of `doc` (`max_named`: most entries of `objects`, and
/// most strips read from `strips`).
///
/// # Errors
/// As [`kinds`], [`scope`] and [`strips`]; digits that are not a small whole number.
pub fn request(doc: &Document, input: &RenumberInput, max_named: usize) -> Result<Request, CmdError> {
    let kinds = kinds(input.kind.as_deref())?;
    let scope = scope(doc, input, max_named)?;
    let digits = match input.digits {
        None => None,
        Some(d) => Some(u8::try_from(d).map_err(|_| CmdError::refused(numbering::BAD_DIGITS))?),
    };
    let strips = if kinds.contains(&Kind::Terminals) {
        let named: Vec<String> = input.strips.iter().flatten().flatten().take(max_named).cloned().collect();
        strips(doc, &named, &scope)?
    } else {
        BTreeSet::new()
    };
    // Brief 036 (FP8c): each strip's positions, numbered as groups.
    let positions = strips.iter().map(|c| (*c, crate::terminal_types::position_groups(doc, *c))).collect();
    Ok(Request { kinds, scope, start: input.start, digits, strips, positions })
}

/// `finalise`'s request: the provisional tags and unnumbered nets (design 026 §5.3).
pub fn finalise_request() -> Request {
    Request {
        kinds: [Kind::Devices, Kind::Cables, Kind::Wires].into(),
        scope: Scope::Provisional,
        start: None,
        digits: None,
        strips: BTreeSet::new(),
        positions: Default::default(),
    }
}
