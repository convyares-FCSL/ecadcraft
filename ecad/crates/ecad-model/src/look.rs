//! The look as the rules see it: the kinds of text a project draws ([`TextKind`]), a kind's
//! resolved style ([`TextStyle`]), and [`TextLook`], through which the electrical rules ask how a
//! text is drawn and how wide it is.
//!
//! The rules lay out their own content (where a tag goes, how wide a table is) but own no look:
//! an implementation of [`TextLook`] answers over the fonts the drawing engine has.

use crate::objects::styles::text::Weight;
use crate::registry::Scope;
use crate::{Document, StyleRef, Word};

crate::word_enum! {
/// A kind of text: one row of the style table. Its word is the key word in
/// `text.<word>.height` and the commands.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum TextKind {
    /// A device tag beside a symbol.
    Tag,
    /// A connection point label (`A1`, `13`, strip `1`).
    PointLabel,
    /// A device's description under its tag.
    Description,
    /// A device's part number under its tag.
    PartNumber,
    /// A continuation's name.
    ContinuationName,
    /// Cross-reference texts.
    Reference,
    /// A continuation caption.
    ContinuationCaption,
    /// Wire labels (height stored in `settings.wiring.label_height`).
    WireLabel,
    /// Cable look: core labels, wire numbers at the slash, label box lines.
    CableText,
    /// A location box's `+A1`.
    #[strum(serialize = "location")]
    LocationLabel,
    /// Title block values.
    TitleValue,
    /// Title block box captions.
    TitleCaption,
    /// Project and page title in the title block.
    TitleMain,
    /// Frame column and row labels.
    Grid,
    /// Report table cells and contact-table rows.
    Table,
    /// Report column headers.
    TableHeader,
    /// Free text: its font only (a typed height is the user's).
    Free,
    /// The title block's page number box.
    TitlePage,
    /// The title block's revision box and pending marker.
    TitleRev,
    /// Location and strip headings, `(continued)`.
    TableHeading,
    /// Cover lines.
    Cover,
    /// A PLC channel's function, outside its card (`Pump 1 run`, `Spare`).
    ChannelText,
    /// A PLC channel's address inside its cell (`I0.3`).
    Address,
}
}

impl TextKind {
    /// Every kind, in the order of the style table.
    pub const ALL: [TextKind; 23] = [
        TextKind::Tag,
        TextKind::PointLabel,
        TextKind::Description,
        TextKind::PartNumber,
        TextKind::ContinuationName,
        TextKind::Reference,
        TextKind::ContinuationCaption,
        TextKind::WireLabel,
        TextKind::CableText,
        TextKind::LocationLabel,
        TextKind::TitleValue,
        TextKind::TitleCaption,
        TextKind::TitleMain,
        TextKind::Grid,
        TextKind::Table,
        TextKind::TableHeader,
        TextKind::Free,
        TextKind::TitlePage,
        TextKind::TitleRev,
        TextKind::TableHeading,
        TextKind::Cover,
        TextKind::ChannelText,
        TextKind::Address,
    ];

    /// The built-in text style of this kind: its key word.
    pub fn builtin_style(self) -> StyleRef {
        StyleRef::Builtin(self.word().to_string())
    }
}

/// A kind's text style, resolved for a project.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct TextStyle {
    /// Height in drawing units (mm, or inches in an inch project); always finite and in range.
    pub height: f64,
    pub weight: Weight,
    /// The drawing engine's text style an entity of this kind names: the project font's regular or
    /// bold style, or the style of a font of the kind's own.
    pub style_name: &'static str,
    /// The style's colour, or `None` for ink. The engine draws it in its
    /// colour on the sheet drawn on; a mono print prints ink regardless.
    pub colour: Option<TextColour>,
}

/// A text style's colour, before the sheet is known: a colour of its own, or a colour
/// code whose colour depends on the sheet (the drawing engine picks the colour when it draws).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TextColour {
    /// `#RRGGBB`.
    Rgb(u8, u8, u8),
    /// A colour code: its first colour, an index into the colour codes.
    Code(usize),
}

/// How the rules' texts look: a kind's style, and the width of a text drawn in it.
///
/// # Semantics
/// Every method reads `doc` as it is now and never fails: a hostile stored value counts as absent,
/// and a width is finite and not negative.
pub trait TextLook {
    /// The style `kind` is drawn in at `at` (the narrowest level that sets it).
    fn style(&self, doc: &Document, kind: TextKind, at: Scope) -> TextStyle;

    /// The width of `value` drawn in `kind`'s font with `s` (a style derived from `kind`'s).
    fn width(&self, doc: &Document, kind: TextKind, s: TextStyle, value: &str) -> f64;

    /// The width of `value` drawn with `s` in the font family `family`, or in the project font
    /// when `None` (an object's own text style).
    fn family_width(&self, doc: &Document, family: Option<&str>, s: TextStyle, value: &str) -> f64;

    /// The width of `value` at `height` in the stroke font (the cable look's labels).
    fn stroke_width(&self, value: &str, height: f64) -> f64;

    /// The width of `value` drawn as `kind` in its project style.
    fn text_width(&self, doc: &Document, kind: TextKind, value: &str) -> f64 {
        self.width(doc, kind, self.style(doc, kind, Scope::project()), value)
    }
}
