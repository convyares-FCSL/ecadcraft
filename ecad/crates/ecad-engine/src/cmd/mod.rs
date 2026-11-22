//! ECAD commands (Path A). Their JSON forms build transactions from resolved inputs (IDs,
//! coordinates) and commit them through [`Project::apply_edit`].
//!
//! They reach the command line, scripts, the control channel and MCP through the engine's
//! extension command registry. `convertcomponent`, `addterminal`, `place`, `wire`, `connect`,
//! `disconnect`, `cableslash`, `cableview` and `cablecore` also have click-based interactive forms
//! ([`crate::interact`], briefs 008, 004, 005 and 009), started by a bare command name, a menu or a
//! tool button; they commit through these JSON forms. `cableview` records nothing: a cable's
//! view is presentation only ([`crate::views`]).
//! Brief 010 adds `cableref`, `cablecontinue` and `cablegoto` (with interactive forms),
//! `cablerefs` (read only), `renamepage` and `movepage` ([`crate::references`]).
//! Programmatic calls (`Session::execute`, `name {json}`) always run the JSON form and never
//! prompt.
//!
//! This module is the registry and the shared helpers; each area's commands are in `cmd/<area>`
//! (lane 067 step 11; `cmd/plc/` has the PLC commands).

pub mod accessory;
pub mod bom;
pub mod cable;
pub(crate) mod cable_draw;
pub mod connector;
pub(crate) mod device;
pub(crate) mod generic;
pub mod input;
pub mod numbering;
pub mod output;
pub(crate) mod page;
pub mod page_type;
pub mod plc;
pub mod rating;
pub(crate) mod reference;
pub mod release;
pub mod sheet;
pub mod style;
pub mod symbol;
pub mod terminal;
pub mod text;
pub mod title_block;
pub mod wire;
pub(crate) mod xref;

use cadcraft_engine::{CommandSpec, EngineError, Session};
use ecad_electrical::CmdError;
use ecad_model::{Color, Definition, PageId, Primitive, Pt, Style, TerminalDef};
use serde_json::Value;

use crate::access::{Pages, owner_of_active, try_lock};
use crate::interact;
use crate::project::Project;
pub use crate::strips::core_label;
use ecad_electrical::cmd::{connect, continuation, default, designation, device_link, level, location, prop};

/// The commands this crate adds. Lanes in flight register theirs in [`crate::lanes`], not here;
/// their lines move here at the phase-end tidy-up.
pub fn specs() -> Vec<CommandSpec> {
    let mut v = base_specs();
    v.extend(generic_specs());
    v.extend(crate::cmd::sheet::specs());
    v.extend(crate::history::specs());
    v.extend(crate::merging::specs());
    v.extend(crate::cmd::numbering::specs());
    v.extend(crate::cmd::xref::specs());
    v.extend(crate::cmd::output::specs());
    v.extend(crate::cmd::terminal::specs());
    v.extend(crate::cmd::accessory::specs());
    v.extend(crate::cmd::release::specs());
    v.extend(crate::cmd::text::specs());
    #[cfg(not(target_arch = "wasm32"))]
    v.extend(crate::agent_access::specs());
    v.extend(crate::look::specs());
    v.extend(crate::cmd::connector::specs());
    v.extend(crate::cmd::plc::specs());
    v.extend(crate::cmd::bom::specs());
    v.extend(crate::cmd::symbol::specs());
    v.extend(crate::cmd::style::specs());
    v.extend(crate::cmd::plc::page::specs());
    for (_, lane) in crate::lanes::LANES {
        v.extend(lane());
    }
    v
}

