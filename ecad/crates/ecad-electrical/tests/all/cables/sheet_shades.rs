//! Which shade a colour code draws in follows the sheet's own `dark` flag, never its colours.

use ecad_electrical::cables::words::{CODE_COLOURS, code_drawn, drawn};
use ecad_model::palette::Color;
use ecad_model::palette::sheet::{DARK, PAPER, Sheet};

#[test]
fn a_recoloured_sheet_keeps_its_shade() {
    let recoloured_dark = Sheet { sheet: Color::hex(0x202830), ..DARK };
    let paper_like_dark = Sheet { sheet: DARK.sheet, ..PAPER };
    for (i, (colour, paper, on_dark)) in CODE_COLOURS.iter().enumerate() {
        assert_eq!(code_drawn(i, &recoloured_dark), Some(*on_dark), "{colour:?} on a recoloured dark sheet");
        assert_eq!(drawn(*colour, &recoloured_dark), *on_dark, "{colour:?} on a recoloured dark sheet");
        assert_eq!(code_drawn(i, &paper_like_dark), Some(*paper), "{colour:?} on paper with the dark sheet's colour");
        assert_eq!(drawn(*colour, &PAPER), *paper, "{colour:?} on paper");
    }
}
