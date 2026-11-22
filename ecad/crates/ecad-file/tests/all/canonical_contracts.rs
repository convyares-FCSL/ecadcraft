//! Canonical form (RFC 8785), checksum and known-hash contracts (proposal §20.3, §20.10, §20.11).

use ecad_file::canonical::{CanonicalError, MAX_SAFE_INTEGER, canonical, es_number};
use serde_json::{Value, json};

#[test]
fn numbers_follow_ecmascript_number_to_string() {
    let cases: &[(f64, &str)] = &[
        (0.0, "0"),
        (-0.0, "0"),
        (1.0, "1"),
        (-1.0, "-1"),
        (0.1, "0.1"),
        (4.5, "4.5"),
        (0.002, "0.002"),
        (0.000001, "0.000001"),
        (1e-6, "0.000001"),
        (1e-7, "1e-7"),
        (1.5e-7, "1.5e-7"),
        (123456.789, "123456.789"),
        (333333333.3333333, "333333333.3333333"),
        (1e20, "100000000000000000000"),
        (123456789012345680000.0, "123456789012345680000"),
        (295147905179352830000.0, "295147905179352830000"),
        (1e21, "1e+21"),
        (1e23, "1e+23"),
        (1.5e300, "1.5e+300"),
        (9007199254740991.0, "9007199254740991"),
        (9007199254740992.0, "9007199254740992"),
        (5e-324, "5e-324"),
        (-5e-324, "-5e-324"),
        (1.7976931348623157e308, "1.7976931348623157e+308"),
        (-1.7976931348623157e308, "-1.7976931348623157e+308"),
        (std::f64::consts::PI, "3.141592653589793"),
        (std::f64::consts::FRAC_PI_2, "1.5707963267948966"),
    ];
    for (x, want) in cases {
        assert_eq!(es_number(*x).unwrap(), *want, "{x:e}");
        // Shortest round-trip: the text reads back to the same double.
        assert_eq!(want.parse::<f64>().unwrap(), if *x == 0.0 { 0.0 } else { *x });
    }
    assert_eq!(es_number(f64::NAN), Err(CanonicalError::NonFinite));
    assert_eq!(es_number(f64::INFINITY), Err(CanonicalError::NonFinite));
}

/// Doubles exactly halfway between two shortest digit strings: ECMAScript picks the even last
/// digit, where a round-half-up formatter (Rust's `{:e}`) picks the odd one. The expected strings
/// are Node.js `String(x)`; a local comparison of 260,000 such ties and 600,000 random doubles
/// against Node.js found no difference.
#[test]
fn ties_between_shortest_digits_choose_the_even_digit() {
    let cases: &[(u64, &str)] = &[
        (0x42e6_f7d5_21ea_90a4, "202029508023429.12"),
        (0x4315_ef1f_5e32_4ea9, "1543473128575914.2"),
        (0x4316_0f8c_28d9_ea81, "1552386035710624.2"),
        (0x42e3_63c8_0c04_17f4, "170554231365823.62"),
        (0x427c_9004_85f4_3c80, "1962804797251.7812"),
        (0x4241_e956_4d75_6200, "153858382570.76562"),
        (0x41dd_5a41_7d1f_4000, "1969817076.4882812"),
        (0x4177_1ead_9c54_0000, "24242905.770507812"),
        (0x4107_7585_2e80_0000, "192176.64770507812"),
        (0x409f_22c1_1000_0000, "1992.6885375976562"),
    ];
    for (bits, want) in cases {
        let x = f64::from_bits(*bits);
        assert_eq!(es_number(x).unwrap(), *want, "{bits:016x}");
        assert_eq!(es_number(-x).unwrap(), format!("-{want}"), "{bits:016x} negated");
        assert_eq!(want.parse::<f64>().unwrap(), x, "{want} round-trips");
    }
}

#[test]
fn integers_must_be_exact_doubles() {
    let ok = |v: Value| String::from_utf8(canonical(&v).unwrap()).unwrap();
    assert_eq!(ok(json!(MAX_SAFE_INTEGER)), "9007199254740991");
    assert_eq!(ok(json!(9_007_199_254_740_992u64)), "9007199254740992");
    assert_eq!(ok(json!(-42)), "-42");
    assert_eq!(ok(json!(1.0)), "1");
    for bad in [json!(9_007_199_254_740_993u64), json!(u64::MAX), json!(i64::MIN + 1)] {
        assert!(matches!(canonical(&bad), Err(CanonicalError::InexactInteger(_))), "{bad}");
    }
}

