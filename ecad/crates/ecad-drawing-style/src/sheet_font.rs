//! The sheet font's two faces: Barlow Semi
//! Condensed Regular and SemiBold, from `ecad/assets/fonts/` (OFL-1.1, `ATTRIBUTION.md`), embedded
//! once here so the shell (`ecad-widgets`, egui) and the page (`ecad-engine`, CADCraft's text
//! renderer) use the same bytes. Plain data: nothing is read at run time.

/// The family name the page's text styles name.
pub const FAMILY: &str = "Barlow Semi Condensed";

/// Barlow Semi Condensed Regular.
pub const REGULAR: &[u8] = include_bytes!("../../../assets/fonts/barlow-semi-condensed/BarlowSemiCondensed-Regular.ttf");

/// Barlow Semi Condensed SemiBold.
pub const SEMIBOLD: &[u8] = include_bytes!("../../../assets/fonts/barlow-semi-condensed/BarlowSemiCondensed-SemiBold.ttf");
