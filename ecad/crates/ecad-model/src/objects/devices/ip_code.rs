//! A degree of protection by an enclosure (IEC 60529): `IP65`, `IPX4`.
//!
//! # Semantics
//! - Two characteristic numerals: against solid objects (0 to 6) and against water (0 to 9). Either
//!   may be `X`, not given, but not both.
//! - It reads `IP65`, `ip 65`, `IP6X` and `IPX4` and is written in capitals with no space (`IP65`),
//!   which is also how it is stored.
//!
//! This module excludes the additional and supplementary letters (`IP20B`, `IP65W`): a code with
//! them is refused rather than shortened.

use std::fmt;

use serde::{Deserialize, Deserializer, Serialize, Serializer};

/// The largest numeral against solid objects.
const MAX_SOLIDS: u8 = 6;
/// The largest numeral against water.
const MAX_WATER: u8 = 9;

/// A degree of protection: its numerals against solid objects and against water (`None` is `X`).
///
/// # Example
/// ```
/// use ecad_model::objects::devices::IpCode;
/// let ip = IpCode::parse("ip 65").unwrap();
/// assert_eq!(ip.to_string(), "IP65");
/// assert_eq!((ip.solids(), ip.water()), (Some(6), Some(5)));
/// assert_eq!(IpCode::parse("IPX4").unwrap().to_string(), "IPX4");
/// assert_eq!(serde_json::to_string(&ip).unwrap(), r#""IP65""#);
/// assert!(IpCode::parse("IP75").is_none() && IpCode::parse("IPXX").is_none());
/// ```
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct IpCode {
    solids: Option<u8>,
    water: Option<u8>,
}

impl IpCode {
    /// The code with these numerals (`None` is `X`), when both are in range and one is given.
    pub fn new(solids: Option<u8>, water: Option<u8>) -> Option<IpCode> {
        let ok = solids.is_none_or(|s| s <= MAX_SOLIDS) && water.is_none_or(|w| w <= MAX_WATER) && (solids.is_some() || water.is_some());
        ok.then_some(IpCode { solids, water })
    }

    /// The code `text` gives (`IP65`, `ip 65`, `IPX4`), case and spaces after `IP` aside.
    pub fn parse(text: &str) -> Option<IpCode> {
        let t = text.trim();
        let rest = t.get(..2).filter(|p| p.eq_ignore_ascii_case("ip")).and_then(|_| t.get(2..))?.trim_start();
        let mut numerals = rest.chars().map(|c| if c.eq_ignore_ascii_case(&'x') { Some(None) } else { c.to_digit(10).map(|d| Some(d as u8)) });
        let (Some(Some(solids)), Some(Some(water)), None) = (numerals.next(), numerals.next(), numerals.next()) else { return None };
        IpCode::new(solids, water)
    }

    /// The numeral against solid objects, unless it is `X`.
    pub fn solids(self) -> Option<u8> {
        self.solids
    }

    /// The numeral against water, unless it is `X`.
    pub fn water(self) -> Option<u8> {
        self.water
    }
}

impl fmt::Display for IpCode {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let numeral = |n: Option<u8>| n.map_or_else(|| "X".to_string(), |d| d.to_string());
        write!(f, "IP{}{}", numeral(self.solids), numeral(self.water))
    }
}

impl Serialize for IpCode {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        s.collect_str(self)
    }
}

impl<'de> Deserialize<'de> for IpCode {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<IpCode, D::Error> {
        let text = String::deserialize(d)?;
        IpCode::parse(&text).ok_or_else(|| serde::de::Error::custom(format!("not an IP code: {text:?}")))
    }
}