/// The generic commands (run by [`generic::run`]) not registered in [`base_specs`].
fn generic_specs() -> Vec<CommandSpec> {
    // `linkdevices`, `unlinkdevices` and `userfield` keep CADCraft's undo grouping, as before.
    let drawing_undo = |c: CommandSpec| CommandSpec { undoable: true, ..c };
    vec![
        generic::spec::<default::SetdefaultParams>(),
        generic::spec::<designation::DesignationsParams>(),
        generic::spec::<designation::AddParams>(),
        generic::spec::<designation::EditParams>(),
        generic::spec::<designation::MergeParams>(),
        generic::spec::<designation::RemoveParams>(),
        generic::spec::<level::TermlabelParams>(),
        generic::spec::<level::TopositionParams>(),
        generic::spec::<continuation::ContcaptionParams>().interactive(interact::contcaption),
        drawing_undo(generic::spec::<device_link::LinkParams>()),
        drawing_undo(generic::spec::<device_link::UnlinkParams>()),
        drawing_undo(generic::spec::<device_link::UserFieldParams>()),
        generic::spec::<location::SetlocationParams>(),
        generic::spec::<location::SetfunctionParams>(),
        generic::spec::<location::ConflictsParams>(),
        generic::spec::<prop::SetpropParams>(),
    ]
}

/// The generic commands with their schemas.
const GENERIC_TYPED: &[input::Typed] = &[
    generic::typed::<connect::ConnectParams>(),
    generic::typed::<connect::DisconnectParams>(),
    generic::typed::<default::SetdefaultParams>(),
    generic::typed::<designation::DesignationsParams>(),
    generic::typed::<designation::AddParams>(),
    generic::typed::<designation::EditParams>(),
    generic::typed::<designation::MergeParams>(),
    generic::typed::<designation::RemoveParams>(),
    generic::typed::<level::TermlabelParams>(),
    generic::typed::<level::TopositionParams>(),
    generic::typed::<continuation::ContcaptionParams>(),
    generic::typed::<device_link::LinkParams>(),
    generic::typed::<device_link::UnlinkParams>(),
    generic::typed::<device_link::UserFieldParams>(),
    generic::typed::<location::SetlocationParams>(),
    generic::typed::<location::SetfunctionParams>(),
    generic::typed::<location::ConflictsParams>(),
    generic::typed::<prop::SetpropParams>(),
];

