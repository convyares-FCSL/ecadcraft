//! The **ECAD** tab in CADCraft's Tool Sets panel, beside Drafting and Modeling: buttons for the
//! ECAD commands implemented so far, as a convenience for manual testing. It is not a new command
//! system and has no interactive tools of its own.
//!
//! CADCraft shows the tab and queues clicks (`CadApp::extra_tool_tabs`,
//! `CadApp::take_tool_requests`); everything ECAD about it is here:
//! - **Run** buttons type the command into the same path as the banner's Command field, so the
//!   command's own validation, transaction and undo apply. Convert to Component uses the current
//!   selection.
//! - **Template** buttons put an editable command line in the Command field, with a short note
//!   on what to fill in, and run nothing. IDs and handles are filled in from the current
//!   selection where it names the objects; everything else is a `<placeholder>`, and a line
//!   still holding a template's placeholder is refused instead of run.

use std::collections::{BTreeSet, HashMap};

use cadcraft_engine::doc::Handle;
use cadcraft_ui_egui::palettes::{ExtraTool, ExtraToolGroup, ExtraToolTab};
use ecad_cadcraft::{Mapped, Project};
use ecad_model::{ComponentId, ConductorId, ItemId, Subject, TerminalId};

/// The tab's name in the Tool Sets panel.
pub const TAB: &str = "ECAD";

/// The tab's groups, in order.
pub const GROUPS: &[&str] = &["Components", "Wiring", "Edit", "Pages", "Inspect"];

/// What a button does.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    /// Run this command line now (it needs no values beyond the selection).
    Run(&'static str),
    /// Prefill the Command field with a template.
    Template,
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
        id: "ecad.convertcomponent",
        group: "Components",
        label: "Convert to Component",
        tooltip: "Make the selected drawing objects one component (runs now)",
        kind: Kind::Run("convertcomponent"),
    },
    Tool {
        id: "ecad.addterminal",
        group: "Components",
        label: "Add Terminal",
        tooltip: "Add a terminal to a component at a point (fills in the Command field)",
        kind: Kind::Template,
    },
    Tool {
        id: "ecad.place",
        group: "Components",
        label: "Place Symbol",
        tooltip: "Place a coil symbol component (fills in the Command field)",
        kind: Kind::Template,
    },
    Tool {
        id: "ecad.wire",
        group: "Wiring",
        label: "Wire",
        tooltip: "Draw a wire; ends on terminals attach (fills in the Command field)",
        kind: Kind::Template,
    },
    Tool {
        id: "ecad.connect",
        group: "Wiring",
        label: "Connect",
        tooltip: "Attach a wire end to a terminal (fills in the Command field)",
        kind: Kind::Template,
    },
    Tool {
        id: "ecad.disconnect",
        group: "Wiring",
        label: "Disconnect",
        tooltip: "Detach a wire end (fills in the Command field)",
        kind: Kind::Template,
    },
    Tool {
        id: "ecad.setprop",
        group: "Edit",
        label: "Set Property",
        tooltip: "Set a property such as the tag (fills in the Command field)",
        kind: Kind::Template,
    },
    Tool {
        id: "ecad.cableadd",
        group: "Wiring",
        label: "Add to Cable",
        tooltip: "Group wires into a cable (fills in the Command field)",
        kind: Kind::Template,
    },
    Tool {
        id: "ecad.delete",
        group: "Edit",
        label: "Delete Electrical Object",
        tooltip: "Delete a component or wire with its connections (fills in the Command field)",
        kind: Kind::Template,
    },
    Tool {
        id: "ecad.newpage",
        group: "Pages",
        label: "New Page",
        tooltip: "Add a page to the project (fills in the Command field)",
        kind: Kind::Template,
    },
    Tool {
        id: "ecad.openpage",
        group: "Pages",
        label: "Open Page",
        tooltip: "Open a closed page again (fills in the Command field)",
        kind: Kind::Template,
    },
    Tool {
        id: "ecad.inspect",
        group: "Inspect",
        label: "Inspect Project",
        tooltip: "Show the project's IDs and history at the command line (read-only; runs now)",
        kind: Kind::Run("ecadinspect"),
    },
];

