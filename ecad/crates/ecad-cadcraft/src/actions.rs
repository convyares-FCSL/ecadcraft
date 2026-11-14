//! The action table (brief 041, design `ui-architecture.md` §7.2): where a person finds each
//! command. A command ([`cadcraft_engine::CommandSpec`]) says **what** happens; an [`Action`]
//! says where it appears: in a tool group's flyout, in fixed slots of an object kind's wheel, on a
//! key, and under which words Ctrl+K finds it. Nothing here defines a command: every action runs
//! a registered command by id (or, for the few things only the window does, such as showing a
//! row, names [`Params::App`]); a test checks every id is registered and every user-visible ECAD
//! command appears in an action or in [`PROGRAMMATIC_ONLY`].
//!
//! # Semantics
//! - **Tool groups** (UI rev 3: Select, Draw, Text | Device, Terminal, PLC | Wire, Cable |
//!   Location) are the actions with a `group`, in order; a tool whose command does not exist yet
//!   is not listed. A group with a key (V D T W C) picks its last-used tool and steps.
//! - **The wheel** of a kind is the actions naming that kind in `wheel`, at their slot. Slots mean
//!   the same on every wheel (spec §5): [`Slot::Bottom`] deletes, [`Slot::Right`] goes to what the
//!   object relates to, [`Slot::BottomRight`] shows its row; the three on the left copy, turn or
//!   change it. An empty slot stays empty.
//! - **Keys** act on the selection while the pointer is over the page and nothing prompts; no
//!   two actions share one.
//! - **MCP** keeps exposing commands, not actions: this table adds nothing an agent needs.
//!
//! This module is data and pure functions: no UI toolkit, no project reads.

use ecad_tokens::Icon;

use crate::interact::kind::Target;

/// The kinds of thing a wheel is for (spec §5).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum ObjectKind {
    /// A device's symbol (a tagged or untagged component, not a terminal strip).
    Device,
    /// A wire, away from its ends.
    Wire,
    /// A wire's end (attached or loose).
    WireEnd,
    /// A terminal of a terminal strip (the DIN-rail device), or the strip's link line.
    Terminal,
    /// A cable's slash or fan.
    Cable,
    /// A text: a device's tag, a wire's label, or free text.
    Text,
    /// A cable reference end or a device cross-reference.
    Reference,
    /// The page where nothing is under the cursor.
    EmptyPage,
    /// A page's row in the Pages tab.
    PageRow,
    /// More than one object selected, or plain drawn geometry.
    Mixed,
}

impl ObjectKind {
    pub const ALL: [ObjectKind; 10] = [
        ObjectKind::Device,
        ObjectKind::Wire,
        ObjectKind::WireEnd,
        ObjectKind::Terminal,
        ObjectKind::Cable,
        ObjectKind::Text,
        ObjectKind::Reference,
        ObjectKind::EmptyPage,
        ObjectKind::PageRow,
        ObjectKind::Mixed,
    ];

    /// Its name in words.
    pub const fn words(self) -> &'static str {
        match self {
            ObjectKind::Device => "device",
            ObjectKind::Wire => "wire",
            ObjectKind::WireEnd => "wire end",
            ObjectKind::Terminal => "terminal",
            ObjectKind::Cable => "cable",
            ObjectKind::Text => "text",
            ObjectKind::Reference => "reference",
            ObjectKind::EmptyPage => "page",
            ObjectKind::PageRow => "page row",
            ObjectKind::Mixed => "selection",
        }
    }
}

/// A wheel slot, clockwise from the top (spec §5).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Slot {
    Top,
    TopRight,
    Right,
    BottomRight,
    Bottom,
    BottomLeft,
    Left,
    TopLeft,
}

impl Slot {
    /// The eight slots, clockwise from the top.
    pub const ALL: [Slot; 8] = [Slot::Top, Slot::TopRight, Slot::Right, Slot::BottomRight, Slot::Bottom, Slot::BottomLeft, Slot::Left, Slot::TopLeft];

    /// Its index in [`Slot::ALL`] (clockwise from the top).
    pub fn index(self) -> usize {
        Slot::ALL.iter().position(|s| *s == self).unwrap_or(0)
    }

    /// Whether it is one of the three on the left (copy, turn or change it).
    pub fn left_side(self) -> bool {
        matches!(self, Slot::BottomLeft | Slot::Left | Slot::TopLeft)
    }
}

/// The tool groups of the Draw workspace (UI rev 3, "Tool strip"), in strip order. Dividers sit
/// between Text and Device, PLC and Wire, and Cable and Location.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum ToolGroup {
    Select,
    Draw,
    Text,
    /// Place a symbol or a connector.
    Device,
    Terminal,
    Plc,
    /// Wire (and, later, bridge and continuation).
    Wire,
    Cable,
    /// Location box (and, later, function box and device or PLC outline).
    Location,
}

