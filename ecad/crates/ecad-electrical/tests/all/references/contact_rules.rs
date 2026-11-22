//! The contact grammar and ordering rules of device references (design 025 §2, FP1, FP2).

use ecad_electrical::references::devices::parse::{Placement, parse_mirror, parse_offer, table_of_typed};
use ecad_electrical::references::devices::read::table_text_key;
use ecad_electrical::references::devices::table::natural_key;
use ecad_model::Pt;
use ecad_model::objects::devices::ContactTable;

#[test]
fn the_offer_grammar() {
    let labels = |t: &str| {
        parse_offer(t).map(|v| {
            v.iter().map(|o| format!("{}/{} {}{}", o.labels[0], o.labels[1], if o.power { "P" } else { "" }, o.kind.upper())).collect::<Vec<_>>()
        })
    };
    assert_eq!(labels("3P+1NO+1NC").unwrap(), ["1/2 PNO", "3/4 PNO", "5/6 PNO", "13/14 NO", "21/22 NC"]);
    assert_eq!(labels("3p, 2no ,1nc").unwrap(), ["1/2 PNO", "3/4 PNO", "5/6 PNO", "13/14 NO", "23/24 NO", "31/32 NC"]);
    assert_eq!(labels("13/14 NO, 21/22 NC").unwrap(), ["13/14 NO", "21/22 NC"]);
    assert_eq!(labels("L1/T1 P").unwrap(), ["L1/T1 PNO"]);
    assert_eq!(labels("3P+1PNC").unwrap(), ["1/2 PNO", "3/4 PNO", "5/6 PNO", "7/8 PNC"]);
    for bad in ["3X", "", "+", "0NO", "NO", "/14 NO", "33NO+", "1NO++1NC"] {
        assert!(parse_offer(bad).is_err(), "{bad:?}");
    }
    assert_eq!(parse_offer("3X").unwrap_err().to_string(), "\"3X\" is not a contact list. Write it like 3P+1NO+1NC, or 13/14 NO, 21/22 NC.");
    assert!(parse_offer("32NO").is_ok());
    assert!(parse_offer("33NO").is_err());
    assert!(parse_offer("30NO+3NC").is_err());
    assert!(parse_offer(&"1NO+".repeat(5000)).is_err());
    assert!(parse_offer("99999999999999999999NO").is_err());
}

#[test]
fn the_mirror_values() {
    assert_eq!(parse_mirror("Below").unwrap(), Placement::Below);
    assert_eq!(parse_mirror(" at 12, -30.5 ").unwrap(), Placement::At(Pt::new(12.0, -30.5)));
    assert_eq!(parse_mirror("at 12,-30.5").unwrap().word(), "at 12,-30.5");
    for bad in ["at NaN,1", "at 1", "at inf,2", "at 99999,0", "sideways", ""] {
        assert!(parse_mirror(bad).is_err(), "{bad:?}");
    }
    assert_eq!(table_of_typed(Some("Project default (auto)")).unwrap(), None);
    assert_eq!(table_of_typed(Some("OFF")).unwrap(), Some(ContactTable::Off));
}

#[test]
fn the_table_text_keys_are_the_wide_case() {
    for k in ["text.table.style", "text.font"] {
        assert!(table_text_key(k), "{k}");
    }
    assert!(!table_text_key("text.tag.style"));
}

#[test]
fn natural_order() {
    let mut v = vec!["13", "2", "A10", "A2", "1"];
    v.sort_by_key(|s| natural_key(s));
    assert_eq!(v, ["1", "2", "13", "A2", "A10"]);
}
