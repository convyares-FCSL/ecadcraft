//! The rating fields' value types: each reads a number with its unit or a bare number in its own
//! unit, a voltage with AC or DC, an IP code, a choice's word or label; every group's field words
//! are unique.

use ecad_model::Word;
use ecad_model::geometry::units::{Amps, Current, MilliAmps, Percent, RatedVoltage, Volts, Watts};
use ecad_model::objects::devices::fields::{FieldError, FieldValue, put, put_after};
use ecad_model::objects::devices::{DeviceGroup, IpCode, Starting};

#[test]
fn each_type_reads_its_units_and_refuses_the_rest() {
    assert_eq!(Watts::parse("7.5 kW"), Ok(Watts(7500.0)));
    assert_eq!(Watts::parse("750"), Ok(Watts(750.0)));
    assert_eq!(Watts::parse("7.5kw"), Ok(Watts(7500.0)));
    assert_eq!(Amps::parse("500 mA"), Ok(Amps(0.5)));
    assert_eq!(MilliAmps::parse("0.03 A"), Ok(MilliAmps(30.0)));
    assert_eq!(Volts::parse("400 VAC"), Ok(Volts(400.0)));
    assert_eq!(RatedVoltage::parse("24 VDC"), Ok(RatedVoltage { volts: Volts(24.0), current: Current::Dc }));
    assert_eq!(RatedVoltage::parse("230 V ac"), Ok(RatedVoltage { volts: Volts(230.0), current: Current::Ac }));
    assert_eq!(<Starting as FieldValue>::parse("star-delta"), Ok(Starting::StarDelta));
    assert_eq!(<Starting as FieldValue>::parse("vfd"), Ok(Starting::Vfd));
    assert_eq!(f64::parse("0.85"), Ok(0.85));
    for (bad, reason) in [
        (Watts::parse("15 A"), "a power, such as 7.5 kW"),
        (Watts::parse("0"), "a power, such as 7.5 kW"),
        (Watts::parse("-3 kW"), "a power, such as 7.5 kW"),
        (Watts::parse("NaN"), "a power, such as 7.5 kW"),
    ] {
        assert_eq!(bad, Err(FieldError::Bad(reason.to_string())));
    }
    assert_eq!(RatedVoltage::parse("24 V"), Err(FieldError::Bad("a voltage with AC or DC, such as 24 V DC".to_string())));
    assert_eq!(f64::parse("1.2"), Err(FieldError::Bad("a number more than 0 and at most 1, such as 0.85".to_string())));
    assert!(<Starting as FieldValue>::parse("fast").is_err_and(|e| e == FieldError::Bad("one of direct, star_delta, soft, vfd".into())));
}

#[test]
fn an_ip_code_reads_its_two_numerals_and_refuses_the_rest() {
    for (text, shown) in [("IP65", "IP65"), ("ip 65", "IP65"), ("IPX4", "IPX4"), ("ip2x", "IP2X"), ("IP69", "IP69"), ("IP00", "IP00")] {
        let ip = <IpCode as FieldValue>::parse(text).unwrap_or_else(|e| panic!("{text}: {e:?}"));
        assert_eq!(ip.show(), shown, "{text}");
        assert_eq!(serde_json::from_str::<IpCode>(&serde_json::to_string(&ip).unwrap()).unwrap(), ip, "{text} stored");
    }
    for text in ["65", "IP", "IP7", "IP75", "IP6", "IPXX", "IP655", "IP20B", "IP-65", "IP６５", "x"] {
        assert_eq!(<IpCode as FieldValue>::parse(text), Err(FieldError::Bad("an IP code, such as IP65 or IPX4".into())), "{text}");
    }
    assert!(serde_json::from_str::<IpCode>(r#""IP75""#).is_err(), "a stored code out of range is refused");
}

#[test]
fn a_field_is_written_from_text_its_drawing_prefix_allowed_and_empty_clears_it() {
    let mut eff: Option<Percent> = None;
    put_after(&["eff"], &mut eff, Some("eff 91 %")).unwrap();
    assert_eq!(eff, Some(Percent(91.0)));
    put(&mut eff, Some("  ")).unwrap();
    assert_eq!(eff, None);
    let mut w: Option<Watts> = Some(Watts(1.0));
    assert_eq!(put(&mut w, Some("x")), Err(FieldError::Bad("a power, such as 7.5 kW".into())));
    assert_eq!(w, Some(Watts(1.0)), "a refused text leaves the field");
}

#[test]
fn every_group_declares_each_field_word_once() {
    for g in <DeviceGroup as Word>::ALL {
        let words: Vec<&str> = g.rating_fields().iter().map(|f| f.word).collect();
        let mut unique = words.clone();
        unique.sort_unstable();
        unique.dedup();
        assert_eq!(unique.len(), words.len(), "{}", g.label());
        assert!(g.rating_fields().iter().all(|f| !f.label.is_empty() && g.rating_field(f.word).is_some()));
    }
}