impl ToolGroup {
    pub const ALL: [ToolGroup; 9] = [
        ToolGroup::Select,
        ToolGroup::Draw,
        ToolGroup::Text,
        ToolGroup::Device,
        ToolGroup::Terminal,
        ToolGroup::Plc,
        ToolGroup::Wire,
        ToolGroup::Cable,
        ToolGroup::Location,
    ];

    /// Its key, if it keeps one. UI rev 3 keeps a key only where a group of 041's survives
    /// (Select V, Draw D, Text T, Wire W, Cable C); Device, Terminal, PLC and Location have none
    /// (P now reopens the symbol picker while placing).
    pub const fn letter(self) -> Option<char> {
        match self {
            ToolGroup::Select => Some('V'),
            ToolGroup::Draw => Some('D'),
            ToolGroup::Text => Some('T'),
            ToolGroup::Wire => Some('W'),
            ToolGroup::Cable => Some('C'),
            ToolGroup::Device | ToolGroup::Terminal | ToolGroup::Plc | ToolGroup::Location => None,
        }
    }

    /// Its name, the label under its button.
    pub const fn name(self) -> &'static str {
        match self {
            ToolGroup::Select => "Select",
            ToolGroup::Draw => "Draw",
            ToolGroup::Text => "Text",
            ToolGroup::Device => "Device",
            ToolGroup::Terminal => "Terminal",
            ToolGroup::Plc => "PLC",
            ToolGroup::Wire => "Wire",
            ToolGroup::Cable => "Cable",
            ToolGroup::Location => "Location",
        }
    }

    /// Its icon on the strip.
    pub const fn icon(self) -> Icon {
        match self {
            ToolGroup::Select => Icon::Select,
            ToolGroup::Draw => Icon::Pencil,
            ToolGroup::Text => Icon::Text,
            ToolGroup::Device => Icon::Pin,
            ToolGroup::Terminal => Icon::TerminalPin,
            ToolGroup::Plc => Icon::Plc,
            ToolGroup::Wire => Icon::WireBend,
            ToolGroup::Cable => Icon::CableBundle,
            ToolGroup::Location => Icon::LocationBox,
        }
    }

    /// Whether a divider follows it on the strip.
    pub const fn divider_after(self) -> bool {
        matches!(self, ToolGroup::Text | ToolGroup::Plc | ToolGroup::Cable)
    }

    /// Its tools, in flyout order.
    pub fn tools(self) -> Vec<&'static Action> {
        let mut v: Vec<&Action> = ACTIONS.iter().filter(|a| a.group.is_some_and(|(g, _)| g == self)).collect();
        v.sort_by_key(|a| a.group.map_or(0, |(_, i)| i));
        v
    }
}

/// A key an action is on (spec §5, "Keyboard that remains"). Ctrl is Cmd on macOS.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Key {
    Delete,
    F2,
    R,
    X,
    CtrlC,
    CtrlV,
    CtrlD,
    CtrlZ,
    CtrlY,
}

/// The keys the shell itself takes (group letters, Esc, F, `]`, PgUp/PgDn, Ctrl+1–4, Ctrl+K, Ctrl+/,
/// Alt+Left/Right), in words: an action may not use them.
pub const SHELL_KEYS: &[&str] =
    &["V", "D", "T", "W", "C", "F", "]", "Esc", "PgUp", "PgDn", "Ctrl+1", "Ctrl+2", "Ctrl+3", "Ctrl+4", "Ctrl+K", "Ctrl+/"];

impl Key {
    /// Its name as a key cap shows it.
    pub const fn words(self) -> &'static str {
        match self {
            Key::Delete => "Del",
            Key::F2 => "F2",
            Key::R => "R",
            Key::X => "X",
            Key::CtrlC => "Ctrl+C",
            Key::CtrlV => "Ctrl+V",
            Key::CtrlD => "Ctrl+D",
            Key::CtrlZ => "Ctrl+Z",
            Key::CtrlY => "Ctrl+Y",
        }
    }
}

/// Which id of the object an [`Params::Id`] action passes, and under which JSON key.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IdOf {
    /// `{"component": id}`.
    Component,
    /// `{"conductor": id}`.
    Conductor,
    /// `{"cable": id}`.
    Cable,
    /// `{"source": representation id}` (a reference's go-to).
    Source,
    /// `{"conductors": [id]}`.
    Conductors,
    /// Whichever of component, conductor or cable the object is (`ecaddelete`).
    Electrical,
    /// `{"conductor": id, "end": "A"|"B"}`.
    End,
}

