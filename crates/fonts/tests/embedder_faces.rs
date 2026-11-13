//! ECAD-SEAM(owner): embedder fonts. Faces an embedding application compiles into its binary are
//! given to the font table once (`ttf::register_faces`) and found by family name like an embedded
//! craft-fonts font; without the call nothing changes; a second call is refused, not a panic.
//! The faces are process-wide, so this is one test in its own binary.

use std::path::PathBuf;

use cadcraft_fonts::ttf::{self, EmbeddedFace, FacesAlreadyRegistered};

/// The bytes of a system TrueType font with Latin capitals, if there is one (a bounded walk). No
/// font file is committed here.
fn a_system_ttf() -> Option<Vec<u8>> {
    let mut stack: Vec<PathBuf> =
        ["C:/Windows/Fonts", "/System/Library/Fonts/Supplemental", "/Library/Fonts", "/usr/share/fonts", "/usr/local/share/fonts"]
            .map(PathBuf::from)
            .to_vec();
    let mut seen = 0;
    while let Some(d) = stack.pop() {
        for e in std::fs::read_dir(&d).into_iter().flatten().flatten() {
            seen += 1;
            if seen > 5_000 {
                return None;
            }
            let p = e.path();
            if p.is_dir() {
                stack.push(p);
            } else if p.extension().is_some_and(|x| x.eq_ignore_ascii_case("ttf")) {
                let Ok(b) = std::fs::read(&p) else { continue };
                if ttf::faces(&std::sync::Arc::new(b.clone())).first().is_some_and(|f| f.has_glyph('K') && f.has_glyph('a')) {
                    return Some(b);
                }
            }
        }
    }
    None
}

#[test]
fn embedder_faces_are_found_by_family_once_registered() {
    const FAMILY: &str = "CADCraft Embedder Test";
    // Unregistered: nothing is listed and the family is not found, as before the seam.
    assert!(ttf::embedder_faces().is_empty());
    assert!(ttf::find(FAMILY).is_none());

    // Without a system font to borrow bytes from (web-like CI images), only the refusal is checked.
    let bytes: &'static [u8] = match a_system_ttf() {
        Some(b) => Box::leak(b.into_boxed_slice()),
        None => &[0, 1, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0],
    };
    let real = bytes.len() > 12;
    let faces: &'static [EmbeddedFace] =
        Box::leak(Box::new([EmbeddedFace { family: FAMILY, style: "Regular", bytes }, EmbeddedFace { family: FAMILY, style: "SemiBold", bytes }]));
    assert_eq!(faces[1].stem(), "cadcraftembeddertest-semibold");
    let before = ttf::generation();
    assert_eq!(ttf::register_faces(faces), Ok(()));
    assert!(ttf::generation() > before, "text drawn before may change font");
    assert_eq!(ttf::embedder_faces().len(), 2);
    if real {
        for name in [FAMILY, "CADCraft Embedder Test SemiBold", "cadcraftembeddertest-semibold", "cadcraft embedder test"] {
            let face = ttf::find(name);
            assert!(face.as_ref().is_some_and(|f| f.has_glyph('K')), "{name} is found");
        }
    }
    assert!(ttf::find("CADCraft Embedder Test Black").is_none(), "only the styles given");

    // A second call is refused and the first faces stay.
    let other: &'static [EmbeddedFace] = Box::leak(Box::new([EmbeddedFace { family: "Other Embedder", style: "Regular", bytes }]));
    assert_eq!(ttf::register_faces(other), Err(FacesAlreadyRegistered));
    assert_eq!(ttf::embedder_faces().len(), 2);
    assert!(ttf::find("Other Embedder").is_none());
    if real {
        assert!(ttf::find(FAMILY).is_some());
        // Looking for fonts again keeps them.
        ttf::rescan();
        assert!(ttf::find(FAMILY).is_some(), "a rescan keeps the embedder's faces");
    }
}
