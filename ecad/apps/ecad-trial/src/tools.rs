//! The **ECAD** tab in CADCraft's Tool Sets panel, beside Drafting and Modeling: buttons for the
//! ECAD commands implemented so far. It is not a new command system: every button reaches the
//! command itself.
//!
//! CADCraft shows the tab and queues clicks (`CadApp::extra_tool_tabs`,
//! `CadApp::take_tool_requests`); everything ECAD about it is here:
//! - **Start** buttons start the command's interactive form (brief 008: prompts, picks, previews,
//!   Enter and Esc at CADCraft's command line), exactly as typing its bare name does. Convert to
//!   Component converts the current selection at once, or asks to select objects.
//! - **Run** buttons type the command into the same path as the ECAD command field, so the
//!   command's own validation, transaction and undo apply.
//! - **Template** buttons open the ECAD command field with an editable command line and a short
//!   note on what to fill in, and run nothing. IDs and handles are filled in from the current
//!   selection where it names the objects; everything else is a `<placeholder>`, and a line
//!   still holding a template's placeholder is refused instead of run.
//! - **Trial** buttons act in the trial itself: Preview Reference (brief 016) and the **ECAD Dev**
//!   tab's developer tools (now the Project panel's Developer section: the command field, the debugger and snapshots), which
//!   replaced the trial's top banner (owner, 2026-11-08).
//!
//! The command window stays because CADCraft's own command line takes a space as Enter: it can
//! run `name {json}` only when the line has no space at all, so JSON with spaces or a title such
//! as `Motor control` cannot be typed there.

use std::collections::{BTreeSet, HashMap};

use cadcraft_engine::doc::Handle;
use cadcraft_ui_egui::palettes::{ExtraTool, ExtraToolGroup, ExtraToolTab};
use ecad_cadcraft::{Mapped, Project};
use ecad_model::{CableId, ComponentId, ConductorId, ItemId, PageId, Subject, TerminalId};

/// The tab's name in the Tool Sets panel.
pub const TAB: &str = "ECAD";

/// The tab's groups, in order.
pub const GROUPS: &[&str] = &["Components", "Wiring", "Numbering", "Edit", "Text"];

/// The developer tools' group: shown in the Project panel's collapsed **Developer** section, not
/// in Tool Sets (owner, 2026-11-08).
pub const DEV_GROUP: &str = "Developer";

/// What a button does.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    /// Start this command's interactive form (its prompts appear at the command line).
    Start(&'static str),
    /// Run this command line now (it needs no values beyond the selection).
    Run(&'static str),
    /// Open the ECAD command field with a template.
    Template,
    /// Handled by the trial itself (no command).
    Trial,
    /// Open or close the symbol search window (brief 018, [`crate::search`]).
    Search,
}

/// One button: a stable, namespaced id, its group, label and tooltip.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Tool {
    pub id: &'static str,
    /// One of [`GROUPS`].
    pub group: &'static str,
    pub label: &'static str,
    pub tooltip: &'static str,
    pub kind: Kind,
}

