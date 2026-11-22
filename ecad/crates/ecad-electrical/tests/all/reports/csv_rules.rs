//! The rules of a report's CSV (design 024 §10): RFC 4180 quoting and Excel-safe prefixes.

use ecad_electrical::reports::{Cell, Table, csv};

/// RFC 4180 quoting and Excel-safe prefixes, including the full-width formula characters Excel
/// also reads as formulas (`＝`, `＋`, `－`, `＠`).
#[test]
fn csv_quotes_and_neutralises_formula_starts() {
    let row = |v: &[&str]| v.iter().map(|t| Cell { text: (*t).to_string(), flag: None }).collect::<Vec<_>>();
    let t = Table {
        headers: vec!["A", "B", "C"],
        rows: vec![
            row(&["a,b", "say \"hi\"", "two\nlines"]),
            row(&["=1+1", "+CP1", "-K01"]),
            row(&["@SUM(A1)", "\tx", "\rx"]),
            row(&["plain", "", "?"]),
        ],
    };
    let out = csv(&t, true);
    assert!(out.starts_with('\u{feff}'));
    let body = out.trim_start_matches('\u{feff}');
    let lines: Vec<&str> = body.split("\r\n").collect();
    assert_eq!(lines[0], "A,B,C");
    assert_eq!(lines[1], "\"a,b\",\"say \"\"hi\"\"\",\"two\nlines\"");
    assert_eq!(lines[2], "'=1+1,'+CP1,'-K01");
    assert_eq!(lines[3], "'@SUM(A1),'\tx,\"'\rx\"");
    assert_eq!(lines[4], "plain,,?");
    // Without Excel-safe, values are written as they are.
    assert!(csv(&t, false).contains("=1+1,+CP1,-K01\r\n"));
}

#[test]
fn csv_neutralises_full_width_formula_starts() {
    let row = |v: &[&str]| v.iter().map(|t| Cell { text: (*t).to_string(), flag: None }).collect::<Vec<_>>();
    let t = Table { headers: vec!["A", "B", "C", "D"], rows: vec![row(&["＠SUM(A1)", "＝1+1", "＋CP1", "－K01"])] };
    let out = csv(&t, true);
    assert!(out.contains("'＠SUM(A1),'＝1+1,'＋CP1,'－K01\r\n"), "{out:?}");
}

/// The CSV a report writes: quoted, and safe to open in a spreadsheet.
#[test]
fn csv_is_excel_safe_and_quoted() {
    let t = Table {
        headers: vec!["Tags", "Description"],
        rows: vec![vec![cell("-K01, -K02"), cell("Say \"hi\"")], vec![cell("+CP1"), cell("=SUM(A1)")]],
    };
    let s = csv(&t, true);
    assert!(s.starts_with('\u{feff}'));
    assert!(s.contains("\"'-K01, -K02\",\"Say \"\"hi\"\"\"\r\n"));
    assert!(s.contains("'+CP1,'=SUM(A1)\r\n"));
    let plain = csv(&t, false);
    assert!(plain.contains("+CP1,=SUM(A1)\r\n"));
}

fn cell(t: &str) -> Cell {
    Cell { text: t.to_string(), flag: None }
}
