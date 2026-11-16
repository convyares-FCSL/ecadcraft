//! Brief 070: typed command inputs. Every registered command is typed (or on the shrink-only
//! list of commands not converted yet), with its typed run function and a JSON Schema; input
//! refusals use one uniform wording; rule refusals keep their words.

use ecad_cadcraft::cmd::{schema_of, specs, typed_commands};
use serde_json::{Value, json};

use crate::common::H;

/// Registered commands still reading their JSON by hand. Brief 070 converts them module by
/// module: remove ids as they are typed, never add one.
const UNTYPED: &[&str] = &[
    "convertcomponent",
    "addterminal",
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
    "newpage",
    "openpage",
    "renamepage",
    "movepage",
    "ecadinspect",
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
    "xrefs",
    "contacts",
    "refgoto",
    "mirrormove",
    "terminals",
    "bridge",
    "stripplan",
    "cablecores",
    "accessory",
    "jumper",
    "prerelease",
    "release",
    "textstyle",
    "showtext",
    "movetext",
    "hiddentext",
    "pagetexts",
    "defaults",
    "agentaccess",
    "termlabel",
    "toposition",
    "canvassheet",
    "setdefault",
    "connector",
    "pinlabel",
    "plcslot",
    "plcracks",
    "plcchannels",
    "plcchannel",
    "iolist",
    "plcaddress",
    "partline",
    "bomitem",
    "article",
    "symbolnew",
    "symboledit",
    "symbolimpact",
    "symbolupdate",
    "makesymbol",
    "contcaption",
    "standard",
    "setstyle",
    "deletestyle",
    "promote",
    "styles",
    "plcpage",
    "conflicts",
    "setlocation",
    "setfunction",
    "designations",
    "designationadd",
    "designationedit",
    "designationmerge",
    "designationremove",
    "folderprops",
    "renumberpages",
    "deletepage",
    "titleblocks",
    "titleblock",
    "titleblockimport",
    "plcmaketags",
    "plcexport",
    "plcimport",
    "plccardtype",
    "rating",
    "ratingtable",
    "ratings",
    "cablenew",
    "cableedit",
    "cableremove",
];

#[test]
fn every_command_is_typed_with_a_schema_or_still_listed() {
    let all = specs();
    let typed = typed_commands();
    for t in &typed {
        let spec = all.iter().find(|s| s.id == t.id).unwrap_or_else(|| panic!("{} is typed but not registered", t.id));
        assert!(std::ptr::fn_addr_eq(spec.run, t.run), "{} is registered with another run function", t.id);
        let schema = schema_of(t.id).unwrap_or(Value::Null);
        assert_eq!(schema["type"], json!("object"), "{}: {schema}", t.id);
        assert!(schema["properties"].is_object(), "{}: {schema}", t.id);
        assert!(!spec.params.is_empty(), "{} keeps its params text", t.id);
        assert!(!UNTYPED.contains(&t.id), "{} is typed now: remove it from UNTYPED", t.id);
    }
    let untyped: Vec<&str> = all.iter().map(|s| s.id).filter(|id| !typed.iter().any(|t| t.id == *id)).collect();
    let unlisted: Vec<&&str> = untyped.iter().filter(|id| !UNTYPED.contains(id)).collect();
    assert!(unlisted.is_empty(), "registered without a typed input and not in UNTYPED: {unlisted:?}");
    let stale: Vec<&&str> = UNTYPED.iter().filter(|id| !untyped.contains(id)).collect();
    assert!(stale.is_empty(), "in UNTYPED but not an untyped registered command: {stale:?}");
}

#[test]
fn a_schema_says_what_the_command_takes() {
    let wire = schema_of("wire").unwrap_or(Value::Null);
    assert_eq!(wire["required"], json!(["points"]));
    assert_eq!(wire["properties"]["points"]["minItems"], json!(2));
    assert_eq!(wire["properties"]["points"]["items"]["format"], json!("point"));
    assert_eq!(wire["additionalProperties"], json!(false), "unknown parameters are refused");
    let net = schema_of("net").unwrap_or(Value::Null);
    assert!(net["properties"]["conductor"].to_string().contains("hex-id"), "{net}");
    let renumber = schema_of("renumberwires").unwrap_or(Value::Null);
    assert!(renumber.to_string().contains("\"unnumbered\""), "{renumber}");
}

