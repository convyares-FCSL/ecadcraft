//! What a device does, as calculations on its typed data (design 072 §2.4): [`Supply`], [`Load`],
//! [`Protects`], [`Senses`], [`Signals`] and [`Contacts`], and [`DeviceKind`], which gives a
//! [`DeviceType`]'s view as each of them.
//!
//! # Semantics
//! - Every answer is read or worked out from the device's typed fields (`Device.device_type`),
//!   never from its symbols, its properties or its rating text. A value the fields do not give is
//!   `None`, never a guess: a supply's power is its watts, else V × A of its output; its current
//!   its amps, else W ÷ V. A transformer's VA is not W, so it gives a current (VA ÷ V) but no
//!   power.
//! - A load's demand is its own rated power and current. A current is worked out from a power only
//!   when nothing is assumed: at a DC voltage (P ÷ V), or for a motor whose power factor and
//!   efficiency are typed.
//! - [`DeviceKind`]'s views are exhaustive over the groups: a new group must say what it is.
//!
//! - [`Contacts`] is every group that physically has contacts: switching devices, breakers, RCDs
//!   and RCBOs (their auxiliaries) and switching sensors. Their offer is a typed [`ContactSet`].
//!
//! This module excludes the circuit side (the rating checks' `Circuit`, lane D), trip curves,
//! numbering an offer's poles (`crate::xref`), connection points (`Connects`, step 4) and PLC addresses (lane C).
#![deny(clippy::wildcard_enum_match_arm)]

use ecad_model::DeviceType;
use ecad_model::device::{
    Breaker, ContactSet, ContactTable, Indicator, IndicatorColour, Motor, Output, Phases, PowerSupply, Protection, ProtectionKind, Rcbo, Rcd,
    Sensing, Sensor, SignalType, Switching,
};
use ecad_model::units::{Amps, Current, RatedVoltage, Watts};

/// A device that supplies a circuit: a power supply or a transformer.
pub trait Supply {
    /// Its output voltage, when typed.
    fn output(&self) -> Option<RatedVoltage>;
    /// The real power it can deliver.
    fn available_power(&self) -> Option<Watts>;
    /// The current it can deliver.
    fn available_current(&self) -> Option<Amps>;
}

/// What a load draws.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct Demand {
    /// Its rated (real) power.
    pub power: Option<Watts>,
    /// Its current.
    pub current: Option<Amps>,
}

/// A device that draws power: a consumer, a motor, an indicator, a coil.
pub trait Load {
    /// What it draws when supplied at `at`; `None` when nothing is typed.
    fn demand(&self, at: RatedVoltage) -> Option<Demand>;
}

/// An overcurrent protective device (an RCD is protective, but not against overcurrent).
pub trait Protects {
    /// Its rated current In (a fuse bank's is per output: [`Protection::rated_current`]).
    fn rated_current(&self) -> Option<Amps>;
    /// Whether it protects against overcurrent: every kind except an RCD.
    fn overcurrent(&self) -> bool;
}

/// A sensor.
pub trait Senses {
    /// What it senses.
    fn sensing(&self) -> Sensing;
    /// The signal it gives.
    fn output(&self) -> Output;
    /// Its supply voltage, for a powered output.
    fn supply(&self) -> Option<RatedVoltage>;
}

/// An indicator.
pub trait Signals {
    /// How it signals.
    fn signal(&self) -> SignalType;
    /// Its colour, when typed.
    fn colour(&self) -> Option<IndicatorColour>;
}

/// A device type's views as each trait (design 072 §2.4).
///
/// # Example
/// ```
/// use ecad_electrical::traits::{DeviceKind, Supply};
/// use ecad_model::DeviceType;
/// use ecad_model::device::{PowerSupply, Psu};
/// use ecad_model::units::{Amps, Current, RatedVoltage, Volts, Watts};
/// let dc24 = RatedVoltage { volts: Volts(24.0), current: Current::Dc };
/// let t = DeviceType::PowerSupply(PowerSupply::Psu(Psu { output: Some(dc24), output_current: Some(Amps(5.0)), ..Psu::default() }));
/// assert_eq!(t.as_supply().and_then(|s| s.available_power()), Some(Watts(120.0)));
/// assert!(t.as_load().is_none());
/// ```
pub trait DeviceKind {
    fn as_supply(&self) -> Option<&dyn Supply>;
    fn as_load(&self) -> Option<&dyn Load>;
    fn as_protects(&self) -> Option<&dyn Protects>;
    fn as_senses(&self) -> Option<&dyn Senses>;
    fn as_signals(&self) -> Option<&dyn Signals>;
    fn as_contacts(&self) -> Option<&dyn Contacts>;
    fn as_contacts_mut(&mut self) -> Option<&mut dyn Contacts>;
}

