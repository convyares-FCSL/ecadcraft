//! Canonical form (RFC 8785) and frame checksum contracts (proposal §20.10, §20.11).

use ecad_file::canonical::{CanonicalError, MAX_SAFE_INTEGER, canonical, es_number};
use ecad_file::codec::{Codec, FrameError, JsonLines, PREFIX_LEN};
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
    assert_eq!(
        String::from_utf8(canonical(&s).unwrap()).unwrap(),
        "\"q\\\" b\\\\ \\b\\t\\n\\f\\r \\u0001 \\u001f \u{7f} / ü\""
    );
    assert_eq!(String::from_utf8(canonical(&json!([null, true, false, []])).unwrap()).unwrap(), "[null,true,false,[]]");
}

#[test]
fn the_crc_covers_exactly_the_payload_bytes() {
    assert_eq!(ecad_file::crc32c(b""), 0);
    assert_eq!(ecad_file::crc32c(b"123456789"), 0xE306_9283);
    let v = json!({"type": "tx", "n": 1.5});
    let line = JsonLines.encode(&v).unwrap();
    assert!(line.ends_with(b"}\n"));
    assert_eq!(&line[..8], b"{\"crc\":\"");
    assert_eq!(&line[16..PREFIX_LEN], b"\",\"rec\":");
    let frame = &line[..line.len() - 1];
    let payload = JsonLines.payload(frame).unwrap();
    assert_eq!(payload, canonical(&v).unwrap().as_slice(), "the payload is the canonical form");
    let crc = u32::from_str_radix(std::str::from_utf8(&line[8..16]).unwrap(), 16).unwrap();
    assert_eq!(crc, ecad_file::crc32c(payload));
    // The whole line is valid JSON.
    let whole: Value = serde_json::from_slice(frame).unwrap();
    assert_eq!(whole["rec"], v);
    // Any changed payload byte fails the CRC; any changed frame byte fails the shape or CRC.
    for i in 0..frame.len() {
        let mut bad = frame.to_vec();
        bad[i] ^= 0x01;
        let r = JsonLines.payload(&bad);
        assert!(r.is_err(), "byte {i}");
        if i >= PREFIX_LEN && i < frame.len() - 1 {
            assert!(matches!(r, Err(FrameError::Crc { .. })), "payload byte {i}");
        }
    }
    for bad in [&b""[..], b"{}", b"{\"crc\":\"0000000G\",\"rec\":{}}", b"{\"crc\":\"00000000\",\"rec\":"] {
        assert!(matches!(JsonLines.payload(bad), Err(FrameError::Shape(_))));
    }
}
