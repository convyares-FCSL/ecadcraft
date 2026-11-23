//! Text anchors of a starter.

use super::*;

/// What a text anchor is for.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TextRole {
    Tag,
    Description,
    Rating,
    /// The rating of a fuse bank's fuse, by its label (`rating.<n>`).
    RatingOf(String),
    /// A card body's caption (`-A01 slot 2`), under the body.
    Caption,
    /// A channel cell's channel number, address and description.
    ChannelNo,
    Address,
    ChannelText,
}

/// Horizontal alignment of text at its anchor.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HAlign {
    Left,
    Centre,
    Right,
}

/// Vertical alignment of text at its anchor.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VAlign {
    Bottom,
    Middle,
    Top,
}

/// The first place the text placement tries for one text of a symbol, in the symbol's local frame (it rotates
/// and mirrors with the symbol); `rotation` in degrees.
#[derive(Debug, Clone, PartialEq)]
pub struct Anchor {
    pub role: TextRole,
    pub at: Pt,
    pub h: HAlign,
    pub v: VAlign,
    pub rotation: f64,
}

/// The gap between a symbol's box and its text, and one line's step, in millimetres.
/// Geometry of the anchor rule, not a text size: the text styles decide sizes.
const ANCHOR_GAP: f64 = 2.0;
const ANCHOR_LINE: f64 = 4.0;

/// The text anchors of `s`: the default rule from its box, with the overrides of
/// fuse banks, RCBO and RCDs, card bodies and connectors. A tagless symbol has none.
///
/// # Semantics
/// Default rule, box (lo, hi), mid = (lo.y + hi.y)/2: Tag at (lo.x − 2, mid), right, middle;
/// Description at (lo.x − 2, mid − 4), right, top; Rating at (hi.x + 2, mid), left, middle.
pub fn anchors(s: &Starter) -> Vec<Anchor> {
    if s.kind == Kind::Tagless {
        return Vec::new();
    }
    let Ok(d) = s.definition() else { return Vec::new() };
    let (lo, hi) = bounds(d);
    let mid = (lo.y + hi.y) / 2.0;
    let a = |role: TextRole, x: f64, y: f64, h: HAlign, v: VAlign| Anchor { role, at: p(x, y), h, v, rotation: 0.0 };
    let tag = a(TextRole::Tag, lo.x - ANCHOR_GAP, mid, HAlign::Right, VAlign::Middle);
    let description = a(TextRole::Description, lo.x - ANCHOR_GAP, mid - ANCHOR_LINE, HAlign::Right, VAlign::Top);
    let rating = a(TextRole::Rating, hi.x + ANCHOR_GAP, mid, HAlign::Left, VAlign::Middle);
    let mut out = match s.key {
        // Breakers carry their rating by the tag.
        K::Rcbo2 | K::Rcd2 | K::Rcd4 => vec![
            tag,
            a(TextRole::Rating, lo.x - ANCHOR_GAP, mid - ANCHOR_LINE, HAlign::Right, VAlign::Top),
            a(TextRole::Description, lo.x - ANCHOR_GAP, mid - 2.0 * ANCHOR_LINE, HAlign::Right, VAlign::Top),
        ],
        // A channel cell's texts in the cell's frame.
        K::Chdi | K::Chdq | K::Chai | K::Chao | K::Chrtd | K::Chiol => {
            let h = cell_shape(s.key).map_or(5.0, |(w, _)| w / 2.0);
            vec![
                tag,
                // The mark band's top line, and outside the frame.
                a(TextRole::ChannelNo, -h + 1.0, CELL_TOP - HEADER_BAND - 1.0, HAlign::Left, VAlign::Top),
                a(TextRole::Address, 0.0, CELL_TOP - HEADER_BAND - 1.0, HAlign::Centre, VAlign::Top),
                Anchor {
                    role: TextRole::ChannelText,
                    at: p(0.0, CELL_TOP - FRAME_DEPTH - OUTSIDE_GAP),
                    h: HAlign::Right,
                    v: VAlign::Middle,
                    rotation: 90.0,
                },
            ]
        }
        // Card bodies: the tag above, the description inside under the header.
        // And the caption under the body.
        K::Plccpu | K::Plcps | K::Plcdi | K::Plcdq | K::Plcai | K::Plcao | K::Plcrtd | K::Plciol | K::Plcim | K::Plcpm | K::Plccm => vec![
            a(TextRole::Tag, lo.x, hi.y + ANCHOR_GAP, HAlign::Left, VAlign::Bottom),
            a(TextRole::Description, -13.0, 12.0, HAlign::Left, VAlign::Top),
            rating,
            a(TextRole::Caption, lo.x, lo.y - ANCHOR_GAP, HAlign::Left, VAlign::Top),
        ],
        // A pin shows its half's tag only, beside it.
        K::Pinpair | K::Pins | K::Pinp => vec![a(TextRole::Tag, -2.5, 0.0, HAlign::Right, VAlign::Middle)],
        K::Socket2
        | K::Socket3
        | K::Socket4
        | K::Socket6
        | K::Socket8
        | K::Socket12
        | K::Plug2
        | K::Plug3
        | K::Plug4
        | K::Plug6
        | K::Plug8
        | K::Plug12
        | K::Dsub9s
        | K::Dsub15s
        | K::Dsub25s
        | K::Dsub9p
        | K::Dsub15p
        | K::Dsub25p
        | K::M12s4
        | K::M12s5
        | K::M12s8
        | K::M12p4
        | K::M12p5
        | K::M12p8
        | K::Rj45
        | K::Usba
        | K::Usbb => vec![tag, description],
        K::Coil
        | K::No
        | K::Nc
        | K::Power3
        | K::Mcb1
        | K::Mcb2
        | K::Fuse
        | K::Psu
        | K::Pbno
        | K::Pbnc
        | K::Sel
        | K::Lamp
        | K::Motor3
        | K::Motor1
        | K::Trafo
        | K::TerminalBlock
        | K::TerminalBlockPe
        | K::Di
        | K::Do
        | K::Continuation
        | K::Co
        | K::Swco
        | K::Power4
        | K::Ssrin
        | K::Ssrout
        | K::Fuseterminalblock
        | K::Fusebank4
        | K::Fusebank6
        | K::Fusebank8
        | K::Fusebank10
        | K::Fusebank12
        | K::Prox3
        | K::Floatno
        | K::Floatnc
        | K::Rollerno
        | K::Rollernc
        | K::Interlocknc
        | K::Interlockno
        | K::Valve
        | K::Valve2
        | K::Tc
        | K::Heater
        | K::Term2L1
        | K::Term2L2
        | K::Term2
        | K::Term2bL1
        | K::Term2bL2
        | K::Term2b
        | K::Term3L1
        | K::Term3L2
        | K::Term3L3
        | K::Term3
        | K::TermlnpeL1
        | K::TermlnpeL2
        | K::TermlnpeL3
        | K::Termlnpe
        | K::TermsensorL1
        | K::TermsensorL2
        | K::TermsensorL3
        | K::Termsensor
        | K::Termdisc
        | K::Terminalblockfuse => vec![tag, description, rating],
    };
    if is_fuse_bank(s) {
        for (t, key) in d.connection_points.iter().filter_map(|t| Some((t, t.key.as_def().filter(|k| ecad_model::naming::words::is_digits(k))?))) {
            out.push(Anchor {
                role: TextRole::RatingOf(key.to_string()),
                at: p(t.at.x + 3.0, -8.0),
                h: HAlign::Left,
                v: VAlign::Middle,
                rotation: 90.0,
            });
        }
    }
    out
}
