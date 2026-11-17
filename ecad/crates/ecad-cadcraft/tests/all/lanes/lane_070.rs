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
    "accessory",
    "jumper",
    "prerelease",
    "release",
    "agentaccess",
    "termlabel",
    "toposition",
    "canvassheet",
    "setdefault",
    "connector",
    "pinlabel",
    "partline",
    "bomitem",
    "article",
    "contcaption",
    "conflicts",
    "setlocation",
    "setfunction",
    "folderprops",
    "renumberpages",
    "deletepage",
    "titleblocks",
    "titleblock",
    "titleblockimport",
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
        ("potential", json!({"conductor": "1"}), "`value` (text, or null) is required"),
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
        ("movefolder", json!({"folder": "1"}), "`parent` (hex id, or null) is required"),
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

/// Input refusals of the text commands: one wording.
#[test]
fn the_text_commands_refuse_input_uniformly() {
    let rows: &[(&str, Value, &str)] = &[
        ("textstyle", json!({"kind": "tag", "height": "x"}), "`height` must be a number, or null"),
        ("textstyle", json!({"kinds": 1}), "`kinds` must be a list of objects, or null"),
        ("showtext", json!({"show": "on"}), "`kind` (text) is required"),
        ("showtext", json!({"kind": "tag"}), "`show` (text, or null) is required"),
        ("movetext", json!({"to": [1, 2]}), "`representation` (hex id) is required"),
        ("movetext", json!({"representation": "1", "to": [1]}), "`to` must be a point [x, y], or null"),
        ("hiddentext", json!({"on": "yes"}), "`on` must be true or false, or null"),
        ("pagetexts", json!({"page": "zz"}), "`page` must be a hex id, or null"),
        ("defaults", json!({"group": 1}), "`group` must be text, or null"),
    ];
    let mut h = H::new(7011);
    for (cmd, params, says) in rows {
        assert_eq!(h.run(cmd, params.clone()).err().as_deref(), Some(*says), "{cmd} {params}");
    }
}

/// Rule refusals of the text commands keep their words.
#[test]
fn the_text_commands_keep_their_rule_words() {
    let rows: &[(&str, Value, &str)] = &[
        ("textstyle", json!({"kind": "tag", "height": 11}), ecad_cadcraft::text_cmds::BAD_HEIGHT),
        ("textstyle", json!({"height": 3}), ecad_cadcraft::text_cmds::NEEDS_KIND),
        ("textstyle", json!({"kinds": [{"height": 3}]}), ecad_cadcraft::text_cmds::NEEDS_KIND),
        ("textstyle", json!({"font": ""}), ecad_cadcraft::text_cmds::BAD_FONT),
        ("showtext", json!({"kind": "tag", "show": "maybe"}), ecad_cadcraft::text_cmds::BAD_SHOW),
        (
            "showtext",
            json!({"kind": "tag", "show": "on", "page": "1", "component": "2"}),
            "Name at most one of representation, component, conductor and page.",
        ),
        ("showtext", json!({"kind": "tag", "show": "on", "page": "1"}), "No page 00000000000000000000000000000001."),
        ("movetext", json!({"representation": "1"}), ecad_cadcraft::text_cmds::BAD_POINT),
        ("movetext", json!({"representation": "1", "to": [1, 2], "justify": "XX"}), ecad_cadcraft::text_cmds::BAD_JUSTIFY),
        ("movetext", json!({"representation": "1", "to": [1, 2]}), "No representation 00000000000000000000000000000001."),
        ("pagetexts", json!({"page": "1"}), "No page 00000000000000000000000000000001."),
        ("defaults", json!({"key": "no.such"}), "No default \"no.such\"."),
    ];
    let mut h = H::new(7012);
    for (cmd, params, says) in rows {
        assert_eq!(h.run(cmd, params.clone()).err().as_deref(), Some(*says), "{cmd} {params}");
    }
    let e = h.run("showtext", json!({"kind": "nope", "show": "on"})).err().unwrap_or_default();
    assert!(e.starts_with("Unknown text kind \"nope\". Kinds: tag, point_label"), "{e}");
}

/// The typed text commands still work, and answer as before.
#[test]
fn the_typed_text_commands_still_work() {
    let mut h = H::new(7013);
    let read = h.run("textstyle", json!({})).unwrap_or(Value::Null);
    assert_eq!(read["changed"], json!(false));
    let set = h.run("textstyle", json!({"kind": "tag", "height": 3, "kinds": [{"kind": "point_label", "weight": "bold"}]})).unwrap_or(Value::Null);
    assert_eq!(set["changed"], json!(true));
    let same = h.run("textstyle", json!({"kind": "tag", "height": 3})).unwrap_or(Value::Null);
    assert_eq!(same["changed"], json!(false));
    let shown = h.run("showtext", json!({"kind": "tag", "show": "off"})).unwrap_or(Value::Null);
    assert_eq!(shown, json!({"level": "project", "target": null, "kind": "tag", "show": "off", "changed": true}));
    let back = h.run("showtext", json!({"kind": "tag", "show": null})).unwrap_or(Value::Null);
    assert_eq!((&back["show"], &back["changed"]), (&Value::Null, &json!(true)));
    let on = h.run("hiddentext", json!({})).unwrap_or(Value::Null);
    assert_eq!(on["on"], json!(true));
    let texts = h.run("pagetexts", json!({})).unwrap_or(Value::Null);
    assert!(texts["texts"].is_array(), "{texts}");
    let d = h.run("defaults", json!({"key": "text.tag.show"})).unwrap_or(Value::Null);
    assert_eq!(d["defaults"].as_array().map(Vec::len), Some(1), "{d}");
}

