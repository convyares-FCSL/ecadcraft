//! Typed input: the grammars of text a person types into a field, each refusing with a named
//! [`InputError`] in the words the field shows. The app passes what was typed and shows the
//! refusal; it parses nothing itself.
//!
//! # Example
//! ```
//! use ecad_electrical::input::{renumber_digits, InputError};
//! assert_eq!(renumber_digits(" 3 "), Ok(3));
//! assert_eq!(renumber_digits("9"), Err(InputError::RenumberDigits));
//! ```

use std::fmt;

/// Why a typed value was refused. `Display` is the words under the field.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InputError {
    /// Project settings: Tag digits.
    TagDigits,
    /// Project settings: the page number step.
    PageNumberStep,
    /// Project settings: the label height.
    LabelHeight,
    /// Project settings: the shortest labelled wire.
    LabelMinLength,
    /// Renumber: Start.
    RenumberStart,
    /// Renumber: Digits.
    RenumberDigits,
    /// A text height outside 0.5 to 10 mm.
    TextHeight,
    /// A PLC card's start address.
    PlcStart,
    /// A PLC card's slot.
    PlcSlot,
    /// A PLC channel's pins that are not `role:pin` pairs.
    PinPairs,
}

impl fmt::Display for InputError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            InputError::TagDigits => "Tag digits is a whole number.",
            InputError::PageNumberStep => "The page number step is a whole number.",
            InputError::LabelHeight => "Label height is 0.5 to 10 mm.",
            InputError::LabelMinLength => "Shortest labelled wire is 0 to 100 mm.",
            InputError::RenumberStart => "Start is a whole number from 1.",
            InputError::RenumberDigits => "Digits is a whole number from 1 to 6.",
            InputError::TextHeight => crate::styles::commands::text::BAD_HEIGHT,
            InputError::PlcStart => "The start is a byte number, such as 4 or %Q4.0.",
            InputError::PlcSlot => "The slot is a whole number.",
            InputError::PinPairs => "Pins are role:pin pairs, such as +:1 -:5.",
        })
    }
}

impl std::error::Error for InputError {}

/// Project settings: Tag digits.
///
/// # Errors
/// [`InputError::TagDigits`] when it is not a whole number.
pub fn tag_digits(text: &str) -> Result<u64, InputError> {
    text.trim().parse().map_err(|_| InputError::TagDigits)
}

/// Tag digits as the example under the field uses them: two when the field holds nothing usable.
#[must_use]
pub fn tag_digits_or_default(text: &str) -> u8 {
    text.trim().parse::<u8>().unwrap_or(2)
}

/// Project settings: the page number step.
///
/// # Errors
/// [`InputError::PageNumberStep`] when it is not a whole number.
pub fn page_number_step(text: &str) -> Result<u32, InputError> {
    text.trim().parse().map_err(|_| InputError::PageNumberStep)
}

/// Project settings: the label height, one of the heights the labels offer.
///
/// # Errors
/// [`InputError::LabelHeight`].
pub fn label_height(text: &str) -> Result<f64, InputError> {
    text.trim().parse().ok().filter(|h| ecad_model::connectivity::wiring::LABEL_HEIGHTS.contains(h)).ok_or(InputError::LabelHeight)
}

/// Project settings: the shortest labelled wire, one of the lengths offered.
///
/// # Errors
/// [`InputError::LabelMinLength`].
pub fn label_min_length(text: &str) -> Result<f64, InputError> {
    text.trim().parse().ok().filter(|x| ecad_model::connectivity::wiring::LABEL_MIN_LENGTHS.contains(x)).ok_or(InputError::LabelMinLength)
}

/// Renumber: Start, from 1.
///
/// # Errors
/// [`InputError::RenumberStart`].
pub fn renumber_start(text: &str) -> Result<u64, InputError> {
    text.trim().parse::<u64>().ok().filter(|n| (1..=999_999_999).contains(n)).ok_or(InputError::RenumberStart)
}

/// Renumber: Digits, 1 to 6.
///
/// # Errors
/// [`InputError::RenumberDigits`].
pub fn renumber_digits(text: &str) -> Result<u8, InputError> {
    text.trim().parse::<u8>().ok().filter(|n| (1..=6).contains(n)).ok_or(InputError::RenumberDigits)
}