#[test]
fn keys_sort_by_utf16_and_strings_escape_minimally() {
    let v = json!({"\u{1F600}": 1, "\u{FF5E}": 2, "b": 3, "a": 4, "é": 5});
    let text = String::from_utf8(canonical(&v).unwrap()).unwrap();
    // UTF-16: "a" < "b" < "é" (U+00E9) < U+1F600 (D83D…) < U+FF5E. UTF-8 order would put U+FF5E first.
    assert_eq!(text, "{\"a\":4,\"b\":3,\"é\":5,\"\u{1F600}\":1,\"\u{FF5E}\":2}");
    let s = json!("q\" b\\ \u{8}\u{9}\u{a}\u{c}\u{d} \u{1} \u{1f} \u{7f} / ü");
    assert_eq!(String::from_utf8(canonical(&s).unwrap()).unwrap(), "\"q\\\" b\\\\ \\b\\t\\n\\f\\r \\u0001 \\u001f \u{7f} / ü\"");
    assert_eq!(String::from_utf8(canonical(&json!([null, true, false, []])).unwrap()).unwrap(), "[null,true,false,[]]");
}

#[test]
fn the_crc_has_its_known_values() {
    assert_eq!(ecad_file::crc32c(b""), 0);
    assert_eq!(ecad_file::crc32c(b"123456789"), 0xE306_9283);
}

/// The record format, the canonical form and the chain, pinned (brief 069: the golden version 1
/// file used to pin them; these values were checked against it, record for record, when it was
/// removed). Existing content hashes and chain values must never change: a changed
/// record needs a schema version and an upcast, not an edit to these values.
#[test]
fn record_and_definition_hashes_are_pinned() {
    use crate::common::*;
    use ecad_file::chain::{self, ChainKind};
    let f = fixture();
    let doc = f.ledger.document();
    let hex = |h: ecad_model::ContentHash| h.hex();
    let got = [
        ("coil", hex(ecad_file::definition_hash(&coil(3.0)).unwrap())),
        ("d1", hex(ecad_file::definition_hash(&doc.definitions[&f.d1]).unwrap())),
        ("d2", hex(ecad_file::definition_hash(&doc.definitions[&f.d2]).unwrap())),
    ];
    let mut prev = chain::seed(ChainKind::Project, PROJECT.0);
    for tx in f.ledger.log() {
        let v = ecad_file::record::tx_value(tx, &prev).unwrap();
        prev = chain::tx_hash(&canonical(&v).unwrap());
    }
    let head = ("head", chain::hex(&prev));
    let want = [
        ("coil", "bb4d1a684aaf0e60e45df389ce4feab0a053c14e64bc5d223c386444b3603027"),
        ("d1", "bb4d1a684aaf0e60e45df389ce4feab0a053c14e64bc5d223c386444b3603027"),
        ("d2", "07a2ac5ece005a559bd3731a052dfe1adec94820e426a43fc0f411dbb27711b4"),
        // Design 072 F14, F20 (lanes B, C, D, E, G) and lane D F2c: device, wire, page and project records, wires drawn as routes,
        // cable cores with their position; F3′ (lane B): the representation roles; lane F F1b: a
        // connection point's owner.
        // 0.2.3 structure move: ConnectionPoint ops, `connection_points` fields, `jumper_bar` keys.
        // 0.2.3 S1 (#2): a cable's `tag`, `tag_fixed` and `description` are record fields.
        // 0.2.3 S4 (#37): `NameVersion` names a `PointName` (typed rev or version), revision records are typed.
        ("head", "ae059c1632dbe5cbea011f802298a250a748896fc6f9814f9b228b959e74241b"),
    ];
    for ((name, h), (wname, w)) in got.iter().chain(std::iter::once(&head)).zip(want) {
        assert_eq!((*name, h.as_str()), (wname, w), "a pinned hash changed");
    }
}
