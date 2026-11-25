//! Drawing code (the Engine and ecad-electrical) takes a drawn text's height and style from the drawing
//! style, never from a literal.

use super::*;

/// Drawn text never ignores its style. The crates that draw and
/// lay out pages ([`DRAWN_TEXT_CRATES`]) hold no literal text height, style name or text-size
/// constant outside the places that define styles ([`DRAWN_TEXT_EXEMPT`]): a text's height, weight,
/// font and colour come from its `TextKind`'s style. A hard zero.
pub(crate) const DRAWN_TEXT_CRATES: &[&str] = &["ecad-engine", "ecad-electrical"];

/// Where literal text styling is allowed, as fragments of a file's path under the workspace:
/// the styles and their data define the sizes (`/styles/`, `/defaults/`); symbol definitions draw
/// glyph text as line art in symbol units; screen-only overlays (`interact`, the pointer, the
/// property forms) are sized in pixels; an imported title block keeps the user's own sizes; commands
/// (every `commands` file or folder, and the generic runner) draw nothing.
pub(crate) const DRAWN_TEXT_EXEMPT: &[&str] = &[
    "/styles/",
    "/defaults/",
    "ecad-electrical/src/symbols/",
    "ecad-engine/src/pointer/",
    "ecad-engine/src/properties/forms/",
    "/interact/",
    "/interact.rs",
    "ecad-engine/src/title_blocks/dxf.rs",
    "/commands/",
    "/commands.rs",
    "ecad-engine/src/generic.rs",
];

/// The lines of one source file (`rel`) that style drawn text literally, before its test module,
/// outside `//` comments: a numeric `height:`, a quoted `style:` or `"Standard"`, the style names
/// `STYLE_BOLD` / `STYLE_REGULAR`, or a text-size constant (`..._TEXT_MM`, `..._HEIGHT_MM`,
/// `LABEL_HEIGHT`…; bounds named `MIN_`/`MAX_` are guards and allowed).
pub(crate) fn drawn_text_places(rel: &str, text: &str) -> Vec<String> {
    let code = strip_line_comments(text);
    let lines: Vec<&str> = code.lines().collect();
    let end = (0..lines.len())
        .find(|&i| {
            lines[i].trim() == "#[cfg(test)]"
                && lines.get(i + 1).is_some_and(|n| n.trim_start().starts_with("mod ") || n.trim_start().starts_with("pub mod "))
        })
        .unwrap_or(lines.len());
    let numeric_after = |l: &str, key: &str| {
        l.match_indices(key).any(|(i, _)| {
            let rest = l.get(i + key.len()..).unwrap_or_default().trim_start().trim_start_matches('-');
            rest.chars().next().is_some_and(|c| c.is_ascii_digit())
        })
    };
    let size_const = |l: &str| {
        let t = l.trim_start().trim_start_matches("pub ").trim_start_matches("pub(crate) ");
        let Some(rest) = t.strip_prefix("const ") else { return false };
        let name: String = rest.chars().take_while(|c| c.is_ascii_uppercase() || c.is_ascii_digit() || *c == '_').collect();
        let sizes = ["TEXT_MM", "TEXT_HEIGHT", "HEIGHT_MM", "LABEL_HEIGHT", "CAP_HEIGHT", "TEXT_SIZE"];
        !name.starts_with("MIN_") && !name.starts_with("MAX_") && sizes.iter().any(|s| name.contains(s)) && numeric_after(rest, "f64 =")
    };
    let mut out: Vec<String> = lines
        .get(..end)
        .unwrap_or_default()
        .iter()
        .enumerate()
        .filter(|(_, l)| numeric_after(l, "height:") || l.contains("style: \"") || l.contains("\"Standard\"") || size_const(l))
        .map(|(i, _)| format!("{rel}:{}: {}", i + 1, text.lines().nth(i).unwrap_or_default().trim()))
        .collect();
    out.extend(name_places(rel, text, lines.get(..end).unwrap_or_default(), &["STYLE_BOLD", "STYLE_REGULAR"]));
    out.sort();
    out.dedup();
    out
}

/// [`drawn_text_places`] over [`DRAWN_TEXT_CRATES`], outside [`DRAWN_TEXT_EXEMPT`].
pub(crate) fn drawn_text_violations(meta: &Value) -> Result<Vec<String>, String> {
    const WHY: &str = "drawn text reads its TextKind's style: height, weight, font and colour come from the styles and their TOML data";
    let root = PathBuf::from(meta["workspace_root"].as_str().ok_or("cargo metadata: no workspace root")?);
    let mut out = Vec::new();
    for (name, _, dir, _) in classed_packages(meta)? {
        let (true, Some(dir)) = (DRAWN_TEXT_CRATES.contains(&name), dir) else { continue };
        let mut files = Vec::new();
        rust_files(&dir.join("src"), &mut files);
        for f in files {
            let rel = f.strip_prefix(&root).unwrap_or(&f).to_string_lossy().replace('\\', "/");
            if DRAWN_TEXT_EXEMPT.iter().any(|e| rel.contains(e)) {
                continue;
            }
            let text = std::fs::read_to_string(&f).map_err(|e| format!("{}: {e}", f.display()))?;
            out.extend(drawn_text_places(&rel, &text).into_iter().map(|p| format!("{p} ({WHY})")));
        }
    }
    Ok(out)
}
