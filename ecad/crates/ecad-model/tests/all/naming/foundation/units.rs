//! Design 072 §9.3: the unit types store as bare numbers, convert angles in one place and add up
//! only where it means something.

use ecad_model::LengthUnit;
use ecad_model::geometry::units::{Amps, Current, Degrees, Mm, Mm2, Radians, RatedVoltage, Volts};

#[test]
fn units_store_as_bare_numbers() -> Result<(), serde_json::Error> {
    assert_eq!(serde_json::to_string(&Mm(2.5))?, "2.5");
    assert_eq!(serde_json::from_str::<Amps>("16")?, Amps(16.0));
    let v = RatedVoltage { volts: Volts(24.0), current: Current::Dc };
    assert_eq!(serde_json::to_string(&v)?, r#"{"volts":24.0,"current":"dc"}"#);
    assert_eq!(serde_json::from_str::<RatedVoltage>(r#"{"volts":24.0,"current":"dc"}"#)?, v);
    assert!(serde_json::from_str::<RatedVoltage>(r#"{"volts":24.0,"current":"ac/dc"}"#).is_err());
    Ok(())
}

#[test]
fn units_display_with_their_symbol() {
    let table: &[(String, &str)] = &[
        (Amps(10.0).to_string(), "10 A"),
        (Mm2(1.5).to_string(), "1.5 mm²"),
        (Mm(0.5).to_string(), "0.5 mm"),
        (RatedVoltage { volts: Volts(230.0), current: Current::Ac }.to_string(), "230 V AC"),
    ];
    for (got, want) in table {
        assert_eq!(got, want);
    }
}

#[test]
fn angles_convert_in_one_place() {
    let r: Radians = Degrees(180.0).into();
    assert!((r.0 - std::f64::consts::PI).abs() < 1e-12);
    let d: Degrees = Radians(std::f64::consts::FRAC_PI_2).into();
    assert!((d.0 - 90.0).abs() < 1e-12);
}

#[test]
fn lengths_add_and_scale() {
    assert_eq!(Mm(2.0) + Mm(3.0), Mm(5.0));
    assert_eq!(Mm(2.0) - Mm(3.0), Mm(-1.0));
    assert_eq!(Mm(2.0) * 1.5, Mm(3.0));
    assert!((Mm(25.4).to_units(LengthUnit::Inch) - 1.0).abs() < 1e-12);
    assert_eq!(Mm(7.0).to_units(LengthUnit::Millimetre), 7.0);
    assert!(Amps(6.0) < Amps(10.0));
}
