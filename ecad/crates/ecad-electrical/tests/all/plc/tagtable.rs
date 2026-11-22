//! Rules of `plc::tagtable` (moved from its unit tests): a document or text in, a result out.

use ecad_electrical::plc::tagtable::*;

#[test]
fn addresses_normalise() {
    for (raw, want) in [
        ("%Q2.2", "Q2.2"),
        ("%IX0.0", "I0.0"),
        ("E0.1", "I0.1"),
        ("A3.0", "Q3.0"),
        ("%IW64", "IW64"),
        ("PEW256", "IW256"),
        ("%Q 2.2 :P", "Q2.2"),
        ("q2.2", "Q2.2"),
    ] {
        assert_eq!(normalise(raw), Address::Io(want.into()), "{raw}");
    }
    for raw in ["%M0.0", "DB1.DBX0.0", "T1", "", "MW10"] {
        assert_eq!(normalise(raw), Address::NotIo, "{raw}");
    }
    for raw in ["%Q99999.9", "%Q2.9", "%Q-1.0", "%Q2", "%IW", "%Q2.2.2"] {
        assert_eq!(normalise(raw), Address::Bad, "{raw}");
    }
}

#[test]
fn csv_round_trips() {
    let rows = vec![
        TagRow { name: "Valve, \"inlet\"".into(), address: "Q2.2".into(), data_type: "Bool".into(), comment: "Line\nbreak".into(), extra: None },
        TagRow { name: "Level".into(), address: "IW64".into(), data_type: "Int".into(), comment: String::new(), extra: None },
    ];
    let bytes = GenericCsv.write(&rows).unwrap_or_default();
    assert!(GenericCsv.sniff(&bytes));
    let back = GenericCsv.read(&bytes, &Caps::default()).unwrap_or_default();
    let names: Vec<&str> = back.rows.iter().map(|r| r.name.as_str()).collect();
    assert_eq!(names, ["Valve, \"inlet\"", "Level"]);
    assert_eq!(back.rows.first().map(|r| r.comment.as_str()), Some("Linebreak"));
}

#[test]
fn formulas_are_written_as_text_and_read_back() {
    let rows =
        vec![TagRow { name: "Pump".into(), address: "Q0.0".into(), data_type: "Bool".into(), comment: "=HYPERLINK(\"x\")".into(), extra: None }];
    let bytes = GenericCsv.write(&rows).unwrap_or_default();
    let text = String::from_utf8(bytes.clone()).unwrap_or_default();
    assert!(text.contains("\"'=HYPERLINK(\"\"x\"\")\""), "{text}");
    let back = GenericCsv.read(&bytes, &Caps::default()).unwrap_or_default();
    assert_eq!(back.rows.first().map(|r| r.comment.as_str()), Some("=HYPERLINK(\"x\")"));
    for f in ["+1", "-1", "@SUM(A1)", "\u{ff20}x"] {
        assert!(neutral(f).starts_with('\''), "{f}");
    }
    assert_eq!(neutral("Pump -1"), "Pump -1");
}

#[test]
fn a_missing_column_is_named() {
    let e = GenericCsv.read(b"name,type,comment\nA,Bool,x\n", &Caps::default()).unwrap_err();
    assert_eq!(e.to_string(), "The file has no \"address\" column.");
    assert!(GenericCsv.sniff(b"name,type,comment\n"));
    let e = GenericCsv.read(b"address,type\n%Q0.0,Bool\n", &Caps::default()).unwrap_err();
    assert_eq!(e.to_string(), "The file has no \"name\" column.");
}

#[test]
fn semicolons_and_a_bom_read() {
    let t = "\u{feff}Name;Address;Type;Comment\r\nA;%Q2.2;Bool;x\r\nB;%M0.0;Bool;\r\n";
    let r = GenericCsv.read(t.as_bytes(), &Caps::default()).unwrap_or_default();
    assert_eq!(r.rows.len(), 1);
    assert_eq!(r.not_io, 1);
}

#[test]
fn unbalanced_quotes_and_garbage_never_panic() {
    let cases: Vec<Vec<u8>> = vec![
        b"name,address\n\"open,%Q0.0\n".to_vec(),
        vec![0, 159, 146, 150],
        b"".to_vec(),
        b"PK\x03\x04garbage".to_vec(),
        ",,,,,,,,\n".repeat(10).into_bytes(),
    ];
    for c in cases {
        let _ = GenericCsv.read(&c, &Caps::default());
        let _ = GenericCsv.sniff(&c);
    }
}
