//! The ECAD command registry ([`specs`]), [`call`] (run a command and read its reply as its type) and
//! the shared helpers of the commands that need the session.
//!
//! Every command has one definition ([`ecad_electrical::commands::request::Request`]: its id, params and
//! reply), whichever kind it is. Its reply becomes JSON only at the edge: the generic runner
//! ([`crate::generic`]), a session command's run ([`input::typed`]), and [`call`] reading it back.
//!
//! A command's JSON form builds a transaction from resolved inputs (IDs, coordinates) and commits it through
//! [`Project::apply_edit`]. Generic commands are declared in `ecad-electrical` and run by the one generic
//! runner; the commands that need the session live in their feature area's `commands` module of this crate
//! (for example [`crate::pages::commands`], [`crate::references::commands`]) and register in [`specs`].
//! Commands reach the command line, scripts, the control channel and MCP through the engine's extension
//! command registry. Some also have click-based forms (each feature's `interact` module), started by a bare command name, a
//! menu or a tool button; those commit through the JSON forms. Programmatic calls (`Session::execute`,
//! `name {json}`) always run the JSON form and never prompt. `cableview` records nothing: a cable's view is
//! presentation only ([`crate::cables::views`]).

pub(crate) mod input;
pub mod reply;

use cadcraft_engine::{CommandSpec, EngineError, Session};
use ecad_electrical::CmdError;
use ecad_electrical::commands::request::Request;
use ecad_model::{Color, PageId, Style};
use serde_json::Value;

use crate::access::{Pages, owner_of_active, try_lock};
use crate::generic::Entry;
use crate::project::Project;
use ecad_electrical::circuits::commands as rating;
use ecad_electrical::continuations::commands as continuation;
use ecad_electrical::numbering::commands as numbering;
use ecad_electrical::properties::commands as prop;
use ecad_electrical::{
    cables::commands::cables as cable, devices::commands::device_links as device_link, devices::commands::devices as device,
    locations::commands::designations as designation, locations::commands::locations as location, reports::commands::bom, reports::commands::boxes,
    reports::commands::lists, styles::commands::defaults as default, styles::commands::styles as style, styles::commands::text,
    terminal_strips::commands::accessories as accessory, terminal_strips::commands::levels as level, wires::commands::connect,
    wires::commands::nets as net,
};

/// The commands this crate adds, each with its parameters text made from its schema ([`help_of`]).
pub fn specs() -> Vec<CommandSpec> {
    let mut v = base_specs();
    v.extend(GENERIC.iter().map(Entry::spec));
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
    v.extend(crate::pages::commands::page_types::specs());
    v.extend(crate::title_blocks::commands::specs());
    for s in &mut v {
        s.params = help_of(s.id);
    }
    v
}