/// The buttons, in panel order. Only commands that exist (`ecad_cadcraft::commands::specs`).
pub const TOOLS: &[Tool] = &[
    Tool {
        id: "ecad.findsymbol",
        group: "Components",
        label: "Find Symbol",
        tooltip: "Type a few letters (coil, contact, fuse, -Q, -K01) and click a symbol or device to place it",
        kind: Kind::Search,
    },
    Tool {
        id: "ecad.convertcomponent",
        group: "Components",
        label: "Convert to Component",
        tooltip: "Make drawing objects one component (uses the selection, or asks you to select)",
        kind: Kind::Start("convertcomponent"),
    },
    Tool {
        id: "ecad.addterminal",
        group: "Components",
        label: "Add Connection Point",
        tooltip: "Click on a component's outline to add connection points; Enter to finish",
        kind: Kind::Start("addterminal"),
    },
    Tool {
        id: "ecad.place",
        group: "Components",
        label: "Place Symbol",
        tooltip: "Type a symbol (coil, no, nc, mcb1…) or Enter for the last one, then click to place devices; R rotates, D adds to a device, T sets the tag; Enter to finish",
        kind: Kind::Start("place"),
    },
    // Brief 028 (design 028 §3.2, §4): beside Place Symbol.
    Tool {
        id: "ecad.terminals",
        group: "Components",
        label: "Place Terminals",
        tooltip: "Enter for the last strip (or New, PE, a strip's tag), then draw a line across the wires: one terminal on each, numbered in order",
        kind: Kind::Start("terminals"),
    },
    Tool {
        id: "ecad.bridge",
        group: "Components",
        label: "Draw Bridge",
        tooltip: "Click a terminal, then another of the same strip: a bridge; the next click continues from it, Enter ends",
        kind: Kind::Start("bridge"),
    },
    // Brief 030 (design 030 §2.3, §5.4): after Draw Bridge.
    Tool {
        id: "ecad.tool.accessory",
        group: "Components",
        label: "Strip Accessory",
        tooltip: "Click a terminal (or Start/End), then Partition, Endplate, endStop, Marker, Label or Remove; repeats until Enter",
        kind: Kind::Start("accessory"),
    },
    Tool {
        id: "ecad.tool.jumper",
        group: "Components",
        label: "Jumper",
        tooltip: "Click a terminal, then the next: a link through every position between (Skip: only those picked); Enter writes the link",
        kind: Kind::Start("jumper"),
    },
    Tool {
        id: "ecad.deviceprops",
        group: "Components",
        label: "Device Properties",
        tooltip: "Click a device to see and change its tag, description, maker and part number",
        kind: Kind::Start("deviceprops"),
    },
    Tool {
        id: "ecad.mirrormove",
        group: "Components",
        label: "Move Mirror Table",
        tooltip: "Click a coil or its contact table, then where the table goes (or Auto, Below, Right, Off)",
        kind: Kind::Start("mirrormove"),
    },
    Tool {
        id: "ecad.devicedelete",
        group: "Components",
        label: "Delete Device",
        tooltip: "Click a device to delete it with every symbol on every page (asks first)",
        kind: Kind::Start("devicedelete"),
    },
    Tool {
        id: "ecad.wire",
        group: "Wiring",
        label: "Wire",
        tooltip: "Click points to draw a wire; ends on connection points attach; Enter to finish",
        kind: Kind::Start("wire"),
    },
    // Brief 022: junctions, wire numbers.
    Tool {
        id: "ecad.junction",
        group: "Wiring",
        label: "Junction",
        tooltip: "Click a wire where a T joins it: a junction dot, and the wire is split there",
        kind: Kind::Start("junction"),
    },
    // Brief 032: repair wire ends that meet.
    Tool {
        id: "ecad.joinwires",
        group: "Wiring",
        label: "Join Wires",
        tooltip: "Click where wire ends meet: two ends become one wire; an end on a wire gets a junction",
        kind: Kind::Start("joinwires"),
    },
    Tool {
        id: "ecad.wirenumber",
        group: "Wiring",
        label: "Fix Wire Number",
        tooltip: "Click a wire, type the number its whole net should carry (. releases a fixed one)",
        kind: Kind::Start("wirenumber"),
    },
    Tool {
        id: "ecad.renumberwires",
        group: "Wiring",
        label: "Renumber Wires",
        tooltip: "Open Renumber with Wires chosen: preview, then Apply (fixed numbers stay)",
        kind: Kind::Trial,
    },
    // Brief 026 (design §3, §5.3): the Numbering group.
    Tool {
        id: "ecad.renumber",
        group: "Numbering",
        label: "Renumber…",
        tooltip: "Renumber devices, cables or wires in drawing order: preview, then Apply (one undo)",
        kind: Kind::Trial,
    },
    Tool {
        id: "ecad.finalise",
        group: "Numbering",
        label: "Finalise numbering",
        tooltip: "Give the provisional tags and wire numbers real numbers, in drawing order",
        kind: Kind::Trial,
    },
    Tool {
        id: "ecad.connect",
        group: "Wiring",
        label: "Connect",
        tooltip: "Click a wire near its end, then a connection point",
        kind: Kind::Start("connect"),
    },
    Tool {
        id: "ecad.disconnect",
        group: "Wiring",
        label: "Disconnect",
        tooltip: "Click a wire near an attached end to detach it",
        kind: Kind::Start("disconnect"),
    },
    // Brief 024: location boxes.
    Tool {
        id: "ecad.tool.locationbox",
        group: "Components",
        label: "Location Box",
        tooltip: "Draw a dashed rectangle or closed polyline labelled +…: every device symbol inside takes its location",
        kind: Kind::Start("locationbox"),
    },
    Tool {
        id: "ecad.cableslash",
        group: "Wiring",
        label: "Cable Slash",
        tooltip: "Draw a short line across wires to make them one cable",
        kind: Kind::Start("cableslash"),
    },
    Tool {
        id: "ecad.cableview",
        group: "Wiring",
        label: "Cable View",
        tooltip: "Click a cable to switch it between traditional and fan view (changes nothing in the project)",
        kind: Kind::Start("cableview"),
    },
    Tool {
        id: "ecad.cablecore",
        group: "Wiring",
        label: "Core Label",
        tooltip: "Click a wire in a cable and type its core number or colour",
        kind: Kind::Start("cablecore"),
    },
    Tool {
        id: "ecad.cablefan",
        group: "Wiring",
        label: "Cable Join",
        tooltip: "Click a cable shown as a fan near one end, then where its trunk should join the spine",
        kind: Kind::Start("cablefan"),
    },
    Tool {
        id: "ecad.cableref",
        group: "Wiring",
        label: "Cable Reference",
        tooltip: "Click a cable, then where it leaves this page: places a reference to its other end",
        kind: Kind::Start("cableref"),
    },
    Tool {
        id: "ecad.cablecontinue",
        group: "Wiring",
        label: "Continue Cable",
        tooltip: "Continue a cable from its reference on another page: click where it arrives and which way its wires run",
        kind: Kind::Start("cablecontinue"),
    },
    Tool {
        id: "ecad.cablegoto",
        group: "Wiring",
        label: "Go to Reference",
        tooltip: "Click a cable reference to open the page of its other end",
        kind: Kind::Start("cablegoto"),
    },
    Tool {
        id: "ecad.cablepreview",
        group: "Wiring",
        label: "Preview Reference",
        tooltip: "Preview the other end of the selected cable reference without leaving this page",
        kind: Kind::Trial,
    },
    Tool {
        id: "ecad.setprop",
        group: "Edit",
        label: "Set Property",
        tooltip: "Set a property such as the tag, or a cable's designation or type (opens the ECAD command field)",
        kind: Kind::Template,
    },
    Tool {
        id: "ecad.cableadd",
        group: "Wiring",
        label: "Add to Cable",
        tooltip: "Group wires into a cable (opens the ECAD command field)",
        kind: Kind::Template,
    },
    Tool {
        id: "ecad.delete",
        group: "Edit",
        label: "Delete Electrical Object",
        tooltip: "Delete a component, wire or cable with its connections (opens the ECAD command field)",
        kind: Kind::Template,
    },
    Tool {
        id: "ecad.inspect",
        group: DEV_GROUP,
        label: "Inspect Project",
        tooltip: "Show the project's IDs and history at the command line (read-only; runs now)",
        kind: Kind::Run("ecadinspect"),
    },
    Tool { id: "ecad.dev.debugger", group: DEV_GROUP, label: "Debugger", tooltip: "Open or close the read-only debugger panel", kind: Kind::Trial },
    Tool {
        id: "ecad.dev.snapshot",
        group: DEV_GROUP,
        label: "Export Inspector Snapshot",
        tooltip: "Inspector snapshots are diagnostic files for the developer inspector, not saved ECAD projects; they cannot be reopened.",
        kind: Kind::Trial,
    },
    Tool {
        id: "ecad.dev.command",
        group: DEV_GROUP,
        label: "JSON Command",
        tooltip: "Open the ECAD command field: type a command with JSON parameters, for example wire {\"points\":[[0,0],[40,0]]}",
        kind: Kind::Trial,
    },
    // Brief 031 (design §5.5, §5.7): the Text group (`ecad.toolset.text`).
    Tool {
        id: "ecad.tool.showtext",
        group: "Text",
        label: "Show/Hide Text",
        tooltip: "Click a tag or label to hide it, or a symbol to show its hidden tag (Device, Page or Project: wider)",
        kind: Kind::Start("showtext"),
    },
    Tool {
        id: "ecad.tool.movetext",
        group: "Text",
        label: "Move Text",
        tooltip: "Click a tag or label, then where it goes (Justify: its alignment; Reset: back to automatic)",
        kind: Kind::Start("movetext"),
    },
    Tool {
        id: "ecad.view.hiddentext",
        group: "Text",
        label: "Show Hidden Text",
        tooltip: "Draw hidden tags and labels dimmed, or stop drawing them (display only; never printed)",
        kind: Kind::Run("hiddentext"),
    },
];