/// Input refusals of the style commands: one wording.
#[test]
fn the_style_commands_refuse_input_uniformly() {
    let rows: &[(&str, Value, &str)] = &[
        ("standard", json!({"new": 5}), "`new` must be text, or null"),
        ("standard", json!({"remove": true, "replace_with": "x"}), "`replace_with` must be an object, or null"),
        ("setstyle", json!({"id": "tag"}), "`catalogue` (text) is required"),
        ("setstyle", json!({"catalogue": "text", "id": "tag", "fields": 1}), "`fields` must be an object, or null"),
        ("setstyle", json!({"catalogue": "text", "id": "tag", "fields": {"height": true}}), "`fields` must be an object, or null"),
        (
            "setstyle",
            json!({"catalogue": "text", "id": "tag", "colour": "RD"}),
            "`colour` is not a parameter of this command; it takes `catalogue`, `copy_of`, `fields`, `id`, `layer`, `name`, `new`, `rename`",
        ),
        ("deletestyle", json!({"id": "x"}), "`catalogue` (text) is required"),
        ("promote", json!({"key": 5}), "`key` must be text, or null"),
        ("styles", json!({"catalogue": 5}), "`catalogue` must be text, or null"),
    ];
    let mut h = H::new(7014);
    for (cmd, params, says) in rows {
        assert_eq!(h.run(cmd, params.clone()).err().as_deref(), Some(*says), "{cmd} {params}");
    }
}

/// Rule refusals of the style commands keep their words.
#[test]
fn the_style_commands_keep_their_rule_words() {
    use ecad_cadcraft::style_cmds::{BUILTIN_KEPT, NEEDS_NAME, STANDARD_FIRST, STYLE_NEEDS_NAME};
    let rows: &[(&str, Value, &str)] = &[
        ("standard", json!({"name": "S"}), STANDARD_FIRST),
        ("standard", json!({"remove": true}), STANDARD_FIRST),
        ("standard", json!({"new": " "}), NEEDS_NAME),
        ("setstyle", json!({"catalogue": "nope", "id": "tag"}), "No catalogue \"nope\"; the catalogues are"),
        ("setstyle", json!({"catalogue": "text"}), "Name the style: `id` or `name`."),
        ("setstyle", json!({"catalogue": "text", "id": "tag"}), "Give `fields` to set, `rename`, or `new: true`."),
        ("setstyle", json!({"catalogue": "text", "id": "tag", "layer": "standard", "fields": {"height": 3}}), STANDARD_FIRST),
        ("setstyle", json!({"catalogue": "text", "id": "tag", "layer": "x", "fields": {"height": 3}}), "Unknown layer \"x\": project or standard."),
        ("setstyle", json!({"catalogue": "text", "id": "tag", "fields": {"nope": 1}}), "No field \"nope\"; Text styles have"),
        ("setstyle", json!({"catalogue": "text", "id": "tag", "fields": {"height": 99}}), "height is"),
        ("setstyle", json!({"catalogue": "text", "new": true}), STYLE_NEEDS_NAME),
        ("deletestyle", json!({"catalogue": "text", "id": "tag"}), BUILTIN_KEPT),
        ("promote", json!({"key": "no.such"}), "No default \"no.such\"."),
        ("promote", json!({"id": "tag"}), "`catalogue` is required:"),
        ("promote", json!({"catalogue": "text", "id": "tag"}), STANDARD_FIRST),
        ("styles", json!({"catalogue": "nope"}), "No catalogue \"nope\""),
    ];
    let mut h = H::new(7015);
    for (cmd, params, says) in rows {
        let e = h.run(cmd, params.clone()).err().unwrap_or_default();
        assert!(e.starts_with(says), "{cmd} {params}: {e}");
    }
}

/// The typed style commands still work, and answer as before.
#[test]
fn the_typed_style_commands_still_work() {
    let mut h = H::new(7016);
    let read = h.run("standard", json!({})).unwrap_or(Value::Null);
    assert_eq!(read, json!({"name": "IEC (built in)", "id": null}));
    let made = h.run("standard", json!({"new": "Plant"})).unwrap_or(Value::Null);
    assert_eq!((&made["changed"], &made["standard"]["name"]), (&json!(true), &json!("Plant")));
    let same = h.run("standard", json!({"name": "Plant"})).unwrap_or(Value::Null);
    assert_eq!(same["changed"], json!(false));
    let new = h.run("setstyle", json!({"catalogue": "text", "new": true, "name": "Big", "fields": {"height": 5}})).unwrap_or(Value::Null);
    assert_eq!((&new["name"], &new["changed"]), (&json!("Big"), &json!(true)));
    let id = new["id"].as_str().unwrap_or_default().to_string();
    let set = h.run("setstyle", json!({"catalogue": "text", "id": id, "fields": {"weight": "bold"}})).unwrap_or(Value::Null);
    assert_eq!(set["fields"]["weight"]["value"], json!("bold"));
    let listed = h.run("styles", json!({"catalogue": "text", "users": true})).unwrap_or(Value::Null);
    assert!(listed["styles"].as_array().is_some_and(|a| a.iter().any(|s| s["name"] == json!("Big"))), "{listed}");
    let up = h.run("promote", json!({"catalogue": "text", "name": "Big"})).unwrap_or(Value::Null);
    assert_eq!(up["changed"], json!(true));
    let gone = h.run("standard", json!({"remove": true})).unwrap_or(Value::Null);
    assert_eq!(gone["standard"]["name"], json!("IEC (built in)"));
    let own = h.run("setstyle", json!({"catalogue": "text", "new": true, "name": "Small"})).unwrap_or(Value::Null);
    let d = h.run("deletestyle", json!({"catalogue": "text", "name": "Small"})).unwrap_or(Value::Null);
    assert_eq!((&d["deleted"], &d["moved"]), (&own["id"], &json!(0)));
}

