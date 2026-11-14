//! The UI type ramp (spec §4, "Type"), in px at 100 % UI scale. The UI font is IBM Plex Sans.

/// One UI text style.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct TypeStyle {
    /// Font size, px.
    pub size: f32,
    /// Line height, px.
    pub line: f32,
    /// Weight (400 regular, 500 medium, 600 semibold).
    pub weight: u16,
}

/// Section labels, badges, key caps.
pub const CAPTION: TypeStyle = TypeStyle { size: 11.0, line: 14.0, weight: 600 };
/// Tables at density, status bar, hints.
pub const SMALL: TypeStyle = TypeStyle { size: 12.0, line: 16.0, weight: 400 };
/// Everything else.
pub const BODY: TypeStyle = TypeStyle { size: 13.0, line: 18.0, weight: 400 };
/// Body text with emphasis.
pub const BODY_EMPHASIS: TypeStyle = TypeStyle { weight: 500, ..BODY };
/// Panel and card titles.
pub const TITLE: TypeStyle = TypeStyle { size: 15.0, line: 20.0, weight: 600 };
/// The selected object's name, screen titles.
pub const OBJECT: TypeStyle = TypeStyle { size: 20.0, line: 26.0, weight: 600 };
/// Point keys and slot labels on symbol drawings in the Library (UI rev 3).
pub const MICRO: TypeStyle = TypeStyle { size: 9.0, line: 12.0, weight: 400 };
/// An empty screen's hint (UI rev 3).
pub const LEAD: TypeStyle = TypeStyle { size: 14.0, line: 20.0, weight: 400 };

/// Every style as `(name, style)`, names as in `tokens.md` (`ui_type.body`).
pub const ALL: [(&str, TypeStyle); 8] = [
    ("ui_type.caption", CAPTION),
    ("ui_type.small", SMALL),
    ("ui_type.body", BODY),
    ("ui_type.body_emphasis", BODY_EMPHASIS),
    ("ui_type.title", TITLE),
    ("ui_type.object", OBJECT),
    ("ui_type.micro", MICRO),
    ("ui_type.lead", LEAD),
];