/// The generic commands, each listed once: its registration, its schema and what the agent gate
/// reads come from its entry.
const GENERIC: &[Entry] = &[
    Entry::of::<connect::Connect>().interactive(crate::properties::forms::connect::connect),
    Entry::of::<connect::Disconnect>().interactive(crate::properties::forms::connect::disconnect),
    Entry::json::<default::SetdefaultParams>(),
    Entry::json::<default::DefaultsParams>(),
    Entry::json::<ecad_drawing_style::text::commands::TextstyleParams>(),
    Entry::json::<text::PagetextsParams>(),
    Entry::json::<text::MovetextParams>().interactive(crate::styles::interact::movetext),
    Entry::json::<designation::DesignationsParams>(),
    Entry::json::<designation::AddParams>(),
    Entry::json::<designation::EditParams>(),
    Entry::json::<designation::MergeParams>(),
    Entry::json::<designation::RemoveParams>(),
    Entry::json::<level::TerminalblocklabelParams>(),
    Entry::json::<level::TopositionParams>(),
    Entry::json::<continuation::ContcaptionParams>().interactive(crate::continuations::interact::contcaption),
    Entry::json::<device_link::LinkParams>().undoable(),
    Entry::json::<device_link::UnlinkParams>().undoable(),
    Entry::json::<device_link::UserFieldParams>().undoable(),
    Entry::json::<location::SetlocationParams>(),
    Entry::json::<location::SetfunctionParams>(),
    Entry::json::<location::ConflictsParams>(),
    Entry::json::<prop::SetpropParams>(),
    Entry::json::<cable::CablenewParams>(),
    Entry::json::<cable::CableeditParams>(),
    Entry::json::<cable::CableremoveParams>(),
    Entry::json::<style::StylesParams>(),
    Entry::json::<style::StandardParams>(),
    Entry::json::<style::SetstyleParams>(),
    Entry::json::<style::DeletestyleParams>(),
    Entry::json::<style::PromoteParams>(),
    Entry::json::<ecad_drawing_style::title_blocks::commands::TitleblocksParams>(),
    Entry::json::<ecad_drawing_style::title_blocks::commands::TitleblockParams>(),
    Entry::json::<bom::PartlineParams>(),
    Entry::json::<bom::BomitemParams>().interactive(crate::reports::interact::bomitem),
    Entry::json::<bom::ArticleParams>(),
    Entry::json::<boxes::LocationboxParams>().interactive(crate::locations::interact::locationbox),
    Entry::json::<boxes::FunctionboxParams>().interactive(crate::locations::interact::functionbox),
    Entry::json::<lists::BomParams>(),
    Entry::json::<lists::WirelistParams>(),
    Entry::json::<lists::CablelistParams>(),
    Entry::json::<ecad_electrical::terminal_strips::commands::place_blocks::TerminalBlocksParams>()
        .interactive(crate::terminal_strips::interact::terminal_blocks::terminal_blocks),
    Entry::json::<ecad_electrical::terminal_strips::commands::reads::StripplanParams>(),
    Entry::json::<ecad_electrical::terminal_strips::commands::reads::CablecoresParams>(),
    Entry::json::<ecad_electrical::plc::commands::racks::PlcslotParams>(),
    Entry::json::<ecad_electrical::plc::commands::racks::PlcracksParams>(),
    Entry::json::<ecad_electrical::plc::commands::racks::PlccardtypeParams>(),
    Entry::json::<ecad_electrical::plc::commands::channel::PlcchannelParams>(),
    Entry::json::<ecad_electrical::plc::commands::iolist::IolistParams>(),
    Entry::json::<ecad_electrical::plc::commands::address::PlcaddressParams>(),
    Entry::json::<ecad_electrical::plc::commands::tags::PlcmaketagsParams>(),
    Entry::json::<ecad_electrical::plc::commands::exchange::PlcexportParams>(),
    Entry::json::<ecad_electrical::plc::commands::exchange::PlcimportParams>(),
    Entry::json::<accessory::AccessoryParams>().interactive(crate::terminal_strips::interact::accessories::accessory),
    Entry::json::<accessory::JumperParams>().interactive(crate::terminal_strips::interact::accessories::jumper),
    Entry::json::<rating::RatingParams>(),
    Entry::json::<rating::RatingtableParams>(),
    Entry::json::<rating::RatingsParams>(),
    Entry::json::<numbering::RenumberParams>(),
    Entry::json::<numbering::FinaliseParams>(),
    Entry::json::<numbering::NumberingParams>(),
    Entry::json::<ecad_electrical::pages::commands::page_types::FolderpropsParams>(),
    Entry::json::<ecad_electrical::pages::commands::page_types::RenumberpagesParams>(),
    Entry::json::<ecad_electrical::pages::commands::page_edits::RenamepageParams>(),
    Entry::json::<ecad_electrical::pages::commands::page_edits::MovepageParams>(),
    Entry::json::<ecad_electrical::pages::commands::folders::NewfolderParams>(),
    Entry::json::<ecad_electrical::pages::commands::folders::RenamefolderParams>(),
    Entry::json::<ecad_electrical::pages::commands::folders::MovefolderParams>(),
    Entry::json::<ecad_electrical::pages::commands::folders::DeletefolderParams>(),
    Entry::json::<ecad_electrical::pages::commands::props::PagepropsParams>(),
    Entry::json::<ecad_electrical::pages::commands::props::ProjectpropsParams>(),
    Entry::json::<ecad_electrical::pages::commands::props::RevisionParams>(),
    Entry::json::<ecad_electrical::references::commands::devices::DeviceReferencesParams>(),
    Entry::json::<ecad_electrical::references::commands::devices::ContactsParams>(),
    Entry::json::<ecad_electrical::references::commands::devices::MirrormoveParams>().interactive(crate::references::interact::devices::mirrormove),
    Entry::json::<ecad_electrical::references::commands::cables::CableReferenceParams>()
        .interactive(crate::references::interact::cables::cablereference),
    Entry::json::<ecad_electrical::references::commands::cables::CablecontinueParams>()
        .interactive(crate::references::interact::cables::cablecontinue),
    Entry::json::<ecad_electrical::references::commands::cables::CablereferencesParams>(),
    Entry::json::<device::EcaddeleteParams>(),
    Entry::json::<device::SymbolsParams>(),
    Entry::json::<device::DevicesParams>(),
    Entry::json::<device::TagsettingsParams>(),
    Entry::json::<device::DevicepropsParams>().interactive(crate::devices::interact::deviceprops),
    Entry::json::<device::DevicedeleteParams>().interactive(crate::devices::interact::devicedelete),
    Entry::of::<net::Nets>(),
    Entry::of::<net::Net>(),
    Entry::of::<net::Wirenumber>().interactive(crate::wires::interact::wirenumber),
    Entry::of::<net::Potential>(),
    Entry::of::<net::Category>(),
    Entry::of::<net::Wiresettings>(),
    Entry::of::<net::Renumberwires>().interactive(crate::wires::interact::renumberwires),
    Entry::of::<net::Checks>(),
    Entry::json::<ecad_electrical::symbols::builder::NewParams>(),
    Entry::json::<ecad_electrical::symbols::builder::EditParams>(),
    Entry::json::<ecad_electrical::symbols::builder::ImpactParams>(),
    Entry::json::<ecad_electrical::symbols::builder::UpdateParams>(),
];