/// The tab as CADCraft's Tool Sets panel shows it (developer switch only). Brief 041: every
/// button's command is now reachable from an action ([`covered`]); the buttons stay here, behind
/// the developer switch, until the tests that drive the forms through them move to the actions.
pub fn tab() -> ExtraToolTab {
    ExtraToolTab { name: TAB.into(), groups: GROUPS.iter().map(|g| group(g, true)).collect() }
}

/// The command a button reaches (brief 041): its own for Start and Run, the template's, the
/// symbol search's, or the dialog's. `None` for the developer tools.
pub fn command_of(t: &Tool) -> Option<&'static str> {
    Some(match t.kind {
        Kind::Start(c) => c,
        Kind::Run(line) => line.split_whitespace().next()?,
        Kind::Search => "symbols",
        Kind::Template => match t.id {
            "ecad.setprop" => "setprop",
            "ecad.cableadd" => "cableadd",
            "ecad.delete" => "ecaddelete",
            _ => return None,
        },
        Kind::Trial => match t.id {
            "ecad.renumber" => "renumber",
            "ecad.renumberwires" => "renumberwires",
            "ecad.finalise" => "finalise",
            // Preview Reference is the reference wheel's Preview (an action with no command).
            "ecad.cablepreview" => return ecad_cadcraft::actions::find("reference.preview").map(|_| "reference.preview"),
            _ => return None,
        },
    })
}

