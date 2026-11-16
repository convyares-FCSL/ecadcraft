//! Brief 070: typed command inputs. Every typed command is registered with its typed run function
//! and offers a JSON Schema; the commands not typed yet can only become fewer; and a typed
//! command refuses bad input in the same words as before.

use ecad_cadcraft::cmd::{schema_of, specs, typed_commands};
use serde_json::{Value, json};

use crate::common::H;

/// Registered commands still reading their JSON by hand. Brief 070 moves them module by module:
/// lower this as they move, never raise it.
const UNTYPED_MAX: usize = 132;

#[test]
fn every_typed_command_is_registered_with_its_schema() {
    let all = specs();
    for t in typed_commands() {
        let spec = all.iter().find(|s| s.id == t.id).unwrap_or_else(|| panic!("{} is typed but not registered", t.id));
        assert!(std::ptr::fn_addr_eq(spec.run, t.run), "{} is registered with another run function", t.id);
        let schema = schema_of(t.id).unwrap_or(Value::Null);
        assert_eq!(schema["type"], json!("object"), "{}: {schema}", t.id);
        assert!(schema["properties"].is_object(), "{}: {schema}", t.id);
        assert!(!spec.params.is_empty(), "{} keeps its params text", t.id);
    }
}

#[test]
fn the_untyped_commands_only_become_fewer() {
    let typed: Vec<&str> = typed_commands().iter().map(|t| t.id).collect();
    let untyped: Vec<&str> = specs().iter().map(|s| s.id).filter(|id| !typed.contains(id)).collect();
    assert!(untyped.len() <= UNTYPED_MAX, "{} commands are untyped, at most {UNTYPED_MAX} may be: {untyped:?}", untyped.len());
}

#[test]
fn a_schema_says_what_the_command_takes() {
    let wire = schema_of("wire").unwrap_or(Value::Null);
    assert_eq!(wire["required"], json!(["points"]));
    assert!(wire["properties"]["points"]["description"].as_str().is_some_and(|d| d.contains("points")), "{wire}");
    assert_eq!(wire["additionalProperties"], json!(false), "unknown parameters are refused");
}

/// The wire commands refuse bad input word for word as their hand-written readers did.
#[test]
fn the_wire_commands_keep_their_words() {
    let rows: &[(&str, Value, &str)] = &[
        ("wire", json!({}), "`points` [[x,y], …] is required"),
        ("wire", json!({"points": 5}), "`points` [[x,y], …] is required"),
        ("wire", json!({"points": [[0, 0]]}), "`points` needs at least two [x,y] points"),
        ("wire", json!({"points": [[0, 0], ["a", 1]]}), "`[\"a\",1]` is not a finite [x,y] point"),
        ("wire", json!({"points": [[0, 0], [9, 0]], "potential": 5}), "`potential` must be a string"),
        ("wire", json!({"points": [[0, 0], [9, 0]], "join": 1}), "`join` must be true or false"),
        ("wire", json!({"points": [[0, 0], [9, 0]], "join": true, "junction": false}), "`join` and `junction` disagree; give only `join`"),
        ("wire", json!({"points": [[0, 0], [9, 0]], "category": 3}), "`category` must be a wire category's ID or name"),
        ("nets", json!({"limit": "x"}), "`limit` must be a number"),
        ("nets", json!({"after": "zz"}), "`after` must be a cursor from `next`"),
        ("net", json!({}), "one of `conductor`, `terminal` or `number` is required"),
        ("net", json!({"conductor": "zz"}), "`conductor` (hex id) is required"),
        ("net", json!({"conductor": "1"}), "No such wire."),
        ("net", json!({"terminal": "1"}), "No such connection point."),
        ("net", json!({"number": "999"}), "no net has number 999"),
        ("wirenumber", json!({"conductor": "1"}), "`value` is required (a wire number, or null to release a fixed number)"),
        ("wirenumber", json!({"conductor": "1", "value": 3}), "`value` must be a string, or null to release a fixed number"),
        ("wirenumber", json!({"conductor": "1", "value": "a b"}), "A wire number has no spaces or control characters."),
        ("potential", json!({}), "`conductor` (hex id) is required"),
        ("potential", json!({"conductor": "1"}), "`value` is required (a potential such as 24V, or null to clear it)"),
        ("potential", json!({"conductor": "1", "value": "24V"}), "No such wire."),
        ("category", json!({"conductor": "1", "value": 3}), "`value` is required: a wire category (its ID or name), or null to clear it"),
        ("category", json!({"value": null}), "Give one of `conductor`, `net` or `conductors`."),
        ("category", json!({"conductors": "1", "value": null}), "`conductors` (array of hex ids) is required"),
        ("wiresettings", json!({"digits": 9}), "`digits` must be 1 to 6"),
        ("wiresettings", json!({"scheme": 1}), "`scheme` must be a string"),
        ("wiresettings", json!({"potentials": 1}), "`potentials` must be the whole table: [{name, prefix, numbered}]"),
        ("wiresettings", json!({"potentials": [{"name": "24V", "colour": "RD"}]}), ecad_electrical::msg::CATEGORY_COLOURS),
        ("renumberwires", json!({"scope": "x"}), "`scope` must be \"unnumbered\" or \"all\", not \"x\""),
        ("junction", json!({}), "`at` [x,y] is required"),
        ("joinwires", json!({"at": "here"}), "`at` [x,y] is required"),
        ("joinwires", json!({"at": [500, 500]}), ecad_electrical::msg::NO_ENDS_TO_JOIN),
        ("checks", json!({"kinds": ["nope"]}), "unknown check kind \"nope\""),
        ("checks", json!({"kinds": "nope"}), "`kinds` must be a list of check kinds"),
    ];
    let mut h = H::new(7001);
    for (cmd, params, says) in rows {
        assert_eq!(h.run(cmd, params.clone()).err().as_deref(), Some(*says), "{cmd} {params}");
    }
}

/// A parameter a command does not take is refused, naming it (it was silently ignored before).
#[test]
fn an_unknown_parameter_is_refused() {
    let mut h = H::new(7002);
    let e = h.run("wire", json!({"points": [[0, 0], [9, 0]], "colour": "RD"})).err().unwrap_or_default();
    assert!(e.starts_with("unknown field `colour`"), "{e}");
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