fn base_specs() -> Vec<CommandSpec> {
    vec![
        CommandSpec::new("convertcomponent", "Convert to Device", crate::cmd::input::typed::<crate::cmd::device::ConvertcomponentParams>)
            .params("{handles?: [hex]} (default: selection)")
            .interactive(interact::convert)
            .noundo(),
        CommandSpec::new("addterminal", "Add Connection Point", crate::cmd::input::typed::<crate::cmd::device::AddterminalParams>)
            .params("{handle: hex (an item of the component), at: [x,y], label?}")
            .interactive(interact::addterminal)
            .noundo(),
        CommandSpec::new("wire", "Wire", crate::cmd::input::typed::<crate::cmd::wire::WireParams>)
            .params(
                "{points: [[x,y], …], potential?: string, join?: bool (true)} (ends on connection points attach; an end on another wire's open end joins it into one line, an end on a line's middle lands there (a T); open ends lying on the new line land on it)",
            )
            .interactive(interact::wire)
            .noundo(),
        generic::spec::<connect::ConnectParams>().interactive(interact::connect),
        generic::spec::<connect::DisconnectParams>().interactive(interact::disconnect),
        CommandSpec::new("cableslash", "Cable Slash", crate::cmd::input::typed::<crate::cmd::cable_draw::CableslashParams>)
            .params("{from: [x,y], to: [x,y], tag?} (the wires the slash crosses become one cable)")
            .interactive(interact::cableslash)
            .noundo(),
        CommandSpec::new("cableview", "Cable View", crate::cmd::input::typed::<crate::cmd::cable_draw::CableviewParams>)
            .params("{cable?: id, view?: \"traditional\"|\"fan\"} (no cable: every cable's view; presentation only)")
            .interactive(interact::cableview)
            .noundo(),
        CommandSpec::new("cablecore", "Core Label", crate::cmd::input::typed::<crate::cmd::cable_draw::CablecoreParams>)
            .params(
                "{conductor: id, core: string|null} | {cable: id, cores: [string|null, …]} (null, empty or spaces clear)",
            )
            .interactive(interact::cablecore)
            .noundo(),
        CommandSpec::new("cablefan", "Cable Join", crate::cmd::input::typed::<crate::cmd::cable_draw::CablefanParams>)
            .params(
                "{cable: id, end?: \"A\"|\"B\", at: [x,y] | fraction 0..1 | null} (where the trunk joins a fanned \
end's spine; null: the middle)",
            )
            .interactive(interact::cablefan)
            .noundo(),
        CommandSpec::new("cableref", "Cable Reference", crate::cmd::input::typed::<crate::cmd::reference::CablerefParams>)
            .params("{cable: id, at: [x,y]} (a reference end of the cable on this page)")
            .interactive(interact::cableref)
            .noundo(),
        CommandSpec::new("cablecontinue", "Continue Cable", crate::cmd::input::typed::<crate::cmd::reference::CablecontinueParams>)
            .params("{cable: id, at: [x,y], toward: [x,y], members?: [conductor id]} (the cable's second drawing)")
            .interactive(interact::cablecontinue)
            .noundo(),
        CommandSpec::new("cablegoto", "Go to Reference", crate::cmd::input::typed::<crate::cmd::reference::CablegotoParams>)
            .params("{reference: id} (opens the other end's page; records nothing)")
            .interactive(interact::cablegoto)
            .noundo(),
        CommandSpec::new("cablerefs", "List Cable References", crate::cmd::input::typed::<crate::cmd::reference::CablerefsParams>)
            .params("{cable?: id} (read only)")
            .noundo(),
        CommandSpec::new("ecaddelete", "Delete Electrical Object", crate::cmd::input::typed::<crate::cmd::device::EcaddeleteParams>)
            .params("{conductor: id} | {component: id} | {cable: id}")
            .noundo(),
        CommandSpec::new("cableadd", "Add to Cable", crate::cmd::input::typed::<crate::cmd::cable_draw::CableaddParams>)
            .params("{cable?: id, conductors: [id], tag?}")
            .noundo(),
        CommandSpec::new("place", "Place Symbol", crate::cmd::input::typed::<crate::cmd::device::PlaceParams>)
            .params(
                "{at: [x,y], rotation?: degrees, mirror?: bool, symbol?: key, definition?: id, device?: id, tag?} (`symbol`: a starter symbol, see `symbols`; `device` adds it to that device; `tag` names a new one, or adds it to the device that has that tag)",
            )
            .interactive(interact::place)
            .noundo(),
        CommandSpec::new("symbols", "Find Symbol", crate::cmd::input::typed::<crate::cmd::device::SymbolsParams>)
            .params("{query?} (starter symbols and devices matching it; read only)")
            .noundo(),
        CommandSpec::new("devices", "List Devices", crate::cmd::input::typed::<crate::cmd::device::DevicesParams>)
            .params("{tag?} (devices whose tag starts with it; read only)")
            .noundo(),
        CommandSpec::new("tagsettings", "Tag Settings", crate::cmd::input::typed::<crate::cmd::device::TagsettingsParams>)
            .params(
                "{padding?: 1..6, prefixes?: {symbol: prefix|null}} (automatic tags: digits, and this project's prefix per starter symbol; no parameters lists them)",
            )
            .noundo(),
        CommandSpec::new("deviceprops", "Device Properties", crate::cmd::input::typed::<crate::cmd::device::DevicepropsParams>)
            .params("{component: id} (a device's properties; read only; interactive: change them)")
            .interactive(interact::deviceprops)
            .noundo(),
        CommandSpec::new("devicedelete", "Delete Device", crate::cmd::input::typed::<crate::cmd::device::DevicedeleteParams>)
            .params("{component: id} (deletes the device with every symbol, as `ecaddelete`)")
            .interactive(interact::devicedelete)
            .noundo(),
        CommandSpec::new("nets", "List Nets", crate::cmd::input::typed::<crate::cmd::wire::NetsParams>)
            .params("{page?: id, number?, conductor?: id, terminal?: id, all?: bool, limit?: n (200, max 2000), after?: cursor} (read only)")
            .noundo(),
        CommandSpec::new("net", "Show Net", crate::cmd::input::typed::<crate::cmd::wire::NetParams>)
            .params("{conductor: id} | {terminal: id} | {number} (read only)")
            .noundo(),
        CommandSpec::new("wirenumber", "Fix Wire Number", crate::cmd::input::typed::<crate::cmd::wire::WirenumberParams>)
            .params(
                "{conductor: id | number, value: string|null, conflict?: \"swap\"|\"another\"|\"merge\"} | {conductor: id, fixed: bool} (fixes the net's number; null or fixed false releases it; a number another net uses needs `conflict`)",
            )
            .interactive(interact::wirenumber)
            .noundo(),
        CommandSpec::new("potential", "Set Potential", crate::cmd::input::typed::<crate::cmd::wire::PotentialParams>)
            .params("{conductor: id, value: string|null} (the potential of the whole net)")
            .noundo(),
        CommandSpec::new("wiresettings", "Wire Settings", crate::cmd::input::typed::<crate::cmd::wire::WiresettingsParams>)
            .params(
                "{scheme?: \"potential\"|\"page\"|\"page-column\", digits?: 1..6, unclassified_prefix?, potentials?: [{name, prefix, numbered}] (colours and cross-sections: wire categories), colour_style?: \"code2\"|\"code3\"|\"name\", label_height?: mm} (no parameters reads; colours are IEC 60757 codes by default, free text allowed)",
            )
            .noundo(),
        CommandSpec::new("renumberwires", "Renumber Wires", crate::cmd::input::typed::<crate::cmd::wire::RenumberwiresParams>)
            .params("{scope?: \"unnumbered\"|\"all\"} (fixed numbers never change)")
            .interactive(interact::renumberwires)
            .noundo(),
        CommandSpec::new("checks", "Checks", crate::cmd::input::typed::<crate::cmd::wire::ChecksParams>)
            .params("{kinds?: [kind], limit?: n (200, max 2000), after?: n} (read only)")
            .noundo(),
        CommandSpec::new("newpage", "New Page", crate::cmd::input::typed::<crate::cmd::page::NewpageParams>)
            .params(
                "{title?, number?, folder?: id, after?: page id, type?: page type, generated?: kind, strip?: id, style?: \"table\"|\"graphic\"} (after: right after that page, in its folder; type: \"schematic\", \"network\", \"front\"… (front and report need generated); number: default the next after the page before it, or a folder's start number for its first page; generated: \"cover\", \"contents\", \"revisions\", \"bom-location\", \"terminals\" (strip: one strip; style: the terminal plan's style) or \"cables\")",
            )
            .noundo(),
        CommandSpec::new("openpage", "Open Page", crate::cmd::input::typed::<crate::cmd::page::OpenpageParams>).params("{page: id}").noundo(),
        CommandSpec::new("renamepage", "Rename Page", crate::cmd::input::typed::<crate::cmd::page::RenamepageParams>).params("{page: id, title}").noundo(),
        CommandSpec::new("movepage", "Move Page", crate::cmd::input::typed::<crate::cmd::page::MovepageParams>)
            .params("{page: id, index: n} (1 = first) | {page: id, folder: id|null, index?: n} (into a folder, 1 = first of its pages)")
            .noundo(),
        CommandSpec::new("ecadinspect", "Inspect Project", crate::cmd::input::typed::<crate::cmd::page::EcadinspectParams>)
            .params("{seq?: n} (read-only snapshot)")
            .noundo(),
        CommandSpec::new("joinwires", "Join Wires", crate::cmd::input::typed::<crate::cmd::wire::JoinwiresParams>)
            .params("{at: [x,y]} (two open ends there become one line; open ends on a line's middle, or three or more ends, land on one line there (a T))")
            .interactive(interact::joinwires)
            .noundo(),
    ]
}

