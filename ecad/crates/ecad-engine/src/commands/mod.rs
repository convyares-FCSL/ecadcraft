//! ECAD commands (Path A). Their JSON forms build transactions from resolved inputs (IDs,
//! coordinates) and commit them through [`Project::apply_edit`].
//!
//! They reach the command line, scripts, the control channel and MCP through the engine's
//! extension command registry. `convertcomponent`, `addconnectionpoint`, `place`, `wire`, `connect`,
//! `disconnect`, `cableslash`, `cableview` and `cablecore` also have click-based interactive forms
//! ([`crate::interact`], briefs 008, 004, 005 and 009), started by a bare command name, a menu or a
//! tool button; they commit through these JSON forms. `cableview` records nothing: a cable's
//! view is presentation only ([`crate::cables::views`]).
//! Brief 010 adds `cablereference`, `cablecontinue` and `cablegoto` (with interactive forms),
//! `cablereferences` (read only) ([`crate::references`]).
//! Programmatic calls (`Session::execute`, `name {json}`) always run the JSON form and never
//! prompt.
//!
//! This module is the registry and the shared helpers; each area's commands are in `cmd/<area>`
//! (lane 067 step 11; `cmd/plc/` has the PLC commands).

pub mod input;

use cadcraft_engine::{CommandSpec, EngineError, Session};
use ecad_electrical::CmdError;
use ecad_model::{Color, ConnectionPointDef, Definition, PageId, Primitive, Pt, Style};
use serde_json::Value;

use crate::access::{Pages, owner_of_active, try_lock};
use crate::interact;
use crate::project::Project;
use ecad_electrical::circuits::commands as rating;
use ecad_electrical::continuations::commands as continuation;
use ecad_electrical::numbering::commands as numbering;
use ecad_electrical::properties::commands as prop;
pub use ecad_electrical::terminal_strips::core_label;
use ecad_electrical::{
    cables::commands::cables as cable, devices::commands::device_links as device_link, devices::commands::devices as device,
    locations::commands::designations as designation, locations::commands::locations as location, reports::commands::bom, reports::commands::boxes,
    reports::commands::lists, styles::commands::defaults as default, styles::commands::styles as style,
    terminal_strips::commands::accessories as accessory, terminal_strips::commands::levels as level, wires::commands::connect,
    wires::commands::nets as net,
};

/// The commands this crate adds. Lanes in flight register theirs in [`LANES`], not here;
/// their lines move here at the phase-end tidy-up.
pub fn specs() -> Vec<CommandSpec> {
    let mut v = base_specs();
    v.extend(generic_specs());
    v.extend(crate::pages::commands::sheets::specs());
    v.extend(crate::history::specs());
    v.extend(crate::history::merging::specs());
    v.extend(crate::references::commands::devices::specs());
    v.extend(crate::reports::commands::specs());
    v.extend(crate::terminal_strips::commands::specs());
    v.extend(crate::releases::commands::specs());
    v.extend(crate::styles::commands::specs());
    v.extend(crate::agent::access::specs());
    v.extend(crate::cables::draw::specs());
    v.extend(crate::connectors::commands::specs());
    v.extend(crate::plc::commands::specs());
    v.extend(crate::symbols::commands::specs());
    v.extend(crate::plc::commands::page::specs());
    for (_, lane) in LANES {
        v.extend(lane());
    }
    v
}