/// A device that has contacts (design 025 FP1, design 072 §2.4): what the part offers.
pub trait Contacts {
    /// The contacts it offers; empty when none are typed.
    fn contacts(&self) -> &ContactSet;
    /// The contacts it offers, to change.
    fn contacts_mut(&mut self) -> &mut ContactSet;
    /// Where its contact table goes, when it has a table of its own (a switching device's); `None`
    /// takes the project's default.
    fn contact_table(&self) -> Option<ContactTable> {
        None
    }
}

impl Contacts for Switching {
    fn contacts(&self) -> &ContactSet {
        &self.contacts
    }
    fn contacts_mut(&mut self) -> &mut ContactSet {
        &mut self.contacts
    }
    fn contact_table(&self) -> Option<ContactTable> {
        self.contact_table
    }
}

macro_rules! contacts_field {
    ($($t:ty),*) => {$(
        impl Contacts for $t {
            fn contacts(&self) -> &ContactSet {
                &self.contacts
            }
            fn contacts_mut(&mut self) -> &mut ContactSet {
                &mut self.contacts
            }
        }
    )*};
}
contacts_field!(Breaker, Rcd, Rcbo, Sensor);

impl DeviceKind for DeviceType {
    fn as_supply(&self) -> Option<&dyn Supply> {
        match self {
            DeviceType::PowerSupply(p) => Some(p),
            DeviceType::Protection(_)
            | DeviceType::Switching(_)
            | DeviceType::Sensor(_)
            | DeviceType::Motor(_)
            | DeviceType::Load(_)
            | DeviceType::Indicator(_)
            | DeviceType::TerminalStrip(_)
            | DeviceType::Connector(_)
            | DeviceType::PlcRack(_)
            | DeviceType::PlcModule(_)
            | DeviceType::General => None,
        }
    }

    fn as_load(&self) -> Option<&dyn Load> {
        match self {
            DeviceType::Motor(m) => Some(m),
            DeviceType::Load(l) => Some(l),
            DeviceType::Indicator(i) => Some(i),
            DeviceType::Switching(s) => Some(s),
            DeviceType::PowerSupply(_)
            | DeviceType::Protection(_)
            | DeviceType::Sensor(_)
            | DeviceType::TerminalStrip(_)
            | DeviceType::Connector(_)
            | DeviceType::PlcRack(_)
            | DeviceType::PlcModule(_)
            | DeviceType::General => None,
        }
    }

    fn as_protects(&self) -> Option<&dyn Protects> {
        match self {
            DeviceType::Protection(p) => Some(p),
            DeviceType::PowerSupply(_)
            | DeviceType::Switching(_)
            | DeviceType::Sensor(_)
            | DeviceType::Motor(_)
            | DeviceType::Load(_)
            | DeviceType::Indicator(_)
            | DeviceType::TerminalStrip(_)
            | DeviceType::Connector(_)
            | DeviceType::PlcRack(_)
            | DeviceType::PlcModule(_)
            | DeviceType::General => None,
        }
    }

    fn as_senses(&self) -> Option<&dyn Senses> {
        match self {
            DeviceType::Sensor(s) => Some(s),
            DeviceType::PowerSupply(_)
            | DeviceType::Protection(_)
            | DeviceType::Switching(_)
            | DeviceType::Motor(_)
            | DeviceType::Load(_)
            | DeviceType::Indicator(_)
            | DeviceType::TerminalStrip(_)
            | DeviceType::Connector(_)
            | DeviceType::PlcRack(_)
            | DeviceType::PlcModule(_)
            | DeviceType::General => None,
        }
    }

    fn as_signals(&self) -> Option<&dyn Signals> {
        match self {
            DeviceType::Indicator(i) => Some(i),
            DeviceType::PowerSupply(_)
            | DeviceType::Protection(_)
            | DeviceType::Switching(_)
            | DeviceType::Sensor(_)
            | DeviceType::Motor(_)
            | DeviceType::Load(_)
            | DeviceType::TerminalStrip(_)
            | DeviceType::Connector(_)
            | DeviceType::PlcRack(_)
            | DeviceType::PlcModule(_)
            | DeviceType::General => None,
        }
    }