/// Input refusals of the wire commands: one wording, from the parameter's name and kind.
#[test]
fn the_wire_commands_refuse_input_uniformly() {
    let rows: &[(&str, Value, &str)] = &[
        ("wire", json!({}), "`points` (list of point [x, y]s) is required"),
        ("wire", json!({"points": 5}), "`points` must be a list of point [x, y]s"),
        ("wire", json!({"points": [[0, 0]]}), "`points` needs at least 2 entries"),
        ("wire", json!({"points": [[0, 0], ["a", 1]]}), "`points` must be a list of point [x, y]s"),
        ("wire", json!({"points": [[0, 0], [9, 0]], "potential": 5}), "`potential` must be text, or null"),
        ("wire", json!({"points": [[0, 0], [9, 0]], "join": 1}), "`join` must be true or false, or null"),
        ("nets", json!({"limit": "x"}), "`limit` must be a whole number, or null"),
        ("net", json!({"conductor": "zz"}), "`conductor` must be a hex id, or null"),
        ("wirenumber", json!({"conductor": "1"}), "`value` (text, or null) is required"),
        ("wirenumber", json!({"conductor": "1", "value": 3}), "`value` must be text, or null"),
        (
            "wirenumber",
            json!({"conductor": "1", "value": "7", "conflict": "x"}),
            "`conflict` must be one of \"swap\", \"another\", \"merge\", or null",
        ),
        ("potential", json!({}), "`conductor` (hex id) is required"),
        ("potential", json!({"conductor": "1"}), "`value` (text) is required"),
        ("category", json!({"conductors": "1", "value": null}), "`conductors` must be a list of hex ids, or null"),
        ("wiresettings", json!({"scheme": 1}), "`scheme` must be text, or null"),
        ("wiresettings", json!({"potentials": 1}), "`potentials` must be a list of objects, or null"),
        ("renumberwires", json!({"scope": "x"}), "`scope` must be one of \"unnumbered\", \"all\", or null"),
        ("junction", json!({}), "`at` (point [x, y]) is required"),
        ("joinwires", json!({"at": "here"}), "`at` must be a point [x, y]"),
        ("checks", json!({"kinds": "nope"}), "`kinds` must be a list of texts, or null"),
    ];
    let mut h = H::new(7001);
    for (cmd, params, says) in rows {
        assert_eq!(h.run(cmd, params.clone()).err().as_deref(), Some(*says), "{cmd} {params}");
    }
}

/// Rule refusals keep their words.
#[test]
fn the_wire_commands_keep_their_rule_words() {
    let rows: &[(&str, Value, &str)] = &[
        ("wire", json!({"points": [[0, 0], [9, 0]], "join": true, "junction": false}), "`join` and `junction` disagree; give only `join`"),
        ("nets", json!({"after": "zz"}), "`after` must be a cursor from `next`"),
        ("net", json!({}), "one of `conductor`, `terminal` or `number` is required"),
        ("net", json!({"conductor": "1"}), "No such wire."),
        ("net", json!({"terminal": "1"}), "No such connection point."),
        ("net", json!({"number": "999"}), "no net has number 999"),
        ("wirenumber", json!({"conductor": "1", "value": "a b"}), "A wire number has no spaces or control characters."),
        ("potential", json!({"conductor": "1", "value": "24V"}), "No such wire."),
        ("category", json!({"value": null}), "Give one of `conductor`, `net` or `conductors`."),
        ("wiresettings", json!({"digits": 9}), "`digits` must be 1 to 6"),
        ("wiresettings", json!({"potentials": [{"name": "24V", "colour": "RD"}]}), ecad_electrical::msg::CATEGORY_COLOURS),
        ("joinwires", json!({"at": [500, 500]}), ecad_electrical::msg::NO_ENDS_TO_JOIN),
        ("checks", json!({"kinds": ["nope"]}), "unknown check kind \"nope\""),
    ];
    let mut h = H::new(7004);
    for (cmd, params, says) in rows {
        assert_eq!(h.run(cmd, params.clone()).err().as_deref(), Some(*says), "{cmd} {params}");
    }
}

