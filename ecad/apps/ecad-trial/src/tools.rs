//! The **ECAD** tab in CADCraft's Tool Sets panel, beside Drafting and Modeling: buttons for the
//! ECAD commands implemented so far. It is not a new command system: every button reaches the
//! command itself.
//!
//! CADCraft shows the tab and queues clicks (`CadApp::extra_tool_tabs`,
//! `CadApp::take_tool_requests`); everything ECAD about it is here:
//! - **Start** buttons start the command's interactive form (brief 008: prompts, picks, previews,
//!   Enter and Esc at CADCraft's command line), exactly as typing its bare name does. Convert to
//!   Component converts the current selection at once, or asks to select objects.
//! - **Run** buttons type the command into the same path as the banner's Command field, so the
//!   command's own validation, transaction and undo apply.
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
    /// Start this command's interactive form (its prompts appear at the command line).
    Start(&'static str),
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
        tooltip: "Make drawing objects one component (uses the selection, or asks you to select)",
        kind: Kind::Start("convertcomponent"),
    },
    Tool {
        id: "ecad.addterminal",
        group: "Components",
        label: "Add Terminal",
        tooltip: "Click on a component's outline to add terminals; Enter to finish",
        kind: Kind::Start("addterminal"),
    },
    Tool {
        id: "ecad.place",
        group: "Components",
        label: "Place Symbol",
        tooltip: "Place coil symbols by clicking; R then Enter rotates, M then Enter mirrors; Enter to finish",
        kind: Kind::Start("place"),
    },
    Tool {
        id: "ecad.wire",
        group: "Wiring",
        label: "Wire",
        tooltip: "Click points to draw a wire; ends on terminals attach; Enter to finish",
        kind: Kind::Start("wire"),
    },
    Tool {
        id: "ecad.connect",
        group: "Wiring",
        label: "Connect",
        tooltip: "Click a wire near its end, then a terminal",
        kind: Kind::Start("connect"),
    },
    Tool {
        id: "ecad.disconnect",
        group: "Wiring",
        label: "Disconnect",
        tooltip: "Click a wire near an attached end to detach it",
        kind: Kind::Start("disconnect"),
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
