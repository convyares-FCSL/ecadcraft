//! Brief 041 (U4b): the action table over the existing commands, what is under the cursor, and
//! the running tool's intent.

use crate::common::{H, id};

use cadcraft_engine::geom::Vec2;
use ecad_cadcraft::actions::{self, ACTIONS, Key, ObjectKind, PROGRAMMATIC_ONLY, Params, SHELL_KEYS, Slot, ToolGroup};
use ecad_cadcraft::interact::{intent, kind};
use ecad_model::{ComponentId, ConductorId, End, PageId};
use ecad_tokens::Icon;
use serde_json::json;
use std::collections::{BTreeSet, HashSet};

/// ECAD's command ids before this lane (the registry MCP's `list_commands` reads): the lane adds
/// none, removes none (acceptance 1).
const ECAD_COMMANDS: &[&str] = &[
    // Brief 044.
    "connector",
    "pinlabel",
    "convertcomponent",
    "addterminal",
    "wire",
    "connect",
    "disconnect",
    "cableslash",
    "cableview",
    "cablecore",
    "cablefan",
    "cableref",
    "cablecontinue",
    "cablegoto",
    "cablerefs",
    "ecaddelete",
    "setprop",
    "cableadd",
    "place",
    "symbols",
    "devices",
    "tagsettings",
    "deviceprops",
    "devicedelete",
    "nets",
    "net",
    "wirenumber",
    "potential",
    "wiresettings",
    "renumberwires",
    "junction",
    "checks",
    "newpage",
    "openpage",
    "renamepage",
    "movepage",
    "ecadinspect",
    "joinwires",
    "history",
    "steps",
    "pointinfo",
    "version",
    "showpoint",
    "closepoint",
    "restore",
    "branches",
    "branch",
    "renamebranch",
    "switchbranch",
    "archivebranch",
    "reopenbranch",
    "merge",
    "undomerge",
    "renumber",
    "finalise",
    "numbering",
    "contacts",
    "refgoto",
    "mirrormove",
    "xrefs",
    "bom",
    "wirelist",
    "cablelist",
    "printbook",
    "reports",
    "locationbox",
    "terminals",
    "bridge",
    "stripplan",
    "cablecores",
    "accessory",
    "jumper",
    "prerelease",
    "release",
    "pageprops",
    "projectprops",
    "newfolder",
    "renamefolder",
    "movefolder",
    "deletefolder",
    "revision",
    "issuerevision",
    "openrevision",
    "closerevision",
    "undoissue",
    "textstyle",
    "showtext",
    "movetext",
    "hiddentext",
    "pagetexts",
    "defaults",
    "agentaccess",
    "termlabel",
    "toposition",
    "setdefault",
    "canvassheet",
    // Brief 047.
    "partline",
    "bomitem",
    "article",
    // Brief 048.
    "symbolnew",
    "symboledit",
    "symbolimpact",
    "symbolupdate",
    "makesymbol",
];

fn ecad_ids() -> BTreeSet<String> {
    ecad_cadcraft::commands::specs().iter().map(|s| s.id.to_string()).collect()
}

#[test]
fn the_lane_adds_no_command_so_mcp_lists_the_same_ones() {
    let want: BTreeSet<String> = ECAD_COMMANDS.iter().map(|s| (*s).to_string()).collect();
    assert_eq!(ecad_ids(), want, "ECAD's registry (what MCP's list_commands lists) changed");
}

#[test]
fn every_action_runs_a_registered_command() {
    ecad_cadcraft::install();
    for a in ACTIONS {
        if a.command.is_empty() {
            assert!(matches!(a.params, Params::App(_)), "{}: only a window action may name no command", a.id);
            continue;
        }
        assert!(cadcraft_engine::cmd::find_command(a.command).is_some(), "{}: `{}` is not a registered command", a.id, a.command);
    }
}

#[test]
fn every_ecad_command_is_in_an_action_or_programmatic_only() {
    let ids = ecad_ids();
    for id in &ids {
        let listed = PROGRAMMATIC_ONLY.iter().any(|(c, why)| c == id && !why.is_empty());
        assert!(actions::covers(id) || listed, "`{id}` is in no action and not in PROGRAMMATIC_ONLY");
        assert!(!(actions::covers(id) && listed), "`{id}` is in an action and also in PROGRAMMATIC_ONLY");
    }
    for (c, _) in PROGRAMMATIC_ONLY {
        assert!(ids.contains(*c), "PROGRAMMATIC_ONLY names `{c}`, which is no ECAD command");
    }
}

