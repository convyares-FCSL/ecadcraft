//! Rules of `terminal_strips` (moved from its unit tests): a document or text in, a result out.

use ecad_electrical::terminal_strips::cores::*;
use ecad_electrical::terminal_strips::*;
use ecad_model::Core;
use ecad_model::*;

#[test]
fn offers_read_and_refuse_with_a_way_forward() {
    let pos = |t: &str| parse_offer(t).map(|o| (o.positions, o.pe));
    assert_eq!(pos("12"), Ok((12, 0)));
    assert_eq!(pos(" 12 + 2pe "), Ok((14, 2)), "12 feed-through and 2 PE");
    assert_eq!(pos("0+2PE"), Ok((2, 2)));
    assert_eq!(pos("12+2PE+4x3"), Ok((18, 2)), "brief 036: type codes");
    assert!(parse_offer("990+10PE").is_err(), "999 in all");
    assert_eq!(offer_value(Some("12 + 2 PE")), Ok(Some("12+2PE".into())));
    assert_eq!(offer_value(Some("  ")), Ok(None));
    assert!(parse_offer("12X").unwrap_err().to_string().contains("Write 12, 12+2PE or 12+4x3"));
    assert!(parse_offer("1000").is_err());
    assert!(parse_offer("0").is_err());
    assert_eq!(parse_offer("2+3PE").map(Offer::text), Ok("2+3PE".into()));
    assert!(parse_offer(&"9".repeat(10_000)).is_err());
}

#[test]
fn core_offers_from_cores_and_from_type() {
    assert_eq!(parse_cores("3").unwrap(), ["1", "2", "3"]);
    assert_eq!(parse_cores("3g").unwrap(), ["1", "2", GNYE]);
    assert_eq!(parse_cores("3X").unwrap(), ["1", "2", "3"]);
    assert_eq!(parse_cores("BN, BK ,GNYE").unwrap(), ["BN", "BK", "GNYE"]);
    assert!(parse_cores("7Q").unwrap_err().to_string().contains("Write 7, 7G, 5X, 4x2x0.25 (pairs)"));
    assert!(parse_cores("1000").is_err());
    assert!(parse_cores("BN,BN").is_err());
    assert!(parse_cores("BN,,BK").is_err());
    assert_eq!(cores_value(Some("7g")), Ok(Some("7G".into())));
    assert_eq!(offer_from_type("YSLY-JZ 7G1.5").as_deref(), Some("7G"));
    assert_eq!(offer_from_type("H07RN-F"), None);
    assert_eq!(offer_from_type("12G"), None);
}

/// Owner decisions 2026-11-12: the core notations of a cable type.
#[test]
fn cable_type_notations() {
    let cores = |t: &str| {
        let n = type_notation(t).unwrap_or_else(|| panic!("no count in {t}"));
        (n.cores(), n.labels().iter().filter(|l| *l == GNYE).count(), n.group)
    };
    assert_eq!(cores("12G0.5"), (12, 1, 1));
    assert_eq!(cores("12X0.5"), (12, 0, 1));
    assert_eq!(cores("4(2x0.25)"), (8, 0, 2), "4 pairs, never 2 cores");
    assert_eq!(cores("4x2x0.25"), (8, 0, 2));
    assert_eq!(cores("4 x 2 x 0.25"), (8, 0, 2));
    assert_eq!(cores("3x1.5"), (3, 0, 1));
    assert_eq!(cores("3x2.5"), (3, 0, 1), "2.5 mm² is no pair");
    assert_eq!(cores("LiYCY (TP) 4x2x0.25"), (8, 0, 2));
    assert_eq!(cores("YSLY-JZ 7G1.5"), (7, 1, 1));
    assert_eq!(cores("YSLY-OZ 7X1.5"), (7, 0, 1));
    assert_eq!(cores("YSLY-JZ 7x1.5"), (7, 1, 1), "-JZ means G");
    assert_eq!(cores("YSLY-OZ 7x1.5"), (7, 0, 1), "-OZ means X");
    assert_eq!(cores("LiYY 3(3x0.5)"), (9, 0, 3));
    let pairs = type_notation("4(2x0.25)").map(Notation::labels).unwrap_or_default();
    assert_eq!(pairs.get(..3), Some(&["1a".to_string(), "1b".to_string(), "2a".to_string()][..]));
    assert_eq!(offer_from_type("4x2x0.25").as_deref(), Some("4x2"));
    assert_eq!(parse_cores("4(2x0.25)").map(|v| v.len()), Ok(8));
    assert_eq!(cores_value(Some(" 4(2x0.25) ")), Ok(Some("4(2x0.25)".into())));
    assert!(type_notation(&"9".repeat(10_000)).is_none());
    assert!(type_notation("60x2x0.5").is_none(), "120 cores is past the cap");
}

#[test]
fn stored_cores_meet_the_offer_by_label() {
    let m = |core: Option<&str>| Core::on_wire(WireId(1), core.and_then(|c| ecad_model::CoreLabel::new(c).ok()));
    let free = |core: &str| Core::unplaced(ecad_model::CoreLabel::new(core).ok());
    let offer = |t: &str| Some((parse_cores(t).unwrap(), OfferSource::Cores, t.to_string()));
    let c = count_cores(offer("7G"), &[m(Some("1")), m(Some("2")), m(Some("3")), m(Some("4")), m(Some("5")), free("6"), free(GNYE)]);
    assert_eq!(c.used(), 5);
    assert_eq!(c.spare, ["6", GNYE]);
    assert_eq!(c.spare_text().as_deref(), Some("2 (6, GNYE)"));
    assert_eq!(c.matched[2].as_deref(), Some("3"));
    assert_eq!(c.matched[5], None, "an unplaced core is on no line");
    // No order-based matching: an unlabelled placed core is over, never given a free label.
    let c = count_cores(offer("3"), &[m(Some("1")), m(Some("1")), m(None), free("2")]);
    assert_eq!(c.twice, [1]);
    assert_eq!(c.over, [1, 2]);
    assert_eq!(c.spare, ["2"]);
    let c = count_cores(None, &[m(None), m(None)]);
    assert_eq!(c.used(), 2);
    assert_eq!(c.spare_text(), None);
}