/// A parameter a command does not take is refused, naming it (it was silently ignored before).
#[test]
fn an_unknown_parameter_is_refused() {
    let mut h = H::new(7002);
    let e = h.run("wire", json!({"points": [[0, 0], [9, 0]], "colour": "RD"})).err().unwrap_or_default();
    assert_eq!(e, "`colour` is not a parameter of this command; it takes `category`, `join`, `junction`, `points`, `potential`");
    assert!(h.doc().conductors.is_empty(), "nothing was drawn");
}

/// The typed commands still work: a wire, its net, a fixed number and a released one.
#[test]
fn the_typed_wire_commands_still_work() {
    let mut h = H::new(7003);
    let w = h.run("wire", json!({"points": [[0, 0], [30, 0]]})).unwrap_or(Value::Null);
    let c = w["conductor"].as_str().unwrap_or_default().to_string();
    let n = h.run("net", json!({"conductor": c})).unwrap_or(Value::Null);
    assert_eq!(n["conductors"], json!([c]));
    let f = h.run("wirenumber", json!({"conductor": c, "value": "W7"})).unwrap_or(Value::Null);
    assert_eq!(f, json!({"number": "W7", "fixed": true, "conductors": 1}));
    let r = h.run("wirenumber", json!({"conductor": c, "fixed": false})).unwrap_or(Value::Null);
    assert_eq!(r, json!({"number": "W7", "fixed": false, "conductors": 1}));
    let again = h.run("wirenumber", json!({"conductor": c, "value": null})).unwrap_or(Value::Null);
    assert_eq!(again["changed"], json!(false));
    let listed = h.run("nets", json!({"limit": 1})).unwrap_or(Value::Null);
    assert_eq!(listed["nets"].as_array().map(Vec::len), Some(1));
}

/// Input refusals of the page, project, folder and revision commands: one wording.
#[test]
fn the_sheet_commands_refuse_input_uniformly() {
    let rows: &[(&str, Value, &str)] = &[
        ("newfolder", json!({}), "`name` (text) is required"),
        ("newfolder", json!({"name": 5}), "`name` must be text"),
        ("renamefolder", json!({"name": "x"}), "`folder` (hex id) is required"),
        ("movefolder", json!({"folder": "1"}), "`parent` (hex id) is required"),
        ("deletefolder", json!({"folder": "zz"}), "`folder` must be a hex id"),
        ("pageprops", json!({"number": 5}), "`number` must be text, or null"),
        ("projectprops", json!({"tag_dash": "yes"}), "`tag_dash` must be true or false, or null"),
        (
            "projectprops",
            json!({"reference_style": "x"}),
            "`reference_style` must be one of \"page-zone\", \"page-column-row\", \"page-column\", or null",
        ),
        ("revision", json!({"fields": 1}), "`fields` must be an object, or null"),
        ("openrevision", json!({}), "`rev` (text) is required"),
        ("closerevision", json!({"rev": "A"}), "`rev` is not a parameter of this command; it takes none"),
    ];
    let mut h = H::new(7005);
    for (cmd, params, says) in rows {
        assert_eq!(h.run(cmd, params.clone()).err().as_deref(), Some(*says), "{cmd} {params}");
    }
}