    fn as_contacts(&self) -> Option<&dyn Contacts> {
        match self {
            DeviceType::Switching(s) => Some(s),
            DeviceType::Sensor(s) => Some(s),
            DeviceType::Protection(Protection::Mcb(b)) => Some(b),
            DeviceType::Protection(Protection::Rcd(r)) => Some(r),
            DeviceType::Protection(Protection::Rcbo(r)) => Some(r),
            DeviceType::Protection(Protection::Fuse(_) | Protection::FuseTerminal(_) | Protection::FuseBank(_))
            | DeviceType::PowerSupply(_)
            | DeviceType::Motor(_)
            | DeviceType::Load(_)
            | DeviceType::Indicator(_)
            | DeviceType::TerminalStrip(_)
            | DeviceType::Connector(_)
            | DeviceType::PlcRack(_)
            | DeviceType::PlcModule(_)
            | DeviceType::General => None,
        }
    }

    fn as_contacts_mut(&mut self) -> Option<&mut dyn Contacts> {
        match self {
            DeviceType::Switching(s) => Some(s),
            DeviceType::Sensor(s) => Some(s),
            DeviceType::Protection(Protection::Mcb(b)) => Some(b),
            DeviceType::Protection(Protection::Rcd(r)) => Some(r),
            DeviceType::Protection(Protection::Rcbo(r)) => Some(r),
            DeviceType::Protection(Protection::Fuse(_) | Protection::FuseTerminal(_) | Protection::FuseBank(_))
            | DeviceType::PowerSupply(_)
            | DeviceType::Motor(_)
            | DeviceType::Load(_)
            | DeviceType::Indicator(_)
            | DeviceType::TerminalStrip(_)
            | DeviceType::Connector(_)
            | DeviceType::PlcRack(_)
            | DeviceType::PlcModule(_)
            | DeviceType::General => None,
        }
    }
}

/// `a ÷ b` when `b` is a usable divisor.
fn over(a: f64, b: f64) -> Option<f64> {
    (b.is_finite() && b > 0.0).then(|| a / b)
}

impl Supply for PowerSupply {
    fn output(&self) -> Option<RatedVoltage> {
        match self {
            PowerSupply::Psu(p) => p.output,
            PowerSupply::Transformer(t) => t.output,
        }
    }

    fn available_power(&self) -> Option<Watts> {
        match self {
            PowerSupply::Psu(p) => p.output_power.or_else(|| Some(Watts(p.output?.volts.0 * p.output_current?.0))),
            PowerSupply::Transformer(_) => None,
        }
    }

    fn available_current(&self) -> Option<Amps> {
        match self {
            PowerSupply::Psu(p) => p.output_current.or_else(|| over(p.output_power?.0, p.output?.volts.0).map(Amps)),
            PowerSupply::Transformer(t) => over(t.rated_power?.0, t.output?.volts.0).map(Amps),
        }
    }
}

/// A demand from a typed power and current, `None` when neither is; a DC current from the power.
fn demand(power: Option<Watts>, current: Option<Amps>, at: RatedVoltage) -> Option<Demand> {
    let dc = || match at.current {
        Current::Dc => over(power?.0, at.volts.0).map(Amps),
        Current::Ac => None,
    };
    let current = current.or_else(dc);
    (power.is_some() || current.is_some()).then_some(Demand { power, current })
}

impl Load for ecad_model::device::Load {
    fn demand(&self, at: RatedVoltage) -> Option<Demand> {
        demand(self.rated_power, None, at)
    }
}

impl Load for Indicator {
    fn demand(&self, at: RatedVoltage) -> Option<Demand> {
        demand(self.power, None, at)
    }
}

impl Load for Switching {
    fn demand(&self, at: RatedVoltage) -> Option<Demand> {
        demand(self.control().and_then(|c| c.power), None, at)
    }
}

impl Load for Motor {
    /// A motor's current is its rated current, else worked out from its shaft power, power factor
    /// and efficiency at `at` (√3 for three phases).
    fn demand(&self, at: RatedVoltage) -> Option<Demand> {
        let worked = || {
            let input = self.rated_power?.0 / (self.efficiency?.0 / 100.0);
            let phases = match self.phases {
                Phases::Single => 1.0,
                Phases::Three => 3f64.sqrt(),
            };
            over(input, phases * at.volts.0 * self.power_factor?).map(Amps)
        };
        demand(self.rated_power, self.rated_current.or_else(worked), at)
    }
}

impl Protects for Protection {
    fn rated_current(&self) -> Option<Amps> {
        Protection::rated_current(self, None)
    }

    fn overcurrent(&self) -> bool {
        match self.kind() {
            ProtectionKind::Rcd => false,
            ProtectionKind::Fuse | ProtectionKind::FuseTerminal | ProtectionKind::Mcb | ProtectionKind::Rcbo | ProtectionKind::FuseBank => true,
        }
    }
}