fn base_specs() -> Vec<CommandSpec> {
    vec![
        CommandSpec::new(
            "convertcomponent",
            "Convert to Device",
            crate::commands::input::run_json::<crate::devices::commands::ConvertcomponentParams>,
        )
        .interactive(crate::properties::forms::convert::convert)
        .noundo(),
        CommandSpec::new(
            "addconnectionpoint",
            "Add Connection Point",
            crate::commands::input::run_json::<crate::devices::commands::AddConnectionPointParams>,
        )
        .interactive(crate::properties::forms::add_connection_point::addconnectionpoint)
        .noundo(),
        CommandSpec::new("wire", "Wire", crate::commands::input::typed::<crate::wires::commands::Wire>)
            .interactive(crate::properties::forms::wire::wire)
            .noundo(),
        CommandSpec::new("cableslash", "Cable Slash", crate::commands::input::run_json::<crate::cables::commands::CableslashParams>)
            .interactive(crate::properties::forms::cable_slash::cableslash)
            .noundo(),
        CommandSpec::new("cableview", "Cable View", crate::commands::input::run_json::<crate::cables::commands::CableviewParams>)
            .interactive(crate::properties::forms::cable_view::cableview)
            .noundo(),
        CommandSpec::new("cablecore", "Core Label", crate::commands::input::run_json::<crate::cables::commands::CablecoreParams>)
            .interactive(crate::properties::forms::cable_core::cablecore)
            .noundo(),
        CommandSpec::new("cablefan", "Cable Join", crate::commands::input::run_json::<crate::cables::commands::CablefanParams>)
            .interactive(crate::properties::forms::cable_fan::cablefan)
            .noundo(),
        CommandSpec::new("cablegoto", "Go to Reference", crate::commands::input::run_json::<crate::references::commands::cables::CablegotoParams>)
            .interactive(crate::references::interact::cables::cablegoto)
            .noundo(),
        CommandSpec::new("cableadd", "Add to Cable", crate::commands::input::run_json::<crate::cables::commands::CableaddParams>).noundo(),
        CommandSpec::new("place", "Place Symbol", crate::commands::input::run_json::<crate::devices::commands::PlaceParams>)
            .interactive(crate::devices::interact::place)
            .noundo(),
        CommandSpec::new("newpage", "New Page", crate::commands::input::run_json::<crate::pages::commands::pages::NewpageParams>).noundo(),
        CommandSpec::new("openpage", "Open Page", crate::commands::input::run_json::<crate::pages::commands::pages::OpenpageParams>).noundo(),
        CommandSpec::new("ecadinspect", "Inspect Project", crate::commands::input::run_json::<crate::pages::commands::pages::EcadinspectParams>)
            .noundo(),
        CommandSpec::new("joinwires", "Join Wires", crate::commands::input::typed::<crate::wires::commands::Joinwires>)
            .interactive(crate::wires::interact::joinwires)
            .noundo(),
    ]
}