/// A text height in mm, 0.5 to 10.
///
/// # Errors
/// [`InputError::TextHeight`].
pub fn text_height(text: &str) -> Result<f64, InputError> {
    text.trim().parse::<f64>().ok().filter(|h| h.is_finite() && (0.5..=10.0).contains(h)).ok_or(InputError::TextHeight)
}

/// A frame line weight in mm; `default` when the field holds no number.
#[must_use]
pub fn frame_weight(text: &str, default: f64) -> f64 {
    text.parse().unwrap_or(default)
}

/// A style field's typed value: a number when the text is one, else the text.
#[must_use]
pub fn style_value(text: &str) -> serde_json::Value {
    match text.trim().parse::<f64>().ok().and_then(serde_json::Number::from_f64) {
        Some(n) => serde_json::Value::Number(n),
        None => serde_json::Value::String(text.to_string()),
    }
}

/// A tag prefix as stored: no leading `-` (`-K` and `K` are the same prefix).
#[must_use]
pub fn tag_prefix(text: &str) -> String {
    text.trim().trim_start_matches('-').to_string()
}

/// A tag prefix as shown beside a tag: with its leading `-`.
#[must_use]
pub fn tag_prefix_shown(text: &str) -> String {
    format!("-{}", tag_prefix(text))
}

/// A PLC card's start: the byte a typed address names (`%Q4.0`, `Q4`, `IW64`, `4`); empty unpins.
///
/// # Errors
/// [`InputError::PlcStart`] when there is no byte number in it.
pub fn plc_start(text: &str) -> Result<Option<u32>, InputError> {
    let t = text.trim();
    if t.is_empty() {
        return Ok(None);
    }
    let digits: String = t.trim_start_matches(|c: char| !c.is_ascii_digit()).chars().take_while(char::is_ascii_digit).collect();
    digits.parse().map(Some).map_err(|_| InputError::PlcStart)
}

/// A PLC card's slot.
///
/// # Errors
/// [`InputError::PlcSlot`] when it is not a whole number.
pub fn plc_slot(text: &str) -> Result<u32, InputError> {
    text.trim().parse().map_err(|_| InputError::PlcSlot)
}

/// The pins typed for a channel with several roles: `+:1 -:5`, each part `role:pin` for one of
/// `roles`. Empty text gives no pairs.
///
/// # Errors
/// [`InputError::PinPairs`] when a part has no colon or names a role the channel does not have.
pub fn pin_pairs(text: &str, roles: &[ecad_model::ChannelRole]) -> Result<Vec<(String, String)>, InputError> {
    let mut out = Vec::new();
    for part in text.split_whitespace() {
        let (role, pin) = part.split_once(':').ok_or(InputError::PinPairs)?;
        if !roles.iter().any(|r| ecad_model::ChannelRole::exact(role) == Some(*r)) {
            return Err(InputError::PinPairs);
        }
        out.push((role.to_string(), pin.to_string()));
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn typed_numbers_are_refused_by_name() {
        assert_eq!(renumber_start("0"), Err(InputError::RenumberStart));
        assert_eq!(renumber_start(" 12 "), Ok(12));
        assert_eq!(tag_digits("x"), Err(InputError::TagDigits));
        assert_eq!(page_number_step("5"), Ok(5));
        assert_eq!(text_height("11"), Err(InputError::TextHeight));
        assert_eq!(text_height("2.5"), Ok(2.5));
        assert_eq!(frame_weight("x", 0.5), 0.5);
        assert_eq!(InputError::RenumberDigits.to_string(), "Digits is a whole number from 1 to 6.");
    }

    #[test]
    fn prefixes_have_one_spelling() {
        assert_eq!(tag_prefix(" -K "), "K");
        assert_eq!(tag_prefix_shown("K"), "-K");
        assert_eq!(tag_prefix_shown("--K"), "-K");
    }

    #[test]
    fn plc_input_reads_the_byte() {
        assert_eq!(plc_start("%Q4.0"), Ok(Some(4)));
        assert_eq!(plc_start("IW64"), Ok(Some(64)));
        assert_eq!(plc_start(""), Ok(None));
        assert_eq!(plc_start("Q"), Err(InputError::PlcStart));
        assert_eq!(plc_slot("3"), Ok(3));
        assert_eq!(plc_slot("x"), Err(InputError::PlcSlot));
    }

    #[test]
    fn style_values_are_numbers_when_they_are_numbers() {
        assert_eq!(style_value("1.5"), serde_json::json!(1.5));
        assert_eq!(style_value("H07"), serde_json::json!("H07"));
    }
}
