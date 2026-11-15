//! Brief 060 (tidy-up guards, R0): `setprop` on several components walks the same rules as on one.

use ecad_cadcraft::checks::{Kind, Severity};
use ecad_model::ComponentId;
use serde_json::{Value, json};

use crate::common::{H, id, pts};

/// Two connector rows and one terminal strip on the active page.
fn fixture(h: &mut H) -> (Vec<ComponentId>, ComponentId) {
    let mut conns = Vec::new();
    for y in [50.0, 120.0] {
        let v = h.run("connector", json!({"halves": "both", "family": "plain", "pins": 4, "layout": "row", "at": [50, y]})).unwrap();
        conns.push(id(&v, "component"));
    }
    for i in 0..3 {
        let x = 200.0 + 10.0 * f64::from(i);
        h.run("wire", json!({"points": pts(&[(x, 200.0), (x, 240.0)])})).unwrap();
    }
    let v = h.run("terminals", json!({"from": [190, 220], "to": [240, 220]})).unwrap();
    (conns, id(&v, "strip"))
}

/// Every (key, value) the single-component path refuses is refused on several components too, and
/// a value it stores canonically is stored the same way (review 2026-11, defect 1).
#[test]
fn setprop_on_several_components_walks_the_single_component_rules() {
    let mut h = H::new(6001);
    let (conns, strip) = fixture(&mut h);
    let label_key = "term_label.1.1";
    let long_label = "L".repeat(30);
    // (components, key, refused value, accepted value, stored value)
    let cases: Vec<(Vec<ComponentId>, &str, &str, &str, &str)> = vec![
        (conns.clone(), "pins", "abc", " 6 ", "6"),
        (conns.clone(), "plc.addr.start", "x1", "12", "12"),
        (vec![strip], "terminal.level.separator", "too-long", ":", ":"),
        (vec![strip], label_key, &long_label, "Top", "Top"),
    ];
    for (cs, key, bad, good, stored) in cases {
        let single = h.run("setprop", json!({"component": cs[0].hex(), "key": key, "value": bad}));
        assert!(single.is_err(), "{key}={bad:?} on one component should be refused, got {single:?}");
        let many: Vec<Value> = cs.iter().map(|c| json!(c.hex())).collect();
        let before = h.head();
        let multi = h.run("setprop", json!({"components": many, "key": key, "value": bad}));
        assert!(multi.is_err(), "{key}={bad:?} on several components should be refused, got {multi:?}");
        assert_eq!(h.head(), before, "{key}: a refusal records nothing");
        h.run("setprop", json!({"components": many, "key": key, "value": good})).unwrap();
        let d = h.doc();
        for c in &cs {
            assert_eq!(d.components[c].properties.get(key).map(String::as_str), Some(stored), "{key} on {}", c.hex());
        }
    }
}

/// A PLC card's `channels` is its channel count on several cards as on one (pack re-run, bug 7);
/// on a connector it stays free text in both paths.
#[test]
fn channels_on_several_plain_devices_stays_free_text() {
    let mut h = H::new(6002);
    let (conns, _) = fixture(&mut h);
    let many: Vec<Value> = conns.iter().map(|c| json!(c.hex())).collect();
    h.run("setprop", json!({"components": many, "key": "channels", "value": "8"})).unwrap();
    let d = h.doc();
    for c in &conns {
        assert_eq!(d.components[c].properties.get("channels").map(String::as_str), Some("8"));
    }
}

