//! The embedded faces (brief 037, acceptance criterion 6): the `ui` and `sheet` families are
//! registered with the embedded faces, and the fonts README's hashes match the committed files.

use ecad_ui::fonts::{self, FACES, LICENCES};
use egui::{FontDefinitions, FontFamily};
use sha2::{Digest, Sha256};

const README: &str = include_str!(concat!(env!("CARGO_MANIFEST_DIR"), "/../../assets/fonts/README.md"));

fn sha256(bytes: &[u8]) -> String {
    Sha256::digest(bytes).iter().map(|b| format!("{b:02x}")).collect()
}

/// `file → (bytes, sha256)` from the README's table.
fn readme_rows() -> Vec<(String, usize, String)> {
    README
        .lines()
        .filter(|l| l.starts_with("| `"))
        .filter_map(|l| {
            let cells: Vec<&str> = l.trim_matches('|').split('|').map(|c| c.trim().trim_matches('`')).collect();
            Some((cells.first()?.to_string(), cells.get(2)?.parse().ok()?, cells.get(3)?.to_string()))
        })
        .collect()
}

#[test]
fn the_readme_hashes_match_the_committed_files() {
    let rows = readme_rows();
    let files: Vec<(&str, &[u8])> = FACES.iter().map(|f| (f.file, f.bytes)).chain(LICENCES).collect();
    assert_eq!(rows.len(), files.len(), "one README row per file: {rows:?}");
    for (file, bytes) in files {
        let (_, size, hash) = rows.iter().find(|(f, _, _)| f == file).unwrap_or_else(|| panic!("{file} has no README row"));
        assert_eq!(bytes.len(), *size, "{file}: size");
        assert_eq!(&sha256(bytes), hash, "{file}: SHA-256");
    }
    let total: usize = FACES.iter().map(|f| f.bytes.len()).sum();
    assert_eq!(total, 821_140, "the README's measured size");
}

#[test]
fn ui_and_sheet_families_are_registered_with_the_embedded_faces() {
    let base = FontDefinitions::default();
    let base_proportional = base.families.get(&FontFamily::Proportional).cloned().unwrap_or_default();
    let defs = fonts::with_ecad_faces(base);
    for face in FACES {
        let chain = defs.families.get(&FontFamily::Name(face.family.into())).expect("family registered");
        assert_eq!(chain.first().map(String::as_str), Some(face.key), "{} heads with its face", face.family);
        assert_eq!(&chain[1..], base_proportional.as_slice(), "{}: then the host's fallbacks", face.family);
        assert!(defs.font_data.get(face.key).is_some_and(|d| (*d.font).as_ref() == face.bytes), "{}: the embedded bytes", face.key);
    }
    assert_eq!(defs.families[&FontFamily::Proportional].first().map(String::as_str), Some("ecad-ibm-plex-sans-400"));
    assert!(defs.families.contains_key(&FontFamily::Name(fonts::UI.into())));
    assert!(defs.families.contains_key(&FontFamily::Name(fonts::SHEET.into())));
    // Idempotent: applying it again changes nothing.
    assert!(fonts::with_ecad_faces(defs.clone()) == defs);
}

/// Installed in a context, the families are in use from the next pass, and the text styles then
/// name them.
#[test]
fn installed_fonts_reach_the_text_styles() {
    let ctx = egui::Context::default();
    let pass = |ctx: &egui::Context| {
        let mut out = ctx.run_ui(egui::RawInput::default(), |ui| ecad_ui::style::apply(ui.ctx()));
        out.textures_delta.clear();
    };
    pass(&ctx);
    fonts::install(&ctx, FontDefinitions::default());
    assert!(!fonts::installed(&ctx), "not in use in the pass that installs them");
    pass(&ctx);
    assert!(fonts::installed(&ctx));
    // The style names ECAD's families in a pass where they are in use.
    pass(&ctx);
    assert!(ctx.fonts(|f| f.definitions().families.contains_key(&FontFamily::Name(fonts::SHEET.into()))));
    let body = ctx.global_style().text_styles[&egui::TextStyle::Body].clone();
    assert_eq!(body.family, FontFamily::Name(fonts::UI.into()));
    assert_eq!(body.size, ecad_tokens::ui_type::BODY.size);
    let heading = ctx.global_style().text_styles[&egui::TextStyle::Heading].clone();
    assert_eq!(heading.family, FontFamily::Name(fonts::UI_SEMIBOLD.into()));
}
