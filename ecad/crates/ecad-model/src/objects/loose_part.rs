//! Loose parts: panel-only material on the bill of materials.
//!
//! # Semantics
//! - A [`LoosePart`] is a part that belongs to no device and is not drawn: trunking, DIN rail,
//!   cable ties, crimps. It has no tag, no symbol and no connection point. It is listed on the BOM
//!   with its quantity and unit, in its location and function.
//! - A quantity ([`Qty`]) is kept in thousandths and written as a decimal text (`"3.6"`): more
//!   than 0, at most 9 999, at most three decimals, read only in the form it is written in. A count of pieces is whole; a length in metres
//!   need not be.
//! - Loose parts are kept in `Document.loose_parts` by [`crate::naming::id::LoosePartId`], made, changed
//!   and removed by `CreateLoosePart`, `ModifyLoosePart` and `DeleteLoosePart` (the whole part
//!   recorded), as drawing aids are.
//!
//! - [`Bom`] is what goes on the bill of materials: a loose part gives one
//!   [`BomLine`], a device its main part.
//!
//! This module excludes the BOM itself (ecad-electrical's outputs: grouping, locations, part lines
//! of a device, which are still properties), the commands that add and edit loose parts, and layout
//! (a loose part becomes a layout item in 0.2.4).

use std::fmt;

use serde::{Deserialize, Serialize};

use crate::Device;
use crate::naming::id::{FunctionId, LocationId};
use crate::objects::devices::PartText;

/// The most characters a loose part's description has.
pub const MAX_DESCRIPTION: usize = 200;

/// A loose part's description: one line, 1 to [`MAX_DESCRIPTION`] characters. Longer text is
/// refused, never cut.
pub type LooseDescription = crate::naming::text::Text<MAX_DESCRIPTION>;

/// The largest quantity, in thousandths (9 999): keeps sums far from overflow.
pub const MAX_QTY_MILLI: u64 = 9_999_000;

crate::word_enum! {
    /// What a loose part is counted in: pieces or metres.
    #[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
    pub enum ItemUnit {
        /// Pieces: a whole count.
        #[default]
        Pcs,
        /// Metres: a length, with up to three decimals.
        #[strum(serialize = "m")]
        Metres,
    }
}

impl ItemUnit {
    fn is_pcs(&self) -> bool {
        *self == ItemUnit::Pcs
    }
}

/// A quantity in thousandths: a count of pieces, or metres. Written as a decimal text (`"3.6"`).
///
/// # Example
/// ```
/// use ecad_model::objects::loose_part::Qty;
/// let q = Qty::parse("3.6").unwrap();
/// assert_eq!(q, Qty(3600));
/// assert_eq!(serde_json::to_string(&q).unwrap(), r#""3.6""#);
/// assert!(serde_json::from_str::<Qty>(r#""0""#).is_err());
/// ```
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Qty(pub u64);

impl Qty {
    /// One piece.
    pub const ONE: Qty = Qty(1000);

    /// Whether it is a whole number.
    pub fn whole(self) -> bool {
        self.0.is_multiple_of(1000)
    }

    /// A quantity as written: plain digits with at most three decimals, more than 0 and at most
    /// 9 999 (no sign, no exponent, so never NaN or infinity). `None` for anything else.
    pub fn parse(text: &str) -> Option<Qty> {
        let t = text.trim();
        let (w, frac) = t.split_once('.').unwrap_or((t, ""));
        let digits_or_none = |t: &str| t.is_empty() || crate::naming::words::is_digits(t);
        if (w.is_empty() && frac.is_empty()) || w.len() > 6 || frac.len() > 3 || !digits_or_none(w) || !digits_or_none(frac) {
            return None;
        }
        let whole: u64 = if w.is_empty() { 0 } else { w.parse().ok()? };
        let thousandths: u64 = if frac.is_empty() { 0 } else { format!("{frac:0<3}").parse().ok()? };
        let q = whole.checked_mul(1000)?.checked_add(thousandths)?;
        (q > 0 && q <= MAX_QTY_MILLI).then_some(Qty(q))
    }
}

impl Default for Qty {
    fn default() -> Self {
        Qty::ONE
    }
}

impl fmt::Display for Qty {
    /// `4`, `5.6`, `0.75`: at most three decimals, trailing zeros trimmed.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let (w, r) = (self.0 / 1000, self.0 % 1000);
        if r == 0 {
            return write!(f, "{w}");
        }
        let frac = format!("{r:03}");
        write!(f, "{w}.{}", frac.trim_end_matches('0'))
    }
}

impl Serialize for Qty {
    fn serialize<S: serde::Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        s.collect_str(self)
    }
}

