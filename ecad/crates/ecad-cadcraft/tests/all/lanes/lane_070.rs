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
    "locationbox",
    "functionbox",
    "bom",
    "wirelist",
    "cablelist",
    "printbook",
    "reports",
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