/// The check kinds as brief 060 found them: JSON name (persisted in files, filters and agents'
/// calls), Show-list words and severity. New kinds need no line here; these may never change.
const KINDS_AT_060: &[(&str, &str, Severity)] = &[
    ("joins-potentials", "Net joins two potentials", Severity::Error),
    ("shared-number", "Two nets share a number", Severity::Error),
    ("two-fixed", "One net, two fixed numbers", Severity::Error),
    ("open-end", "Open end", Severity::Warning),
    ("loose-wire", "Loose wire", Severity::Warning),
    ("looks-connected", "Looks connected", Severity::Warning),
    ("drawn-away", "End drawn away from its connection point", Severity::Warning),
    ("single-point", "Net with a single connection point", Severity::Warning),
    ("thin-junction", "Junction with fewer than three wires", Severity::Info),
    ("unconnected", "Connection point not connected", Severity::Info),
    ("lonely-continuation", "Continuation without partner", Severity::Info),
    ("mixed-potentials", "Mixed potentials in one net", Severity::Info),
    ("unnumbered", "Unnumbered nets", Severity::Info),
    ("unknown-name", "Continuation name not a potential", Severity::Warning),
    ("duplicate-tag", "Two cables share a tag", Severity::Error),
    ("provisional-tag", "Provisional tag", Severity::Warning),
    ("provisional-wires", "Wire numbers provisional", Severity::Warning),
    ("location-unused", "Location used by none of its symbols", Severity::Warning),
    ("function-unused", "Function used by none of its symbols", Severity::Warning),
    ("function-none", "Devices without a function", Severity::Info),
    ("designation-missing", "Function or location no longer in the list", Severity::Warning),
    ("designation-duplicate", "Two entries share a code", Severity::Warning),
    ("two-heads", "A device with two heads", Severity::Warning),
    ("box-crosses", "Location box outline crosses itself", Severity::Warning),
    ("boxes-overlap", "Location boxes overlap without nesting", Severity::Info),
    ("no-part-number", "Devices without a part number", Severity::Info),
    ("no-location", "Devices without a location", Severity::Info),
    ("too-many-contacts", "More contacts than the part offers", Severity::Warning),
    ("too-many-terminals", "More terminals than the strip offers", Severity::Warning),
    ("terminal-twice", "Terminal drawn twice", Severity::Error),
    ("too-many-cores", "More cores used than the cable offers", Severity::Warning),
    ("core-twice", "Core used by two wires", Severity::Warning),
    ("no-core-count", "Cables without a core count", Severity::Info),
    ("orphan-reference", "Cable reference with no other end", Severity::Warning),
    ("accessory-orphan", "Accessory at a position the strip lacks", Severity::Warning),
    ("no-accessory-part", "Strip accessories without a part number", Severity::Info),
    ("link-across-partition", "Link crosses a partition", Severity::Warning),
    ("link-orphan", "Link names positions the strip lacks", Severity::Warning),
    ("link-on-non-strip", "Link on a device that is not a strip", Severity::Warning),
    ("table-over-title", "Contact table over the title block", Severity::Warning),
    ("continuation-clash", "Continuation sheet has another page's number", Severity::Warning),
    ("level-twice", "Terminal level drawn twice", Severity::Error),
    ("position-mixed", "Position mixes terminal types", Severity::Warning),
    ("level-missing", "Link names a level the position lacks", Severity::Warning),
    ("fuse-device", "Fuse terminals that are devices of their own", Severity::Info),
    ("no-fuse-rating", "Fuse positions without a rating", Severity::Info),
    ("unknown-colour", "Colour code not defined (drawn in ink); define it in Standards", Severity::Info),
    ("dashes-left-out", "Two-colour wires drawn in one colour (too many dashes on the page)", Severity::Warning),
    ("slot-twice", "Two cards in one slot", Severity::Error),
    ("rack-missing", "Card names a rack that is not in the project", Severity::Warning),
    ("slot-range", "Card in a slot its rack does not have", Severity::Warning),
    ("unracked", "PLC cards in no rack", Severity::Info),
    ("channel-twice", "Channel drawn twice", Severity::Error),
    ("channel-over", "Channel beyond the card's count", Severity::Warning),
    ("channel-type", "Channel drawn as another card type", Severity::Warning),
    ("address-overlap", "Address blocks overlap", Severity::Warning),
    ("address-twice", "Address given twice", Severity::Warning),
    ("address-overflow", "Address block past byte 65535", Severity::Warning),
    ("no-address", "Used channels with no address", Severity::Info),
    ("wire-through-card", "Wire runs through a PLC card", Severity::Warning),
    ("plc-text-wide", "PLC texts wider than their cells", Severity::Info),
    ("connector-pin-twice", "Connector pin drawn twice on one half", Severity::Error),
    ("connector-family", "Connector halves of two families", Severity::Warning),
    ("connector-size", "Connector halves of two sizes", Severity::Warning),
    ("connector-lone-pin", "Connector pins without a mate", Severity::Info),
    ("connector-over", "Connector pin beyond its pin count", Severity::Info),
    ("item-no-part-number", "Items without a part number", Severity::Info),
    ("fuse-no-insert", "Fuses without an insert", Severity::Info),
    ("article-conflict", "One part number, two article numbers", Severity::Warning),
    ("part-for-missing", "Part line names a fuse or position the device lacks", Severity::Warning),
    ("bad-part-key", "Part line or item value ECAD does not read", Severity::Info),
    ("style-unknown", "Style not found; drawn without it", Severity::Info),
    ("ignored-old-setting", "Set in an older build, now ignored: set it in Standards", Severity::Info),
    ("style-limits", "Style records past the limits are ignored", Severity::Info),
    ("category-unknown", "Wire category not found; drawn without one", Severity::Info),
    ("category-potential-twice", "Two wire categories are the default for one potential", Severity::Warning),
    ("wire-allowance", "Wire length not set (set an allowance in Standards)", Severity::Info),
    ("page-type-unknown", "Page type not found; drawn as its usual type", Severity::Warning),
    ("page-type-field", "Page type value not read; its usual value is used", Severity::Warning),
    ("folder-start-unknown", "Folder start number not read", Severity::Info),
    ("folder-runs-overlap", "A folder's page numbers run into another folder's", Severity::Warning),
    ("cover-legend-cut", "The cover's symbol legend does not show every symbol", Severity::Info),
    ("cover-notes-cut", "The cover's notes are past 40 notes of 400 characters; the rest are not shown", Severity::Warning),
    ("rating-under", "Wire or cable rated below its protective device", Severity::Error),
    ("rating-too-hot", "Conductor at or above its maximum temperature", Severity::Error),
    ("rating-load-over", "Load above the protective device's rating", Severity::Warning),
    ("voltdrop-over", "Cable volt drop over its limit", Severity::Warning),
    ("rating-bundling-unset", "Panel wires with no bundling factor", Severity::Warning),
    ("rating-device-missing", "Protected by names no device", Severity::Warning),
    ("rating-no-device", "Rated wires and cables with no protective device", Severity::Info),
    ("rating-two-devices", "Fed by two protective devices", Severity::Info),
    ("rating-device-unread", "Device rating has no readable current", Severity::Info),
    ("rating-named-device", "Protected by a device that is not a protective kind", Severity::Info),
    ("rating-no-data", "Protected wires with no rating data", Severity::Info),
    ("rating-no-table", "No rating table value for this subject", Severity::Info),
    ("rating-table-bad-row", "Rating table rows not read", Severity::Info),
    ("rating-table-odd", "Rating table values out of order", Severity::Info),
    ("rating-assumed", "Cables with no cable type (values assumed)", Severity::Info),
    ("rating-no-current", "Cables with no load and no device (no volt drop)", Severity::Info),
    ("rating-bad-value", "Rating value ECAD does not read", Severity::Info),
    ("core-open", "Cable core open at an end (tick Not used for a spare)", Severity::Error),
    ("title-value-cut", "A value is cut in the title block", Severity::Info),
    ("title-binding-unknown", "Title block fields this build cannot fill", Severity::Info),
    ("frame-sheet-differs", "Page sheet differs from its frame's; the frame wins", Severity::Warning),
    ("cable-not-drawn", "Cable not in the drawing yet", Severity::Info),
    ("cable-end-off-plan", "Cable end drawn away from its plan", Severity::Warning),
    ("no-gland", "Cable end with no gland size", Severity::Info),
    ("cable-bad-value", "Cable value ECAD does not read", Severity::Warning),
    ("plc-tag-twice", "PLC tag given twice in one PLC", Severity::Error),
    ("plc-tag-chars", "PLC tag an export leaves out", Severity::Warning),
];