/// Input refusals of the function and location list commands: one wording.
#[test]
fn the_designation_commands_refuse_input_uniformly() {
    let rows: &[(&str, Value, &str)] = &[
        ("designations", json!({"aspect": 1}), "`aspect` must be text, or null"),
        ("designationadd", json!({"code": "=F1"}), "`aspect` (text) is required"),
        ("designationadd", json!({"aspect": "function"}), "`code` (text) is required"),
        ("designationedit", json!({"aspect": "function", "code": "=F1", "description": 5}), "`description` must be text, or null"),
        ("designationmerge", json!({"aspect": "function", "from": "=F1"}), "`into` (text) is required"),
        ("designationremove", json!({"unused": true, "x": 1}), "`x` is not a parameter of this command; it takes `aspect`, `code`, `key`, `unused`"),
    ];
    let mut h = H::new(7017);
    for (cmd, params, says) in rows {
        assert_eq!(h.run(cmd, params.clone()).err().as_deref(), Some(*says), "{cmd} {params}");
    }
}

/// Rule refusals of the function and location list commands keep their words.
#[test]
fn the_designation_commands_keep_their_rule_words() {
    let mut h = H::new(7018);
    assert!(h.run("designationadd", json!({"aspect": "function", "code": "=F1"})).is_ok());
    assert!(h.run("designationadd", json!({"aspect": "function", "code": "=F2"})).is_ok());
    let rows: &[(&str, Value, &str)] = &[
        ("designations", json!({"aspect": "x"}), "`aspect` is \"function\" or \"location\""),
        ("designationadd", json!({"aspect": "function", "code": "=F1"}), "=F1 is already in the list."),
        ("designationedit", json!({"aspect": "function"}), "`code` (a key or a code such as =F1) is required"),
        ("designationedit", json!({"aspect": "function", "code": "=F9", "to": "=F8"}), "=F9 is not in the project's list of functions."),
        ("designationedit", json!({"aspect": "function", "code": "=F1", "to": "=F2"}), "=F2 is already in the list. Merge =F1 into it instead?"),
        ("designationmerge", json!({"aspect": "function", "from": "=F1", "into": "=F1"}), "=F1 and =F1 are the same entry."),
        ("designationremove", json!({"code": "=F1"}), "`aspect` is required: \"function\" or \"location\""),
        ("designationremove", json!({"aspect": "function"}), "`code` (a key or a code such as =F1) is required"),
    ];
    for (cmd, params, says) in rows {
        assert_eq!(h.run(cmd, params.clone()).err().as_deref(), Some(*says), "{cmd} {params}");
    }
}

/// The typed function and location list commands still work, and answer as before.
#[test]
fn the_typed_designation_commands_still_work() {
    let mut h = H::new(7019);
    let a = h.run("designationadd", json!({"aspect": "function", "code": "=F1", "description": "Pumps"})).unwrap_or(Value::Null);
    assert_eq!((&a["changed"], &a["code"]), (&json!(true), &json!("=F1")));
    let key = a["key"].as_str().unwrap_or_default().to_string();
    let e = h.run("designationedit", json!({"aspect": "function", "key": key, "code": "=F3"})).unwrap_or(Value::Null);
    assert_eq!(e, json!({"changed": true, "key": key, "code": "=F3"}));
    let same = h.run("designationedit", json!({"aspect": "function", "code": "=F3", "description": "Pumps"})).unwrap_or(Value::Null);
    assert_eq!(same["changed"], json!(false));
    assert!(h.run("designationadd", json!({"aspect": "f", "code": "=F4"})).is_ok());
    let m = h.run("designationmerge", json!({"aspect": "function", "from": "=F4", "into": "=F3"})).unwrap_or(Value::Null);
    assert_eq!((&m["from"], &m["into"], &m["key"]), (&json!("=F4"), &json!("=F3"), &json!(key)));
    let list = h.run("designations", json!({"aspect": "function"})).unwrap_or(Value::Null);
    assert_eq!(list["functions"]["entries"].as_array().map(Vec::len), Some(1), "{list}");
    let r = h.run("designationremove", json!({"aspect": "function", "code": "=F3"})).unwrap_or(Value::Null);
    assert_eq!(r, json!({"changed": true, "code": "=F3"}));
    let none = h.run("designationremove", json!({"unused": true})).unwrap_or(Value::Null);
    assert_eq!(none, json!({"changed": false, "removed": []}));
}