/// Whether an action covers the button (brief 041): its command, or Preview Reference's action.
pub fn covered(t: &Tool) -> bool {
    command_of(t).is_some_and(|c| ecad_cadcraft::actions::covers(c) || ecad_cadcraft::actions::find(c).is_some())
}

/// The tools of the Project panel's Developer section, in order; Export Inspector Snapshot only
/// when `snapshots` is on (`--snapshots`).
pub fn developer_tools(snapshots: bool) -> Vec<&'static Tool> {
    TOOLS.iter().filter(|t| t.group == DEV_GROUP && (snapshots || t.id != "ecad.dev.snapshot")).collect()
}

fn group(name: &str, snapshots: bool) -> ExtraToolGroup {
    ExtraToolGroup {
        name: name.into(),
        tools: TOOLS
            .iter()
            .filter(|t| t.group == name && (snapshots || t.id != "ecad.dev.snapshot"))
            .map(|t| ExtraTool { id: t.id.into(), label: t.label.into(), tooltip: t.tooltip.into() })
            .collect(),
    }
}

/// The tool with this id.
pub fn find(id: &str) -> Option<&'static Tool> {
    TOOLS.iter().find(|t| t.id == id)
}

/// Most selected handles followed into the model for one template; a larger selection fills in
/// only what these name (templates take at most a handful of IDs).
pub const MAX_PICKED: usize = 1_000;

/// What the current selection names: its handles, and the ECAD objects they map to.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Picked {
    pub handles: Vec<Handle>,
    pub components: Vec<ComponentId>,
    pub conductors: Vec<ConductorId>,
    /// Cables whose slashes or fan lines are selected.
    pub cables: Vec<CableId>,
    /// Terminals whose markers are selected.
    pub terminals: Vec<TerminalId>,
    /// The selection had more than [`MAX_PICKED`] handles; only the first were followed.
    pub truncated: bool,
    /// The page shown in the active drawing, when it is an ECAD page (brief 010: the page
    /// templates fill it in).
    pub page: Option<PageId>,
}

impl Picked {
    /// Follow the active page's selected handles through its handle map (read only).
    ///
    /// At most [`MAX_PICKED`] handles are followed, through one item-to-subject index built when
    /// the first item is met, so the cost is bounded by the project, not by the selection.
    pub fn from_selection(p: &Project, uid: u64, handles: Vec<Handle>) -> Self {
        let mut subjects = BTreeSet::new();
        let mut terminals = BTreeSet::new();
        let doc = p.ledger.document();
        let map = p.page_of(uid).and_then(|page| p.pages.get(&page)).map(|v| &v.map);
        let mut index: Option<HashMap<ItemId, Subject>> = None;
        for h in handles.iter().take(MAX_PICKED) {
            match map.and_then(|m| m.get(*h)) {
                Some(Mapped::Item(i)) => {
                    let index =
                        index.get_or_insert_with(|| doc.representations.values().flat_map(|r| r.items.iter().map(|i| (*i, r.subject))).collect());
                    if let Some(s) = index.get(&i) {
                        subjects.insert(*s);
                    }
                }
                Some(Mapped::Marker(rep, t)) => {
                    if let Some(r) = doc.representations.get(&rep) {
                        subjects.insert(r.subject);
                    }
                    terminals.insert(t);
                }
                // A line of a cable's fan view names its cable (brief 005).
                Some(Mapped::Fan(k)) => {
                    subjects.insert(Subject::Cable(k));
                }
                // A link line names its strip (brief 030).
                Some(Mapped::Link(c, _)) => {
                    subjects.insert(Subject::Component(c));
                }
                // A cable reference's text names its cable (brief 010); a device's tag text or
                // terminal label names its device (brief 018).
                Some(Mapped::RefText(r) | Mapped::Tag(r) | Mapped::Xref(r)) => {
                    if let Some(r) = doc.representations.get(&r) {
                        subjects.insert(r.subject);
                    }
                }
                // A conductor's label names its wire (brief 022).
                Some(Mapped::Label(w)) => {
                    subjects.insert(Subject::Conductor(w));
                }
                // The sheet frame (brief 019) names nothing.
                Some(Mapped::Frame(_)) | None => {}
            }
        }
        Picked {
            page: p.page_of(uid),
            truncated: handles.len() > MAX_PICKED,
            handles,
            components: subjects.iter().filter_map(|s| if let Subject::Component(c) = s { Some(*c) } else { None }).collect(),
            conductors: subjects.iter().filter_map(|s| if let Subject::Conductor(c) = s { Some(*c) } else { None }).collect(),
            cables: subjects.iter().filter_map(|s| if let Subject::Cable(c) = s { Some(*c) } else { None }).collect(),
            terminals: terminals.into_iter().collect(),
        }
    }
}