/// The tab as CADCraft's Tool Sets panel shows it.
pub fn tab() -> ExtraToolTab {
    ExtraToolTab {
        name: TAB.into(),
        groups: GROUPS
            .iter()
            .map(|g| ExtraToolGroup {
                name: (*g).into(),
                tools: TOOLS
                    .iter()
                    .filter(|t| t.group == *g)
                    .map(|t| ExtraTool { id: t.id.into(), label: t.label.into(), tooltip: t.tooltip.into() })
                    .collect(),
            })
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
    /// Terminals whose markers are selected.
    pub terminals: Vec<TerminalId>,
    /// The selection had more than [`MAX_PICKED`] handles; only the first were followed.
    pub truncated: bool,
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
                    let index = index.get_or_insert_with(|| {
                        doc.representations.values().flat_map(|r| r.items.iter().map(|i| (*i, r.subject))).collect()
                    });
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
                None => {}
            }
        }
        Picked {
            truncated: handles.len() > MAX_PICKED,
            handles,
            components: subjects
                .iter()
                .filter_map(|s| if let Subject::Component(c) = s { Some(*c) } else { None })
                .collect(),
            conductors: subjects
                .iter()
                .filter_map(|s| if let Subject::Conductor(c) = s { Some(*c) } else { None })
                .collect(),
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

/// The template line and its note for a template tool (`None` for a run tool or an unknown id).
pub fn template(id: &str, picked: &Picked) -> Option<(String, &'static str)> {
    let hex_or = |v: Option<String>, placeholder: &str| v.unwrap_or_else(|| placeholder.to_string());
    let component = one(&picked.components).map(ComponentId::hex);
    let conductor = one(&picked.conductors).map(ConductorId::hex);
    let terminal = one(&picked.terminals).map(TerminalId::hex);
    Some(match id {
        "ecad.addterminal" => (
            format!(
                r#"addterminal {{"handle":"{}","at":[<x>,<y>]}}"#,
                hex_or(one(&picked.handles).map(|h| h.hex()), "<handle>")
            ),
            "Add Terminal: handle = one object of the component (select it first to fill it in); at = where the \
terminal goes. Replace the <placeholders>, then press Enter.",
        ),
        "ecad.place" => (
            r#"place {"at":[<x>,<y>],"rotation":0}"#.to_string(),
            "Place Symbol: at = insertion point; rotation in degrees (optional). Replace the <placeholders>, then \
press Enter.",
        ),
        "ecad.wire" => (
            r#"wire {"points":[[<x1>,<y1>],[<x2>,<y2>]]}"#.to_string(),
            "Wire: two or more points; an end placed on a terminal attaches to it. Replace the <placeholders> (add \
points as needed), then press Enter.",
        ),
        "ecad.connect" => (
            format!(
                r#"connect {{"conductor":"{}","end":"<A or B>","terminal":"{}"}}"#,
                hex_or(conductor, "<conductor id>"),
                hex_or(terminal, "<terminal id>")
            ),
            "Connect: which wire end (A = first point, B = last) goes to which terminal. Select the wire and a \
terminal marker to fill in their IDs, or find them with Inspect Project. Then press Enter.",
        ),
        "ecad.disconnect" => (
            format!(r#"disconnect {{"conductor":"{}","end":"<A or B>"}}"#, hex_or(conductor, "<conductor id>")),
            "Disconnect: which wire end to detach (A = first point, B = last). Select the wire to fill in its ID. \
Then press Enter.",
        ),
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
            let target = match (component, conductor) {
                (Some(c), None) => format!(r#""component":"{c}""#),
                (None, Some(c)) => format!(r#""conductor":"{c}""#),
                _ => r#""component":"<component id>""#.to_string(),
            };
            (
                format!(r#"ecaddelete {{{target}}}"#),
                "Delete Electrical Object: select one component or wire to fill in its ID (use \"conductor\" for a \
wire). Undo brings it back. Then press Enter.",
            )
        }
        "ecad.newpage" => (
            r#"newpage {"title":"<title>"}"#.to_string(),
            "New Page: the new page's title. Replace the <placeholder>, then press Enter.",
        ),
        "ecad.openpage" => (
            r#"openpage {"page":"<page id>"}"#.to_string(),
            "Open Page: the page's ID, listed by Inspect Project (under \"pages\"). Then press Enter.",
        ),
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