/// A float switch as `symbolnew` takes it (design 048 §9.1's example), with `extra` on top.
fn float_switch(extra: Value) -> Value {
    let mut p = json!({
        "name": "Float switch", "key": "float1",
        "graphics": [{"kind": "polyline", "vertices": [[-2.5, -7.5], [2.5, -7.5], [2.5, 7.5], [-2.5, 7.5]], "closed": true}],
        "points": [{"key": 1, "label": "BN", "at": [0, 5]}, {"key": "2", "label": null, "at": [0, -5]}],
        "slots": [{"text": "tag", "at": [-4, 0], "h": "right", "v": "middle", "rotation": 0}],
        "defaults": {"prefix": "B", "description": "Float switch", "join": "device"},
    });
    if let (Some(o), Some(e)) = (p.as_object_mut(), extra.as_object()) {
        o.extend(e.clone());
    }
    p
}

/// Input refusals of the symbol builder's commands: one wording.
#[test]
fn the_symbol_commands_refuse_input_uniformly() {
    let rows: &[(&str, Value, &str)] = &[
        ("symbolnew", json!({"name": 5}), "`name` must be text, or null"),
        ("symbolnew", json!({"points": [{"key": "1", "at": "x"}]}), "`points` must be a list of objects, or null"),
        ("symbolnew", json!({"slots": [{"text": "tag", "at": [0, 0], "h": "up"}]}), "`slots` must be a list of objects, or null"),
        ("symbolnew", json!({"defaults": {"prefix": 5}}), "`defaults` must be an object, or null"),
        ("symbolnew", json!({"from": "zz"}), "`from` must be a hex id, or null"),
        ("symbolimpact", json!({}), "`definition` (hex id) is required"),
        ("symboledit", json!({"definition": "1", "copies": "x"}), "`copies` must be a list of hex ids, or null"),
        ("symboledit", json!({"definition": "1", "update": 5}), "`update` must be text, or null"),
        ("symbolupdate", json!({"lineage": "zz"}), "`lineage` must be a hex id, or null"),
        ("makesymbol", json!({"at": [1]}), "`at` must be a point [x, y], or null"),
        ("symbolupdate", json!({"colour": 1}), "`colour` is not a parameter of this command; it takes `copies`, `lineage`, `to`"),
    ];
    let mut h = H::new(7020);
    for (cmd, params, says) in rows {
        assert_eq!(h.run(cmd, params.clone()).err().as_deref(), Some(*says), "{cmd} {params}");
    }
}

/// Rule refusals of the symbol builder's commands keep their words.
#[test]
fn the_symbol_commands_keep_their_rule_words() {
    let rows: &[(&str, Value, &str)] = &[
        ("symbolnew", json!({}), "A symbol needs a name."),
        ("symbolnew", json!({"points": [{"at": [0, 0]}]}), "Each point needs a `key`."),
        ("symbolnew", json!({"points": [{"key": "1"}]}), "Point 1 needs `at` [x, y]."),
        ("symbolnew", json!({"slots": [{"at": [0, 0]}]}), "Each slot needs a `text`."),
        ("symbolnew", json!({"slots": [{"text": "tag"}]}), "The tag slot needs `at` [x, y]."),
        ("symbolnew", json!({"graphics": [{"kind": "nope"}]}), "A shape could not be read: "),
        ("symbolnew", float_switch(json!({"key": "coil"})), "The key coil is a built-in symbol's; choose another."),
        ("symboledit", json!({"definition": "1", "update": "x"}), "`update` is all, selected, none or new."),
        ("symboledit", json!({"definition": "1"}), "No such symbol."),
        ("symbolupdate", json!({}), "Name the `copies` or the `lineage` to update."),
        ("symbolupdate", json!({"copies": []}), "Name a placed copy or a lineage."),
        ("makesymbol", json!({"handles": []}), "Select the lines, arcs and texts to make a symbol of."),
        ("makesymbol", json!({"handles": ["zz"]}), "`\"zz\"` is not a valid handle"),
        ("makesymbol", json!({"component": "1"}), "No such device."),
    ];
    let mut h = H::new(7021);
    for (cmd, params, says) in rows {
        let e = h.run(cmd, params.clone()).err().unwrap_or_default();
        assert!(e.starts_with(says), "{cmd} {params}: {e}");
    }
}

/// The typed symbol builder's commands still work, and answer as before.
#[test]
fn the_typed_symbol_commands_still_work() {
    let mut h = H::new(7022);
    let v1 = h.run("symbolnew", float_switch(json!({}))).unwrap_or(Value::Null);
    assert_eq!((&v1["version"], &v1["key"], &v1["name"]), (&json!(1), &json!("float1"), &json!("Float switch")), "{v1}");
    let def = v1["definition"].as_str().unwrap_or_default().to_string();
    let lineage = v1["lineage"].as_str().unwrap_or_default().to_string();
    let impact = h.run("symbolimpact", float_switch(json!({"definition": def, "name": "Float"}))).unwrap_or(Value::Null);
    assert_eq!((&impact["version"], &impact["copies"], &impact["problems"]), (&json!(2), &json!(0), &json!([])), "{impact}");
    let same = h.run("symboledit", float_switch(json!({"definition": def, "update": "None"}))).err().unwrap_or_default();
    assert_eq!(same, "Nothing changed.");
    let v2 = h.run("symboledit", float_switch(json!({"definition": def, "name": "Float", "update": "None", "builtins": "x"}))).unwrap_or(Value::Null);
    assert_eq!((&v2["version"], &v2["lineage"], &v2["updated"], &v2["left"]), (&json!(2), &json!(lineage), &json!(0), &json!(0)), "{v2}");
    let none = h.run("symbolupdate", json!({"lineage": lineage})).err().unwrap_or_default();
    assert_eq!(none, "Nothing to update: every copy shows this version.");
}

