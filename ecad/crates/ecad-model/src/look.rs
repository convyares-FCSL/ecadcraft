//! The look as the rules see it: the kinds of text a project draws ([`TextKind`]), a kind's
//! resolved style ([`TextStyle`]), whether a text is shown ([`Shown`]), the frame's text styles
//! and lines ([`FrameText`], [`FrameDraw`]) and its title block ([`FrameBlock`]), and [`Look`],
//! through which the electrical rules ask for all of them.
//!
//! The rules lay out their own content (where a tag goes, how wide a table is) but own no look:
//! an implementation of [`Look`] answers from the project's styles and the fonts the drawing
//! engine has.

use std::sync::Arc;

use crate::document::sheet::{Margins, SheetSpec};
use crate::objects::styles::frame::BlockAt;
use crate::objects::styles::text::Weight;
use crate::objects::title_block::TitleBlockLayout;
use crate::registry::Scope;
use crate::{DeviceId, Document, PageId, RepresentationId, StyleRef, WireId, Word};

/// The registry key of the project font.
pub const FONT_KEY: &str = "text.font";

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

    /// The registry key choosing this kind's text style: `text.<kind>.style`.
    pub fn style_key(self) -> String {
        format!("text.{}.style", self.word())
    }

    /// Words for this kind in labels and echoes: `tag`, `connection point labels`…
    pub fn words(self) -> &'static str {
        match self {
            TextKind::Tag => "tag",
            TextKind::PointLabel => "connection point labels",
            TextKind::Description => "description",
            TextKind::PartNumber => "part number",
            TextKind::ContinuationName => "continuation name",
            TextKind::Reference => "references",
            TextKind::ContinuationCaption => "continuation captions",
            TextKind::LocationLabel => "location labels",
            TextKind::WireLabel => "wire labels",
            k => k.word(),
        }
    }

    /// This kind's name as the Properties panel's Texts rows show it: `Tag`, `Pin labels`…
    pub fn shown_name(self) -> String {
        match self {
            TextKind::PointLabel => "Pin labels".to_string(),
            k => {
                let mut c = k.words().chars();
                c.next().map(|f| f.to_uppercase().chain(c).collect()).unwrap_or_default()
            }
        }
    }
}

/// The refusal of a `text.` key through `projectprops`, `pageprops` or `setprop`.
pub const RESERVED_KEY: &str = "\"text.…\" is set with Text styles (textstyle, showtext, movetext).";

/// Whether `key` is one of the text commands' keys (`text.` prefix, any case).
pub fn is_reserved_key(key: &str) -> bool {
    key.trim().get(..5).is_some_and(|p| p.eq_ignore_ascii_case("text."))
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

/// An object's own text style as drawn: its height, weight and colour, and the drawing engine's
/// text style naming its font. `family` is the font family it draws in when that is not the
/// project font, so the page installs that style.
#[derive(Debug, Clone, PartialEq)]
pub struct OwnStyle {
    pub style: TextStyle,
    /// The drawing engine's text style an entity names.
    pub name: String,
    pub family: Option<String>,
}

crate::word_enum! {
    /// Whether a text is shown.
    #[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
    pub enum Shown {
        #[default]
        #[strum(serialize = "yes")]
        On,
        #[strum(serialize = "no")]
        Off,
        /// Once per run: on the first symbol of each run, hidden on the others. Tags only.
        First,
    }
}

impl Shown {
    /// The value typed in a box: a stored word, any yes/no word ([`crate::naming::words::flag`]), or
    /// its words in the panels (`Shown`, `Hidden`, `Once per run`), in any case.
    pub fn from_input(text: &str) -> Option<Shown> {
        let t = text.trim();
        <Self as Word>::parse(t)
            .or_else(|| crate::naming::words::flag(t).map(|on| if on { Shown::On } else { Shown::Off }))
            .or_else(|| Shown::ALL_LABELS.into_iter().find(|s| s.label().eq_ignore_ascii_case(t)))
    }

    const ALL_LABELS: [Shown; 3] = [Shown::On, Shown::Off, Shown::First];

    /// Its words in the panels.
    pub fn label(self) -> &'static str {
        match self {
            Shown::On => "Shown",
            Shown::Off => "Hidden",
            Shown::First => "Once per run",
        }
    }
}

/// The built-in run rule of a symbol: strip terminals and PLC channels show their tag once per
/// run unless something narrower says otherwise.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum RunRule {
    /// Any other symbol.
    #[default]
    None,
    /// A strip terminal (starter kind `ConnectionPoint`).
    Strip,
    /// A PLC channel (starter kind `Plc`).
    Plc,
}

/// What a text belongs to, for [`Look::shown`]: a symbol (its representation, device and page),
/// a conductor, a page or the project.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct TextTarget {
    pub page: Option<PageId>,
    pub component: Option<DeviceId>,
    pub representation: Option<RepresentationId>,
    pub conductor: Option<WireId>,
    /// The symbol's run rule (only for a tag).
    pub run: RunRule,
}

impl TextTarget {
    /// A page.
    pub fn page(page: PageId) -> Self {
        TextTarget { page: Some(page), ..TextTarget::default() }
    }

    /// A symbol: `rep`, its device and its page (looked up in `doc`), with its run rule.
    pub fn symbol(doc: &Document, rep: RepresentationId, run: RunRule) -> Self {
        let s = Scope::symbol(doc, rep);
        TextTarget { page: s.page, component: s.component, representation: Some(rep), conductor: None, run }
    }