/// The generic commands (run by [`crate::generic::run`]) not registered in [`base_specs`].
fn generic_specs() -> Vec<CommandSpec> {
    // `linkdevices`, `unlinkdevices` and `userfield` keep CADCraft's undo grouping, as before.
    let drawing_undo = |c: CommandSpec| CommandSpec { undoable: true, ..c };
    vec![
        crate::generic::spec::<default::SetdefaultParams>(),
        crate::generic::spec::<designation::DesignationsParams>(),
        crate::generic::spec::<designation::AddParams>(),
        crate::generic::spec::<designation::EditParams>(),
        crate::generic::spec::<designation::MergeParams>(),
        crate::generic::spec::<designation::RemoveParams>(),
        crate::generic::spec::<level::TerminalblocklabelParams>(),
        crate::generic::spec::<level::TopositionParams>(),
        crate::generic::spec::<continuation::ContcaptionParams>().interactive(interact::contcaption),
        drawing_undo(crate::generic::spec::<device_link::LinkParams>()),
        drawing_undo(crate::generic::spec::<device_link::UnlinkParams>()),
        drawing_undo(crate::generic::spec::<device_link::UserFieldParams>()),
        crate::generic::spec::<location::SetlocationParams>(),
        crate::generic::spec::<location::SetfunctionParams>(),
        crate::generic::spec::<location::ConflictsParams>(),
        crate::generic::spec::<prop::SetpropParams>(),
        crate::generic::spec::<cable::CablenewParams>(),
        crate::generic::spec::<cable::CableeditParams>(),
        crate::generic::spec::<cable::CableremoveParams>(),
        crate::generic::spec::<style::StylesParams>(),
        crate::generic::spec::<style::StandardParams>(),
        crate::generic::spec::<style::SetstyleParams>(),
        crate::generic::spec::<style::DeletestyleParams>(),
        crate::generic::spec::<style::PromoteParams>(),
        crate::generic::spec::<ecad_electrical::title_blocks::commands::TitleblocksParams>(),
        crate::generic::spec::<ecad_electrical::title_blocks::commands::TitleblockParams>(),
        crate::generic::spec::<bom::PartlineParams>(),
        crate::generic::spec::<bom::BomitemParams>().interactive(interact::bomitem),
        crate::generic::spec::<bom::ArticleParams>(),
        crate::generic::spec::<boxes::LocationboxParams>().interactive(interact::locationbox),
        crate::generic::spec::<boxes::FunctionboxParams>().interactive(interact::functionbox),
        crate::generic::spec::<lists::BomParams>(),
        crate::generic::spec::<lists::WirelistParams>(),
        crate::generic::spec::<lists::CablelistParams>(),
        crate::generic::spec::<ecad_electrical::terminal_strips::commands::place_blocks::TerminalBlocksParams>()
            .interactive(interact::terminal_blocks),
        crate::generic::spec::<ecad_electrical::terminal_strips::commands::reads::StripplanParams>(),
        crate::generic::spec::<ecad_electrical::terminal_strips::commands::reads::CablecoresParams>(),
        crate::generic::spec::<ecad_electrical::plc::commands::racks::PlcslotParams>(),
        crate::generic::spec::<ecad_electrical::plc::commands::racks::PlcracksParams>(),
        crate::generic::spec::<ecad_electrical::plc::commands::racks::PlccardtypeParams>(),
        crate::generic::spec::<ecad_electrical::plc::commands::channel::PlcchannelParams>(),
        crate::generic::spec::<ecad_electrical::plc::commands::iolist::IolistParams>(),
        crate::generic::spec::<ecad_electrical::plc::commands::address::PlcaddressParams>(),
        crate::generic::spec::<ecad_electrical::plc::commands::tags::PlcmaketagsParams>(),
        crate::generic::spec::<ecad_electrical::plc::commands::exchange::PlcexportParams>(),
        crate::generic::spec::<ecad_electrical::plc::commands::exchange::PlcimportParams>(),
        crate::generic::spec::<accessory::AccessoryParams>().interactive(interact::accessory),
        crate::generic::spec::<accessory::JumperParams>().interactive(interact::jumper),
        crate::generic::spec::<rating::RatingParams>(),
        crate::generic::spec::<rating::RatingtableParams>(),
        crate::generic::spec::<rating::RatingsParams>(),
        crate::generic::spec::<numbering::RenumberParams>(),
        crate::generic::spec::<numbering::FinaliseParams>(),
        crate::generic::spec::<numbering::NumberingParams>(),
        crate::generic::spec::<ecad_electrical::pages::commands::page_types::FolderpropsParams>(),
        crate::generic::spec::<ecad_electrical::pages::commands::page_types::RenumberpagesParams>(),
        crate::generic::spec::<ecad_electrical::pages::commands::page_edits::RenamepageParams>(),
        crate::generic::spec::<ecad_electrical::pages::commands::page_edits::MovepageParams>(),
        crate::generic::spec::<ecad_electrical::pages::commands::folders::NewfolderParams>(),
        crate::generic::spec::<ecad_electrical::pages::commands::folders::RenamefolderParams>(),
        crate::generic::spec::<ecad_electrical::pages::commands::folders::MovefolderParams>(),
        crate::generic::spec::<ecad_electrical::pages::commands::folders::DeletefolderParams>(),
        crate::generic::spec::<ecad_electrical::pages::commands::props::PagepropsParams>(),
        crate::generic::spec::<ecad_electrical::pages::commands::props::ProjectpropsParams>(),
        crate::generic::spec::<ecad_electrical::pages::commands::props::RevisionParams>(),
        crate::generic::spec::<ecad_electrical::references::commands::devices::DeviceReferencesParams>(),
        crate::generic::spec::<ecad_electrical::references::commands::devices::ContactsParams>(),
        crate::generic::spec::<ecad_electrical::references::commands::devices::MirrormoveParams>().interactive(interact::mirrormove),
        crate::generic::spec::<ecad_electrical::references::commands::cables::CableReferenceParams>().interactive(interact::cablereference),
        crate::generic::spec::<ecad_electrical::references::commands::cables::CablecontinueParams>().interactive(interact::cablecontinue),
        crate::generic::spec::<ecad_electrical::references::commands::cables::CablereferencesParams>(),
        crate::generic::spec::<device::EcaddeleteParams>(),
        crate::generic::spec::<device::SymbolsParams>(),
        crate::generic::spec::<device::DevicesParams>(),
        crate::generic::spec::<device::TagsettingsParams>(),
        crate::generic::spec::<device::DevicepropsParams>().interactive(interact::deviceprops),
        crate::generic::spec::<device::DevicedeleteParams>().interactive(interact::devicedelete),
        crate::generic::spec::<net::NetsParams>(),
        crate::generic::spec::<net::NetParams>(),
        crate::generic::spec::<net::WirenumberParams>().interactive(interact::wirenumber),
        crate::generic::spec::<net::PotentialParams>(),
        crate::generic::spec::<net::CategoryParams>(),
        crate::generic::spec::<net::WiresettingsParams>(),
        crate::generic::spec::<net::RenumberwiresParams>().interactive(interact::renumberwires),
        crate::generic::spec::<net::ChecksParams>(),
        crate::generic::spec::<ecad_electrical::symbols::builder::NewParams>(),
        crate::generic::spec::<ecad_electrical::symbols::builder::EditParams>(),
        crate::generic::spec::<ecad_electrical::symbols::builder::ImpactParams>(),
        crate::generic::spec::<ecad_electrical::symbols::builder::UpdateParams>(),
    ]
}

