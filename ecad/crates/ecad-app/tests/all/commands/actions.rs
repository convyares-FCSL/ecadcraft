//! Brief 041 (U4b): the action table over the existing commands (moved here from ecad-engine
//! with the table, design 065 §3 cut 13).

use ecad_app::commands::actions::{self, ACTIONS, Key, ObjectKind, PROGRAMMATIC_ONLY, Params, SHELL_KEYS, Slot, ToolGroup};
use ecad_widgets::tokens::Icon;
use std::collections::{BTreeSet, HashSet};

/// ECAD's command ids before this lane (the registry MCP's `list_commands` reads): the lane adds
/// none, removes none (acceptance 1).
const ECAD_COMMANDS: &[&str] = &[
    // Brief 044.
    "connector",
    "pinlabel",
    "convertcomponent",
    "addconnectionpoint",
    "wire",
    "connect",
    "disconnect",
    "cableslash",
    "cableview",
    "cablecore",
    "cablefan",
    "cablereference",
    "cablecontinue",
    "cablegoto",
    "cablereferences",
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
    "references",
    "bom",
    "wirelist",
    "cablelist",
    "printbook",
    "reports",
    "locationbox",
    "terminalblocks",
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
    // Brief 045.
    "plcslot",
    "plcracks",
    "plcchannels",
    "plcchannel",
    "plcaddress",
    "iolist",
    // Brief 047.
    "partline",
    "bomitem",
    "article",
    // Brief 051.
    "plcpage",
    // Brief 050a.
    "standard",
    "setstyle",
    "deletestyle",
    "promote",
    "styles",
    // Brief 050b.
    "category",
    // Brief 048.
    "symbolnew",
    "symboledit",
    "symbolimpact",
    "symbolupdate",
    "makesymbol",
    // Brief 049.
    "contcaption",
    // Brief 054.
    "folderprops",
    "renumberpages",
    "deletepage",
    // Brief 057.
    "rating",
    "ratingtable",
    "ratings",
    // Brief 059.
    "cablenew",
    "cableedit",
    "cableremove",
    // Brief 052.
    "setlocation",
    "setfunction",
    "designations",
    "designationadd",
    "designationedit",
    "designationmerge",
    "designationremove",
    "conflicts",
    "functionbox",
    // Brief 056.
    "plcmaketags",
    "plcexport",
    "plcimport",
    "plccardtype",
    // Brief 055.
    "titleblocks",
    "titleblock",
    "titleblockimport",
    // Lane 072 B step 5.
    "userfield",
    "linkdevices",
    "unlinkdevices",
];

fn ecad_ids() -> BTreeSet<String> {
    ecad_engine::commands::specs().iter().map(|s| s.id.to_string()).collect()
}

#[test]
fn the_lane_adds_no_command_so_mcp_lists_the_same_ones() {
    let want: BTreeSet<String> = ECAD_COMMANDS.iter().map(|s| (*s).to_string()).collect();
    assert_eq!(ecad_ids(), want, "ECAD's registry (what MCP's list_commands lists) changed");
}

#[test]
fn every_action_runs_a_registered_command() {
    ecad_engine::install();
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
        // Brief 048 (Q1): Edit symbol took TopLeft; Mirror keeps X and the selection wheel.
        ["device.edittag", "device.properties", "device.goto", "device.row", "device.delete", "edit.duplicate", "edit.rotate", "symbol.edit"]
    );
    for k in ObjectKind::ALL {
        assert!(actions::wheel(k).iter().flatten().count() <= 8);
    }
}

#[test]
fn tool_groups_list_tools_window_actions_and_greyed_placeholders() {
    // UI rev 3's strip order and flyouts (boards 1.09, 1.10).
    let names: Vec<Vec<&str>> = ToolGroup::ALL.iter().map(|g| g.tools().iter().map(|a| a.label).collect()).collect();
    assert_eq!(
        names,
        [
            vec!["Select"],
            vec!["Line", "Polyline", "Rectangle", "Circle", "Arc"],
            vec!["Text", "Note", "Revision cloud"],
            vec!["Symbol", "Connector"],
            vec!["Terminal", "Terminals across wires", "Draw a link (bridge)", "Swap top and bottom", "Reorder the strip…", "Renumber…"],
            vec!["Rack overview", "Detail page…", "Card…", "Channel…"],
            vec!["Wire", "Bridge", "Continuation"],
            vec!["Cable slash", "Continue cable"],
            vec!["Location box", "Function box", "Function and location box", "Device or PLC outline"],
        ]
    );
    // An item is a tool, a window action, or a placeholder that never runs (greyed in place).
    let cx = actions::Context { target: None, writable: true };
    for g in ToolGroup::ALL {
        for a in g.tools() {
            let placeholder = a.params == Params::App(actions::App::Coming);
            assert!(a.is_tool() || matches!(a.params, Params::App(_)), "{} in {g:?} is neither a tool nor a window action", a.id);
            assert_eq!(placeholder, matches!((a.enabled)(&cx), actions::Enabled::Greyed { .. }), "{}: only a placeholder is greyed", a.id);
        }
    }
    // Rev 3 keeps a key only where a group of 041's survives.
    let letters: String = ToolGroup::ALL.iter().filter_map(|g| g.letter()).collect();
    assert_eq!(letters, "VDTWC");
}

#[test]
fn search_finds_actions_by_label_and_words_but_not_undo() {
    let ids = |q: &str| actions::search(q, 20).iter().map(|a| a.id).collect::<Vec<_>>();
    assert!(ids("fix number").contains(&"wire.fixnumber"));
    assert!(ids("convert").contains(&"component.convert"));
    assert!(ids("undo").is_empty(), "undo is a key only");
    assert!(ids("").is_empty());
}

// ------------------------------------------------------------------------------- Place group (041)
// Moved from ecad-engine's `tester_044_connectors` with the table.

/// §4.1: Place ▸ Connector is one action in the Place group, slot 2, starting `connector`, found by
/// its search words; the device wheel and the terminal wheel gain §9.3's slots.
#[test]
fn place_connector_action_row() {
    use ecad_app::commands::actions::{self, ACTIONS, ToolGroup};
    let a = ACTIONS.iter().find(|a| a.id == "place.connector").expect("place.connector");
    assert_eq!(a.command, "connector");
    assert_eq!(a.label, "Connector");
    // UI rev 3: Place became the Device group (Symbol, Connector); Terminal is its own group.
    assert!(matches!(a.group, Some((ToolGroup::Device, 1))), "{:?}", a.group);
    let place: Vec<&str> = ToolGroup::Device.tools().iter().map(|a| a.label).collect();
    assert_eq!(place, ["Symbol", "Connector"]);
    for w in ["connector", "plug", "socket", "d-sub", "m12", "mated"] {
        assert!(actions::search(w, 20).iter().any(|x| x.id == "place.connector"), "search {w:?}");
    }
    assert!(ACTIONS.iter().any(|a| a.command == "pinlabel"), "Rename pin on the terminal wheel");
}
