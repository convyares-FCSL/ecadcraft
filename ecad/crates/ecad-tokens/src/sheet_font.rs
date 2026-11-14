//! The sheet font's two faces (spec §4 "Type"; design `ui-architecture.md` §4): Barlow Semi
//! Condensed Regular and SemiBold, from `ecad/assets/fonts/` (OFL-1.1, `ATTRIBUTION.md`), embedded
//! once here so the shell (`ecad-ui`, egui) and the page (`ecad-cadcraft`, CADCraft's text
//! renderer through US3) use the same bytes (brief 040). Plain data: nothing is read at run time.

/// The family name the page's text styles name.
pub const FAMILY: &str = "Barlow Semi Condensed";

/// Barlow Semi Condensed Regular (400).
pub const REGULAR: &[u8] = include_bytes!("../../../assets/fonts/barlow-semi-condensed/BarlowSemiCondensed-Regular.ttf");

/// Barlow Semi Condensed SemiBold (600).
pub const SEMIBOLD: &[u8] = include_bytes!("../../../assets/fonts/barlow-semi-condensed/BarlowSemiCondensed-SemiBold.ttf");
