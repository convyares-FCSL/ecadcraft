//! Rules of `connectors` (moved from its unit tests): a document or text in, a result out.

use ecad_electrical::connectors::*;
use ecad_electrical::symbols::{self, Side};
use ecad_model::objects::devices::ConnectorFamily;
use ecad_model::*;

#[test]
fn families_name_their_halves() {
    assert_eq!(ConnectorFamily::Dsub.half_key(Side::Plug, 9).as_deref(), Some("dsub9p"));
    assert_eq!(ConnectorFamily::M12.half_key(Side::Socket, 5).as_deref(), Some("m12s5"));
    assert_eq!(ConnectorFamily::Plain.half_key(Side::Socket, 5), None);
    assert_eq!(ConnectorFamily::of_typed("D-sub"), Some(ConnectorFamily::Dsub));
    assert_eq!(ConnectorFamily::Dsub.size_refusal(), "A D-sub has 9, 15 or 25 pins. Pick one, or draw pin by pin with Pins.");
    for f in ConnectorFamily::ALL.iter().copied() {
        for n in f.sizes() {
            for side in [Side::Socket, Side::Plug] {
                let key = f.half_key(side, *n).unwrap_or_default();
                let s = symbols::catalogue::by_key(&key).unwrap_or_else(|| panic!("{key}"));
                assert_eq!((family_of(s), size_of(s), s.side()), (Some(f), Some(*n), Some(side)), "{key}");
            }
        }
    }
}

#[test]
fn names_are_checked() {
    assert_eq!(names_for(&Names::Letters, 3), Ok(vec!["A".into(), "B".into(), "C".into()]));
    assert!(names_for(&Names::Letters, 27).is_err());
    assert_eq!(parse_list("L, N ,PE"), Ok(vec!["L".into(), "N".into(), "PE".into()]));
    assert!(parse_list("P E").is_err_and(|e| e.to_string().contains("spaces")));
    assert!(parse_list("L,L").is_err());
    assert!(parse_list(&"x,".repeat(5_000)).is_err());
    assert!(check_name("a\u{1}").is_err());
    assert!(check_name("123456789").is_err());
    assert!(names_for(&Names::List(vec!["L".into()]), 3).is_err_and(|e| e.to_string() == "Give 3 pin names; got 1."));
}
