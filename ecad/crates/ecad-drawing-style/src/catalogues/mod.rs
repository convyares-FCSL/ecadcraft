//! The look's style catalogues: text styles, location boxes and frames. Each is a
//! [`ecad_model::registry::styles::Catalogue`] (fields, built-ins, how a field resolves);
//! finding a style, its users and the checks span every catalogue in `ecad_electrical::styles`.

use ecad_model::registry::styles::{Catalogue, hex_rgb};

pub mod frame;
pub mod location;
pub mod text;

/// This crate's catalogues: one line each.
#[rustfmt::skip]
pub static CATALOGUES: &[&Catalogue] = &[
    &text::CATALOGUE,
    &location::CATALOGUE,
    &frame::CATALOGUE,
];

/// Whether `s` is a colour a style field takes: `none`, one IEC 60757 code
/// (`RD`, `BLU`), or `#rrggbb`.
pub(crate) fn is_colour(s: &str) -> bool {
    let t = s.trim();
    t.eq_ignore_ascii_case("none") || hex_rgb(t).is_some() || ecad_electrical::cables::words::colour_code(t).is_some_and(|(_, two)| two.is_none())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn colours_are_none_one_code_or_hex() {
        for ok in ["none", "NONE", "RD", "blu", "#c62828"] {
            assert!(is_colour(ok), "{ok}");
        }
        for bad in ["", "GNYE", "#12345", "#gggggg", "purple"] {
            assert!(!is_colour(bad), "{bad}");
        }
    }
}