/// What the window itself does for an [`Params::App`] action.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum App {
    /// Back to selecting: end the running command.
    Select,
    /// Select the object and show the right panel (the inspector).
    Properties,
    /// Go to the device's next symbol on another page (its contacts or coil).
    GotoOther,
    /// Lists ▸ Device list.
    DeviceRow,
    /// Lists ▸ Terminal plan.
    TerminalRow,
    /// Lists ▸ Reports (the cable report).
    CableRow,
    /// Select every drawn wire of the wire's net on this page.
    ShowNet,
    /// Go to the cable's reference on another page.
    CableReference,
    /// Preview a cable reference's other end without leaving the page.
    PreviewReference,
    /// Edit a text: a tag in Device Properties, a wire label in Fix number, free text in place.
    EditText,
    /// Clear the selection and show the page's properties in the right panel.
    PageProperties,
    /// The Pages tab with the page's row.
    PageRow,
    /// Show the page.
    OpenPage,
    /// A new page after this one (the Pages tab's New page).
    NewPage,
    /// Rename the page (the Pages tab's rename).
    RenamePage,
    /// Preview the page without leaving the one shown.
    PreviewPage,
    /// A new folder beside the page's row.
    NewFolder,
    /// Open the symbol search (Place ▸ Symbol's finder).
    FindSymbol,
    /// The Renumber dialog (devices; wires; finalising).
    Renumber,
    RenumberWires,
    Finalise,
    /// Project settings (the wire settings are a section of it).
    WireSettings,
    /// Open the Text panel's command field with a template for the object (no form exists yet).
    Template,
}

/// How an action runs its command on the object (or the selection).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Params {
    /// Start the command's prompts (a tool).
    Start,
    /// Start the command's prompts, then give it the point the wheel opened at as the click that
    /// picks the object (the form then asks for what else it needs).
    StartAt,
    /// Select the object's drawn handles (the selection, for a key), then start the command, which
    /// takes the selection first.
    Selected,
    /// Run the JSON form now with the object's id: one transaction.
    Id(IdOf),
    /// Run the JSON form now with `{"at": [x, y]}`, the point the wheel opened at.
    At,
    /// A short list of up to eight choices ([`Choices`]); the choice completes the JSON form.
    Choose(Choices),
    /// Done by the window ([`App`]); `command` names what it reads, if anything.
    App(App),
}

/// A slot's short list (spec §5: one list of up to eight choices, never a second wheel).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Choices {
    /// The project's potentials (`potential {conductor, value}`), and "None".
    Potentials,
}

/// Most choices a slot's list shows.
pub const MAX_CHOICES: usize = 8;

/// Whether an action can run now.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Enabled {
    Ok,
    /// Greyed, in place: why, and the next step offered (an action id), if any.
    Greyed {
        why: String,
        next: Option<&'static str>,
    },
}

/// What `enabled` reads: the object, and whether the project takes writes.
#[derive(Debug, Clone, Copy)]
pub struct Context<'a> {
    pub target: Option<&'a Target>,
    /// The project is open for writing (not read only, no revision shown).
    pub writable: bool,
}

/// One action (design §7.2).
#[derive(Clone, Copy)]
pub struct Action {
    /// Stable id: `wire.fixnumber`.
    pub id: &'static str,
    /// The command it runs (a registered command id); empty only for an [`Params::App`] action
    /// that reads no command.
    pub command: &'static str,
    pub params: Params,
    pub label: &'static str,
    pub icon: Icon,
    /// Tool group and order in its flyout.
    pub group: Option<(ToolGroup, u8)>,
    /// Its fixed slots, per kind.
    pub wheel: &'static [(ObjectKind, Slot)],
    pub key: Option<Key>,
    /// Extra words Ctrl+K finds it by (with the label).
    pub search: &'static [&'static str],
    /// Whether Ctrl+K lists it (Undo and Redo are keys only, owner answer 8).
    pub searchable: bool,
    pub enabled: fn(&Context) -> Enabled,
}

impl std::fmt::Debug for Action {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Action").field("id", &self.id).field("command", &self.command).finish_non_exhaustive()
    }
}

impl Action {
    /// Whether it changes the project (a read, a go-to or a view change does not).
    pub fn writes(&self) -> bool {
        !matches!(self.params, Params::App(_)) && !READS.contains(&self.command)
    }

    /// Whether it starts a tool that keeps prompting ([`Params::Start`]).
    pub fn is_tool(&self) -> bool {
        self.params == Params::Start
    }

    /// Whether Ctrl+K finds it for `query` (every word of the query in its label or search words,
    /// case-insensitive).
    pub fn matches(&self, query: &str) -> bool {
        if !self.searchable {
            return false;
        }
        let hay = format!("{} {} {}", self.label, self.search.join(" "), self.command).to_lowercase();
        let words: Vec<String> = query.split_whitespace().map(str::to_lowercase).collect();
        !words.is_empty() && words.iter().all(|w| hay.contains(w.as_str()))
    }
}

/// Commands that change nothing in the project (reads, views and go-tos).
const READS: &[&str] = &["cableview", "refgoto", "cablegoto", "net", "contacts", "symbols", "stripplan"];

/// The project is read only.
pub const READ_ONLY: &str = "The project is read only.";

fn ok(_: &Context) -> Enabled {
    Enabled::Ok
}

/// Writes need a writable project.
fn write(c: &Context) -> Enabled {
    if c.writable { Enabled::Ok } else { Enabled::Greyed { why: READ_ONLY.into(), next: None } }
}