/// Input refusals of the cross-reference commands: one wording.
#[test]
fn the_xref_commands_refuse_input_uniformly() {
    let rows: &[(&str, Value, &str)] = &[
        ("xrefs", json!({"limit": "x"}), "`limit` must be a whole number, or null"),
        ("xrefs", json!({"after": -1}), "`after` must be a whole number, or null"),
        ("xrefs", json!({"cable": "zz"}), "`cable` must be a hex id, or null"),
        ("contacts", json!({}), "`component` (hex id) is required"),
        ("refgoto", json!({"target": "1"}), "`source` (hex id) is required"),
        ("mirrormove", json!({"component": "1", "at": [1]}), "`at` must be a point [x, y], or null"),
        ("mirrormove", json!({"component": "1", "placement": 5}), "`placement` must be text, or null"),
        (
            "mirrormove",
            json!({"component": "1", "side": "left"}),
            "`side` is not a parameter of this command; it takes `at`, `component`, `placement`",
        ),
    ];
    let mut h = H::new(7023);
    for (cmd, params, says) in rows {
        assert_eq!(h.run(cmd, params.clone()).err().as_deref(), Some(*says), "{cmd} {params}");
    }
}

/// Rule refusals of the cross-reference commands keep their words.
#[test]
fn the_xref_commands_keep_their_rule_words() {
    let mut h = H::new(7024);
    let k = h.run("place", json!({"symbol": "coil", "at": [50, 50]})).unwrap_or(Value::Null);
    let c = k["component"].as_str().unwrap_or_default().to_string();
    let rows: &[(&str, Value, &str)] = &[
        ("refgoto", json!({"source": "1"}), "no such reference"),
        ("mirrormove", json!({"component": "1", "placement": "below"}), "no such component"),
        ("mirrormove", json!({"component": c, "at": [1, 1], "placement": "below"}), "give `at` or `placement`, not both"),
        ("mirrormove", json!({"component": c}), "`at` ([x, y]) or `placement` (auto, below, right, off) is required"),
        ("mirrormove", json!({"component": c, "at": [99999, 0]}), "Put the table within 2000 mm of its coil."),
    ];
    for (cmd, params, says) in rows {
        assert_eq!(h.run(cmd, params.clone()).err().as_deref(), Some(*says), "{cmd} {params}");
    }
}

/// The typed cross-reference commands still work, and answer as before.
#[test]
fn the_typed_xref_commands_still_work() {
    let mut h = H::new(7025);
    let k = h.run("place", json!({"symbol": "coil", "at": [50, 50]})).unwrap_or(Value::Null);
    let c = k["component"].as_str().unwrap_or_default().to_string();
    let list = h.run("xrefs", json!({"component": c, "limit": 5, "after": 0})).unwrap_or(Value::Null);
    assert!(list.is_object(), "{list}");
    let t = h.run("contacts", json!({"component": c})).unwrap_or(Value::Null);
    assert!(t.is_object(), "{t}");
    let off = h.run("mirrormove", json!({"component": c, "placement": "off"})).unwrap_or(Value::Null);
    assert_eq!((&off["changed"], &off["mirror"]), (&json!(true), &json!("off")), "{off}");
    let again = h.run("mirrormove", json!({"component": c, "placement": "off"})).unwrap_or(Value::Null);
    assert_eq!(again, json!({"changed": false, "mirror": "off"}));
}

/// Input refusals of the terminal commands: one wording.
#[test]
fn the_terminal_commands_refuse_input_uniformly() {
    let rows: &[(&str, Value, &str)] = &[
        ("terminals", json!({"to": [0, 0]}), "`from` (point [x, y]) is required"),
        ("terminals", json!({"from": [0, 0], "to": "x"}), "`to` must be a point [x, y]"),
        ("terminals", json!({"from": [0, 0], "to": [9, 9], "pe": "yes"}), "`pe` must be true or false, or null"),
        ("terminals", json!({"from": [0, 0], "to": [9, 9], "type": 2}), "`type` must be text, or null"),
        ("terminals", json!({"from": [0, 0], "to": [9, 9], "strip": "zz"}), "`strip` must be a hex id, or null"),
        ("bridge", json!({"from": "1"}), "`to` (hex id) is required"),
        ("stripplan", json!({"limit": "x"}), "`limit` must be a whole number, or null"),
        ("stripplan", json!({"accessories": 1}), "`accessories` must be true or false, or null"),
        ("cablecores", json!({}), "`cable` (hex id) is required"),
        ("cablecores", json!({"cable": "1", "x": 1}), "`x` is not a parameter of this command; it takes `cable`"),
    ];
    let mut h = H::new(7026);
    for (cmd, params, says) in rows {
        assert_eq!(h.run(cmd, params.clone()).err().as_deref(), Some(*says), "{cmd} {params}");
    }
}

