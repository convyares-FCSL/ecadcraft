//! The PLC channel-cell shapes and their graphics.

use super::*;

use crate::plc::rack::CardType as CT;

/// A PLC channel cell: `Kind::Channel`, every point quiet.
pub(super) const fn cell(key: StarterKey, name: &'static str, aliases: &'static [&'static str], t: CT, id: DefinitionId) -> Starter {
    st(key, name, aliases, Kind::Channel(t), id).of(Family::Plc).quiet_at(Quiet::All)
}

/// A cell's width and its points (role, x) (the
/// pack's pitch, 20 mm per single-point channel).
pub(crate) fn cell_shape(key: StarterKey) -> Option<(f64, &'static [(&'static str, f64)])> {
    Some(match key {
        K::Chdi => (20.0, &[("in", 0.0)]),
        K::Chdq => (20.0, &[("out", 0.0)]),
        K::Chai | K::Chao => (20.0, &[("+", -5.0), ("-", 5.0)]),
        K::Chrtd => (40.0, &[("I+", -15.0), ("S+", -5.0), ("S-", 5.0), ("I-", 15.0)]),
        K::Chiol => (30.0, &[("L+", -10.0), ("C/Q", 0.0), ("L-", 10.0)]),
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
        | K::Rcbo2
        | K::Rcd2
        | K::Rcd4
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
        | K::Socket2
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
        | K::Usbb
        | K::Pinpair
        | K::Pins
        | K::Pinp
        | K::Plccpu
        | K::Plcps
        | K::Plcdi
        | K::Plcdq
        | K::Plcai
        | K::Plcao
        | K::Plcrtd
        | K::Plciol
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
        | K::Terminalblockfuse
        | K::Plcim
        | K::Plcpm
        | K::Plccm => return None,
    })
}

/// The line of a cell's points: its top edge, local y.
pub(crate) const CELL_TOP: f64 = 5.0;
/// The header band under the top edge (the dividers), mm.
pub(crate) const HEADER_BAND: f64 = 10.0;
/// The mark band under the header, mm; a cell's mark sits at its centre.
pub(crate) const MARK_BAND: f64 = 15.0;
/// The name band at the built-in text styles, mm (at least 12.5).
pub(crate) const NAME_BAND: f64 = 12.5;
/// A channel text's end under the frame's bottom edge, mm.
pub(crate) const OUTSIDE_GAP: f64 = 2.0;
/// The frame's depth under the top edge at the built-in text styles: header, mark and name bands.
pub(crate) const FRAME_DEPTH: f64 = HEADER_BAND + MARK_BAND + NAME_BAND;
/// The height of a role name drawn under its pin, mm (`point_label`'s built-in).
const ROLE_H: f64 = 1.5;

/// A cell's graphics: its share of the card's top edge through its
/// points, its two dividers down the header band (adjacent cells draw theirs on one line), the role
/// names under the pins of a multi-point cell, and its mark at the centre of the mark band. No
/// bottom edge and no leads: the card's frame closes the box.
pub(super) fn cell_graphics(key: StarterKey) -> Vec<Primitive> {
    let Some((w, points)) = cell_shape(key) else { return Vec::new() };
    let h = w / 2.0;
    let div = CELL_TOP - HEADER_BAND;
    let mut g = vec![line((-h, CELL_TOP), (h, CELL_TOP)), line((-h, CELL_TOP), (-h, div)), line((h, CELL_TOP), (h, div))];
    if points.len() > 1 {
        for (role, x) in points {
            // Under the pin label, which sits right of the point, 2.5 under the edge.
            g.push(text((*x + 1.0, CELL_TOP - 6.5), ROLE_H, role));
        }
    }
    // The marks: new drawings for ECAD (rules: mm, no arcs).
    let c = CELL_TOP - HEADER_BAND - MARK_BAND / 2.0;
    let into = |dy: f64| poly(&[(-1.5, c + 1.5 + dy), (0.0, c - 1.5 + dy), (1.5, c + 1.5 + dy)], false);
    let out = |dy: f64| poly(&[(-1.5, c - 1.5 + dy), (0.0, c + 1.5 + dy), (1.5, c - 1.5 + dy)], false);
    let tilde = poly(&[(-3.0, c - 2.0), (-1.5, c - 1.0), (0.0, c - 2.0), (1.5, c - 3.0), (3.0, c - 2.0)], false);
    match key {
        K::Chdi => g.push(into(0.0)),
        K::Chdq => g.push(out(0.0)),
        K::Chai => g.extend([into(1.5), tilde]),
        K::Chao => g.extend([out(1.5), tilde]),
        // A small resistor with a slash.
        K::Chrtd => g.extend([rect((-2.0, c - 1.0), (2.0, c + 1.0)), line((-2.5, c - 1.5), (2.5, c + 1.5))]),
        // A small diamond.
        K::Chiol => g.push(poly(&[(0.0, c + 1.5), (1.5, c), (0.0, c - 1.5), (-1.5, c)], true)),
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
        | K::Rcbo2
        | K::Rcd2
        | K::Rcd4
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
        | K::Socket2
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
        | K::Usbb
        | K::Pinpair
        | K::Pins
        | K::Pinp
        | K::Plccpu
        | K::Plcps
        | K::Plcdi
        | K::Plcdq
        | K::Plcai
        | K::Plcao
        | K::Plcrtd
        | K::Plciol
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
        | K::Terminalblockfuse
        | K::Plcim
        | K::Plcpm
        | K::Plccm => {}
    }
    g
}

// Aliases.