/// Rule refusals of the page, project, folder and revision commands keep their words.
#[test]
fn the_sheet_commands_keep_their_rule_words() {
    let rows: &[(&str, Value, &str)] = &[
        ("newfolder", json!({"name": " "}), ecad_electrical::msg::FOLDER_NAME),
        ("newfolder", json!({"name": "A", "parent": "1"}), "no such folder"),
        ("movefolder", json!({"folder": "1", "parent": null}), "no such folder"),
        ("pageprops", json!({"title": null}), ecad_electrical::msg::PAGE_TITLE),
        ("pageprops", json!({"generated": "bom"}), ecad_cadcraft::generated::KIND_FIXED),
        ("pageprops", json!({"number": "a b"}), "A page number is 1 to 12 letters, digits or _ (no spaces, `.`, `-` or `/`)."),
        ("pageprops", json!({"function": "=F1", "proposal": "x"}), ecad_electrical::msg::PROPOSAL_WORDS),
        ("projectprops", json!({"page_number_step": 0}), "`page_number_step` must be a whole number from 1 to 1000"),
        ("projectprops", json!({"tag_case": "x"}), "`tag_case` must be \"upper\" or \"typed\""),
        ("projectprops", json!({"props": {"Bad Key": "x"}}), "A property name is 1 to 40 characters a-z, 0-9 or _."),
        ("projectprops", json!({"tag_padding": 9}), "`tag_padding` must be a whole number from 1 to 6"),
        ("revision", json!({"n": 99}), "There is no pending revision row 99."),
        ("revision", json!({"fields": {"issued": "x"}}), "\"issued\" is set by issuing."),
        ("openrevision", json!({"rev": "Z"}), "Rev Z has not been issued."),
        ("undoissue", json!({}), "The last step did not issue a revision; use Undo."),
    ];
    let mut h = H::new(7006);
    for (cmd, params, says) in rows {
        assert_eq!(h.run(cmd, params.clone()).err().as_deref(), Some(*says), "{cmd} {params}");
    }
}

/// The typed page, project, folder and revision commands still work, and answer as before.
#[test]
fn the_typed_sheet_commands_still_work() {
    let mut h = H::new(7007);
    let f = h.run("newfolder", json!({"name": "Power"})).unwrap_or(Value::Null);
    let folder = f["folder"].as_str().unwrap_or_default().to_string();
    assert_eq!(f["name"], json!("Power"));
    let r = h.run("renamefolder", json!({"folder": folder, "name": "Power"})).unwrap_or(Value::Null);
    assert_eq!(r["changed"], json!(false));
    let r = h.run("renamefolder", json!({"folder": folder, "name": "Supply"})).unwrap_or(Value::Null);
    assert_eq!(r["changed"], json!(true));
    let m = h.run("movefolder", json!({"folder": folder, "parent": null})).unwrap_or(Value::Null);
    assert_eq!(m["changed"], json!(false));
    let pp = h.run("pageprops", json!({"number": "7"})).unwrap_or(Value::Null);
    assert_eq!((&pp["number"], &pp["changed"]), (&json!("7"), &json!(true)));
    let read = h.run("pageprops", json!({})).unwrap_or(Value::Null);
    assert!(read.get("changed").is_none(), "no fields only reads: {read}");
    let pr = h.run("projectprops", json!({"title": "Plant", "props": {"site": "North"}})).unwrap_or(Value::Null);
    assert_eq!((&pr["title"], &pr["props"]["site"], &pr["changed"]), (&json!("Plant"), &json!("North"), &json!(true)));
    let same = h.run("projectprops", json!({"sheet": null})).unwrap_or(Value::Null);
    assert_eq!(same["changed"], json!(false), "a null field writes nothing but is not a read");
    let rv = h.run("revision", json!({"description": "First", "drawn": "CV"})).unwrap_or(Value::Null);
    assert_eq!(rv["changed"], json!(true));
    let issued = h.run("issuerevision", json!({"date": "2026-11-16"})).unwrap_or(Value::Null);
    assert_eq!((&issued["rev"], &issued["date"]), (&json!(rv["pending_rev"].clone()), &json!("2026-11-16")));
    let rev = issued["rev"].as_str().unwrap_or_default().to_string();
    let open = h.run("openrevision", json!({"rev": rev.to_lowercase()})).unwrap_or(Value::Null);
    assert_eq!(open["rev"], json!(rev));
    assert_eq!(h.run("closerevision", json!({})).unwrap_or(Value::Null), json!({"closed": rev}));
    let undone = h.run("undoissue", json!({})).unwrap_or(Value::Null);
    assert_eq!(undone, json!({"rev": rev, "undone": true}));
    let d = h.run("deletefolder", json!({"folder": folder})).unwrap_or(Value::Null);
    assert_eq!(d["pages_moved"], json!(0));
}