/// Rule refusals of the terminal commands keep their words.
#[test]
fn the_terminal_commands_keep_their_rule_words() {
    let rows: &[(&str, Value, &str)] = &[
        (
            "terminals",
            json!({"from": [0, 0], "to": [9, 9], "type": "nope"}),
            "No terminal type \"nope\". Types: Feedthrough, PE, x2, x2B, x3, LNPE, SEN, DIS, FUSE.",
        ),
        ("terminals", json!({"from": [0, 0], "to": [0, 0]}), "`from` and `to` must differ"),
        ("terminals", json!({"from": [0, 0], "to": [9, 9]}), "The line crosses no wire. Draw it across the wires to land."),
        (
            "terminals",
            json!({"from": [0, 0], "to": [9, 9], "type": "x2", "level": "Q"}),
            "A two-level terminal has levels 1, 2. Type one of them, or All for the whole terminal.",
        ),
        ("bridge", json!({"from": "1", "to": "2"}), "No such terminal."),
        ("stripplan", json!({"strip": "1"}), "No such terminal strip."),
        ("cablecores", json!({"cable": "1"}), "no such cable"),
    ];
    let mut h = H::new(7027);
    for (cmd, params, says) in rows {
        let e = h.run(cmd, params.clone()).err().unwrap_or_default();
        assert!(e.starts_with(says), "{cmd} {params}: {e}");
    }
}

/// The typed terminal commands still work, and answer as before.
#[test]
fn the_typed_terminal_commands_still_work() {
    let mut h = H::new(7028);
    assert!(h.run("wire", json!({"points": [[0, 0], [0, 100]]})).is_ok());
    assert!(h.run("wire", json!({"points": [[20, 0], [20, 100]]})).is_ok());
    let f = h.run("terminals", json!({"from": [-10, 50], "to": [30, 50]})).unwrap_or(Value::Null);
    let placed = f["terminals"].as_array().cloned().unwrap_or_default();
    assert_eq!(placed.len(), 2, "{f}");
    assert!(f["message"].as_str().is_some_and(|m| m.starts_with("Placed ")), "{f}");
    let (a, b) = (placed[0]["terminal"].clone(), placed[1]["terminal"].clone());
    let br = h.run("bridge", json!({"from": a, "to": b})).unwrap_or(Value::Null);
    assert_eq!((&br["bridge"], &br["changed"]), (&json!(true), &json!(true)), "{br}");
    let again = h.run("bridge", json!({"from": a, "to": b})).unwrap_or(Value::Null);
    assert_eq!(again["changed"], json!(false), "{again}");
    let plan = h.run("stripplan", json!({"limit": 1})).unwrap_or(Value::Null);
    assert_eq!((&plan["next"], &plan["strips"].as_array().map(Vec::len)), (&json!(1), &Some(1)), "{plan}");
    let rest = h.run("stripplan", json!({"after": 1, "accessories": false})).unwrap_or(Value::Null);
    assert_eq!(rest["strips"][0]["first_row"], json!(1), "{rest}");
}

/// Input refusals of the PLC commands: one wording.
#[test]
fn the_plc_commands_refuse_input_uniformly() {
    let rows: &[(&str, Value, &str)] = &[
        ("plcslot", json!({}), "`card` (text) is required"),
        ("plcslot", json!({"card": "-A1", "swap": "yes"}), "`swap` must be true or false, or null"),
        ("plcslot", json!({"card": "-A1", "rack": 5}), "`rack` must be text, or null"),
        ("plcracks", json!({"rack": 1}), "`rack` must be text, or null"),
        ("plcchannels", json!({"from": "here"}), "`from` must be a point [x, y], or null"),
        ("plcchannels", json!({"rest": 1}), "`rest` must be true or false, or null"),
        ("plcchannel", json!({"channel": 0}), "`card` (text) is required"),
        ("plcchannel", json!({"card": "-A1", "channel": 0, "text": 5}), "`text` must be text, or null"),
        ("plcchannel", json!({"card": "-A1", "channel": 0, "pins": {"in": 4}}), "`pins` must be an object, or null"),
        ("iolist", json!({"state": "busy"}), "`state` must be one of \"used\", \"reserved\", \"spare\", or null"),
        ("iolist", json!({"limit": "x"}), "`limit` must be a whole number, or null"),
        ("plcaddress", json!({"card": "-A1", "start": true}), "`start` must be text, or null"),
        ("plcpage", json!({}), "`card` (text) is required"),
        ("plcpage", json!({"card": "-A1", "after": []}), "`after` must be text, or null"),
        ("plcmaketags", json!({"card": 1}), "`card` must be text, or null"),
        ("plcexport", json!({"path": 1}), "`path` must be text, or null"),
        ("plcimport", json!({"what": "all"}), "`what` must be one of \"both\", \"tags\", \"descriptions\", or null"),
        ("plcimport", json!({"dry_run": "yes"}), "`dry_run` must be true or false, or null"),
        ("plccardtype", json!({"card": "-A1"}), "`type` (text) is required"),
        ("plccardtype", json!({"card": "-A1", "type": "DI", "x": 1}), "`x` is not a parameter of this command; it takes `card`, `type`"),
    ];
    let mut h = H::new(7030);
    for (cmd, params, says) in rows {
        assert_eq!(h.run(cmd, params.clone()).err().as_deref(), Some(*says), "{cmd} {params}");
    }
}