    /// A conductor drawn on `page` (for wire labels).
    pub fn conductor(conductor: WireId, page: Option<PageId>) -> Self {
        TextTarget { page, conductor: Some(conductor), ..TextTarget::default() }
    }

    /// The levels a value for this target is read at.
    pub fn scope(self) -> Scope {
        Scope { page: self.page, component: self.component, rep: self.representation, conductor: self.conductor }
    }
}

/// A text kind's style where ECAD draws in paper millimetres (frame, generated pages): its height
/// in millimetres, its style name, and the lineweight of bold drawn in the stroke font.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct TextDraw {
    pub mm: f64,
    pub style: &'static str,
    pub weight: Option<u16>,
}

impl TextDraw {
    /// Whether its style is bold (the drawing engine's style name of a bold weight; a block
    /// item's `bold` comes from its kind's style, never a literal).
    pub fn bold(&self) -> bool {
        self.style.ends_with("BOLD")
    }
}

/// The frame's text styles: grid labels, box captions, values and revision rows, Project and Page
/// title, the Page number box, the Rev box.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct FrameText {
    pub grid: TextDraw,
    pub caption: TextDraw,
    pub value: TextDraw,
    pub main: TextDraw,
    pub page: TextDraw,
    pub rev: TextDraw,
    /// The lineweight bold is drawn with in the title fonts, for a block's bold texts when the
    /// main style is not bold itself.
    pub bold_weight: Option<u16>,
}

impl FrameText {
    /// These styles without the fonts' bold lineweights (which exist only when bold is drawn in
    /// the stroke font): what a rule that reads the block's layout and values needs, the same on
    /// every machine.
    pub fn sizes(self) -> FrameText {
        let plain = |l: TextDraw| TextDraw { weight: None, ..l };
        FrameText {
            grid: plain(self.grid),
            caption: plain(self.caption),
            value: plain(self.value),
            main: plain(self.main),
            page: plain(self.page),
            rev: plain(self.rev),
            bold_weight: None,
        }
    }
}

/// A page frame's resolved look.
#[derive(Debug, Clone, PartialEq)]
pub struct FrameDraw {
    /// Hundredths of a millimetre.
    pub border: u16,
    pub inner: u16,
    /// `none` (ink), an IEC code or `#rrggbb`.
    pub colour: String,
    pub grid_labels: bool,
    /// A short mark at the middle of each side.
    pub centre_marks: bool,
    /// Where the block sits.
    pub block: BlockAt,
    /// The frame's distance from the paper edge.
    pub margins: Margins,
    /// Zone ticks and labels drawn; off when the page type turns zones off, whatever the frame
    /// says.
    pub zones: bool,
}

impl Default for FrameDraw {
    fn default() -> Self {
        FrameDraw {
            border: 50,
            inner: 25,
            colour: "none".into(),
            grid_labels: true,
            centre_marks: true,
            block: BlockAt::BottomFull,
            margins: Margins::default(),
            zones: true,
        }
    }
}

/// A page's title block as drawn: the layout, whether it is the built-in's, and the frame style.
#[derive(Debug, Clone, PartialEq)]
pub struct FrameBlock {
    pub frame: StyleRef,
    pub layout: Arc<TitleBlockLayout>,
    pub builtin: bool,
}

/// How the rules' texts and frames look: a kind's style, whether a text is shown, the width of a
/// text drawn in it, and a page frame's text styles, lines and title block.
///
/// # Semantics
/// Every method reads `doc` as it is now and never fails: a hostile stored value counts as absent,
/// and a width is finite and not negative.
pub trait Look {
    /// The style `kind` is drawn in at `at` (the narrowest level that sets it).
    fn style(&self, doc: &Document, kind: TextKind, at: Scope) -> TextStyle;

    /// The text style `kind` uses at `at`.
    fn style_ref(&self, doc: &Document, kind: TextKind, at: Scope) -> StyleRef;

    /// The own text style of `kind` that the object with `props` chooses, resolved at `at`;
    /// `None` when it has no valid choice of its own.
    fn own_style(&self, doc: &Document, kind: TextKind, at: Scope, props: &crate::Properties) -> Option<OwnStyle>;

    /// Text style `id` as drawn, for a text whose style is chosen by something other than its
    /// kind (a location box's label).
    fn own_style_of(&self, doc: &Document, id: &StyleRef) -> OwnStyle;

    /// Whether a text of `kind` on `target` is shown: the narrowest level that has a value wins;
    /// `First` only ever for a tag; kinds with no visibility setting are always `On`.
    fn shown(&self, doc: &Document, target: TextTarget, kind: TextKind) -> Shown;

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

    /// The style of `kind` in the project.
    fn text_style(&self, doc: &Document, kind: TextKind) -> TextStyle {
        self.style(doc, kind, Scope::project())
    }

    /// The frame's text styles in `doc`, with this look's fonts.
    fn frame_text(&self, doc: &Document) -> FrameText;

    /// The frame look of `page`: its frame style, with zones off when its page type turns them
    /// off.
    fn frame_draw(&self, doc: &Document, page: PageId) -> FrameDraw;

    /// The title block of frame style `id` on `spec` with `margins`: its own when it has one,
    /// else the built-in block computed for the frame's width with `text`'s sizes.
    fn title_block(&self, doc: &Document, id: &StyleRef, spec: &SheetSpec, margins: &Margins, text: &FrameText) -> FrameBlock;
}