pub(crate) fn err(m: impl Into<String>) -> EngineError {
    EngineError::Other(m.into())
}

/// Run `f` with the project owning the active document. The closure gets [`Pages`], not the
/// session, so it cannot call an owner-aware session method while the project is locked; the lock
/// is taken with `try_lock`, so a re-entrant command is refused instead of deadlocking. The
/// closure refuses with a [`CmdError`], which becomes text here, at the edge.
pub(crate) fn on_project<R>(
    s: &mut Session, f: impl FnOnce(&mut Project, &mut Pages<'_>, PageId) -> Result<R, CmdError>,
) -> cadcraft_engine::Result<R> {
    in_project(s, f).map_err(|e| err(e.to_string()))
}

/// [`on_project`] for a typed session command ([`input::Command`]): the refusal stays a
/// [`CmdError`]; the session's own (no project, a busy one) is said in words.
pub(crate) fn in_project<R>(s: &mut Session, f: impl FnOnce(&mut Project, &mut Pages<'_>, PageId) -> Result<R, CmdError>) -> Result<R, CmdError> {
    let owner = owner_of_active(s).map_err(CmdError::refused)?;
    let uid = s.state().map_err(|e| CmdError::refused(e.to_string()))?.uid;
    let mut guard = try_lock(&owner).map_err(CmdError::refused)?;
    let p = guard.as_any_mut().downcast_mut::<Project>().ok_or(CmdError::refused("this page is not owned by an ECAD project"))?;
    let page = p.page_of(uid).ok_or(CmdError::refused("page view not found"))?;
    if p.pages.get(&page).is_some_and(|v| v.unavailable) {
        return Err(CmdError::refused(UNAVAILABLE));
    }
    f(p, &mut Pages(s), page)
}

/// Run command `C` with `params` through the session, as the command line, scripts and MCP do (its
/// echo, undo and agent gate included), and read its reply as its type. JSON is only how the call
/// travels: this is its one edge on the caller's side.
///
/// # Errors
/// The command's refusal (in the words the session gives), or a reply that does not read as
/// `C::Reply` (a command whose edit was committed but whose reply could not be made says so).
///
/// # Example
/// ```no_run
/// # fn f(s: &mut ecad_engine::cad::Session) -> Result<(), ecad_electrical::CmdError> {
/// use ecad_electrical::wires::commands::nets::{Net, NetParams};
/// let net = ecad_engine::commands::call::<Net>(s, NetParams { number: Some("24V-01".into()), ..NetParams::default() })?;
/// println!("{} wires", net.conductors.len());
/// # Ok(()) }
/// ```
pub fn call<C: Request>(s: &mut Session, params: C::Params) -> Result<C::Reply, CmdError> {
    let reply = s.execute(C::ID, &params_json::<C>(&params)?).map_err(|e| CmdError::refused(e.to_string()))?;
    reply_of::<C>(reply)
}

/// `params` as the JSON command `C` reads (for a caller that runs it through its own path, such as
/// the app's command line).
///
/// # Errors
/// Params that cannot be written as JSON (never expected).
pub fn params_json<C: Request>(params: &C::Params) -> Result<Value, CmdError> {
    serde_json::to_value(params).map_err(|e| CmdError::input(format!("{}: the parameters could not be written: {e}", C::ID)))
}

/// Command `C` with `params` as one command line (`name {json}`), for a caller that runs lines as a
/// script (the settings dialogs' OK).
///
/// # Errors
/// Params that cannot be written as JSON (never expected).
pub fn line<C: Request>(params: &C::Params) -> Result<String, CmdError> {
    Ok(format!("{} {}", C::ID, params_json::<C>(params)?))
}

/// Command `C`'s JSON reply read as its type.
///
/// # Errors
/// A reply that does not read as `C::Reply`; its `message` is said when it has one.
pub fn reply_of<C: Request>(reply: Value) -> Result<C::Reply, CmdError> {
    let message = reply.get("message").and_then(Value::as_str).map(str::to_string);
    serde_json::from_value(reply).map_err(|e| CmdError::refused(message.unwrap_or_else(|| format!("{}: the reply does not read: {e}", C::ID))))
}

/// Like [`on_project`] for commands that also work on an unavailable page (open, inspect).
pub(crate) fn on_any_page<R>(s: &mut Session, f: impl FnOnce(&mut Project, &mut Pages<'_>) -> Result<R, CmdError>) -> cadcraft_engine::Result<R> {
    let owner = owner_of_active(s).map_err(err)?;
    let mut guard = try_lock(&owner).map_err(err)?;
    let p = guard.as_any_mut().downcast_mut::<Project>().ok_or_else(|| err("this page is not owned by an ECAD project"))?;
    f(p, &mut Pages(s)).map_err(|e| err(e.to_string()))
}

/// Every command with a typed input: its id, run function and parameter schema.
pub fn typed_commands() -> Vec<input::Typed> {
    let mut v: Vec<input::Typed> = GENERIC.iter().map(|e| e.typed).collect();
    v.extend(
        [
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
        .concat(),
    );
    v
}

/// Whether generic command `id` declares that it only reads ([`ecad_electrical::commands::run::Access`]).
pub(crate) fn declared_read(id: &str) -> bool {
    GENERIC.iter().map(|e| &e.typed).any(|t| t.id == id && t.access == Some(ecad_electrical::commands::run::Access::Reads))
}

/// The ids of the generic commands that declare that they only read.
pub(crate) fn declared_reads() -> impl Iterator<Item = &'static str> {
    GENERIC.iter().map(|e| &e.typed).filter(|t| t.access == Some(ecad_electrical::commands::run::Access::Reads)).map(|t| t.id)
}

/// The parameters of generic command `id` that name a file, as it declares them.
pub(crate) fn declared_file_keys(id: &str) -> impl Iterator<Item = &'static str> {
    GENERIC.iter().map(|e| &e.typed).filter(move |t| t.id == id).flat_map(|t| t.files.keys())
}

/// The JSON Schema of command `id`'s parameters, when its input is typed.
pub fn schema_of(id: &str) -> Option<Value> {
    typed_commands().into_iter().find(|t| t.id == id).map(|t| (t.schema)())
}

/// The JSON Schema of command `id`'s reply, when its reply is typed.
pub fn reply_schema_of(id: &str) -> Option<Value> {
    typed_commands().into_iter().find(|t| t.id == id).and_then(|t| t.reply).map(|f| f())
}

/// The parameters text of typed command `id` (`help`, the command lists of MCP and the agent), made from
/// its schema by [`ecad_electrical::commands::input::help`]: its params struct's docs are the one source.
/// Empty for a command whose input is not typed.
pub fn help_of(id: &str) -> &'static str {
    static HELP: std::sync::OnceLock<std::collections::BTreeMap<&'static str, String>> = std::sync::OnceLock::new();
    let all = HELP.get_or_init(|| typed_commands().into_iter().map(|t| (t.id, ecad_electrical::commands::input::help(&(t.schema)()))).collect());
    all.get(id).map_or("", String::as_str)
}

/// Refusal for edits on a page whose drawing could not be rebuilt; the page takes edits again once it is closed
/// and reopened.
pub const UNAVAILABLE: &str = "this page is unavailable (its drawing could not be rebuilt); close and reopen it";

/// An optional parameter of a given JSON type: absent or `null` is `None`, a wrong type is an error.
pub(crate) fn opt_typed<'a, T>(p: &'a Value, key: &str, what: &str, get: impl Fn(&'a Value) -> Option<T>) -> Result<Option<T>, CmdError> {
    match p.get(key) {
        None | Some(Value::Null) => Ok(None),
        Some(v) => get(v).map(Some).ok_or_else(|| CmdError::input(format!("`{key}` must be {what}"))),
    }
}

/// Most points one WIRE accepts: a route's own bound.
pub(crate) const MAX_WIRE_POINTS: usize = ecad_model::objects::route::MAX_POINTS;

pub(crate) use ecad_electrical::commands::input::MAX_HANDLES;

pub(crate) fn plain_style(p: &Project) -> Style {
    Style { layer: p.default_layer, color: Color::ByLayer, linetype: "ByLayer".into(), lineweight: -1 }
}