/// The generic commands with their schemas.
const GENERIC_TYPED: &[input::Typed] = &[
    crate::generic::typed::<connect::ConnectParams>(),
    crate::generic::typed::<connect::DisconnectParams>(),
    crate::generic::typed::<default::SetdefaultParams>(),
    crate::generic::typed::<designation::DesignationsParams>(),
    crate::generic::typed::<designation::AddParams>(),
    crate::generic::typed::<designation::EditParams>(),
    crate::generic::typed::<designation::MergeParams>(),
    crate::generic::typed::<designation::RemoveParams>(),
    crate::generic::typed::<level::TerminalblocklabelParams>(),
    crate::generic::typed::<level::TopositionParams>(),
    crate::generic::typed::<continuation::ContcaptionParams>(),
    crate::generic::typed::<device_link::LinkParams>(),
    crate::generic::typed::<device_link::UnlinkParams>(),
    crate::generic::typed::<device_link::UserFieldParams>(),
    crate::generic::typed::<location::SetlocationParams>(),
    crate::generic::typed::<location::SetfunctionParams>(),
    crate::generic::typed::<location::ConflictsParams>(),
    crate::generic::typed::<prop::SetpropParams>(),
    crate::generic::typed::<cable::CablenewParams>(),
    crate::generic::typed::<cable::CableeditParams>(),
    crate::generic::typed::<cable::CableremoveParams>(),
    crate::generic::typed::<style::StylesParams>(),
    crate::generic::typed::<style::StandardParams>(),
    crate::generic::typed::<style::SetstyleParams>(),
    crate::generic::typed::<style::DeletestyleParams>(),
    crate::generic::typed::<style::PromoteParams>(),
    crate::generic::typed::<ecad_electrical::title_blocks::commands::TitleblocksParams>(),
    crate::generic::typed::<ecad_electrical::title_blocks::commands::TitleblockParams>(),
    crate::generic::typed::<bom::PartlineParams>(),
    crate::generic::typed::<bom::BomitemParams>(),
    crate::generic::typed::<bom::ArticleParams>(),
    crate::generic::typed::<boxes::LocationboxParams>(),
    crate::generic::typed::<boxes::FunctionboxParams>(),
    crate::generic::typed::<lists::BomParams>(),
    crate::generic::typed::<lists::WirelistParams>(),
    crate::generic::typed::<lists::CablelistParams>(),
    crate::generic::typed::<ecad_electrical::terminal_strips::commands::place_blocks::TerminalBlocksParams>(),
    crate::generic::typed::<ecad_electrical::terminal_strips::commands::reads::StripplanParams>(),
    crate::generic::typed::<ecad_electrical::terminal_strips::commands::reads::CablecoresParams>(),
    crate::generic::typed::<ecad_electrical::plc::commands::racks::PlcslotParams>(),
    crate::generic::typed::<ecad_electrical::plc::commands::racks::PlcracksParams>(),
    crate::generic::typed::<ecad_electrical::plc::commands::racks::PlccardtypeParams>(),
    crate::generic::typed::<ecad_electrical::plc::commands::channel::PlcchannelParams>(),
    crate::generic::typed::<ecad_electrical::plc::commands::iolist::IolistParams>(),
    crate::generic::typed::<ecad_electrical::plc::commands::address::PlcaddressParams>(),
    crate::generic::typed::<ecad_electrical::plc::commands::tags::PlcmaketagsParams>(),
    crate::generic::typed::<ecad_electrical::plc::commands::exchange::PlcexportParams>(),
    crate::generic::typed::<ecad_electrical::plc::commands::exchange::PlcimportParams>(),
    crate::generic::typed::<accessory::AccessoryParams>(),
    crate::generic::typed::<accessory::JumperParams>(),
    crate::generic::typed::<rating::RatingParams>(),
    crate::generic::typed::<rating::RatingtableParams>(),
    crate::generic::typed::<rating::RatingsParams>(),
    crate::generic::typed::<numbering::RenumberParams>(),
    crate::generic::typed::<numbering::FinaliseParams>(),
    crate::generic::typed::<numbering::NumberingParams>(),
    crate::generic::typed::<ecad_electrical::pages::commands::page_types::FolderpropsParams>(),
    crate::generic::typed::<ecad_electrical::pages::commands::page_types::RenumberpagesParams>(),
    crate::generic::typed::<ecad_electrical::pages::commands::page_edits::RenamepageParams>(),
    crate::generic::typed::<ecad_electrical::pages::commands::page_edits::MovepageParams>(),
    crate::generic::typed::<ecad_electrical::pages::commands::folders::NewfolderParams>(),
    crate::generic::typed::<ecad_electrical::pages::commands::folders::RenamefolderParams>(),
    crate::generic::typed::<ecad_electrical::pages::commands::folders::MovefolderParams>(),
    crate::generic::typed::<ecad_electrical::pages::commands::folders::DeletefolderParams>(),
    crate::generic::typed::<ecad_electrical::pages::commands::props::PagepropsParams>(),
    crate::generic::typed::<ecad_electrical::pages::commands::props::ProjectpropsParams>(),
    crate::generic::typed::<ecad_electrical::pages::commands::props::RevisionParams>(),
    crate::generic::typed::<ecad_electrical::references::commands::devices::DeviceReferencesParams>(),
    crate::generic::typed::<ecad_electrical::references::commands::devices::ContactsParams>(),
    crate::generic::typed::<ecad_electrical::references::commands::devices::MirrormoveParams>(),
    crate::generic::typed::<ecad_electrical::references::commands::cables::CableReferenceParams>(),
    crate::generic::typed::<ecad_electrical::references::commands::cables::CablecontinueParams>(),
    crate::generic::typed::<ecad_electrical::references::commands::cables::CablereferencesParams>(),
    crate::generic::typed::<device::EcaddeleteParams>(),
    crate::generic::typed::<device::SymbolsParams>(),
    crate::generic::typed::<device::DevicesParams>(),
    crate::generic::typed::<device::TagsettingsParams>(),
    crate::generic::typed::<device::DevicepropsParams>(),
    crate::generic::typed::<device::DevicedeleteParams>(),
    crate::generic::typed::<net::NetsParams>(),
    crate::generic::typed::<net::NetParams>(),
    crate::generic::typed::<net::WirenumberParams>(),
    crate::generic::typed::<net::PotentialParams>(),
    crate::generic::typed::<net::CategoryParams>(),
    crate::generic::typed::<net::WiresettingsParams>(),
    crate::generic::typed::<net::RenumberwiresParams>(),
    crate::generic::typed::<net::ChecksParams>(),
    crate::generic::typed::<ecad_electrical::symbols::builder::NewParams>(),
    crate::generic::typed::<ecad_electrical::symbols::builder::EditParams>(),
    crate::generic::typed::<ecad_electrical::symbols::builder::ImpactParams>(),
    crate::generic::typed::<ecad_electrical::symbols::builder::UpdateParams>(),
];