#[test]
fn action_ids_are_unique_and_no_two_share_a_key() {
    let mut ids = HashSet::new();
    let mut keys: HashSet<Key> = HashSet::new();
    for a in ACTIONS {
        assert!(ids.insert(a.id), "{} twice", a.id);
        if let Some(k) = a.key {
            assert!(keys.insert(k), "{} shares {}", a.id, k.words());
            assert!(!SHELL_KEYS.contains(&k.words()), "{} takes the shell's {}", a.id, k.words());
        }
    }
}

#[test]
fn slots_mean_the_same_on_every_wheel() {
    for k in ObjectKind::ALL {
        for (s, a) in Slot::ALL.iter().zip(actions::wheel(k)) {
            let Some(a) = a else { continue };
            match s {
                Slot::Bottom => assert_eq!(a.icon, Icon::Delete, "{k:?} bottom is {}", a.id),
                Slot::Right => assert!(a.icon == Icon::GoTo && !a.writes(), "{k:?} right is {}, not a go-to", a.id),
                Slot::BottomRight => assert!(a.icon == Icon::ShowRow && a.label.starts_with("Show"), "{k:?} bottom right is {}", a.id),
                _ => assert_ne!(a.icon, Icon::Delete, "{k:?}: a delete off the bottom slot ({})", a.id),
            }
        }
    }
}

#[test]
fn a_kind_with_fewer_actions_leaves_its_slots_empty() {
    let r = actions::wheel(ObjectKind::Reference);
    let ids: Vec<Option<&str>> = r.iter().map(|a| a.map(|a| a.id)).collect();
    assert_eq!(ids, [Some("reference.goto"), Some("reference.preview"), Some("reference.goto"), None, None, None, None, None]);
    // A wire's Show row waits for the Wires list: the slot stays empty, nothing moves into it.
    assert!(actions::wheel(ObjectKind::Wire)[Slot::BottomRight.index()].is_none());
    let d: Vec<&str> = actions::wheel(ObjectKind::Device).iter().map(|a| a.map_or("-", |a| a.id)).collect();
    assert_eq!(
        d,
        ["device.edittag", "device.properties", "device.goto", "device.row", "device.delete", "edit.duplicate", "edit.rotate", "edit.mirror"]
    );
    for k in ObjectKind::ALL {
        assert!(actions::wheel(k).iter().flatten().count() <= 8);
    }
}

#[test]
fn tool_groups_list_only_tools_that_start_a_command() {
    let names: Vec<Vec<&str>> = ToolGroup::ALL.iter().map(|g| g.tools().iter().map(|a| a.label).collect()).collect();
    assert_eq!(
        names,
        [
            vec!["Select"],
            vec!["Wire"],
            vec!["Symbol", "Terminal", "Connector"],
            vec!["Cable slash", "Continue cable"],
            vec!["Line", "Polyline", "Rectangle", "Circle", "Arc"],
            vec!["Location box"],
            vec!["Text", "Note", "Revision cloud"],
        ]
    );
    for g in ToolGroup::ALL {
        for a in g.tools() {
            assert!(a.is_tool() || a.id == "select", "{} in {g:?} is not a tool", a.id);
        }
    }
    let letters: String = ToolGroup::ALL.iter().map(|g| g.letter()).collect();
    assert_eq!(letters, "VWPCDBT");
}

#[test]
fn search_finds_actions_by_label_and_words_but_not_undo() {
    let ids = |q: &str| actions::search(q, 20).iter().map(|a| a.id).collect::<Vec<_>>();
    assert!(ids("fix number").contains(&"wire.fixnumber"));
    assert!(ids("convert").contains(&"component.convert"));
    assert!(ids("undo").is_empty(), "undo is a key only");
    assert!(ids("").is_empty());
}

// ---------------------------------------------------------------- what is under the cursor

fn new_h() -> H {
    let mut h = H::new(41);
    h.s.viewport_px = (1280.0, 800.0);
    h
}

fn uid(h: &H) -> u64 {
    h.s.state().unwrap().uid
}