pub(crate) fn err(m: impl Into<String>) -> EngineError {
    EngineError::Other(m.into())
}

/// Run `f` with the project owning the active document. The closure gets [`Pages`], not the
/// session, so it cannot call an owner-aware session method while the project is locked; the lock
/// is taken with `try_lock`, so a re-entrant command is refused instead of deadlocking. The
/// closure refuses with a [`CmdError`], which becomes text here, at the edge (brief 070).
pub(crate) fn on_project<R>(
    s: &mut Session, f: impl FnOnce(&mut Project, &mut Pages<'_>, PageId) -> Result<R, CmdError>,
) -> cadcraft_engine::Result<R> {
    let owner = owner_of_active(s).map_err(err)?;
    let uid = s.state()?.uid;
    let mut guard = try_lock(&owner).map_err(err)?;
    let p = guard.as_any_mut().downcast_mut::<Project>().ok_or_else(|| err("this page is not owned by an ECAD project"))?;
    let page = p.page_of(uid).ok_or_else(|| err("page view not found"))?;
    if p.pages.get(&page).is_some_and(|v| v.unavailable) {
        return Err(err(UNAVAILABLE));
    }
    f(p, &mut Pages(s), page).map_err(|e| err(e.to_string()))
}

/// Like [`on_project`] for commands that also work on an unavailable page (open, inspect).
pub(crate) fn on_any_page<R>(s: &mut Session, f: impl FnOnce(&mut Project, &mut Pages<'_>) -> Result<R, CmdError>) -> cadcraft_engine::Result<R> {
    let owner = owner_of_active(s).map_err(err)?;
    let mut guard = try_lock(&owner).map_err(err)?;
    let p = guard.as_any_mut().downcast_mut::<Project>().ok_or_else(|| err("this page is not owned by an ECAD project"))?;
    f(p, &mut Pages(s)).map_err(|e| err(e.to_string()))
}

/// Every command with a typed input (brief 070): its id, run function and parameter schema.
pub fn typed_commands() -> Vec<input::Typed> {
    [
        GENERIC_TYPED,
        crate::cmd::wire::TYPED,
        crate::cmd::sheet::TYPED,
        crate::cmd::output::TYPED,
        crate::cmd::text::TYPED,
        crate::cmd::style::TYPED,
        crate::cmd::symbol::TYPED,
        crate::cmd::xref::TYPED,
        crate::cmd::terminal::TYPED,
        crate::cmd::plc::TYPED,
        crate::cmd::numbering::TYPED,
        crate::cmd::rating::TYPED,
        crate::cmd::bom::TYPED,
        crate::cmd::title_block::TYPED,
        crate::cmd::page_type::TYPED,
        crate::cmd::cable::TYPED,
        crate::cmd::connector::TYPED,
        crate::cmd::accessory::TYPED,
        crate::cmd::release::TYPED,
        crate::cmd::device::TYPED,
        crate::cmd::cable_draw::TYPED,
        crate::cmd::reference::TYPED,
        crate::cmd::page::TYPED,
        crate::history::TYPED,
        crate::merging::TYPED,
        #[cfg(not(target_arch = "wasm32"))]
        crate::agent_access::TYPED,
        crate::look::TYPED,
    ]
    .concat()
}

/// Whether generic command `id` declares that it only reads ([`ecad_electrical::cmd::run::Access`]).
pub fn declared_read(id: &str) -> bool {
    GENERIC_TYPED.iter().any(|t| t.id == id && t.access == Some(ecad_electrical::cmd::run::Access::Reads))
}

/// The ids of the generic commands that declare that they only read.
pub fn declared_reads() -> impl Iterator<Item = &'static str> {
    GENERIC_TYPED.iter().filter(|t| t.access == Some(ecad_electrical::cmd::run::Access::Reads)).map(|t| t.id)
}

/// The parameters of generic command `id` that name a file, as it declares them.
pub fn declared_file_keys(id: &str) -> impl Iterator<Item = &'static str> {
    GENERIC_TYPED.iter().filter(move |t| t.id == id).flat_map(|t| t.files.keys())
}