/// Rule refusals of the PLC commands keep their words.
#[test]
fn the_plc_commands_keep_their_rule_words() {
    let mut h = H::new(7031);
    assert!(h.run("place", json!({"symbol": "plcdi", "at": [100, 200]})).is_ok());
    let rows: &[(&str, Value, &str)] = &[
        ("plcslot", json!({"card": "-A99"}), "No device -A99."),
        ("plcslot", json!({"card": "-A1", "slot": "x"}), "A slot is a whole number from 0 to 999."),
        ("plcslot", json!({"card": "-A1", "rack": null, "slot": 2}), "`slot` is for a card in a rack; `rack: null` takes the card out."),
        ("plcslot", json!({"card": "-A1"}), "The project has no rack: place a CPU (plccpu) or a head module (plcim) first."),
        ("plcracks", json!({"rack": "-A1"}), "-A01 is not a rack; a rack is a CPU or a head module."),
        ("plcchannels", json!({"card": "-A1", "rest": true, "at": [1e13, 0]}), "`at` must be a point [x, y] on the sheet"),
        ("plcchannels", json!({"card": "-A1"}), "`from` [x,y] is required"),
        ("plcchannels", json!({"first": "x", "from": [0, 0], "to": [9, 9]}), "A channel is a whole number from 0 to 9999."),
        ("plcchannels", json!({"type": "ZZ", "from": [0, 0], "to": [9, 9]}), "No card type \"ZZ\". Types: DI, DQ, AI, AO, RTD, IOL."),
        ("plcchannel", json!({"card": "-A1"}), "`channel` (a number as shown) is required"),
        ("plcchannel", json!({"card": "-A1", "channel": 99}), "-A01 has 16 channels (0–15)."),
        ("plcchannel", json!({"card": "-A1", "channel": 0, "move_to": null}), "`move_to` is a channel number"),
        ("plcaddress", json!({}), "`card` (a card's ID or tag) is required"),
        ("plcaddress", json!({"card": "-A1", "size": 0}), "`size` is a whole number of bytes from 1 to 4096."),
        ("plcpage", json!({"card": "-A1", "after": "999"}), "No page 999."),
        ("plcmaketags", json!({}), "Give `card` or `rack` (an ID or tag)."),
        ("plcexport", json!({"format": "xml"}), "No tag table format \"xml\"; the formats are: csv."),
        ("plcimport", json!({}), "Give `path` (a file) or `text` (the tag table)."),
        ("plccardtype", json!({"card": "-A1", "type": "XX"}), "`type` is DI, DQ, AI, AO, RTD or IOL"),
        ("plccardtype", json!({"card": "-A1", "type": "DQ", "x": 1}), "`x` is not a parameter of this command; it takes `card`, `type`"),
    ];
    for (cmd, params, says) in rows {
        let e = h.run(cmd, params.clone()).err().unwrap_or_default();
        assert!(e.starts_with(says), "{cmd} {params}: {e}");
    }
}

/// The typed PLC commands still work, and answer as before.
#[test]
fn the_typed_plc_commands_still_work() {
    let mut h = H::new(7032);
    assert!(h.run("place", json!({"symbol": "plccpu", "at": [50, 200]})).is_ok());
    let card = h.run("place", json!({"symbol": "plcdi", "at": [100, 200]})).unwrap_or_else(|e| json!(e))["component"].clone();
    let slot = h.run("plcslot", json!({"card": card, "slot": "3"})).unwrap_or_else(|e| json!(e));
    assert_eq!((&slot["changed"], &slot["slot"]), (&json!(true), &json!(3)), "{slot}");
    let racks = h.run("plcracks", json!({})).unwrap_or_else(|e| json!(e));
    assert_eq!(racks["racks"].as_array().map(Vec::len), Some(1), "{racks}");
    let ch = h.run("plcchannel", json!({"card": card, "channel": 0, "text": "Start", "tag": "START"})).unwrap_or_else(|e| json!(e));
    assert_eq!(ch["changed"], json!(true), "{ch}");
    let pins = h.run("plcchannel", json!({"card": card, "index": 0, "pins": {"in": "4"}})).unwrap_or_else(|e| json!(e));
    assert_eq!(pins, json!("Channel 0 of -A02 has no in point."), "an undrawn channel has no points to relabel");
    let io = h.run("iolist", json!({"state": "used", "limit": 5})).unwrap_or_else(|e| json!(e));
    assert_eq!(io["cards"].as_array().map(Vec::len), Some(1), "{io}");
    let addr = h.run("plcaddress", json!({"card": card, "start": 8})).unwrap_or_else(|e| json!(e));
    assert_eq!(addr["changed"], json!(true), "{addr}");
    let unpin = h.run("plcaddress", json!({"card": card, "start": null})).unwrap_or_else(|e| json!(e));
    assert_eq!(unpin["changed"], json!(true), "{unpin}");
    let ty = h.run("plccardtype", json!({"card": card, "type": "di"})).unwrap_or_else(|e| json!(e));
    assert_eq!(ty["changed"], json!(false), "{ty}");
    let tags = h.run("plcmaketags", json!({"card": card})).unwrap_or_else(|e| json!(e));
    assert!(tags["made"].is_number(), "{tags}");
    let ex = h.run("plcexport", json!({})).unwrap_or_else(|e| json!(e));
    assert!(ex["text"].as_str().is_some_and(|t| t.contains("START")), "{ex}");
    let text = ex["text"].as_str().unwrap_or_default().to_string();
    let im = h.run("plcimport", json!({"text": text, "dry_run": true, "what": "tags"})).unwrap_or_else(|e| json!(e));
    assert_eq!(im["written"], json!(false), "{im}");
    let page = h.run("plcpage", json!({"card": card, "after": 1})).unwrap_or_else(|e| json!(e));
    assert!(page["message"].as_str().is_some_and(|m| m.starts_with("Made page ")), "{page}");
    let rest = h.run("plcchannels", json!({"card": card, "rest": true, "at": [100, 100]}));
    assert!(rest.is_err(), "every channel is drawn on the card's page: {rest:?}");
}