fn classify(h: &mut H, at: (f64, f64), picked: Option<cadcraft_engine::doc::Handle>, selection: &[cadcraft_engine::doc::Handle]) -> kind::Target {
    let u = uid(h);
    let sel = selection.to_vec();
    h.p(|p| kind::classify(p, u, Vec2::new(at.0, at.1), 0.5, picked, &sel, false))
}

/// `-K01`'s coil at (0, 0) and a wire from its A1 to the right.
fn coil_and_wire() -> (H, PageId, ComponentId, ConductorId) {
    let mut h = new_h();
    let page = h.first_page();
    let k1: ComponentId = id(&h.run("place", json!({"symbol": "coil", "at": [0, 0]})).unwrap(), "component");
    let doc = h.doc();
    let a1 = doc.terminals_of(k1).find(|(_, t)| t.label.as_deref() == Some("A1")).map(|(t, _)| t).unwrap();
    let at = h.terminal_pos(page, a1);
    let w: ConductorId = id(&h.run("wire", json!({"points": [[at.x, at.y], [at.x + 60.0, at.y]]})).unwrap(), "conductor");
    (h, page, k1, w)
}

#[test]
fn the_coil_is_a_device_named_by_its_tag() {
    let (mut h, page, k1, _) = coil_and_wire();
    let item = h.symbol_item(k1);
    let handle = h.handle_of(page, item);
    let t = classify(&mut h, (0.0, 0.0), Some(handle), &[]);
    assert_eq!(t.kind, ObjectKind::Device);
    assert_eq!(t.component, Some(k1));
    assert_eq!(t.name, "-K01");
    assert!(t.handles.contains(&handle));
    assert_eq!(t.others, 0);
}

#[test]
fn a_wire_its_end_and_empty_paper() {
    let (mut h, page, k1, w) = coil_and_wire();
    let (a, b) = h.route_ends(page, w);
    let route = h.route_handle(page, w);
    let mid = ((a.x + b.x) / 2.0, (a.y + b.y) / 2.0);
    let t = classify(&mut h, mid, Some(route), &[]);
    assert_eq!((t.kind, t.conductor), (ObjectKind::Wire, Some(w)));
    let t = classify(&mut h, (b.x, b.y), Some(route), &[]);
    assert_eq!((t.kind, t.conductor, t.end), (ObjectKind::WireEnd, Some(w), Some(End::B)));
    let t = classify(&mut h, (500.0, 500.0), None, &[]);
    assert_eq!(t.kind, ObjectKind::EmptyPage);
    assert!(t.name.starts_with("Page "), "{}", t.name);
    // A selection of two that holds the picked object is mixed.
    let item = h.symbol_item(k1);
    let coil = h.handle_of(page, item);
    let t = classify(&mut h, mid, Some(route), &[route, coil]);
    assert_eq!((t.kind, t.handles.len()), (ObjectKind::Mixed, 2));
}

#[test]
fn wire_intent_names_the_connection_point_under_the_cursor() {
    let (mut h, page, k1, w) = coil_and_wire();
    let doc = h.doc();
    let a2 = doc.terminals_of(k1).find(|(_, t)| t.label.as_deref() == Some("A2")).map(|(t, _)| t).unwrap();
    let a1 = doc.terminals_of(k1).find(|(_, t)| t.label.as_deref() == Some("A1")).map(|(t, _)| t).unwrap();
    let (p1, p2) = (h.terminal_pos(page, a1), h.terminal_pos(page, a2));
    let u = uid(&h);
    let s = h.p(|p| intent::sentence(p, u, "wire", Vec2::new(p2.x, p2.y), 0.5, None));
    assert_eq!(s.as_deref(), Some("joins A2 of -K01"));
    let wire = kind::wire_words(&doc, w);
    assert!(wire.starts_with("wire "), "{wire}");
    let s = h.p(|p| intent::sentence(p, u, "wire", Vec2::new(p1.x, p1.y), 0.5, None));
    assert_eq!(s, Some(format!("joins A1 of -K01 to {wire}")));
    let s = h.p(|p| intent::sentence(p, u, "line", Vec2::new(p1.x, p1.y), 0.5, None));
    assert_eq!(s, None, "other tools show their prompt");
}