/// The JSON Schema of command `id`'s parameters, when its input is typed.
pub fn schema_of(id: &str) -> Option<Value> {
    typed_commands().into_iter().find(|t| t.id == id).map(|t| (t.schema)())
}

/// Refusal for edits on a page whose drawing could not be rebuilt (PROJ-02).
pub const UNAVAILABLE: &str = "this page is unavailable (its drawing could not be rebuilt); close and reopen it";

/// An optional parameter of a given JSON type: absent or `null` is `None`, a wrong type is an error.
pub(crate) fn opt_typed<'a, T>(p: &'a Value, key: &str, what: &str, get: impl Fn(&'a Value) -> Option<T>) -> Result<Option<T>, CmdError> {
    match p.get(key) {
        None | Some(Value::Null) => Ok(None),
        Some(v) => get(v).map(Some).ok_or_else(|| CmdError::input(format!("`{key}` must be {what}"))),
    }
}

/// Most points one WIRE accepts: a route's own bound (design 072 §4.3).
pub(crate) const MAX_WIRE_POINTS: usize = ecad_model::route::MAX_POINTS;

pub(crate) use crate::words::MAX_HANDLES;

pub(crate) fn plain_style(p: &Project) -> Style {
    Style { layer: p.default_layer, color: Color::ByLayer, linetype: "ByLayer".into(), lineweight: -1 }
}