fn one<T: Copy>(v: &[T]) -> Option<T> {
    match v {
        [x] => Some(*x),
        _ => None,
    }
}

/// The template line and its note for a template tool (`None` for a start or run tool, or an unknown
/// id).
pub fn template(id: &str, picked: &Picked) -> Option<(String, &'static str)> {
    let component = one(&picked.components).map(ComponentId::hex);
    let conductor = one(&picked.conductors).map(ConductorId::hex);
    Some(match id {
        "ecad.setprop" => {
            let target = match (component, conductor) {
                (Some(c), None) => format!(r#""component":"{c}""#),
                (None, Some(c)) => format!(r#""conductor":"{c}""#),
                _ => r#""component":"<component id>""#.to_string(),
            };
            (
                format!(r#"setprop {{{target},"key":"tag","value":"<value>"}}"#),
                "Set Property: select one component or wire to fill in its ID; key is the property name (tag shown); \
value is the text, or null to remove it. Then press Enter.",
            )
        }
        "ecad.cableadd" => {
            let wires = if picked.conductors.is_empty() {
                r#""<conductor id>""#.to_string()
            } else {
                picked.conductors.iter().map(|c| format!(r#""{}""#, c.hex())).collect::<Vec<_>>().join(",")
            };
            (
                format!(r#"cableadd {{"conductors":[{wires}],"tag":"<cable tag>"}}"#),
                "Add to Cable: the selected wires are filled in; tag names a new cable (or give \"cable\":\"<id>\" \
to add to an existing one). Then press Enter.",
            )
        }
        "ecad.delete" => {
            let cable = one(&picked.cables).map(CableId::hex);
            let target = match (component, conductor, cable) {
                (Some(c), None, None) => format!(r#""component":"{c}""#),
                (None, Some(c), None) => format!(r#""conductor":"{c}""#),
                (None, None, Some(c)) => format!(r#""cable":"{c}""#),
                _ => r#""component":"<component id>""#.to_string(),
            };
            (
                format!(r#"ecaddelete {{{target}}}"#),
                "Delete Electrical Object: select one component, wire or cable slash to fill in its ID (use \
\"conductor\" for a wire, \"cable\" for a cable; deleting a cable keeps its wires). Undo brings it back. Then \
press Enter.",
            )
        }
        _ => return None,
    })
}

/// The `<placeholder>`s still in `line`: `<` and `>` around letters, digits, spaces, `_` or `-`,
/// starting and ending with a letter or digit (so `a < b > c` is not one).
pub fn placeholders(line: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut rest = line;
    while let Some(start) = rest.find('<') {
        let after = rest.get(start + 1..).unwrap_or_default();
        match after.find('>') {
            Some(end) => {
                let inner = after.get(..end).unwrap_or_default();
                let edge = |c: Option<char>| c.is_some_and(|c| c.is_ascii_alphanumeric());
                if edge(inner.chars().next())
                    && edge(inner.chars().last())
                    && inner.chars().all(|c| c.is_ascii_alphanumeric() || matches!(c, ' ' | '_' | '-'))
                {
                    out.push(format!("<{inner}>"));
                }
                rest = after.get(end + 1..).unwrap_or_default();
            }
            None => break,
        }
    }
    out
}