/// Input refusals of the box and report commands: one wording.
#[test]
fn the_output_commands_refuse_input_uniformly() {
    let rows: &[(&str, Value, &str)] = &[
        ("locationbox", json!({}), "`location` (text) is required"),
        ("locationbox", json!({"location": "+CP1", "corners": [[0, 0]]}), "`corners` needs at least 2 entries"),
        ("functionbox", json!({"function": 5}), "`function` must be text"),
        ("bom", json!({"lines": "x"}), "`lines` must be one of \"flat\", \"nested\", or null"),
        ("bom", json!({"limit": "x"}), "`limit` must be a whole number, or null"),
        ("cablelist", json!({"include_not_drawn": 1}), "`include_not_drawn` must be true or false, or null"),
        ("printbook", json!({"lineweights": "x"}), "`lineweights` must be true or false, or null"),
        ("reports", json!({"x": 1}), "`x` is not a parameter of this command; it takes none"),
    ];
    let mut h = H::new(7008);
    for (cmd, params, says) in rows {
        assert_eq!(h.run(cmd, params.clone()).err().as_deref(), Some(*says), "{cmd} {params}");
    }
}

/// Rule refusals of the box and report commands keep their words.
#[test]
fn the_output_commands_keep_their_rule_words() {
    let scope = "`scope` is \"all\", {folder: id} or {pages: \"100, 105-110\"}";
    let rows: &[(&str, Value, &str)] = &[
        (
            "locationbox",
            json!({"location": "+CP1"}),
            "give exactly one of `corners` (two corners), `points` (a closed outline) or `item` (a closed polyline)",
        ),
        ("locationbox", json!({"location": "+CP1", "corners": [[0, 0], [50, 50]], "proposal": "x"}), ecad_electrical::msg::PROPOSAL_WORDS),
        ("bom", json!({"group": "x"}), "`group` is \"location\" or \"part\""),
        ("bom", json!({"columns": ["x"]}), "`columns` lists optional columns: [\"article\"]"),
        ("printbook", json!({"scope": "some"}), scope),
        ("printbook", json!({"scope": {}}), scope),
        ("printbook", json!({"colour": "pink"}), "`colour` is \"mono\" (black, the colour code in the wire label) or \"colour\" (wire colours)"),
    ];
    let mut h = H::new(7009);
    for (cmd, params, says) in rows {
        assert_eq!(h.run(cmd, params.clone()).err().as_deref(), Some(*says), "{cmd} {params}");
    }
}

/// The typed box and report commands still work, and answer as before.
#[test]
fn the_typed_output_commands_still_work() {
    let mut h = H::new(7010);
    let b = h.run("locationbox", json!({"location": "+CP1", "corners": [[10, 10], [60, 60]]})).unwrap_or(Value::Null);
    assert_eq!(
        (&b["inside"], &b["message"]),
        (&json!(0), &json!("Location box +CP1: 0 devices inside. New location +CP1 added to the project's list."))
    );
    let f = h.run("functionbox", json!({"function": "=F1", "points": [[100, 10], [150, 10], [150, 60], [100, 60]]})).unwrap_or(Value::Null);
    assert!(f["message"].as_str().is_some_and(|m| m.starts_with("Function box =F1")), "{f}");
    let bom = h.run("bom", json!({"group": "part", "columns": ["article"], "lines": "flat"})).unwrap_or(Value::Null);
    assert!(bom["rows"].is_array() && bom["total"].is_number(), "{bom}");
    let wires = h.run("wirelist", json!({"limit": 1})).unwrap_or(Value::Null);
    assert!(wires["rows"].is_array(), "{wires}");
    let cables = h.run("cablelist", json!({"include_not_drawn": false})).unwrap_or(Value::Null);
    assert!(cables["rows"].is_array(), "{cables}");
    assert_eq!(h.run("reports", json!({})).unwrap_or(Value::Null), json!({"open": "reports"}));
}