/// Brief 060 (R7): `Kind::ALL` holds every kind once, each name parses back to its kind, and the
/// names, words and severities kinds had at 060 are unchanged.
#[test]
fn every_check_kind_is_in_the_table_once_and_keeps_its_words() {
    let mut names = std::collections::BTreeSet::new();
    let mut words = std::collections::BTreeSet::new();
    for k in Kind::ALL {
        assert!(names.insert(k.name()), "{k:?}: name {} given twice", k.name());
        assert!(words.insert(k.words()), "{k:?}: words {:?} given twice", k.words());
        assert_eq!(Kind::parse(k.name()), Some(k), "{k:?}");
        assert!(!k.words().is_empty(), "{k:?}");
    }
    assert!(Kind::ALL.windows(2).all(|w| w[0] < w[1]), "Kind::ALL is in declaration order");
    for (name, words, severity) in KINDS_AT_060 {
        let k = Kind::parse(name).unwrap_or_else(|| panic!("kind {name} is gone: persisted names never change"));
        assert_eq!((k.words(), k.severity()), (*words, *severity), "{name}");
    }
    assert_eq!(KINDS_AT_060.len(), 110);
}

/// Brief 060 (R7): each whole-project check source keeps its rows under a key of its own.
#[test]
fn every_project_check_source_has_a_key_of_its_own() {
    let keys: std::collections::BTreeSet<_> = ecad_cadcraft::checks::PROJECT_SOURCES.iter().map(|s| s.key()).collect();
    assert_eq!(keys.len(), ecad_cadcraft::checks::PROJECT_SOURCES.len());
}