impl Senses for Sensor {
    fn sensing(&self) -> Sensing {
        self.sensing
    }

    fn output(&self) -> Output {
        self.output
    }

    fn supply(&self) -> Option<RatedVoltage> {
        self.supply
    }
}

impl Signals for Indicator {
    fn signal(&self) -> SignalType {
        self.signal
    }

    fn colour(&self) -> Option<IndicatorColour> {
        self.colour
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ecad_model::device::{Control, Operator, Psu, Transformer};
    use ecad_model::units::{Percent, VoltAmps, Volts};

    fn v(volts: f64, current: Current) -> RatedVoltage {
        RatedVoltage { volts: Volts(volts), current }
    }

    #[test]
    fn a_supply_gives_what_its_fields_give_and_no_more() {
        let dc24 = Some(v(24.0, Current::Dc));
        let psu = |output, output_current, output_power| PowerSupply::Psu(Psu { input: None, output, output_current, output_power });
        let rows: [(PowerSupply, Option<Watts>, Option<Amps>); 6] = [
            (psu(dc24, Some(Amps(5.0)), Some(Watts(100.0))), Some(Watts(100.0)), Some(Amps(5.0))),
            (psu(dc24, Some(Amps(5.0)), None), Some(Watts(120.0)), Some(Amps(5.0))),
            (psu(dc24, None, Some(Watts(120.0))), Some(Watts(120.0)), Some(Amps(5.0))),
            (psu(None, Some(Amps(5.0)), None), None, Some(Amps(5.0))),
            (psu(dc24, None, None), None, None),
            (
                PowerSupply::Transformer(Transformer {
                    input: Some(v(400.0, Current::Ac)),
                    output: Some(v(24.0, Current::Ac)),
                    rated_power: Some(VoltAmps(240.0)),
                }),
                None,
                Some(Amps(10.0)),
            ),
        ];
        for (s, power, current) in rows {
            let t = DeviceType::PowerSupply(s.clone());
            let sup = t.as_supply().unwrap_or_else(|| panic!("{s:?}"));
            assert_eq!((sup.available_power(), sup.available_current()), (power, current), "{s:?}");
        }
    }

    #[test]
    fn a_load_draws_its_typed_power_and_a_current_only_where_nothing_is_assumed() {
        let dc24 = v(24.0, Current::Dc);
        let ac400 = v(400.0, Current::Ac);
        let lamp = DeviceType::Indicator(Indicator { power: Some(Watts(2.4)), ..Indicator::default() });
        let d = lamp.as_load().and_then(|l| l.demand(dc24)).unwrap();
        assert_eq!(d.power, Some(Watts(2.4)));
        assert!((d.current.unwrap().0 - 0.1).abs() < 1e-9);
        assert_eq!(lamp.as_load().and_then(|l| l.demand(v(230.0, Current::Ac))).and_then(|d| d.current), None);
        let coil =
            DeviceType::Switching(Switching { operator: Operator::Coil(Control { voltage: Some(dc24), power: None }), ..Switching::default() });
        assert_eq!(coil.as_load().and_then(|l| l.demand(dc24)), None);
        let motor =
            |pf, eff| Motor { phases: Phases::Three, rated_power: Some(Watts(7500.0)), power_factor: pf, efficiency: eff, ..Motor::default() };
        let m = DeviceType::Motor(motor(Some(0.85), Some(Percent(90.0))));
        let i = m.as_load().and_then(|l| l.demand(ac400)).and_then(|d| d.current).unwrap();
        assert!((i.0 - 7500.0 / 0.9 / (3f64.sqrt() * 400.0 * 0.85)).abs() < 1e-9, "{i:?}");
        let unknown = DeviceType::Motor(motor(None, Some(Percent(90.0))));
        assert_eq!(unknown.as_load().and_then(|l| l.demand(ac400)).and_then(|d| d.current), None);
        assert!(DeviceType::General.as_load().is_none());
    }

    #[test]
    fn an_rcd_protects_but_not_against_overcurrent() {
        for kind in [
            ProtectionKind::Fuse,
            ProtectionKind::FuseTerminal,
            ProtectionKind::Mcb,
            ProtectionKind::Rcd,
            ProtectionKind::Rcbo,
            ProtectionKind::FuseBank,
        ] {
            let t = DeviceType::Protection(kind.empty());
            let p = t.as_protects().unwrap();
            assert_eq!(p.overcurrent(), kind != ProtectionKind::Rcd, "{kind:?}");
            assert_eq!(p.rated_current(), None);
        }
    }
}
