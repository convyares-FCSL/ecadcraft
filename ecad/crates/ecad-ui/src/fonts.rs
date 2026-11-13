//! The interface and sheet fonts (design §4; brief 037, owner answer Q1: embedded from ECAD's
//! repository). The five faces are included with `include_bytes!` on native and wasm, so they
//! are always present; nothing is read from disk at run time. Sources, versions and hashes are in
//! `ecad/assets/fonts/README.md`.
//!
//! [`install`] takes the host's font set (CADCraft's `theme::font_definitions(language)`, upstream
//! seam US2, with its script fallbacks), puts IBM Plex Sans first in `Proportional`, adds the
//! families [`UI`] and [`SHEET`] and one family per extra weight, and installs the result.

use std::sync::Arc;

use ecad_tokens::ui_type::TypeStyle;
use egui::{FontData, FontDefinitions, FontFamily, FontId};

/// The interface family: IBM Plex Sans Regular, then the host's fallbacks.
pub const UI: &str = "ui";
/// IBM Plex Sans Medium (500).
pub const UI_MEDIUM: &str = "ui-500";
/// IBM Plex Sans SemiBold (600).
pub const UI_SEMIBOLD: &str = "ui-600";
/// The sheet family: Barlow Semi Condensed Regular, then the host's fallbacks (used from U4a).
pub const SHEET: &str = "sheet";
/// Barlow Semi Condensed SemiBold (600).
pub const SHEET_SEMIBOLD: &str = "sheet-600";

/// One embedded face.
#[derive(Clone, Copy, Debug)]
pub struct Face {
    /// Its key in egui's font data.
    pub key: &'static str,
    /// The egui family it heads.
    pub family: &'static str,
    /// Its file under `ecad/assets/fonts/`.
    pub file: &'static str,
    pub bytes: &'static [u8],
}

/// The five faces.
pub const FACES: [Face; 5] = [
    Face {
        key: "ecad-ibm-plex-sans-400",
        family: UI,
        file: "ibm-plex-sans/IBMPlexSans-Regular.ttf",
        bytes: include_bytes!("../../../assets/fonts/ibm-plex-sans/IBMPlexSans-Regular.ttf"),
    },
    Face {
        key: "ecad-ibm-plex-sans-500",
        family: UI_MEDIUM,
        file: "ibm-plex-sans/IBMPlexSans-Medium.ttf",
        bytes: include_bytes!("../../../assets/fonts/ibm-plex-sans/IBMPlexSans-Medium.ttf"),
    },
    Face {
        key: "ecad-ibm-plex-sans-600",
        family: UI_SEMIBOLD,
        file: "ibm-plex-sans/IBMPlexSans-SemiBold.ttf",
        bytes: include_bytes!("../../../assets/fonts/ibm-plex-sans/IBMPlexSans-SemiBold.ttf"),
    },
    Face {
        key: "ecad-barlow-semi-condensed-400",
        family: SHEET,
        file: "barlow-semi-condensed/BarlowSemiCondensed-Regular.ttf",
        bytes: include_bytes!("../../../assets/fonts/barlow-semi-condensed/BarlowSemiCondensed-Regular.ttf"),
    },
    Face {
        key: "ecad-barlow-semi-condensed-600",
        family: SHEET_SEMIBOLD,
        file: "barlow-semi-condensed/BarlowSemiCondensed-SemiBold.ttf",
        bytes: include_bytes!("../../../assets/fonts/barlow-semi-condensed/BarlowSemiCondensed-SemiBold.ttf"),
    },
];

/// Each family's licence text, as `(file, bytes)`; shipped beside the faces.
pub const LICENCES: [(&str, &[u8]); 2] = [
    ("ibm-plex-sans/OFL.txt", include_bytes!("../../../assets/fonts/ibm-plex-sans/OFL.txt")),
    ("barlow-semi-condensed/OFL.txt", include_bytes!("../../../assets/fonts/barlow-semi-condensed/OFL.txt")),
];

/// `base` with ECAD's faces added: IBM Plex Sans first in `Proportional`, the families [`UI`],
/// [`UI_MEDIUM`], [`UI_SEMIBOLD`], [`SHEET`] and [`SHEET_SEMIBOLD`], each followed by `base`'s
/// `Proportional` fallbacks (so scripts the face lacks still draw). `Monospace` is kept; if
/// `base` has none, it gets IBM Plex Sans so text never has no face.
///
/// # Semantics
/// Idempotent: applying it to its own result changes nothing.
pub fn with_ecad_faces(mut base: FontDefinitions) -> FontDefinitions {
    let keys: Vec<&str> = FACES.iter().map(|f| f.key).collect();
    let fallbacks: Vec<String> =
        base.families.get(&FontFamily::Proportional).into_iter().flatten().filter(|n| !keys.contains(&n.as_str())).cloned().collect();
    for face in FACES {
        base.font_data.insert(face.key.into(), Arc::new(FontData::from_static(face.bytes)));
        let mut chain = vec![face.key.to_string()];
        chain.extend(fallbacks.iter().cloned());
        base.families.insert(FontFamily::Name(face.family.into()), chain);
    }
    let ui = FACES[0].key.to_string();
    let mut proportional = vec![ui.clone()];
    proportional.extend(fallbacks);
    base.families.insert(FontFamily::Proportional, proportional);
    let mono = base.families.entry(FontFamily::Monospace).or_default();
    if mono.is_empty() {
        mono.push(ui);
    }
    base
}

/// Where [`install`] records the pass it ran in.
fn installed_id() -> egui::Id {
    egui::Id::unique("ecad_ui::fonts::installed_at_pass")
}

/// Install `base` (the host's font set) with ECAD's faces ([`with_ecad_faces`]).
///
/// # Semantics
/// egui takes new fonts at the start of the next pass (or at once if nothing was laid out yet),
/// so [`font`] names ECAD's families from the pass after this one; until then it gives
/// `Proportional`.
pub fn install(ctx: &egui::Context, base: FontDefinitions) {
    ctx.set_fonts(with_ecad_faces(base));
    let pass = ctx.cumulative_pass_nr();
    ctx.data_mut(|d| d.insert_temp(installed_id(), pass));
}

/// Whether ECAD's families are in use in `ctx`'s current pass ([`install`] ran in an earlier one).
pub fn installed(ctx: &egui::Context) -> bool {
    let at: Option<u64> = ctx.data(|d| d.get_temp(installed_id()));
    at.is_some_and(|at| at < ctx.cumulative_pass_nr())
}

/// The egui family of the interface font at `weight` (400, 500, 600; others round to the nearest).
pub fn ui_family(weight: u16) -> &'static str {
    match weight {
        0..=449 => UI,
        450..=549 => UI_MEDIUM,
        _ => UI_SEMIBOLD,
    }
}

/// The font for a UI text style, once ECAD's fonts are in use in `ctx` ([`installed`]); before
/// that the same size in `Proportional`, so text is never laid out in a family egui does not have.
pub fn font(ctx: &egui::Context, style: TypeStyle) -> FontId {
    let family = if installed(ctx) { FontFamily::Name(ui_family(style.weight).into()) } else { FontFamily::Proportional };
    FontId::new(style.size, family)
}