fn base_specs() -> Vec<CommandSpec> {
    vec![
        CommandSpec::new("convertcomponent", "Convert to Device", crate::commands::input::typed::<crate::devices::commands::ConvertcomponentParams>)
            .params("{handles?: [hex]} (default: selection)")
            .interactive(interact::convert)
            .noundo(),
        CommandSpec::new("addconnectionpoint", "Add Connection Point", crate::commands::input::typed::<crate::devices::commands::AddConnectionPointParams>)
            .params("{handle: hex (an item of the component), at: [x,y], label?}")
            .interactive(interact::addconnectionpoint)
            .noundo(),
        CommandSpec::new("wire", "Wire", crate::commands::input::typed::<crate::wires::commands::WireParams>)
            .params(
                "{points: [[x,y], …], potential?: string, join?: bool (true)} (ends on connection points attach; an end on another wire's open end joins it into one line, an end on a line's middle lands there (a T); open ends lying on the new line land on it)",
            )
            .interactive(interact::wire)
            .noundo(),
        crate::generic::spec::<connect::ConnectParams>().interactive(interact::connect),
        crate::generic::spec::<connect::DisconnectParams>().interactive(interact::disconnect),
        CommandSpec::new("cableslash", "Cable Slash", crate::commands::input::typed::<crate::cables::commands::CableslashParams>)
            .params("{from: [x,y], to: [x,y], tag?} (the wires the slash crosses become one cable)")
            .interactive(interact::cableslash)
            .noundo(),
        CommandSpec::new("cableview", "Cable View", crate::commands::input::typed::<crate::cables::commands::CableviewParams>)
            .params("{cable?: id, view?: \"traditional\"|\"fan\"} (no cable: every cable's view; presentation only)")
            .interactive(interact::cableview)
            .noundo(),
        CommandSpec::new("cablecore", "Core Label", crate::commands::input::typed::<crate::cables::commands::CablecoreParams>)
            .params(
                "{conductor: id, core: string|null} | {cable: id, cores: [string|null, …]} (null, empty or spaces clear)",
            )
            .interactive(interact::cablecore)
            .noundo(),
        CommandSpec::new("cablefan", "Cable Join", crate::commands::input::typed::<crate::cables::commands::CablefanParams>)
            .params(
                "{cable: id, end?: \"A\"|\"B\", at: [x,y] | fraction 0..1 | null} (where the trunk joins a fanned \
end's spine; null: the middle)",
            )
            .interactive(interact::cablefan)
            .noundo(),
        CommandSpec::new("cablegoto", "Go to Reference", crate::commands::input::typed::<crate::references::commands::cables::CablegotoParams>)
            .params("{reference: id} (opens the other end's page; records nothing)")
            .interactive(interact::cablegoto)
            .noundo(),
        CommandSpec::new("cableadd", "Add to Cable", crate::commands::input::typed::<crate::cables::commands::CableaddParams>)
            .params("{cable?: id, conductors: [id], tag?}")
            .noundo(),
        CommandSpec::new("place", "Place Symbol", crate::commands::input::typed::<crate::devices::commands::PlaceParams>)
            .params(
                "{at: [x,y], rotation?: degrees, mirror?: bool, symbol?: key, definition?: id, device?: id, tag?} (`symbol`: a starter symbol, see `symbols`; `device` adds it to that device; `tag` names a new one, or adds it to the device that has that tag)",
            )
            .interactive(interact::place)
            .noundo(),
        CommandSpec::new("newpage", "New Page", crate::commands::input::typed::<crate::pages::commands::pages::NewpageParams>)
            .params(
                "{title?, number?, folder?: id, after?: page id, type?: page type, generated?: kind, strip?: id, style?: \"table\"|\"graphic\"} (after: right after that page, in its folder; type: \"schematic\", \"network\", \"front\"… (front and report need generated); number: default the next after the page before it, or a folder's start number for its first page; generated: \"cover\", \"contents\", \"revisions\", \"bom-location\", \"terminals\" (strip: one strip; style: the terminal plan's style) or \"cables\")",
            )
            .noundo(),
        CommandSpec::new("openpage", "Open Page", crate::commands::input::typed::<crate::pages::commands::pages::OpenpageParams>).params("{page: id}").noundo(),
        CommandSpec::new("ecadinspect", "Inspect Project", crate::commands::input::typed::<crate::pages::commands::pages::EcadinspectParams>)
            .params("{seq?: n} (read-only snapshot)")
            .noundo(),
        CommandSpec::new("joinwires", "Join Wires", crate::commands::input::typed::<crate::wires::commands::JoinwiresParams>)
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
        crate::wires::commands::TYPED,
        crate::pages::commands::sheets::TYPED,
        crate::reports::commands::TYPED,
        crate::styles::commands::TYPED,
        crate::symbols::commands::TYPED,
        crate::references::commands::devices::TYPED,
        crate::terminal_strips::commands::TYPED,
        crate::plc::commands::TYPED,
        crate::title_blocks::commands::TYPED,
        crate::pages::commands::page_types::TYPED,
        crate::connectors::commands::TYPED,
        crate::releases::commands::TYPED,
        crate::devices::commands::TYPED,
        crate::cables::commands::TYPED,
        crate::references::commands::cables::TYPED,
        crate::pages::commands::pages::TYPED,
        crate::history::TYPED,
        crate::history::merging::TYPED,
        crate::agent::access::TYPED,
        crate::cables::draw::TYPED,
    ]
    .concat()
}