/// The experiment's built-in definition: a two-terminal coil, 10 × 20, terminals A1 (top) and A2
/// (bottom).
pub fn builtin_coil() -> Definition {
    let r = [(-5.0, -10.0), (5.0, -10.0), (5.0, 10.0), (-5.0, 10.0)];
    Definition {
        bridges: Vec::new(),
        name: "Coil (experiment)".into(),
        origin: None,
        graphics: vec![Primitive::Polyline { vertices: r.iter().map(|(x, y)| Pt::new(*x, *y)).collect(), closed: true }],
        terminals: vec![
            TerminalDef { key: ecad_model::PointKey::def("A1"), label: Some("A1".into()), at: Pt::new(0.0, 10.0), accepts: Default::default() },
            TerminalDef { key: ecad_model::PointKey::def("A2"), label: Some("A2".into()), at: Pt::new(0.0, -10.0), accepts: Default::default() },
        ],
        ..Default::default()
    }
}

#[cfg(test)]
mod lane_tests {
    /// Lanes register in brief order, once each. A union merge of `lanes.rs` (see that module) can
    /// leave two lanes' lines swapped; this says which to swap.
    #[test]
    fn lanes_are_in_brief_order() {
        let briefs: Vec<&str> = crate::lanes::LANES.iter().map(|(b, _)| *b).collect();
        for pair in briefs.windows(2) {
            assert!(pair[0] < pair[1], "crate::lanes::LANES: {} must come after {}, once each", pair[0], pair[1]);
        }
    }

    /// Every ECAD command id is registered once, so the order of the lists in [`super::specs`]
    /// never decides which command a name finds (lookups take the first match).
    #[test]
    fn command_ids_are_unique() {
        let mut seen = std::collections::BTreeSet::new();
        let twice: Vec<&str> = super::specs().iter().map(|c| c.id).filter(|id| !seen.insert(*id)).collect();
        assert!(twice.is_empty(), "registered more than once: {twice:?}");
    }
}