/// Only free text is deleted here; generated text is hidden instead.
fn delete_text(c: &Context) -> Enabled {
    match c.target {
        Some(t) if t.kind == ObjectKind::Text && !t.free_text => {
            Enabled::Greyed { why: format!("{} is generated: hide it instead.", t.name), next: Some("text.showhide") }
        }
        _ => write(c),
    }
}

/// A device has other symbols to go to.
fn has_other(c: &Context) -> Enabled {
    match c.target {
        Some(t) if t.others == 0 => Enabled::Greyed { why: format!("{} has no symbol on another page.", t.name), next: Some("place.symbol") },
        _ => Enabled::Ok,
    }
}

/// A cable has a reference to go to.
fn has_reference(c: &Context) -> Enabled {
    match c.target {
        Some(t) if t.others == 0 => Enabled::Greyed { why: format!("{} has no reference to another page.", t.name), next: Some("cable.reference") },
        _ => Enabled::Ok,
    }
}

/// A cable reference end can be previewed.
fn is_cable_reference(c: &Context) -> Enabled {
    match c.target {
        Some(t) if t.cable.is_none() => Enabled::Greyed { why: "Only a cable reference has a preview.".into(), next: Some("reference.goto") },
        _ => Enabled::Ok,
    }
}

const fn tool(
    id: &'static str, command: &'static str, label: &'static str, icon: Icon, group: ToolGroup, order: u8, search: &'static [&'static str],
) -> Action {
    Action {
        id,
        command,
        params: Params::Start,
        label,
        icon,
        group: Some((group, order)),
        wheel: &[],
        key: None,
        search,
        searchable: true,
        enabled: write,
    }
}

const fn slot(
    id: &'static str, command: &'static str, params: Params, label: &'static str, icon: Icon, wheel: &'static [(ObjectKind, Slot)],
    enabled: fn(&Context) -> Enabled,
) -> Action {
    Action { id, command, params, label, icon, group: None, wheel, key: None, search: &[], searchable: true, enabled }
}

use ObjectKind as K;
use Slot as S;

/// Every action, in table order.
pub const ACTIONS: &[Action] = &[
    // ---- Tool groups (UI rev 3). Lasso, Branch, Potential or bus line, Jump, Continuation
    // arrow, Function box, Device or PLC outline and PLC's rack, detail page, card and channel
    // tools have no command yet; PLC's group is empty until they do.
    Action {
        id: "select",
        command: "",
        params: Params::App(App::Select),
        label: "Select",
        icon: Icon::Select,
        group: Some((ToolGroup::Select, 0)),
        wheel: &[],
        key: None,
        search: &["pick", "pointer"],
        searchable: true,
        enabled: ok,
    },
    tool("wire.tool", "wire", "Wire", Icon::Wire, ToolGroup::Wire, 0, &["draw wire", "conductor"]),
    tool("place.symbol", "place", "Symbol", Icon::Place, ToolGroup::Device, 0, &["place symbol", "device", "coil", "contact"]),
    tool("place.terminal", "terminals", "Terminal", Icon::Terminal, ToolGroup::Terminal, 0, &["place terminals", "strip"]),
    // Brief 044 (design 044 §4.1): Place ▸ Connector; UI rev 3: Device ▸ Connector.
    tool(
        "place.connector",
        "connector",
        "Connector",
        Icon::Connector,
        ToolGroup::Device,
        1,
        &["connector", "plug", "socket", "d-sub", "m12", "mated"],
    ),
    tool("cable.slash", "cableslash", "Cable slash", Icon::Cable, ToolGroup::Cable, 0, &["new cable"]),
    tool("cable.continue", "cablecontinue", "Continue cable", Icon::GoTo, ToolGroup::Cable, 1, &["cable on another page"]),
    tool("draw.line", "line", "Line", Icon::Line, ToolGroup::Draw, 0, &["geometry"]),
    tool("draw.polyline", "pline", "Polyline", Icon::Polyline, ToolGroup::Draw, 1, &["geometry"]),
    tool("draw.rectangle", "rectang", "Rectangle", Icon::Rectangle, ToolGroup::Draw, 2, &["geometry"]),
    tool("draw.circle", "circle", "Circle", Icon::Circle, ToolGroup::Draw, 3, &["geometry"]),
    tool("draw.arc", "arc", "Arc", Icon::Arc, ToolGroup::Draw, 4, &["geometry"]),
    tool("box.location", "locationbox", "Location box", Icon::Box, ToolGroup::Location, 0, &["location", "+"]),
    tool("text.text", "text", "Text", Icon::Text, ToolGroup::Text, 0, &["free text"]),
    tool("text.note", "mtext", "Note", Icon::Properties, ToolGroup::Text, 1, &["multiline text"]),
    tool("text.cloud", "revcloud", "Revision cloud", Icon::Cloud, ToolGroup::Text, 2, &["revision"]),
    // ---- Device.
    slot("device.edittag", "deviceprops", Params::StartAt, "Edit tag", Icon::Edit, &[(K::Device, S::Top)], write),
    slot("device.properties", "deviceprops", Params::App(App::Properties), "Properties", Icon::Properties, &[(K::Device, S::TopRight)], ok),
    slot("device.goto", "contacts", Params::App(App::GotoOther), "Go to contacts", Icon::GoTo, &[(K::Device, S::Right)], has_other),
    slot("device.row", "", Params::App(App::DeviceRow), "Show row", Icon::ShowRow, &[(K::Device, S::BottomRight)], ok),
    slot("device.delete", "devicedelete", Params::Id(IdOf::Component), "Delete device", Icon::Delete, &[(K::Device, S::Bottom)], write),
    // ---- On the selection (and on the wheels of the kinds they turn or copy).
    Action {
        id: "edit.duplicate",
        command: "copy",
        params: Params::Selected,
        label: "Duplicate",
        icon: Icon::Duplicate,
        group: None,
        wheel: &[(K::Device, S::BottomLeft), (K::Mixed, S::BottomLeft)],
        key: Some(Key::CtrlD),
        search: &["copy", "next free tag"],
        searchable: true,
        enabled: write,
    },
    Action {
        id: "edit.rotate",
        command: "rotate",
        params: Params::Selected,
        label: "Rotate",
        icon: Icon::Rotate,
        group: None,
        wheel: &[(K::Device, S::Left), (K::Mixed, S::Left)],
        key: Some(Key::R),
        search: &["turn"],
        searchable: true,
        enabled: write,
    },
    Action {
        id: "edit.mirror",
        command: "mirror",
        params: Params::Selected,
        label: "Mirror",
        icon: Icon::Mirror,
        group: None,
        wheel: &[(K::Device, S::TopLeft), (K::Mixed, S::TopLeft)],
        key: Some(Key::X),
        search: &["flip"],
        searchable: true,
        enabled: write,
    },
    Action {
        id: "edit.move",
        command: "move",
        params: Params::Selected,
        label: "Move",
        icon: Icon::Move,
        group: None,
        wheel: &[(K::Mixed, S::Top)],
        key: None,
        search: &[],
        searchable: true,
        enabled: write,
    },
    Action {
        id: "edit.copy",
        command: "copyclip",
        params: Params::Selected,
        label: "Copy",
        icon: Icon::Duplicate,
        group: None,
        wheel: &[(K::Mixed, S::TopRight)],
        key: Some(Key::CtrlC),
        search: &["clipboard"],
        searchable: true,
        enabled: ok,
    },
    Action {
        id: "edit.delete",
        command: "erase",
        params: Params::Selected,
        label: "Delete",
        icon: Icon::Delete,
        group: None,
        wheel: &[(K::Mixed, S::Bottom), (K::Text, S::Bottom), (K::Terminal, S::Bottom)],
        key: Some(Key::Delete),
        search: &["erase", "remove"],
        searchable: true,
        enabled: delete_text,
    },
    Action {
        id: "edit.paste",
        command: "pasteclip",
        params: Params::Start,
        label: "Paste",
        icon: Icon::Paste,
        group: None,
        wheel: &[(K::EmptyPage, S::Top)],
        key: Some(Key::CtrlV),
        search: &["clipboard"],
        searchable: true,
        enabled: write,
    },
    Action {
        id: "edit.undo",
        command: "u",
        params: Params::Start,
        label: "Undo",
        icon: Icon::History,
        group: None,
        wheel: &[],
        key: Some(Key::CtrlZ),
        search: &[],
        searchable: false,
        enabled: ok,
    },
    Action {
        id: "edit.redo",
        command: "redo",
        params: Params::Start,
        label: "Redo",
        icon: Icon::History,
        group: None,
        wheel: &[],
        key: Some(Key::CtrlY),
        search: &[],
        searchable: false,
        enabled: ok,
    },
    Action {
        id: "edit.tag",
        command: "deviceprops",
        params: Params::Selected,
        label: "Edit the selected tag",
        icon: Icon::Edit,
        group: None,
        wheel: &[],
        key: Some(Key::F2),
        search: &["rename device", "device properties"],
        searchable: true,
        enabled: write,
    },
    // ---- Wire and wire end.
    slot("wire.fixnumber", "wirenumber", Params::StartAt, "Fix number", Icon::Number, &[(K::Wire, S::Top)], write),
    slot("wire.addtocable", "cableadd", Params::Id(IdOf::Conductors), "Add to cable", Icon::Cable, &[(K::Wire, S::TopRight)], write),
    slot("wire.net", "net", Params::App(App::ShowNet), "Show net", Icon::GoTo, &[(K::Wire, S::Right), (K::WireEnd, S::Right)], ok),
    slot(
        "wire.delete",
        "ecaddelete",
        Params::Id(IdOf::Electrical),
        "Delete",
        Icon::Delete,
        &[(K::Wire, S::Bottom), (K::WireEnd, S::Bottom), (K::Cable, S::Bottom)],
        write,
    ),
    slot("wire.potential", "potential", Params::Choose(Choices::Potentials), "Set potential", Icon::Wire, &[(K::Wire, S::BottomLeft)], write),
    slot("wire.join", "joinwires", Params::At, "Join", Icon::Junction, &[(K::Wire, S::Left)], write),
    slot("wire.settings", "wiresettings", Params::App(App::WireSettings), "Wire settings", Icon::Properties, &[(K::Wire, S::TopLeft)], ok),
    slot("wireend.connect", "connect", Params::StartAt, "Connect", Icon::Connect, &[(K::WireEnd, S::Top)], write),
    slot("wireend.disconnect", "disconnect", Params::Id(IdOf::End), "Disconnect", Icon::Disconnect, &[(K::WireEnd, S::TopRight)], write),
    slot("wireend.junction", "junction", Params::At, "Junction", Icon::Junction, &[(K::WireEnd, S::Left)], write),
    // ---- Terminal.
    slot("terminal.label", "termlabel", Params::App(App::Template), "Edit label", Icon::Edit, &[(K::Terminal, S::Top)], write),
    slot("terminal.jumper", "jumper", Params::StartAt, "Jumper", Icon::Connect, &[(K::Terminal, S::TopRight)], write),
    slot("terminal.plan", "stripplan", Params::App(App::TerminalRow), "Go to strip plan", Icon::GoTo, &[(K::Terminal, S::Right)], ok),
    slot("terminal.row", "", Params::App(App::TerminalRow), "Show row", Icon::ShowRow, &[(K::Terminal, S::BottomRight)], ok),
    slot("terminal.accessory", "accessory", Params::StartAt, "Accessory", Icon::Terminal, &[(K::Terminal, S::BottomLeft)], write),
    // ---- Cable.
    slot("cable.view", "cableview", Params::Id(IdOf::Cable), "Cable view", Icon::Eye, &[(K::Cable, S::Top)], ok),
    slot("cable.continuehere", "cablecontinue", Params::Start, "Continue", Icon::GoTo, &[(K::Cable, S::TopRight)], write),
    slot("cable.goto", "cablegoto", Params::App(App::CableReference), "Go to reference", Icon::GoTo, &[(K::Cable, S::Right)], has_reference),
    slot("cable.row", "", Params::App(App::CableRow), "Show row", Icon::ShowRow, &[(K::Cable, S::BottomRight)], ok),
    slot("cable.join", "cablefan", Params::StartAt, "Join", Icon::Junction, &[(K::Cable, S::BottomLeft)], write),
    slot("cable.cores", "cablecore", Params::StartAt, "Core labels", Icon::Number, &[(K::Cable, S::Left)], write),
    slot("cable.reference", "cableref", Params::StartAt, "Add reference", Icon::GoTo, &[(K::Cable, S::TopLeft)], write),
    // ---- Text.
    slot("text.edit", "", Params::App(App::EditText), "Edit", Icon::Edit, &[(K::Text, S::Top)], write),
    slot("text.showhide", "showtext", Params::StartAt, "Show or hide", Icon::Eye, &[(K::Text, S::TopRight)], write),
    slot("text.move", "movetext", Params::StartAt, "Move text", Icon::Move, &[(K::Text, S::Left)], write),
    // ---- Reference.
    slot("reference.goto", "refgoto", Params::Id(IdOf::Source), "Go to target", Icon::GoTo, &[(K::Reference, S::Top), (K::Reference, S::Right)], ok),
    slot(
        "reference.preview",
        "",
        Params::App(App::PreviewReference),
        "Preview reference",
        Icon::Eye,
        &[(K::Reference, S::TopRight)],
        is_cable_reference,
    ),
    // ---- Empty page and page row.
    slot(
        "page.properties",
        "pageprops",
        Params::App(App::PageProperties),
        "Page properties",
        Icon::Properties,
        &[(K::EmptyPage, S::TopRight), (K::PageRow, S::TopRight)],
        ok,
    ),
    slot("page.row", "", Params::App(App::PageRow), "Show page in Pages", Icon::ShowRow, &[(K::EmptyPage, S::BottomRight)], ok),
    slot("page.new", "newpage", Params::App(App::NewPage), "New page", Icon::NewPage, &[(K::EmptyPage, S::Left), (K::PageRow, S::Left)], write),
    slot("page.open", "openpage", Params::App(App::OpenPage), "Open", Icon::GoTo, &[(K::PageRow, S::Right)], ok),
    slot("page.rename", "renamepage", Params::App(App::RenamePage), "Rename", Icon::Edit, &[(K::PageRow, S::Top)], write),
    slot("page.preview", "", Params::App(App::PreviewPage), "Preview", Icon::Eye, &[(K::PageRow, S::TopLeft)], ok),
    slot("page.folder", "newfolder", Params::App(App::NewFolder), "New folder", Icon::Pages, &[(K::PageRow, S::BottomLeft)], write),
    // ---- Found with Ctrl+K only (no slot yet, or not about one object).
    Action {
        id: "find.symbol",
        command: "symbols",
        params: Params::App(App::FindSymbol),
        label: "Find symbol",
        icon: Icon::Search,
        group: None,
        wheel: &[],
        key: None,
        search: &["library", "symbol search"],
        searchable: true,
        enabled: ok,
    },
    search_only("component.convert", "convertcomponent", Params::Selected, "Make component", Icon::Devices, &["convert to component"]),
    search_only("component.addpoint", "addterminal", Params::Start, "Add connection point", Icon::Connect, &["terminal", "pin"]),
    search_only("device.mirrortable", "mirrormove", Params::Start, "Move contact table", Icon::Move, &["mirror table", "cross reference"]),
    search_only("device.deleteany", "ecaddelete", Params::App(App::Template), "Delete electrical object", Icon::Delete, &["remove"]),
    search_only("object.setprop", "setprop", Params::App(App::Template), "Set property", Icon::Properties, &["tag", "description"]),
    search_only("wire.junctiontool", "junction", Params::Start, "Junction", Icon::Junction, &["dot", "tee"]),
    search_only("wire.jointool", "joinwires", Params::Start, "Join wires", Icon::Junction, &["repair"]),
    search_only("wire.connecttool", "connect", Params::Start, "Connect", Icon::Connect, &["attach"]),
    search_only("wire.disconnecttool", "disconnect", Params::Start, "Disconnect", Icon::Disconnect, &["detach"]),
    search_only("wire.numbertool", "wirenumber", Params::Start, "Fix wire number", Icon::Number, &["number"]),
    search_only("terminal.bridge", "bridge", Params::Start, "Draw bridge", Icon::Connect, &["terminal", "link"]),
    // Brief 044 (design 044 §9.3): the terminal wheel holds strip terminals only (its jumper, plan
    // and accessory slots do not apply to a connector pin), so Rename pin is a Ctrl+K action.
    search_only("connector.renamepin", "pinlabel", Params::App(App::Template), "Rename pin", Icon::Edit, &["pinlabel", "connector pin", "pin name"]),
    search_only("terminal.jumpertool", "jumper", Params::Start, "Jumper", Icon::Connect, &["terminal", "link"]),
    search_only("terminal.accessorytool", "accessory", Params::Start, "Strip accessory", Icon::Terminal, &["endplate", "partition", "marker"]),
    search_only("terminal.toposition", "toposition", Params::Start, "Make strip position", Icon::Terminal, &["fuse terminal"]),
    search_only("cable.viewtool", "cableview", Params::Start, "Cable view", Icon::Eye, &["fan", "traditional"]),
    search_only("cable.corestool", "cablecore", Params::Start, "Core label", Icon::Number, &["core", "colour"]),
    search_only("cable.jointool", "cablefan", Params::Start, "Cable join", Icon::Junction, &["fan", "trunk"]),
    search_only("cable.referencetool", "cableref", Params::Start, "Cable reference", Icon::GoTo, &["other page"]),
    search_only("cable.gototool", "cablegoto", Params::Start, "Go to reference", Icon::GoTo, &["other end"]),
    search_only("cable.add", "cableadd", Params::App(App::Template), "Add to cable", Icon::Cable, &["group wires"]),
    search_only("text.showhidetool", "showtext", Params::Start, "Show or hide text", Icon::Eye, &["hide tag"]),
    search_only("text.movetool", "movetext", Params::Start, "Move text", Icon::Move, &["tag position"]),
    search_only("text.hidden", "hiddentext", Params::Start, "Show hidden text", Icon::Eye, &["dimmed"]),
    search_only("device.propertiestool", "deviceprops", Params::Start, "Device properties", Icon::Properties, &["maker", "part number"]),
    search_only("device.deletetool", "devicedelete", Params::Start, "Delete device", Icon::Delete, &["remove"]),
    search_only("numbering.renumber", "renumber", Params::App(App::Renumber), "Renumber", Icon::Number, &["tags", "devices"]),
    search_only("numbering.wires", "renumberwires", Params::App(App::RenumberWires), "Renumber wires", Icon::Number, &["wire numbers"]),
    search_only("numbering.finalise", "finalise", Params::App(App::Finalise), "Finalise numbering", Icon::Number, &["provisional"]),
    // Brief 045 (design 045 §2.6, §4.8): the card commands, found with Ctrl+K until a card wheel.
    search_only("plc.slot", "plcslot", Params::App(App::Template), "Card slot", Icon::Devices, &["plc", "rack", "card", "move card"]),
    search_only(
        "plc.channels",
        "plcchannels",
        Params::App(App::Template),
        "Place channels",
        Icon::Connect,
        &["plc", "channel", "fence", "spare", "remaining"],
    ),
    search_only("plc.channel", "plcchannel", Params::App(App::Template), "Channel data", Icon::Edit, &["plc", "channel", "description", "pin"]),
    search_only("plc.address", "plcaddress", Params::App(App::Template), "PLC addresses", Icon::Number, &["plc", "address", "fix addresses", "pin"]),
];

const fn search_only(
    id: &'static str, command: &'static str, params: Params, label: &'static str, icon: Icon, search: &'static [&'static str],
) -> Action {
    let enabled = match params {
        Params::App(App::FindSymbol) => ok,
        _ => write,
    };
    Action { id, command, params, label, icon, group: None, wheel: &[], key: None, search, searchable: true, enabled }
}

/// ECAD commands no action shows, each with the reason (design §7.2: nothing is defined twice,
/// and nothing a person needs is reachable only by typing).
pub const PROGRAMMATIC_ONLY: &[(&str, &str)] = &[
    ("setdefault", "Settings ▸ Standards (U4c)"),
    ("canvassheet", "the person's paper or dark sheet choice (U4a), display only"),
    ("agentaccess", "Settings ▸ Agent access (U4d)"),
    ("cablerefs", "a read for agents and tests; the cable wheel's go-to uses the references"),
    ("devices", "a read; the Devices tab and Lists show it"),
    ("tagsettings", "Project settings"),
    ("nets", "a read for agents"),
    ("checks", "a read; the Checks drawer shows it"),
    ("movepage", "the Pages tab (drag)"),
    ("ecadinspect", "developer tools"),
    ("history", "the History tab"),
    ("steps", "the History tab"),
    ("pointinfo", "the History tab"),
    ("version", "the History tab"),
    ("showpoint", "the History tab"),
    ("closepoint", "the History tab"),
    ("restore", "the History tab"),
    ("branches", "the History tab"),
    ("branch", "the History tab"),
    ("renamebranch", "the History tab"),
    ("switchbranch", "the History tab"),
    ("archivebranch", "the History tab"),
    ("reopenbranch", "the History tab"),
    ("merge", "the History tab's merge view"),
    ("undomerge", "the History tab"),
    ("numbering", "Project settings"),
    ("bom", "Publish and Lists ▸ Reports"),
    ("wirelist", "Publish and Lists ▸ Reports"),
    ("cablelist", "Publish and Lists ▸ Reports"),
    ("printbook", "Publish ▸ Print Book"),
    ("reports", "Lists ▸ Reports"),
    ("prerelease", "Publish ▸ Release"),
    ("release", "Publish ▸ Release"),
    ("projectprops", "Project menu ▸ Project info"),
    ("renamefolder", "the Pages tab"),
    ("movefolder", "the Pages tab"),
    ("deletefolder", "the Pages tab"),
    ("revision", "Project menu ▸ Revisions"),
    ("issuerevision", "Project menu ▸ Revisions"),
    ("openrevision", "Project menu ▸ Revisions"),
    ("closerevision", "Project menu ▸ Revisions"),
    ("undoissue", "Project menu ▸ Revisions"),
    ("cablecores", "Lists ▸ Reports"),
    ("textstyle", "Settings ▸ Standards (U4c)"),
    ("pagetexts", "a read for agents"),
    ("defaults", "Settings ▸ Standards (U4c)"),
    ("xrefs", "a read for agents"),
    ("plcracks", "a read; the PLC manager (2.8) draws it"),
    ("iolist", "a read; Lists ▸ I/O shows it"),
    // Brief 047 (design 047 §6, owner Q10): no part-set panel until 2.3's catalogue.
    ("partline", "the command line until 2.3's part-set editor; the inspector's Parts section shows them"),
    ("bomitem", "Lists ▸ Reports ▸ BOM ▸ + Item"),
    ("article", "the command line and Checks' Use … for all until 2.3's catalogue"),
];

/// The action with this id.
pub fn find(id: &str) -> Option<&'static Action> {
    ACTIONS.iter().find(|a| a.id == id)
}

/// The wheel of `kind`: the action in each of the eight slots (clockwise from the top), `None`
/// where the slot is empty. Nothing shifts to fill a gap.
pub fn wheel(kind: ObjectKind) -> [Option<&'static Action>; 8] {
    let mut out = [None; 8];
    for a in ACTIONS {
        for (k, s) in a.wheel {
            if *k == kind
                && let Some(slot) = out.get_mut(s.index())
                && slot.is_none()
            {
                *slot = Some(a);
            }
        }
    }
    out
}

/// The action on `key`, if any.
pub fn on_key(key: Key) -> Option<&'static Action> {
    ACTIONS.iter().find(|a| a.key == Some(key))
}

/// Whether some action runs `command`.
pub fn covers(command: &str) -> bool {
    ACTIONS.iter().any(|a| a.command.eq_ignore_ascii_case(command))
}

/// The actions Ctrl+K lists for `query`, at most `limit`, in table order.
pub fn search(query: &str, limit: usize) -> Vec<&'static Action> {
    ACTIONS.iter().filter(|a| a.matches(query)).take(limit).collect()
}