/// Whether generic command `id` declares that it only reads ([`ecad_electrical::commands::run::Access`]).
pub(crate) fn declared_read(id: &str) -> bool {
    GENERIC_TYPED.iter().any(|t| t.id == id && t.access == Some(ecad_electrical::commands::run::Access::Reads))
}

/// The ids of the generic commands that declare that they only read.
pub(crate) fn declared_reads() -> impl Iterator<Item = &'static str> {
    GENERIC_TYPED.iter().filter(|t| t.access == Some(ecad_electrical::commands::run::Access::Reads)).map(|t| t.id)
}

/// The parameters of generic command `id` that name a file, as it declares them.
pub(crate) fn declared_file_keys(id: &str) -> impl Iterator<Item = &'static str> {
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
pub(crate) const MAX_WIRE_POINTS: usize = ecad_model::objects::route::MAX_POINTS;

pub(crate) use ecad_electrical::cables::words::MAX_HANDLES;

pub(crate) fn plain_style(p: &Project) -> Style {
    Style { layer: p.default_layer, color: Color::ByLayer, linetype: "ByLayer".into(), lineweight: -1 }
}

/// The experiment's built-in definition: a two-terminal coil, 10 × 20, terminals A1 (top) and A2
/// (bottom).
pub(crate) fn builtin_coil() -> Definition {
    let r = [(-5.0, -10.0), (5.0, -10.0), (5.0, 10.0), (-5.0, 10.0)];
    Definition {
        bridges: Vec::new(),
        name: "Coil (experiment)".into(),
        origin: None,
        graphics: vec![Primitive::Polyline { vertices: r.iter().map(|(x, y)| Pt::new(*x, *y)).collect(), closed: true }],
        connection_points: vec![
            ConnectionPointDef {
                key: ecad_model::PointKey::def("A1"),
                label: Some("A1".into()),
                at: Pt::new(0.0, 10.0),
                accepts: Default::default(),
            },
            ConnectionPointDef {
                key: ecad_model::PointKey::def("A2"),
                label: Some("A2".into()),
                at: Pt::new(0.0, -10.0),
                accepts: Default::default(),
            },
        ],
        ..Default::default()
    }
}

pub(crate) const LANES: &[(&str, fn() -> Vec<CommandSpec>)] = &[
    // ("NNN", crate::<module>::specs), one line per lane, below this one.
    ("054", crate::pages::commands::page_types::specs),
    ("055", crate::title_blocks::commands::specs),
];

#[cfg(test)]
mod lane_tests {
    /// Lanes register in brief order, once each. A union merge of `lanes.rs` (see that module) can
    /// leave two lanes' lines swapped; this says which to swap.
    #[test]
    fn lanes_are_in_brief_order() {
        let briefs: Vec<&str> = super::LANES.iter().map(|(b, _)| *b).collect();
        for pair in briefs.windows(2) {
            assert!(pair[0] < pair[1], "LANES: {} must come after {}, once each", pair[0], pair[1]);
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
