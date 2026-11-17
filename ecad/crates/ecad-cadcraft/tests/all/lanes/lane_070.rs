//! Brief 070: typed command inputs. Every registered command is typed (or on the shrink-only
//! list of commands not converted yet), with its typed run function and a JSON Schema; input
//! refusals use one uniform wording; rule refusals keep their words.

use ecad_cadcraft::cmd::{schema_of, specs, typed_commands};
use serde_json::{Value, json};

use crate::common::H;

/// Registered commands still reading their JSON by hand. Brief 070 converts them module by
/// module: remove ids as they are typed, never add one.
const UNTYPED: &[&str] = &[
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
    "cableadd",
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
    "agentaccess",
    "canvassheet",
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
    // Strict input: a switch that is not true or false is refused (it was ignored before).
    let lenient = h.run("symboledit", float_switch(json!({"definition": def, "name": "Float", "update": "None", "builtins": "x"}))).err();
    assert_eq!(lenient.as_deref(), Some("`builtins` must be true or false, or null"));
    let v2 = h.run("symboledit", float_switch(json!({"definition": def, "name": "Float", "update": "None"}))).unwrap_or(Value::Null);
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

/// Input refusals of the cable manager's commands: one wording.
#[test]
fn the_cable_manager_commands_refuse_input_uniformly() {
    let rows: &[(&str, Value, &str)] = &[
        (
            "cablenew",
            json!({"colour": 1}),
            "`colour` is not a parameter of this command; it takes `cable_type`, `cores`, `description`, `from`, `length`, `maker`, `part_number`, `shield`, `tag`, `to`, `type`",
        ),
        ("cablenew", json!({"length": true}), "`length` must be text, or null"),
        ("cablenew", json!({"from": "+CAB1"}), "`from` must be an object, or null"),
        ("cableedit", json!({"set": {}}), "`cable` (text) is required"),
        ("cableedit", json!({"cable": "-W01"}), "`set` (object) is required"),
        ("cableremove", json!({}), "`cable` (text) is required"),
    ];
    let mut h = H::new(7036);
    for (cmd, params, says) in rows {
        assert_eq!(h.run(cmd, params.clone()).err().as_deref(), Some(*says), "{cmd} {params}");
    }
}

/// Rule refusals of the cable manager's commands keep their words.
#[test]
fn the_cable_manager_commands_keep_their_rule_words() {
    let rows: &[(&str, Value, &str)] = &[
        ("cablenew", json!({"length": "x"}), "Length must be a number of metres from"),
        ("cablenew", json!({"shield": "lead"}), "Shield is none, overall, pairs or overall+pairs."),
        ("cablenew", json!({"maker": "x".repeat(65)}), "Maker is at most 64 characters."),
        ("cablenew", json!({"cable_type": "H07X"}), "No cable type \"H07X\" in this project's standards."),
        ("cablenew", json!({"tag": "-W01"}), "-W01 is already the tag of -W01 (cable). Choose another tag."),
        ("cableedit", json!({"cable": "-W09", "set": {}}), "No cable \"-W09\"."),
        (
            "cableedit",
            json!({"cable": "-W01", "set": {"tag": null}}),
            "A cable always has a tag. Type a new tag (such as -W05) instead of clearing it.",
        ),
        ("cableremove", json!({"cable": "-W09"}), "No cable \"-W09\"."),
    ];
    let mut h = H::new(7037);
    assert!(h.run("cablenew", json!({})).is_ok());
    for (cmd, params, says) in rows {
        let e = h.run(cmd, params.clone()).err().unwrap_or_default();
        assert!(e.starts_with(says), "{cmd} {params}: {e}");
    }
}

/// The typed cable manager's commands still work, and answer as before.
#[test]
fn the_typed_cable_manager_commands_still_work() {
    let mut h = H::new(7038);
    let made = h.run("cablenew", json!({"cores": "4G", "length": 12, "from": {"gland": "M20"}})).unwrap_or_else(|e| json!(e));
    assert_eq!(made["tag"], json!("-W01"), "{made}");
    let k = made["cable"].clone();
    let edit = h.run("cableedit", json!({"cable": "-W01", "set": {"maker": "Lapp", "length": null}})).unwrap_or_else(|e| json!(e));
    assert_eq!((&edit["cable"], &edit["written"], &edit["changed"]), (&k, &json!(true), &json!(["maker", "length"])), "{edit}");
    let doc = h.doc();
    let cable = doc.cables.values().next().map(|c| c.properties.clone()).unwrap_or_default();
    assert_eq!(cable.get("maker").map(String::as_str), Some("Lapp"), "{cable:?}");
    let gone = h.run("cableremove", json!({"cable": k})).unwrap_or_else(|e| json!(e));
    assert_eq!(gone["removed"], json!(true), "{gone}");
}

/// Input refusals of the rating commands: one wording.
#[test]
fn the_rating_commands_refuse_input_uniformly() {
    let rows: &[(&str, Value, &str)] = &[
        ("rating", json!({"wire": 5}), "`wire` must be text, or null"),
        ("rating", json!({"cable": "W1", "ambient": true}), "`ambient` must be text, or null"),
        ("rating", json!({"many": 1}), "`many` must be a list of objects, or null"),
        ("rating", json!({"colour": 1}), "`colour` is not a parameter of this command; it takes "),
        ("ratings", json!({"from": "x"}), "`from` must be a whole number, or null"),
        ("ratingtable", json!({"csv": 5}), "`csv` must be text, or null"),
    ];
    let mut h = H::new(7039);
    for (cmd, params, says) in rows {
        let e = h.run(cmd, params.clone()).err().unwrap_or_default();
        assert!(e.starts_with(says), "{cmd} {params}: {e}");
    }
}

/// Rule refusals of the rating commands keep their words.
#[test]
fn the_rating_commands_keep_their_rule_words() {
    let rows: &[(&str, Value, &str)] = &[
        ("rating", json!({}), "Give `wire` (a wire number or conductor ID) or `cable` (a tag or ID)."),
        ("rating", json!({"wire": "1", "cable": "2"}), "Give `wire` or `cable`, not both."),
        ("rating", json!({"wire": "W99"}), "No panel wire \"W99\" (give a wire number or a conductor ID;"),
        ("rating", json!({"cable": "-W9"}), "No cable \"-W9\"."),
        ("rating", json!({"many": [{"wire": "W99"}]}), "No panel wire \"W99\""),
        ("rating", json!({"temperature": 30}), "temperature is not a rating key; the keys are basis, manual,"),
        ("rating", json!({"location": " "}), "`location` names a location, such as +CAB1."),
        ("rating", json!({"location": "+CAB1", "basis": "table"}), "basis is not a location rating key; give `temperature`."),
        ("rating", json!({"location": "+CAB1"}), "Give `temperature` (\u{b0}C), or null to clear it."),
        ("rating", json!({"location": "+CAB1", "temperature": 200}), "temperature is \u{2212}40 to 90 \u{b0}C."),
        ("rating", json!({"location": "+CAB1", "temperature": 30}), "No location box is +CAB1; draw one with Location Box first."),
        ("ratings", json!({"wire": "1", "cable": "2"}), "Give `wire` or `cable`, not both."),
        ("ratingtable", json!({}), "Give `csv` (the rows), `import` (a CSV file) or `export` (a CSV file)."),
        ("ratingtable", json!({"csv": "", "import": "x.csv"}), "Give `csv` or `import`, not both."),
        ("ratingtable", json!({"export": "never-written.csv"}), "No rating table chosen; give `name`."),
        ("ratingtable", json!({"name": "", "csv": "kind,a,b,c,d,value\n"}), "A rating table needs a name"),
        ("ratingtable", json!({"name": "T", "csv": "kind,a,b,c,d,value\n", "layer": "x"}), "layer is project or standard."),
        ("ratingtable", json!({"name": "T", "csv": "kind,a,b,c,d,value\n", "layer": "standard"}), "This project uses IEC (built in)."),
    ];
    let mut h = H::new(7040);
    for (cmd, params, says) in rows {
        let e = h.run(cmd, params.clone()).err().unwrap_or_default();
        assert!(e.starts_with(says), "{cmd} {params}: {e}");
    }
}

/// The typed rating commands still work, and answer as before.
#[test]
fn the_typed_rating_commands_still_work() {
    let mut h = H::new(7041);
    let csv = "kind,a,b,c,d,value\ncapacity,E,pvc,3,1.5,20\n";
    let t = h.run("ratingtable", json!({"name": " Test ", "source": "made up", "csv": csv})).unwrap_or_else(|e| json!(e));
    assert_eq!((t["name"].clone(), t["rows"].clone(), t["changed"].clone()), (json!("Test"), json!(1), json!(true)), "{t}");
    let again = h.run("ratingtable", json!({"name": "test", "csv": csv})).unwrap_or_else(|e| json!(e));
    assert_eq!((again["id"].clone(), again["changed"].clone()), (t["id"].clone(), json!(false)), "{again}");
    let all = h.run("ratings", json!({"from": 0})).unwrap_or_else(|e| json!(e));
    assert!(all["ratings"].is_array() && all["total"].is_number(), "{all}");
    assert!(h.run("locationbox", json!({"location": "+CAB1", "corners": [[0, -30], [200, 130]]})).is_ok());
    let r = h.run("rating", json!({"location": "+CAB1", "temperature": "44"})).unwrap_or_else(|e| json!(e));
    assert_eq!((r["boxes"].clone(), r["temperature"].clone(), r["changed"].clone()), (json!(1), json!("44"), json!(true)), "{r}");
    let r = h.run("rating", json!({"location": "+CAB1", "temperature": null})).unwrap_or_else(|e| json!(e));
    assert_eq!((r["temperature"].clone(), r["changed"].clone()), (Value::Null, json!(true)), "{r}");
}

/// Input refusals of the BOM commands: one wording.
#[test]
fn the_bom_commands_refuse_input_uniformly() {
    let rows: &[(&str, Value, &str)] = &[
        ("partline", json!({"component": "zz"}), "`component` must be a hex id, or null"),
        ("partline", json!({"component": "1", "add": 5}), "`add` must be a list of objects, or null"),
        ("partline", json!({"component": "1", "line": "x"}), "`line` must be a whole number, or null"),
        ("partline", json!({"component": "1", "remove": "yes"}), "`remove` must be true or false, or null"),
        ("bomitem", json!({"description": true}), "`description` must be text, or null"),
        ("bomitem", json!({"colour": "red"}), "`colour` is not a parameter of this command; it takes "),
        ("article", json!({"article_number": "A1"}), "`part_number` (text) is required"),
        ("article", json!({"part_number": "P1"}), "`article_number` (text, or null) is required"),
        ("article", json!({"part_number": "P1", "maker": 3, "article_number": null}), "`maker` must be text, or null"),
    ];
    let mut h = H::new(7042);
    for (cmd, params, says) in rows {
        let e = h.run(cmd, params.clone()).err().unwrap_or_default();
        assert!(e.starts_with(says), "{cmd} {params}: {e}");
    }
}

/// Rule refusals of the BOM commands keep their words.
#[test]
fn the_bom_commands_keep_their_rule_words() {
    let mut h = H::new(7043);
    let k = h.run("place", json!({"symbol": "coil", "at": [100, 100]})).unwrap_or_else(|e| json!(e))["component"].clone();
    let rows: &[(&str, Value, &str)] = &[
        ("partline", json!({"add": {"maker": "M"}}), "`component` or `components` (hex id) is required"),
        ("partline", json!({"components": [], "add": {"maker": "M"}}), "`components` is empty"),
        ("partline", json!({"component": "ab", "add": {"maker": "M"}}), "No component 000000000000000000000000000000ab."),
        ("partline", json!({"component": k, "add": []}), "`add` names no line"),
        ("partline", json!({"component": k, "add": {"kit": "K"}}), "`kit` is not a part line field. Use part_number,"),
        (
            "partline",
            json!({"component": k, "add": {"part_number": " "}}),
            "A part line needs at least one field: part_number, maker or description.",
        ),
        ("partline", json!({"component": k}), "`line` (1 to 99) is required with `set` or `remove`, or give `add`"),
        ("partline", json!({"component": k, "line": 1}), "-K"),
        ("bomitem", json!({"qty": "2"}), "An item needs a description."),
        ("bomitem", json!({"description": "x", "set": {}}), "`set` is not an item field. Use description,"),
        ("bomitem", json!({"item": k, "delete": true}), "is not an item."),
        ("article", json!({"part_number": " ", "article_number": "A"}), "`part_number` is required"),
        ("article", json!({"part_number": "NOPE", "article_number": "A"}), "No part NOPE in this project."),
    ];
    for (cmd, params, says) in rows {
        let e = h.run(cmd, params.clone()).err().unwrap_or_default();
        assert!(e.starts_with(says) || (cmd == &"bomitem" && e.ends_with(says)), "{cmd} {params}: {e}");
    }
    let e = h.run("partline", json!({"component": k, "line": 1, "set": {"maker": "M"}})).err().unwrap_or_default();
    assert!(e.ends_with("has no part line 1."), "{e}");
}

/// The typed BOM commands still work, and answer as before.
#[test]
fn the_typed_bom_commands_still_work() {
    let mut h = H::new(7044);
    let k = h.run("place", json!({"symbol": "coil", "at": [100, 100]})).unwrap_or_else(|e| json!(e))["component"].clone();
    let add = h.run("partline", json!({"component": k, "add": [{"part_number": "P1", "maker": "M", "qty": 2}, {"description": "D"}]}));
    let add = add.unwrap_or_else(|e| json!(e));
    assert_eq!((add["lines"].clone(), add["changed"].clone()), (json!([1, 2]), json!(true)), "{add}");
    let set = h.run("partline", json!({"component": k, "line": 2, "set": {"maker": "N", "description": null}})).unwrap_or_else(|e| json!(e));
    assert_eq!(set["message"], json!("Edited part line 2."), "{set}");
    let rm = h.run("partline", json!({"component": k, "line": 2, "remove": true})).unwrap_or_else(|e| json!(e));
    assert_eq!(rm["message"], json!("Removed part line 2."), "{rm}");
    let art = h.run("article", json!({"part_number": "P1", "article_number": 1234})).unwrap_or_else(|e| json!(e));
    assert_eq!((art["changed"].clone(), art["parts"].clone()), (json!(true), json!(1)), "{art}");
    let item = h.run("bomitem", json!({"description": "Trunking", "qty": "3.5", "unit": "m"})).unwrap_or_else(|e| json!(e));
    assert_eq!(item["changed"], json!(true), "{item}");
    let id = item["item"].clone();
    let same = h.run("bomitem", json!({"item": id, "set": {"description": "Trunking"}})).unwrap_or_else(|e| json!(e));
    assert_eq!(same["changed"], json!(false), "{same}");
    let gone = h.run("bomitem", json!({"item": id, "delete": true})).unwrap_or_else(|e| json!(e));
    assert!(gone["message"].as_str().is_some_and(|m| m.starts_with("Deleted item Trunking")), "{gone}");
}

/// Input refusals of the location commands: one wording.
#[test]
fn the_location_commands_refuse_input_uniformly() {
    let rows: &[(&str, Value, &str)] = &[
        ("setlocation", json!({"component": "1"}), "`value` (text, or null) is required"),
        ("setlocation", json!({"component": "1", "value": 5}), "`value` must be text, or null"),
        ("setlocation", json!({"components": "1", "value": null}), "`components` must be a list of hex ids, or null"),
        ("setlocation", json!({"component": "1", "value": null, "type": 3}), "`type` must be text, or null"),
        ("setfunction", json!({"component": "1", "value": "=F1", "aspect": "location"}), "`aspect` is not a parameter of this command; it takes "),
        ("conflicts", json!({"answer": 1}), "`answer` must be text, or null"),
        ("conflicts", json!({"components": ["zz"]}), "`components` must be a list of hex ids, or null"),
    ];
    let mut h = H::new(7045);
    for (cmd, params, says) in rows {
        let e = h.run(cmd, params.clone()).err().unwrap_or_default();
        assert!(e.starts_with(says), "{cmd} {params}: {e}");
    }
}

/// Rule refusals of the location commands keep their words.
#[test]
fn the_location_commands_keep_their_rule_words() {
    let mut h = H::new(7046);
    let k = h.run("place", json!({"symbol": "coil", "at": [100, 100]})).unwrap_or_else(|e| json!(e))["component"].clone();
    let rows: &[(&str, Value, &str)] = &[
        ("setlocation", json!({"value": "+CP1"}), "`component` or `components` (hex ids) is required"),
        ("setlocation", json!({"component": k, "value": "+CP1", "aspect": "place"}), "`aspect` is \"location\" or \"function\""),
        ("setlocation", json!({"component": k, "value": "=F1", "aspect": "function", "type": "plug"}), "A function has no overrides"),
        ("setlocation", json!({"component": k, "value": "+CP1", "type": "head"}), "The head is the device; set the device's location."),
        ("setlocation", json!({"component": "ab", "value": "+CP1"}), "no such device"),
        ("setfunction", json!({"component": k, "value": "=F1", "type": "plug"}), "A function has no overrides: a device's parts share its function."),
        ("conflicts", json!({"answer": "maybe"}), "`answer` is \"move\", \"half\" or \"leave\""),
        ("conflicts", json!({"aspect": "x"}), "`aspect` is \"location\" or \"function\""),
        ("conflicts", json!({"answer": "half"}), "No connector half is waiting: Half answers a half or pin just placed."),
    ];
    for (cmd, params, says) in rows {
        let e = h.run(cmd, params.clone()).err().unwrap_or_default();
        assert!(e.starts_with(says), "{cmd} {params}: {e}");
    }
}

/// The typed location commands still work, and answer as before.
#[test]
fn the_typed_location_commands_still_work() {
    let mut h = H::new(7047);
    let k = h.run("place", json!({"symbol": "coil", "at": [100, 100]})).unwrap_or_else(|e| json!(e))["component"].clone();
    let set = h.run("setlocation", json!({"components": [k, k], "value": "+CP1"})).unwrap_or_else(|e| json!(e));
    assert_eq!((set["changed"].clone(), set["devices"].clone(), set["location"].clone()), (json!(true), json!(1), json!("+CP1")), "{set}");
    let f = h.run("setfunction", json!({"component": k, "value": "=F1"})).unwrap_or_else(|e| json!(e));
    assert_eq!(f["function"], json!("=F1"), "{f}");
    let o = h.run("setlocation", json!({"component": k, "value": "+X1", "type": " Plug "})).unwrap_or_else(|e| json!(e));
    assert!(o.is_object(), "{o}");
    let clear = h.run("setlocation", json!({"component": k, "value": null, "aspect": "function"})).unwrap_or_else(|e| json!(e));
    assert_eq!((clear["changed"].clone(), clear["function"].clone()), (json!(true), Value::Null), "{clear}");
    let list = h.run("conflicts", json!({})).unwrap_or_else(|e| json!(e));
    assert!(list["conflicts"].is_object() || list["conflicts"].is_array(), "{list}");
    let leave = h.run("conflicts", json!({"answer": " LEAVE "})).unwrap_or_else(|e| json!(e));
    assert_eq!(leave["changed"], json!(false), "{leave}");
    let mv = h.run("conflicts", json!({"answer": "move", "aspect": "location"})).unwrap_or_else(|e| json!(e));
    assert_eq!(mv["changed"], json!(false), "{mv}");
    let sp = h.run("setprop", json!({"component": k, "key": "location", "value": "+CP2"})).unwrap_or_else(|e| json!(e));
    assert_eq!(sp["location"], json!("+CP2"), "{sp}");
}

/// Input refusals of the title block commands: one wording.
#[test]
fn the_title_block_commands_refuse_input_uniformly() {
    let rows: &[(&str, Value, &str)] = &[
        ("titleblocks", json!({"id": "x"}), "`id` is not a parameter of this command; it takes none"),
        ("titleblock", json!({"id": 5}), "`id` must be text, or null"),
        ("titleblock", json!({"id": "sheet", "fields": 1}), "`fields` must be an object, or null"),
        ("titleblock", json!({"new": {"sheet": "A3", "colour": 1}}), "`colour` is not a parameter of this command; it takes "),
        ("titleblock", json!({"new": "A3"}), "`new` must be an object, or null"),
    ];
    let mut h = H::new(7048);
    for (cmd, params, says) in rows {
        let e = h.run(cmd, params.clone()).err().unwrap_or_default();
        assert!(e.starts_with(says), "{cmd} {params}: {e}");
    }
}

/// Rule refusals of the title block commands keep their words.
#[test]
fn the_title_block_commands_keep_their_rule_words() {
    let mut h = H::new(7049);
    let made = h.run("titleblock", json!({"new": {"from": "sheet", "sheet": "A3", "name": "Mine"}})).unwrap_or_else(|e| json!(e));
    let id = made["id"].clone();
    let rows: &[(&str, Value, &str)] = &[
        ("titleblock", json!({}), "Name a frame: `id` or `name` (`titleblocks` lists them)."),
        ("titleblock", json!({"id": "sheet", "name": "New"}), "Built-in. Duplicate it to make your own"),
        ("titleblock", json!({"id": id, "sheet": "Q9"}), ""),
        ("titleblock", json!({"id": id, "body": {"nonsense": 1}}), "The body does not read as a title block: "),
        ("titleblock", json!({"new": {"sheet": "A3", "name": "mine"}}), "A frame style is already called \"mine\"; choose another name."),
        ("titleblock", json!({"new": {"sheet": "A3", "layer": "x"}}), "Unknown layer \"x\": project or standard."),
        ("titleblock", json!({"new": {"sheet": "A3", "layer": "standard"}}), "This project uses IEC (built in)."),
        ("titleblock", json!({"new": {"sheet": "A3", "name": ""}}), "A style needs a name (1 to 64 characters, no control characters)."),
    ];
    for (cmd, params, says) in rows {
        let e = h.run(cmd, params.clone()).err().unwrap_or_default();
        assert!(!e.is_empty() && e.starts_with(says), "{cmd} {params}: {e}");
    }
}

/// The typed title block commands still work, and answer as before.
#[test]
fn the_typed_title_block_commands_still_work() {
    let mut h = H::new(7050);
    let list = h.run("titleblocks", json!({})).unwrap_or_else(|e| json!(e));
    assert!(list["title_blocks"].as_array().is_some_and(|r| !r.is_empty()), "{list}");
    let made = h.run("titleblock", json!({"new": {"from": "blank", "sheet": "A3"}})).unwrap_or_else(|e| json!(e));
    assert_eq!((made["changed"].clone(), made["name"].clone()), (json!(true), json!("A3 title block")), "{made}");
    let id = made["id"].clone();
    let read = h.run("titleblock", json!({"name": "A3 title block"})).unwrap_or_else(|e| json!(e));
    assert_eq!(read["id"], id, "{read}");
    let renamed = h.run("titleblock", json!({"id": id, "name": "Shop"})).unwrap_or_else(|e| json!(e));
    assert_eq!((renamed["name"].clone(), renamed["changed"].clone()), (json!("Shop"), json!(true)), "{renamed}");
    let by_name = h.run("titleblock", json!({"name": "Shop", "rename": "Shop 2"})).unwrap_or_else(|e| json!(e));
    assert_eq!(by_name["name"], json!("Shop 2"), "{by_name}");
    let body = by_name["body"].clone();
    let same = h.run("titleblock", json!({"id": id, "body": body})).unwrap_or_else(|e| json!(e));
    assert_eq!(same["changed"], json!(false), "{same}");
    let second = h.run("titleblock", json!({"new": {"from": "blank", "sheet": "A3"}})).unwrap_or_else(|e| json!(e));
    assert_eq!(second["name"], json!("A3 title block"), "{second}");
}

/// Input refusals of the page type commands: one wording.
#[test]
fn the_page_type_commands_refuse_input_uniformly() {
    let rows: &[(&str, Value, &str)] = &[
        ("folderprops", json!({}), "`folder` (hex id) is required"),
        ("folderprops", json!({"folder": "1", "start": 100}), "`start` must be text, or null"),
        ("renumberpages", json!({"folder": "zz"}), "`folder` must be a hex id, or null"),
        ("deletepage", json!({"pages": "1"}), "`pages` must be a list of hex ids, or null"),
        ("deletepage", json!({"page": "1", "all": true}), "`all` is not a parameter of this command; it takes `page`, `pages`"),
    ];
    let mut h = H::new(7051);
    for (cmd, params, says) in rows {
        let e = h.run(cmd, params.clone()).err().unwrap_or_default();
        assert!(e.starts_with(says), "{cmd} {params}: {e}");
    }
}

/// Rule refusals of the page type commands keep their words.
#[test]
fn the_page_type_commands_keep_their_rule_words() {
    let mut h = H::new(7052);
    let f = h.run("newfolder", json!({"name": "Schematic"})).unwrap_or_else(|e| json!(e))["folder"].clone();
    let first = h.first_page().hex();
    let rows: &[(&str, Value, &str)] = &[
        ("folderprops", json!({"folder": f, "start": "1234567"}), "A start number is 1 to 6 digits."),
        ("folderprops", json!({"folder": "ab"}), "no such folder"),
        ("renumberpages", json!({"folder": "ab"}), "no such folder"),
        ("deletepage", json!({}), "`page` (hex id) is required"),
        ("deletepage", json!({"pages": []}), "Give the page to delete."),
        ("deletepage", json!({"page": "ab"}), "no such page"),
        ("deletepage", json!({"page": first}), "Deleting a drawn page comes later; move its content first."),
    ];
    for (cmd, params, says) in rows {
        let e = h.run(cmd, params.clone()).err().unwrap_or_default();
        assert!(e.starts_with(says), "{cmd} {params}: {e}");
    }
}

/// The typed page type commands still work, and answer as before.
#[test]
fn the_typed_page_type_commands_still_work() {
    let mut h = H::new(7053);
    let f = h.run("newfolder", json!({"name": "Schematic"})).unwrap_or_else(|e| json!(e))["folder"].clone();
    let set = h.run("folderprops", json!({"folder": f, "start": " 100 "})).unwrap_or_else(|e| json!(e));
    assert_eq!((set["start"].clone(), set["changed"].clone()), (json!("100"), json!(true)), "{set}");
    let read = h.run("folderprops", json!({"folder": f})).unwrap_or_else(|e| json!(e));
    assert_eq!((read["start"].clone(), read["name"].clone()), (json!("100"), json!("Schematic")), "{read}");
    let same = h.run("folderprops", json!({"folder": f, "start": "100"})).unwrap_or_else(|e| json!(e));
    assert_eq!(same["changed"], json!(false), "{same}");
    let clear = h.run("folderprops", json!({"folder": f, "start": ""})).unwrap_or_else(|e| json!(e));
    assert_eq!((clear["start"].clone(), clear["changed"].clone()), (Value::Null, json!(true)), "{clear}");
    let r = h.run("renumberpages", json!({})).unwrap_or_else(|e| json!(e));
    assert!(r["renumbered"].is_number() && r["message"].is_string(), "{r}");
    let cover = h.run("newpage", json!({"generated": "cover"})).unwrap_or_else(|e| json!(e))["page"].clone();
    let gone = h.run("deletepage", json!({"pages": [cover]})).unwrap_or_else(|e| json!(e));
    assert_eq!(gone["deleted"], json!([cover]), "{gone}");
    assert!(gone["message"].as_str().is_some_and(|m| m.starts_with("Removed ")), "{gone}");
}

/// Input refusals of the connector commands: one wording.
#[test]
fn the_connector_commands_refuse_input_uniformly() {
    let rows: &[(&str, Value, &str)] = &[
        ("connector", json!({"at": "x"}), "`at` must be a point [x, y], or null"),
        ("connector", json!({"line": [[0, 0]]}), "`line` needs at least 2 entries"),
        ("connector", json!({"pins": "4"}), "`pins` must be a whole number, or null"),
        ("connector", json!({"swap": 1}), "`swap` must be true or false, or null"),
        ("connector", json!({"device": 5}), "`device` must be text, or null"),
        ("connector", json!({"wire": "zz"}), "`wire` must be a hex id, or null"),
        ("pinlabel", json!({"device": "-X1"}), "`to` (text) is required"),
        ("pinlabel", json!({"to": 3}), "`to` must be text"),
    ];
    let mut h = H::new(7054);
    for (cmd, params, says) in rows {
        assert_eq!(h.run(cmd, params.clone()).err().as_deref(), Some(*says), "{cmd} {params}");
    }
}

/// Rule refusals of the connector commands keep their words.
#[test]
fn the_connector_commands_keep_their_rule_words() {
    let rows: &[(&str, Value, &str)] = &[
        ("connector", json!({"rotation": 45}), "`rotation` is 0, 90, 180 or 270"),
        ("connector", json!({"at": [0, 0], "halves": "x"}), "`halves` is socket, plug or both"),
        ("connector", json!({"at": [0, 0], "family": "x"}), "`family` is plain, dsub, m12, rj45, usba or usbb"),
        ("connector", json!({"at": [0, 0], "pins": 99}), "`pins` is a whole number from 1 to 64."),
        ("connector", json!({"at": [0, 0], "layout": "x"}), "`layout` is row or pins"),
        ("connector", json!({"layout": "row", "family": "plain"}), "connector needs at for a row, or line or wire for pins."),
        ("connector", json!({"at": [0, 0], "page": "ab"}), "No such page."),
        ("pinlabel", json!({"to": "2"}), "`device` and `pin`, or `terminal`, is required"),
        ("pinlabel", json!({"terminal": "1", "to": "2"}), "No such connection point."),
    ];
    let mut h = H::new(7055);
    for (cmd, params, says) in rows {
        let e = h.run(cmd, params.clone()).err().unwrap_or_default();
        assert!(e.starts_with(says), "{cmd} {params}: {e}");
    }
}

/// The typed connector commands still work, and answer as before.
#[test]
fn the_typed_connector_commands_still_work() {
    let mut h = H::new(7056);
    let row = json!({"halves": "both", "family": "plain", "pins": 4, "names": "numbers", "layout": "row", "at": [100, 100]});
    let v = h.run("connector", row).unwrap_or_else(|e| json!(e));
    assert_eq!((v["new"].clone(), v["terminals"].as_array().map(Vec::len)), (json!(true), Some(8)), "{v}");
    assert!(v["message"].as_str().is_some_and(|m| m.starts_with("Placed ")), "{v}");
    let c = v["component"].clone();
    let r = h.run("pinlabel", json!({"device": c, "pin": "1", "to": "L"})).unwrap_or_else(|e| json!(e));
    assert_eq!((r["changed"].clone(), r["from"].clone(), r["to"].clone()), (json!(true), json!("1"), json!("L")), "{r}");
    let same = h.run("pinlabel", json!({"device": c, "pin": "L", "to": "L"})).unwrap_or_else(|e| json!(e));
    assert_eq!(same["changed"], json!(false), "{same}");
    let list = h.run("connector", json!({"names": ["L", "N", "PE"], "layout": "row", "family": "plain", "at": [200, 100]}));
    assert!(list.is_ok_and(|v| v["terminals"].as_array().is_some_and(|t| !t.is_empty())));
}

/// Input refusals of the accessory commands: one wording.
#[test]
fn the_accessory_commands_refuse_input_uniformly() {
    let rows: &[(&str, Value, &str)] = &[
        ("accessory", json!({"strip": "zz"}), "`strip` must be a hex id, or null"),
        ("accessory", json!({"strip": "1", "on": "yes"}), "`on` must be true or false, or null"),
        ("accessory", json!({"strip": "1", "clear": "acc.start"}), "`clear` must be a list of texts, or null"),
        ("jumper", json!({"terminals": "1"}), "`terminals` must be a list of hex ids, or null"),
        ("jumper", json!({"strip": "1", "skip": 1}), "`skip` must be true or false, or null"),
        ("jumper", json!({"link": 5}), "`link` must be an object, or null"),
    ];
    let mut h = H::new(7057);
    for (cmd, params, says) in rows {
        assert_eq!(h.run(cmd, params.clone()).err().as_deref(), Some(*says), "{cmd} {params}");
    }
}

/// Rule refusals of the accessory commands keep their words.
#[test]
fn the_accessory_commands_keep_their_rule_words() {
    let rows: &[(&str, Value, &str)] = &[
        ("accessory", json!({"at": "start"}), "`strip` or `terminal` (hex id) is required"),
        ("accessory", json!({"strip": "1", "at": "start", "add": ["end_stop"]}), "No such strip."),
        ("accessory", json!({"terminal": "1", "at": "start", "add": ["end_stop"]}), "No such terminal."),
        ("jumper", json!({}), "`strip` with `positions`, or `terminals`, is required"),
        ("jumper", json!({"link": {"n": 1}}), "`strip` (hex id) is required"),
        ("jumper", json!({"strip": "1", "positions": "1-2"}), "No such strip."),
    ];
    let mut h = H::new(7058);
    for (cmd, params, says) in rows {
        let e = h.run(cmd, params.clone()).err().unwrap_or_default();
        assert!(e.starts_with(says), "{cmd} {params}: {e}");
    }
}

/// The typed accessory commands still work, and answer as before.
#[test]
fn the_typed_accessory_commands_still_work() {
    let mut h = H::new(7059);
    for x in [0, 20, 40] {
        assert!(h.run("wire", json!({"points": [[x, 0], [x, 100]]})).is_ok());
    }
    let f = h.run("terminals", json!({"from": [-10, 50], "to": [50, 50]})).unwrap_or(Value::Null);
    let strip = f["strip"].clone();
    let a = h.run("accessory", json!({"strip": strip, "at": "start", "add": ["end_stop"]})).unwrap_or_else(|e| json!(e));
    assert_eq!((a["changed"].clone(), a["keys"][0]["items"].clone()), (json!(true), json!(["end_stop"])), "{a}");
    let m = h.run("accessory", json!({"strip": strip, "at": "all", "on": true, "items": ["marker"]})).unwrap_or_else(|e| json!(e));
    assert_eq!(m["keys"].as_array().map(Vec::len), Some(3), "{m}");
    let none = h.run("accessory", json!({"strip": strip, "at": "all", "on": true, "items": null})).unwrap_or_else(|e| json!(e));
    assert_eq!(none["keys"][0]["items"], json!([]), "{none}");
    let l = h.run("jumper", json!({"strip": strip, "positions": "1-3"})).unwrap_or_else(|e| json!(e));
    assert_eq!((l["link"].clone(), l["changed"].clone()), (json!(1), json!(true)), "{l}");
    let e = h.run("jumper", json!({"link": {"strip": strip, "n": 1}, "remove": ["3"]})).unwrap_or_else(|e| json!(e));
    assert_eq!(e["positions"], json!("1-2"), "{e}");
    let d = h.run("jumper", json!({"link": {"strip": strip, "n": 1}, "delete": true})).unwrap_or_else(|e| json!(e));
    assert_eq!((d["positions"].clone(), d["changed"].clone()), (Value::Null, json!(true)), "{d}");
}

/// Input refusals of the release commands: one wording.
#[test]
fn the_release_commands_refuse_input_uniformly() {
    let rows: &[(&str, Value, &str)] = &[
        ("prerelease", json!({"limit": "x"}), "`limit` must be a whole number, or null"),
        ("prerelease", json!({"rev": 1}), "`rev` must be text, or null"),
        ("release", json!({"rev": 1}), "`rev` must be text, or null"),
        ("release", json!({"outputs": "bom"}), "`outputs` must be a list of texts, or null"),
        ("release", json!({"lineweights": "x"}), "`lineweights` must be true or false, or null"),
        ("release", json!({"colour": true}), "`colour` must be text, or null"),
        (
            "release",
            json!({"x": 1}),
            "`x` is not a parameter of this command; it takes `accept`, `colour`, `existing`, `folder`, `lineweights`, `outputs`, `provisional`, `rev`",
        ),
    ];
    let mut h = H::new(7060);
    for (cmd, params, says) in rows {
        assert_eq!(h.run(cmd, params.clone()).err().as_deref(), Some(*says), "{cmd} {params}");
    }
}

/// Rule refusals of the release commands keep their words.
#[test]
fn the_release_commands_keep_their_rule_words() {
    let rows: &[(&str, Value, &str)] = &[
        ("release", json!({"outputs": ["nope"]}), "`outputs` names \"nope\"; the slots are "),
        ("release", json!({"existing": "x"}), "`existing` is \"add-missing\", \"replace\" or \"keep-both\", not \"x\""),
        ("release", json!({"colour": "x"}), "`colour` is \"mono\""),
    ];
    let mut h = H::new(7061);
    for (cmd, params, says) in rows {
        let e = h.run(cmd, params.clone()).err().unwrap_or_default();
        assert!(e.starts_with(says), "{cmd} {params}: {e}");
    }
}

/// The typed release commands still work, and answer as before.
#[test]
fn the_typed_release_commands_still_work() {
    let mut h = H::new(7062);
    let v = h.run("prerelease", json!({"limit": 5})).unwrap_or_else(|e| json!(e));
    assert!(v.is_object(), "{v}");
    let same = h.run("prerelease", json!({})).unwrap_or_else(|e| json!(e));
    assert_eq!(v, same);
}

/// Input refusals of the terminal label commands: one wording.
#[test]
fn the_level_commands_refuse_input_uniformly() {
    let rows: &[(&str, Value, &str)] = &[
        ("termlabel", json!({"strip": "1", "level": "x"}), "`level` must be a whole number, or null"),
        ("termlabel", json!({"strip": "1", "text": 5}), "`text` must be text, or null"),
        ("termlabel", json!({"terminal": "1", "move_to": "far"}), "`move_to` must be one of \"next\", \"new\", or null"),
        ("toposition", json!({}), "`device` (hex id) is required"),
        ("toposition", json!({"device": "1", "position": 3}), "`position` must be text, or null"),
    ];
    let mut h = H::new(7063);
    for (cmd, params, says) in rows {
        assert_eq!(h.run(cmd, params.clone()).err().as_deref(), Some(*says), "{cmd} {params}");
    }
}

/// Rule refusals of the terminal label commands keep their words.
#[test]
fn the_level_commands_keep_their_rule_words() {
    let rows: &[(&str, Value, &str)] = &[
        ("termlabel", json!({}), "`terminal` or `strip` (hex id) is required"),
        ("termlabel", json!({"terminal": "1"}), "No such terminal."),
        ("termlabel", json!({"strip": "1"}), "Name the position to relabel with `at` (its current label), or give `terminal`."),
        ("termlabel", json!({"strip": "1", "at": "1"}), "That device is not a terminal strip."),
        ("toposition", json!({"device": "1"}), "No such device."),
    ];
    let mut h = H::new(7064);
    for (cmd, params, says) in rows {
        let e = h.run(cmd, params.clone()).err().unwrap_or_default();
        assert!(e.starts_with(says), "{cmd} {params}: {e}");
    }
}

/// The typed terminal label commands still work, and answer as before.
#[test]
fn the_typed_level_commands_still_work() {
    let mut h = H::new(7065);
    for x in [0, 20] {
        assert!(h.run("wire", json!({"points": [[x, 0], [x, 100]]})).is_ok());
    }
    let f = h.run("terminals", json!({"from": [-10, 50], "to": [30, 50]})).unwrap_or(Value::Null);
    let strip = f["strip"].clone();
    let r = h.run("termlabel", json!({"strip": strip, "at": "2", "position": " 7 "})).unwrap_or_else(|e| json!(e));
    assert_eq!(r["changed"], json!(true), "{r}");
    assert!(r["message"].as_str().is_some_and(|m| m.ends_with(":7 (1 level).")), "{r}");
    let t = h.run("termlabel", json!({"strip": strip, "at": "7", "text": "L1"})).unwrap_or_else(|e| json!(e));
    assert_eq!(t["changed"], json!(true), "{t}");
    let clear = h.run("termlabel", json!({"strip": strip, "at": "7", "text": null})).unwrap_or_else(|e| json!(e));
    assert_eq!(clear["changed"], json!(true), "{clear}");
    let none = h.run("termlabel", json!({"strip": strip, "at": "7", "text": null})).unwrap_or_else(|e| json!(e));
    assert_eq!(none, json!({"changed": false, "message": "Nothing to change."}));
}

/// Input refusals and rule words of `contcaption`.
#[test]
fn contcaption_refuses_input_uniformly_and_keeps_its_rule_words() {
    let input: &[(Value, &str)] = &[
        (json!({"caption": "X"}), "`continuations` (list of hex ids) is required"),
        (json!({"continuations": ["1"]}), "`caption` (text, or null) is required"),
        (json!({"continuations": ["1"], "caption": 5}), "`caption` must be text, or null"),
        (json!({"continuations": "1", "caption": "X"}), "`continuations` must be a list of hex ids"),
    ];
    let mut h = H::new(7066);
    for (params, says) in input {
        assert_eq!(h.run("contcaption", params.clone()).err().as_deref(), Some(*says), "{params}");
    }
    let rules: &[(Value, &str)] = &[
        (json!({"continuations": [], "caption": "X"}), "Name at least one continuation arrow in `continuations`."),
        (json!({"continuations": ["1"], "caption": "A\nB"}), ecad_cadcraft::continuations::CAPTION_RULE),
    ];
    for (params, says) in rules {
        let e = h.run("contcaption", params.clone()).err().unwrap_or_default();
        assert!(e.starts_with(says), "{params}: {e}");
    }
    let schema = schema_of("contcaption").unwrap_or(Value::Null);
    assert_eq!(schema["required"], json!(["continuations", "caption"]), "{schema}");
}

/// Input refusals and rule words of `setdefault`.
#[test]
fn setdefault_refuses_input_uniformly_and_keeps_its_rule_words() {
    let input: &[(Value, &str)] = &[
        (json!({"value": 3}), "`key` (text) is required"),
        (json!({"key": "text.run.gap", "value": true}), "`value` must be text, or null"),
        (json!({"key": "text.run.gap", "value": 3, "reset": "yes"}), "`reset` must be true or false, or null"),
        (json!({"key": "text.run.gap", "level": 2, "value": 3}), "`level` must be text, or null"),
        (json!({"key": "text.tag.show", "level": "object", "component": "zz", "value": "off"}), "`component` must be a hex id, or null"),
    ];
    let mut h = H::new(7067);
    for (params, says) in input {
        assert_eq!(h.run("setdefault", params.clone()).err().as_deref(), Some(*says), "{params}");
    }
    let rules: &[(Value, &str)] = &[
        (json!({"key": "text.nope", "value": 3}), "No default \"text.nope\"."),
        (json!({"key": "text.run.gap", "level": "sideways", "value": 3}), "Unknown level \"sideways\". Levels: standard, project, page, object."),
        (json!({"key": "text.run.gap"}), ecad_cadcraft::default_cmds::NEEDS_VALUE),
        (json!({"key": "text.run.gap", "value": 3, "reset": true}), "Give either `value` or `reset: true`, not both."),
        (json!({"key": "text.run.gap", "value": 101}), "`text.run.gap` is a length from 0 to 100 mm."),
        (json!({"key": "text.tag.show", "level": "object", "value": "off"}), ecad_cadcraft::default_cmds::NEEDS_OBJECT),
        (json!({"key": "text.run.gap", "level": "standard", "value": 3}), ecad_cadcraft::default_cmds::STANDARD_READ_ONLY),
        (json!({"key": "symbol.coil.prefix", "value": "K1"}), "A prefix is 1 to"),
    ];
    for (params, says) in rules {
        let e = h.run("setdefault", params.clone()).err().unwrap_or_default();
        assert!(e.starts_with(says), "{params}: {e}");
    }
}

/// `setdefault` still writes, resets and answers as before.
#[test]
fn the_typed_setdefault_still_works() {
    let mut h = H::new(7068);
    let v = h.run("setdefault", json!({"key": "text.run.gap", "value": 3})).unwrap_or_else(|e| json!(e));
    assert_eq!(v["changed"], json!(true), "{v}");
    let again = h.run("setdefault", json!({"key": "text.run.gap", "value": "3"})).unwrap_or_else(|e| json!(e));
    assert_eq!(again["changed"], json!(false), "{again}");
    let reset = h.run("setdefault", json!({"key": "text.run.gap", "reset": true, "value": null})).unwrap_or_else(|e| json!(e));
    assert_eq!(reset["changed"], json!(true), "{reset}");
    let px = h.run("setdefault", json!({"key": "symbol.coil.prefix", "value": "q"})).unwrap_or_else(|e| json!(e));
    assert_eq!(px["changed"], json!(true), "{px}");
}

/// `titleblockimport` reads its input strictly at the edge; the DXF reader's refusals keep their
/// words.
#[test]
fn titleblockimport_refuses_input_uniformly_and_keeps_its_rule_words() {
    let input: &[(Value, &str)] = &[
        (json!({"dxf": 5}), "`dxf` must be text, or null"),
        (json!({"dxf": "0\nEOF\n", "item": "x"}), "`item` must be a whole number, or null"),
        (json!({"path": "a.dxf", "colour": 1}), "`colour` is not a parameter of this command; it takes `dxf`, `id`, `item`, `name`, `path`, `sheet`"),
    ];
    let mut h = H::new(7069);
    for (params, says) in input {
        assert_eq!(h.run("titleblockimport", params.clone()).err().as_deref(), Some(*says), "{params}");
    }
    let e = h.run("titleblockimport", json!({"sheet": "A3"})).err().unwrap_or_default();
    assert_eq!(e, "Give `path` or `dxf` (the file's text).");
    let schema = schema_of("titleblockimport").unwrap_or(Value::Null);
    assert_eq!(schema["properties"]["item"]["type"], json!(["integer", "null"]), "{schema}");
}

/// Input refusals of `setprop`: one wording.
#[test]
fn setprop_refuses_input_uniformly() {
    let rows: &[(Value, &str)] = &[
        (json!({"component": "1", "value": "x"}), "`key` (text) is required"),
        (json!({"component": "1", "key": "description"}), "`value` (text, or null) is required"),
        (json!({"component": "1", "key": "description", "value": 3}), "`value` must be text, or null"),
        (json!({"component": "zz", "key": "description", "value": "x"}), "`component` must be a hex id, or null"),
        (json!({"components": ["zz"], "key": "description", "value": "x"}), "`components` must be a list of hex ids, or null"),
        (json!({"component": "1", "key": "tag", "value": "-K1", "merge": "yes"}), "`merge` must be true or false, or null"),
        (json!({"component": "1", "key": "tag", "value": "-K1", "conflict": 1}), "`conflict` must be text, or null"),
        (json!({"key": "description", "value": "x"}), "`component`, `conductor` or `cable` (hex id) is required"),
    ];
    let mut h = H::new(7071);
    for (params, says) in rows {
        assert_eq!(h.run("setprop", params.clone()).err().as_deref(), Some(*says), "{params}");
    }
}

/// Rule refusals of `setprop` keep their words.
#[test]
fn setprop_keeps_its_rule_words() {
    let mut h = H::new(7072);
    let a = crate::common::place(&mut h, "coil", (100.0, 100.0))["component"].clone();
    let b = crate::common::place(&mut h, "coil", (200.0, 100.0))["component"].clone();
    let tag_b = h.run("deviceprops", json!({"component": b})).unwrap_or_default()["tag"].clone();
    let rows: &[(Value, &str)] = &[
        (json!({"component": a, "key": "text.tag.show", "value": "off"}), ecad_electrical::text_style::RESERVED_KEY),
        (json!({"components": [a, b], "key": "tag", "value": "-K9"}), "Tags are one per device."),
        (json!({"component": a, "key": "tag_fixed", "value": "maybe"}), "`tag_fixed` is \"yes\" or null, not \"maybe\""),
        (
            json!({"component": a, "key": "tag", "value": tag_b, "conflict": "sideways"}),
            "`conflict` must be \"swap\", \"merge\" or \"next\", not \"sideways\"",
        ),
        (json!({"component": a, "key": "tag", "value": tag_b}), "is used by"),
        (json!({"component": "ab", "key": "description", "value": "x"}), "no such object"),
    ];
    for (params, says) in rows {
        let e = h.run("setprop", params.clone()).err().unwrap_or_default();
        assert!(e.contains(says), "{params}: {e}");
    }
}

/// `setprop` still writes, swaps and answers as before.
#[test]
fn the_typed_setprop_still_works() {
    let mut h = H::new(7073);
    let a = crate::common::place(&mut h, "coil", (100.0, 100.0))["component"].clone();
    let b = crate::common::place(&mut h, "coil", (200.0, 100.0))["component"].clone();
    let v = h.run("setprop", json!({"component": a, "key": "description", "value": "Pump"})).unwrap_or_else(|e| json!(e));
    assert_eq!(v, json!({"changed": true, "value": "Pump", "component": a}), "{v}");
    let same = h.run("setprop", json!({"component": a, "key": "description", "value": "Pump"})).unwrap_or_else(|e| json!(e));
    assert_eq!(same, json!({"changed": false, "value": "Pump"}), "{same}");
    let free = h.run("setprop", json!({"component": a, "key": "note", "value": "x"})).unwrap_or_else(|e| json!(e));
    assert_eq!(free, json!({"changed": true}), "{free}");
    let many = h.run("setprop", json!({"components": [a, b], "key": "note", "value": "y"})).unwrap_or_else(|e| json!(e));
    assert_eq!(many, json!({"changed": true, "devices": 2}), "{many}");
    let tag = h.run("setprop", json!({"component": a, "key": "tag", "value": "-K07"})).unwrap_or_else(|e| json!(e));
    assert_eq!(tag, json!({"changed": true, "value": "-K07", "fixed": true}), "{tag}");
    let swap = h.run("setprop", json!({"component": b, "key": "tag", "value": "-K07", "conflict": "swap"})).unwrap_or_else(|e| json!(e));
    assert_eq!((swap["swapped"].clone(), swap["other"].clone()), (json!(true), json!({"component": a})), "{swap}");
    let release = h.run("setprop", json!({"component": b, "key": "tag_fixed", "value": null})).unwrap_or_else(|e| json!(e));
    assert_eq!(release, json!({"changed": true, "fixed": false}), "{release}");
    let loc = h.run("setprop", json!({"component": a, "key": "location", "value": "+CP1"})).unwrap_or_else(|e| json!(e));
    assert_eq!(loc["location"], json!("+CP1"), "{loc}");
}

/// PROP_RULES (brief 070 item 3): for each row that decides a device's key, `setprop` on one
/// device and on several walks the same row, so both give the same refusal.
#[test]
fn setprop_on_one_and_on_several_refuse_alike_for_each_rule() {
    use ecad_electrical::cmd::prop::{PROP_RULES, rule_for};
    use ecad_model::{ComponentId, ObjectRef};
    let mut h = H::new(7074);
    let coil = crate::common::place(&mut h, "coil", (100.0, 100.0))["component"].clone();
    let coil2 = crate::common::place(&mut h, "coil", (200.0, 100.0))["component"].clone();
    let strip = crate::common::place(&mut h, "terminal", (100.0, 200.0))["component"].clone();
    let strip2 = crate::common::place(&mut h, "terminal", (100.0, 300.0))["component"].clone();
    let socket = crate::common::place(&mut h, "socket4", (300.0, 200.0))["component"].clone();
    let socket2 = crate::common::place(&mut h, "socket4", (300.0, 300.0))["component"].clone();
    let card = crate::common::place(&mut h, "plcdi", (400.0, 200.0))["component"].clone();
    let card2 = crate::common::place(&mut h, "plcdi", (400.0, 400.0))["component"].clone();
    let long = "x".repeat(400);
    // (row, the two devices, key, a value its rule refuses)
    let rows: &[(&str, [&Value; 2], &str, &str)] = &[
        ("tag fixed (design 026)", [&coil, &coil2], "tag_fixed", "maybe"),
        ("device property (design 018)", [&coil, &coil2], "description", &long),
        ("wire, junction or continuation (briefs 022, 049)", [&coil, &coil2], "caption", "Up"),
        ("contacts (brief 025)", [&coil, &coil2], "contacts", "??"),
        ("contact table mirror (brief 025)", [&coil, &coil2], "mirror", "maybe"),
        ("strip offer (brief 028)", [&strip, &strip2], "terminals", "lots"),
        ("PE row part (brief 028)", [&coil, &coil2], "part_number.pe", &long),
        ("connector (brief 044)", [&socket, &socket2], "pins", "99"),
        ("strip terminal defaults (brief 036)", [&strip, &strip2], "terminal.level.separator", "abcd"),
        ("level label (brief 036)", [&strip, &strip2], "term_label.1", &long),
        ("PLC card (brief 045)", [&card, &card2], "plc.addr.start", "x"),
        ("rating (brief 034)", [&coil, &coil2], "rating.1", "10A"),
        ("parts and items (brief 047)", [&coil, &coil2], "part.1.qty", "lots"),
        ("strip accessories, or free text (brief 030)", [&strip, &strip2], "link.1", "zz"),
    ];
    let doc = h.doc();
    for (what, [one, two], key, value) in rows {
        let c = u128::from_str_radix(one.as_str().unwrap_or_default(), 16).map(ComponentId).unwrap_or(ComponentId(0));
        assert_eq!(rule_for(&doc, ObjectRef::Component(c), key).what, *what, "{key}");
        let single = h.run("setprop", json!({"component": one, "key": key, "value": value})).err();
        let many = h.run("setprop", json!({"components": [one, two], "key": key, "value": value})).err();
        assert!(single.is_some(), "{what}: {key} {value} is refused");
        assert_eq!(single, many, "{what}: one and several differ");
    }
    for (what, ..) in rows {
        assert!(PROP_RULES.iter().any(|r| r.what == *what), "{what}");
    }
}

/// Input refusals of `place`: one wording.
#[test]
fn place_refuses_input_uniformly() {
    let rows: &[(Value, &str)] = &[
        (json!({"symbol": "coil"}), "`at` (point [x, y]) is required"),
        (json!({"symbol": "coil", "at": "x"}), "`at` must be a point [x, y]"),
        (json!({"symbol": "coil", "at": [0, 0], "rotation": "x"}), "`rotation` must be a number, or null"),
        (json!({"symbol": "coil", "at": [0, 0], "mirror": 1}), "`mirror` must be true or false, or null"),
        (json!({"symbol": "coil", "at": [0, 0], "tag": 5}), "`tag` must be text, or null"),
        (json!({"symbol": "coil", "at": [0, 0], "location": {"x": 1}}), "`location` must be text, or null"),
        (json!({"symbol": "plcdi", "at": [0, 0], "rack": 5}), "`rack` must be text, or null"),
        (json!({"at": [0, 0], "definition": "zz"}), "`definition` must be a hex id, or null"),
    ];
    let mut h = H::new(7075);
    for (params, says) in rows {
        assert_eq!(h.run("place", params.clone()).err().as_deref(), Some(*says), "{params}");
    }
}

/// Rule refusals of `place` keep their words, in every form.
#[test]
fn place_keeps_its_rule_words() {
    let mut h = H::new(7076);
    let coil = crate::common::place(&mut h, "coil", (100.0, 100.0))["component"].clone();
    let rows: &[(Value, &str)] = &[
        (json!({"symbol": "coil", "definition": "1", "at": [0, 0]}), "`symbol` and `definition` are mutually exclusive"),
        (json!({"tag": "-K9", "at": [0, 0]}), "`symbol` (a starter symbol key, see `symbols`) is required with `device` or `tag`"),
        (json!({"symbol": "nope", "at": [0, 0]}), "No symbol \"nope\". Type List to see them."),
        (json!({"symbol": "coil", "at": [1e15, 0]}), ecad_cadcraft::interact::OUT_OF_RANGE),
        (json!({"symbol": "coil", "at": [0, 0], "device": "zz"}), "`device` (hex id) is required"),
        (json!({"symbol": "coil", "at": [0, 0], "device": coil, "tag": "-K9"}), "`tag` is for a new device; `device` joins an existing one"),
        (json!({"symbol": "coil", "at": [0, 0], "device": "ab"}), "No such device."),
        (json!({"symbol": "coil", "at": [0, 0], "location": " "}), "`location` is \"propose\", \"move\", \"keep\" or a code"),
        (json!({"symbol": "junction", "at": [0, 0], "tag": "-K9"}), "A junction has no tag and is not part of a device."),
        (json!({"symbol": "continuation", "at": [0, 0]}), "`name` is required for a continuation"),
        (json!({"symbol": "junction", "at": [0, 0], "caption": "Up"}), "Only continuation arrows have a caption; that is a junction."),
        (
            json!({"symbol": "termlnpe.2", "at": [0, 0], "device": coil}),
            "is not a terminal strip. Name a strip (-X01), or leave it out for a new one.",
        ),
        (json!({"symbol": "di", "at": [0, 0], "device": coil, "tag": "-A9"}), "`tag` is for a new card; `device` joins an existing one"),
        (json!({"symbol": "plcdi", "at": [0, 0], "rack": "-K99"}), "No device -K99."),
        (json!({"symbol": "socket4", "at": [0, 0], "device": coil}), "is not a connector"),
        (json!({"at": [0, 0], "definition": "ab"}), "no such definition"),
    ];
    for (params, says) in rows {
        let e = h.run("place", params.clone()).err().unwrap_or_default();
        assert!(e.contains(says), "{params}: {e}");
    }
}

/// `place` still places, joins and answers as before.
#[test]
fn the_typed_place_still_works() {
    let mut h = H::new(7077);
    let v = h.run("place", json!({"symbol": "coil", "at": [100, 100], "tag": "-K5"})).unwrap_or_else(|e| json!(e));
    assert_eq!((v["tag"].clone(), v["joined"].clone()), (json!("-K05"), json!(false)), "{v}");
    let j = h.run("place", json!({"symbol": "no", "at": [200, 100], "device": v["component"]})).unwrap_or_else(|e| json!(e));
    assert_eq!(j["joined"], json!(true), "{j}");
    let c = h.run("place", json!({"symbol": "continuation", "at": [300, 100], "name": "24V", "caption": "to page 2"})).unwrap_or_else(|e| json!(e));
    assert_eq!((c["name"].clone(), c["caption"].clone()), (json!("24V"), json!("to page 2")), "{c}");
    let t = h.run("place", json!({"symbol": "terminal", "at": [100, 300]})).unwrap_or_else(|e| json!(e));
    assert!(t["tag"].as_str().is_some_and(|x| x.starts_with("-X")), "{t}");
    let x = h.run("place", json!({"at": [400, 100]})).unwrap_or_else(|e| json!(e));
    assert_eq!(x["terminals"].as_array().map(Vec::len), Some(2), "{x}");
    let k = h.run("place", json!({"symbol": "socket4", "at": [400, 300]})).unwrap_or_else(|e| json!(e));
    assert_eq!(k["joined"], json!(false), "{k}");
    let ch = h.run("place", json!({"symbol": "di", "at": [500, 300], "channel": 3})).unwrap_or_else(|e| json!(e));
    assert_eq!(ch["channel"], json!(3), "{ch}");
}

/// Input refusals of the device commands: one wording.
#[test]
fn the_device_commands_refuse_input_uniformly() {
    let rows: &[(&str, Value, &str)] = &[
        ("symbols", json!({"query": 3}), "`query` must be text, or null"),
        ("devices", json!({"tag": 3}), "`tag` must be text, or null"),
        ("deviceprops", json!({}), "`component` (hex id) is required"),
        ("deviceprops", json!({"component": "zz"}), "`component` must be a hex id"),
        ("devicedelete", json!({}), "`component` (hex id) is required"),
        ("ecaddelete", json!({"conductor": "zz"}), "`conductor` must be a hex id, or null"),
        ("tagsettings", json!({"padding": "x"}), "`padding` must be a whole number, or null"),
        ("tagsettings", json!({"prefixes": {"coil": 5}}), "`prefixes` must be an object, or null"),
        ("convertcomponent", json!({"handles": "x"}), "`handles` must be a list of texts, or null"),
        ("addterminal", json!({"at": [0, 0]}), "`handle` (text) is required"),
        ("addterminal", json!({"handle": "1"}), "`at` (point [x, y]) is required"),
    ];
    let mut h = H::new(7078);
    for (cmd, params, says) in rows {
        assert_eq!(h.run(cmd, params.clone()).err().as_deref(), Some(*says), "{cmd} {params}");
    }
}

/// Rule refusals of the device commands keep their words.
#[test]
fn the_device_commands_keep_their_rule_words() {
    let rows: &[(&str, Value, &str)] = &[
        ("deviceprops", json!({"component": "ab"}), "No such device."),
        ("devicedelete", json!({"component": "ab"}), "no such component"),
        ("ecaddelete", json!({}), "`conductor`, `component` or `cable` (hex id) is required"),
        ("ecaddelete", json!({"conductor": "ab"}), "no such conductor"),
        ("ecaddelete", json!({"cable": "ab"}), "no such cable"),
        ("tagsettings", json!({"padding": 7}), "`padding` must be a whole number from 1 to 6"),
        ("tagsettings", json!({"prefixes": {"coil": "K1"}}), "A prefix is 1 to 10 letters, for example K or SR."),
        ("tagsettings", json!({"prefixes": {"xyz": "K"}}), "No symbol \"xyz\". Type List to see them."),
        ("convertcomponent", json!({"handles": ["zz"]}), "`\"zz\"` is not a valid handle"),
        ("convertcomponent", json!({"handles": []}), "select the geometry to convert"),
        ("convertcomponent", json!({"handles": ["ffff"]}), "the selection holds an object this page does not know"),
        ("addterminal", json!({"handle": "ffff", "at": [0, 0]}), "pick an item of a component"),
    ];
    let mut h = H::new(7079);
    for (cmd, params, says) in rows {
        assert_eq!(h.run(cmd, params.clone()).err().as_deref(), Some(*says), "{cmd} {params}");
    }
}

/// The typed device commands still work, and answer as before.
#[test]
fn the_typed_device_commands_still_work() {
    let mut h = H::new(7080);
    let a = crate::common::place(&mut h, "coil", (100.0, 100.0))["component"].clone();
    let props = h.run("deviceprops", json!({"component": a})).unwrap_or_else(|e| json!(e));
    assert_eq!(props["tag"], json!("-K01"), "{props}");
    let found = h.run("devices", json!({"tag": "-K"})).unwrap_or_else(|e| json!(e));
    assert!(found.to_string().contains("-K01"), "{found}");
    let syms = h.run("symbols", json!({"query": "coil"})).unwrap_or_else(|e| json!(e));
    assert!(syms.to_string().contains("coil"), "{syms}");
    let ts = h.run("tagsettings", json!({"padding": 3, "prefixes": {"coil": "KA"}})).unwrap_or_else(|e| json!(e));
    assert_eq!((ts["changed"].clone(), ts["padding"].clone(), ts["prefixes"]["coil"].clone()), (json!(true), json!(3), json!("-KA")), "{ts}");
    let read = h.run("tagsettings", json!({})).unwrap_or_else(|e| json!(e));
    assert_eq!(read["changed"], json!(false), "{read}");
    let del = h.run("devicedelete", json!({"component": a})).unwrap_or_else(|e| json!(e));
    assert_eq!(del, json!({"deleted": true, "message": "Deleted -K01."}), "{del}");
    let b = crate::common::place(&mut h, "coil", (200.0, 100.0))["component"].clone();
    let gone = h.run("ecaddelete", json!({"component": b})).unwrap_or_else(|e| json!(e));
    assert_eq!(gone, json!({"deleted": true}), "{gone}");
}