/// Input refusals of the numbering commands: one wording.
#[test]
fn the_numbering_commands_refuse_input_uniformly() {
    let rows: &[(&str, Value, &str)] = &[
        ("renumber", json!({"kind": 1}), "`kind` must be text, or null"),
        ("renumber", json!({"start": "x"}), "`start` must be a whole number, or null"),
        ("renumber", json!({"preview": "yes"}), "`preview` must be true or false, or null"),
        ("renumber", json!({"objects": "x"}), "`objects` must be a list of texts, or null"),
        ("renumber", json!({"scope": 5}), "`scope` must be text, or null"),
        ("finalise", json!({"kind": "all"}), "`kind` is not a parameter of this command; it takes `based_on`, `preview`"),
        ("numbering", json!({"provisional": "on"}), "`provisional` must be true or false, or null"),
    ];
    let mut h = H::new(7033);
    for (cmd, params, says) in rows {
        assert_eq!(h.run(cmd, params.clone()).err().as_deref(), Some(*says), "{cmd} {params}");
    }
}

/// Rule refusals of the numbering commands keep their words.
#[test]
fn the_numbering_commands_keep_their_rule_words() {
    let rows: &[(&str, Value, &str)] = &[
        ("renumber", json!({"kind": "lamps"}), "`kind` must be \"devices\", \"cables\", \"wires\", \"terminals\" or \"all\", not \"lamps\""),
        (
            "renumber",
            json!({"scope": "some"}),
            "`scope` must be \"all\", \"selection\", \"provisional\", {prefix}, {folder} or {pages}, not \"some\"",
        ),
        ("renumber", json!({"scope": "selection"}), "`objects` (array of hex ids) is required with scope \"selection\""),
        ("renumber", json!({"scope": "selection", "objects": ["zz"]}), "`\"zz\"` in `objects` is not a valid hex id"),
        ("renumber", json!({"scope": "selection", "objects": ["ab"]}), "ab in `objects` is not a device, cable or wire."),
        ("renumber", json!({"scope": {"prefix": " "}}), "`prefix` must be letters, such as \"-K\", or a potential"),
        ("renumber", json!({"scope": {"folder": "1"}}), "No such folder."),
        ("renumber", json!({"scope": {"pages": ["1"]}}), "No page 00000000000000000000000000000001."),
        ("renumber", json!({"scope": {}}), "`scope` as an object is {prefix}, {folder: id} or {pages: [id]}"),
        ("renumber", json!({"digits": 300}), "Digits is a whole number from 1 to 6."),
        ("renumber", json!({"start": 0}), "Start is a whole number from 1."),
        (
            "renumber",
            json!({"kind": "terminals", "scope": "selection", "strips": ["ab"]}),
            "ab in `strips` is not a terminal strip or one of its terminals.",
        ),
        ("finalise", json!({"based_on": 999}), "The project changed since the preview; check the new list and Apply again."),
    ];
    let mut h = H::new(7034);
    for (cmd, params, says) in rows {
        let e = h.run(cmd, params.clone()).err().unwrap_or_default();
        assert!(e.starts_with(says), "{cmd} {params}: {e}");
    }
}

/// The typed numbering commands still work, and answer as before.
#[test]
fn the_typed_numbering_commands_still_work() {
    let mut h = H::new(7035);
    for x in [100, 50] {
        assert!(h.run("place", json!({"symbol": "coil", "at": [x, 100]})).is_ok());
    }
    let preview = h.run("renumber", json!({"preview": true, "scope": "all", "start": 1})).unwrap_or_else(|e| json!(e));
    assert!(preview["summary"].as_str().is_some_and(|s| s.contains("to change")), "{preview}");
    let count = preview["change_count"].clone();
    let done = h.run("renumber", json!({"kind": "devices", "based_on": count})).unwrap_or_else(|e| json!(e));
    assert!(done["message"].as_str().is_some(), "{done}");
    let read = h.run("numbering", json!({})).unwrap_or_else(|e| json!(e));
    assert_eq!(read["provisional"], json!(false), "{read}");
    let on = h.run("numbering", json!({"provisional": true})).unwrap_or_else(|e| json!(e));
    assert_eq!(on["provisional"], json!(true), "{on}");
    let fin = h.run("finalise", json!({"preview": true})).unwrap_or_else(|e| json!(e));
    assert!(fin["rows"].is_array(), "{fin}");
}