impl<'de> Deserialize<'de> for Qty {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        let text = String::deserialize(d)?;
        // Strict: only the form it is written in (`3.6`, never ` 3.6`, `3.60` or `03.6`).
        Qty::parse(&text).filter(|q| q.to_string() == text).ok_or_else(|| {
            let shown: String = text.chars().take(20).collect();
            serde::de::Error::custom(format!(
                "\"{shown}\" is not a quantity: write a number more than 0 and at most 9999, with at most three decimals"
            ))
        })
    }
}

/// A part on the BOM that is no device (stored intent).
///
/// # Example
/// ```
/// use ecad_model::objects::loose_part::{LooseDescription, LoosePart, Qty};
/// let p = LoosePart::new(LooseDescription::new("DIN rail").unwrap(), Qty(2000));
/// assert_eq!(serde_json::to_string(&p).unwrap(), r#"{"description":"DIN rail","qty":"2"}"#);
/// assert!(serde_json::from_str::<LoosePart>(r#"{"description":"x","qty":"1","colour":"red"}"#).is_err());
/// ```
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LoosePart {
    /// What it is, in words (always given: a BOM line needs it).
    pub description: LooseDescription,
    /// Its part number.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub part_number: Option<PartText>,
    /// Its maker.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub manufacturer: Option<PartText>,
    /// Its article number (the supplier's order number).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub article_number: Option<PartText>,
    /// How many, in [`LoosePart::unit`].
    pub qty: Qty,
    /// What [`LoosePart::qty`] counts.
    #[serde(default, skip_serializing_if = "ItemUnit::is_pcs")]
    pub unit: ItemUnit,
    /// Where it goes (an entry of the project's location list).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub location: Option<LocationId>,
    /// What it is for (an entry of the project's function list).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub function: Option<FunctionId>,
}

impl LoosePart {
    /// `qty` pieces of `description`, with no part, location or function.
    pub fn new(description: LooseDescription, qty: Qty) -> LoosePart {
        LoosePart {
            description,
            part_number: None,
            manufacturer: None,
            article_number: None,
            qty,
            unit: ItemUnit::Pcs,
            location: None,
            function: None,
        }
    }

    /// Why it is not well formed, if it is not: a quantity out of range, or a count of pieces
    /// that is not whole. (Its description is never empty: [`LooseDescription`] holds no empty text.)
    pub fn problem(&self) -> Option<String> {
        if self.qty.0 == 0 || self.qty.0 > MAX_QTY_MILLI {
            return Some(format!("quantity {} is out of range (more than 0, at most 9999)", self.qty));
        }
        if self.unit == ItemUnit::Pcs && !self.qty.whole() {
            return Some(format!("quantity {} pieces is not whole", self.qty));
        }
        None
    }
}

/// One line on the bill of materials: what the part is and how many.
#[derive(Debug, Clone, PartialEq)]
pub struct BomLine {
    /// What it is, in words.
    pub description: Option<String>,
    /// Its part number.
    pub part_number: Option<String>,
    /// Its maker.
    pub manufacturer: Option<String>,
    /// The supplier's order number.
    pub article_number: Option<String>,
    /// How many, in `unit`.
    pub qty: Qty,
    /// What `qty` counts.
    pub unit: ItemUnit,
}

/// What an object puts on the bill of materials.
///
/// # Semantics
/// - A loose part gives one line: its description, part, quantity and unit.
/// - A device gives its main part, one piece, when it names a part number or a description. Its
///   part lines are still `part.<n>.*` properties until their typed move; ecad-electrical's
///   `bom::lines` reads them.
///
/// # Example
/// ```
/// use ecad_model::objects::loose_part::{Bom, LooseDescription, LoosePart, Qty};
/// let p = LoosePart::new(LooseDescription::new("Cable ties").unwrap(), Qty(100_000));
/// let lines = p.bom_lines();
/// assert_eq!((lines.len(), lines[0].qty), (1, Qty(100_000)));
/// ```
pub trait Bom {
    /// Its lines, in order.
    fn bom_lines(&self) -> Vec<BomLine>;
}

fn text<const N: usize>(t: &Option<crate::naming::text::Text<N>>) -> Option<String> {
    t.as_ref().map(|t| t.as_str().to_string())
}

impl Bom for LoosePart {
    fn bom_lines(&self) -> Vec<BomLine> {
        vec![BomLine {
            description: Some(self.description.as_str().to_string()),
            part_number: text(&self.part_number),
            manufacturer: text(&self.manufacturer),
            article_number: text(&self.article_number),
            qty: self.qty,
            unit: self.unit,
        }]
    }
}

impl Bom for Device {
    fn bom_lines(&self) -> Vec<BomLine> {
        if self.part_number.is_none() && self.description.is_none() {
            return Vec::new();
        }
        vec![BomLine {
            description: text(&self.description),
            part_number: text(&self.part_number),
            manufacturer: text(&self.manufacturer),
            article_number: text(&self.article_number),
            qty: Qty::ONE,
            unit: ItemUnit::Pcs,
        }]
    }
}
