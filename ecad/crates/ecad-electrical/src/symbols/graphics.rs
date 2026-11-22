//! The drawn graphics and connection points of every starter.

use super::*;

pub(super) fn p(x: f64, y: f64) -> Pt {
    Pt::new(x, y)
}

pub(super) fn line(a: (f64, f64), b: (f64, f64)) -> Primitive {
    Primitive::Line { a: p(a.0, a.1), b: p(b.0, b.1) }
}

pub(super) fn poly(v: &[(f64, f64)], closed: bool) -> Primitive {
    Primitive::Polyline { vertices: v.iter().map(|(x, y)| p(*x, *y)).collect(), closed }
}

pub(super) fn rect(a: (f64, f64), b: (f64, f64)) -> Primitive {
    poly(&[(a.0, a.1), (b.0, a.1), (b.0, b.1), (a.0, b.1)], true)
}

fn circle(c: (f64, f64), radius: f64) -> Primitive {
    Primitive::Circle { center: p(c.0, c.1), radius }
}

pub(super) fn text(at: (f64, f64), height: f64, value: &str) -> Primitive {
    Primitive::Text { at: p(at.0, at.1), height, rotation: Radians(0.0), value: value.into(), halign: Default::default(), valign: Default::default() }
}

fn shifted(prims: Vec<Primitive>, dx: f64) -> Vec<Primitive> {
    let m = |q: &Pt| p(q.x + dx, q.y);
    prims
        .into_iter()
        .map(|g| match g {
            Primitive::Line { a, b } => Primitive::Line { a: m(&a), b: m(&b) },
            Primitive::Polyline { vertices, closed } => Primitive::Polyline { vertices: vertices.iter().map(m).collect(), closed },
            Primitive::Circle { center, radius } => Primitive::Circle { center: m(&center), radius },
            other @ (Primitive::Arc { .. } | Primitive::Text { .. } | Primitive::Point { .. }) => other,
        })
        .collect()
}

/// A mechanical link drawn dashed (definitions carry no linetype): 1.5 mm dashes, 1 mm gaps.
fn dashed(x0: f64, x1: f64, y: f64) -> Vec<Primitive> {
    let mut out = Vec::new();
    let mut x = x0;
    // At most a few dozen dashes for the widths used here.
    while x < x1 && out.len() < 64 {
        out.push(line((x, y), ((x + 1.5).min(x1), y)));
        x += 2.5;
    }
    out
}

/// A vertical mechanical link drawn dashed, as [`dashed`] along y (design 034 §2.1).
fn dashed_v(x: f64, y0: f64, y1: f64) -> Vec<Primitive> {
    let mut out = Vec::new();
    let mut y = y0;
    while y < y1 && out.len() < 64 {
        out.push(line((x, y), (x, (y + 1.5).min(y1))));
        y += 2.5;
    }
    out
}

/// A closed ellipse of `n` segments (no arcs in definitions, design 034 §2.1).
fn ellipse(c: (f64, f64), rx: f64, ry: f64, n: usize) -> Primitive {
    let v: Vec<(f64, f64)> = (0..n)
        .map(|k| {
            let a = std::f64::consts::TAU * k as f64 / n as f64;
            (c.0 + rx * a.cos(), c.1 + ry * a.sin())
        })
        .collect();
    poly(&v, true)
}

/// A change-over contact, common on top (design 034 §3).
fn contact_co() -> Vec<Primitive> {
    vec![
        line((0.0, 10.0), (0.0, 4.0)),
        line((0.0, -10.0), (0.0, -4.0)),
        line((5.0, -10.0), (5.0, -4.0)),
        line((5.0, -4.0), (3.0, -4.0)),
        line((0.0, 4.0), (4.0, -5.5)),
    ]
}

/// A residual-current toroid and release, the release `dx` right of the RCBO's (design 034 §3).
fn rcd_release(toroid_x: f64, half_width: f64, dx: f64, link_from: f64) -> Vec<Primitive> {
    let mut g =
        vec![ellipse((toroid_x, -7.0), half_width, 1.5, 12), line((13.0 + dx, -7.0), (16.0 + dx, -7.0)), rect((16.0 + dx, -8.5), (19.0 + dx, -5.5))];
    g.extend(dashed_v(17.5 + dx, -5.5, 0.5));
    g.extend(dashed(link_from, 17.5 + dx, 0.5));
    g
}

/// A key for an interlock switch, at the left of its dashed link (design 034 §4).
fn key_mark() -> Vec<Primitive> {
    vec![circle((-12.0, 0.5), 1.0), line((-11.0, 0.5), (-8.0, 0.5)), line((-9.0, 0.5), (-9.0, -0.5))]
}

/// A contact (`make`: NO) operated through a dashed link by a lever ending at `lever_to` with a
/// circle of radius `r` at `mark_y` (float and roller switches, design 034 §4).
fn levered(make: bool, lever_to: f64, r: f64, mark_y: f64) -> Vec<Primitive> {
    let x = if make { -8.0 } else { -6.0 };
    let mut g = if make { contact_no() } else { contact_nc() };
    g.extend(if make { dashed(-8.0, -2.0, 0.5) } else { dashed(-6.0, 1.9, 0.5) });
    g.push(line((x, 0.5), (x, lever_to)));
    g.push(circle((x, mark_y), r));
    g
}

/// The upper half of a circle as `n` segments, from (c.x + r, c.y) over the top to (c.x − r, c.y)
/// (a socket's cup, open downwards; no arcs in definitions).
fn half_circle_up(c: (f64, f64), r: f64, n: usize) -> Primitive {
    let v: Vec<(f64, f64)> = (0..=n)
        .map(|k| {
            let a = std::f64::consts::PI * k as f64 / n as f64;
            (c.0 + r * a.cos(), c.1 + r * a.sin())
        })
        .collect();
    poly(&v, false)
}

/// The pin pitch of connector bodies, in millimetres (design 034 §5).
const PIN_PITCH: f64 = 5.0;

/// What a connector key names: its pin count and form.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Connector {
    Socket(usize),
    Plug(usize),
    DsubSocket(usize),
    DsubPlug(usize),
    M12Socket(usize),
    M12Plug(usize),
    /// A feed-through with `n` pins and a shield point.
    Bulkhead(usize),
}

impl Connector {
    /// The connector body a starter draws; `None` for any other starter.
    fn of(key: StarterKey) -> Option<Connector> {
        Some(match key {
            K::Rj45 => Connector::Bulkhead(8),
            K::Usba => Connector::Bulkhead(4),
            K::Usbb => Connector::Bulkhead(4),
            K::Socket2 => Connector::Socket(2),
            K::Plug2 => Connector::Plug(2),
            K::Socket3 => Connector::Socket(3),
            K::Plug3 => Connector::Plug(3),
            K::Socket4 => Connector::Socket(4),
            K::Plug4 => Connector::Plug(4),
            K::Socket6 => Connector::Socket(6),
            K::Plug6 => Connector::Plug(6),
            K::Socket8 => Connector::Socket(8),
            K::Plug8 => Connector::Plug(8),
            K::Socket12 => Connector::Socket(12),
            K::Plug12 => Connector::Plug(12),
            K::Dsub9s => Connector::DsubSocket(9),
            K::Dsub9p => Connector::DsubPlug(9),
            K::Dsub15s => Connector::DsubSocket(15),
            K::Dsub15p => Connector::DsubPlug(15),
            K::Dsub25s => Connector::DsubSocket(25),
            K::Dsub25p => Connector::DsubPlug(25),
            K::M12s4 => Connector::M12Socket(4),
            K::M12p4 => Connector::M12Plug(4),
            K::M12s5 => Connector::M12Socket(5),
            K::M12p5 => Connector::M12Plug(5),
            K::M12s8 => Connector::M12Socket(8),
            K::M12p8 => Connector::M12Plug(8),
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
            | K::Plccm
            | K::Chdi
            | K::Chdq
            | K::Chai
            | K::Chao
            | K::Chrtd
            | K::Chiol => return None,
        })
    }

    fn pins(self) -> usize {
        match self {
            Connector::Socket(n)
            | Connector::Plug(n)
            | Connector::DsubSocket(n)
            | Connector::DsubPlug(n)
            | Connector::M12Socket(n)
            | Connector::M12Plug(n)
            | Connector::Bulkhead(n) => n,
        }
    }

    /// Whether its pins point up (a socket) or down (a plug); a bulkhead's are at y = 0.
    fn socket(self) -> bool {
        matches!(self, Connector::Socket(_) | Connector::DsubSocket(_) | Connector::M12Socket(_))
    }

    fn graphics(self) -> Vec<Primitive> {
        let n = self.pins();
        let xn = pin_x(n, PIN_PITCH);
        let mut g = Vec::new();
        if let Connector::Bulkhead(_) = self {
            // Rings at every pin and the shield, one point each: a feed-through is one net.
            let xl = pin_x(n + 1, PIN_PITCH);
            g.extend((1..=n + 1).map(|i| circle((pin_x(i, PIN_PITCH), 0.0), 1.5)));
            g.push(rect((-4.0, -4.0), (xl + 4.0, 4.0)));
            g.push(line((-5.0, -5.0), (-5.0, 5.0)));
            g.push(line((xl + 5.0, -5.0), (xl + 5.0, 5.0)));
            return g;
        }
        for i in 1..=n {
            let x = pin_x(i, PIN_PITCH);
            if self.socket() {
                g.push(line((x, 5.0), (x, 1.5)));
                g.push(half_circle_up((x, 0.0), 1.5, 6));
            } else {
                g.push(line((x, -5.0), (x, -1.0)));
                g.push(rect((x - 0.5, -1.0), (x + 0.5, 1.5)));
            }
        }
        g.push(match self {
            Connector::DsubSocket(_) => poly(&[(-3.5, 3.5), (xn + 3.5, 3.5), (xn + 2.5, -2.5), (-2.5, -2.5)], true),
            Connector::DsubPlug(_) => poly(&[(-2.5, 2.5), (xn + 2.5, 2.5), (xn + 3.5, -3.5), (-3.5, -3.5)], true),
            Connector::Socket(_) | Connector::M12Socket(_) => rect((-3.5, -2.5), (xn + 3.5, 3.5)),
            Connector::Plug(_) | Connector::M12Plug(_) | Connector::Bulkhead(_) => rect((-3.5, -3.5), (xn + 3.5, 2.5)),
        });
        match self {
            Connector::M12Socket(_) => g.push(circle((-6.5, 0.5), 2.0)),
            Connector::M12Plug(_) => g.push(circle((-6.5, -0.5), 2.0)),
            Connector::Socket(_) | Connector::Plug(_) | Connector::DsubSocket(_) | Connector::DsubPlug(_) | Connector::Bulkhead(_) => {}
        }
        g
    }

    fn connection_points(self, t: &dyn Fn(&str, f64, f64) -> ConnectionPointDef) -> Vec<ConnectionPointDef> {
        let n = self.pins();
        let y = match self {
            Connector::Bulkhead(_) => 0.0,
            Connector::Socket(_) | Connector::DsubSocket(_) | Connector::M12Socket(_) => 5.0,
            Connector::Plug(_) | Connector::DsubPlug(_) | Connector::M12Plug(_) => -5.0,
        };
        let mut v: Vec<ConnectionPointDef> = (1..=n).map(|i| t(&i.to_string(), pin_x(i, PIN_PITCH), y)).collect();
        if let Connector::Bulkhead(_) = self {
            v.push(t("SH", pin_x(n + 1, PIN_PITCH), 0.0));
        }
        v
    }
}

/// A PLC card body as the card's power end (brief 051, design 051 §2.4): on each side that has
/// points, an edge through them and dividers between neighbouring points and at both ends, down
/// (or up) the header band; width the points' span plus 10, at least 20. The type word is the
/// frame's type text now, not graphics.
fn card_body(points: &[(f64, f64)]) -> Vec<Primitive> {
    let mut g = Vec::new();
    for side in [1.0_f64, -1.0] {
        let mut xs: Vec<f64> = points.iter().filter(|(_, y)| y.signum() == side).map(|(x, _)| *x).collect();
        if xs.is_empty() {
            continue;
        }
        xs.sort_by(f64::total_cmp);
        let (lo, hi) = (xs.first().copied().unwrap_or(0.0), xs.last().copied().unwrap_or(0.0));
        let pad = ((20.0 - (hi - lo)) / 2.0).max(5.0);
        let (l, r) = (lo - pad, hi + pad);
        let y = 25.0 * side;
        let d = y - HEADER_BAND * side;
        g.push(line((l, y), (r, y)));
        let mut cuts = vec![l, r];
        cuts.extend(xs.windows(2).filter_map(|w| Some((w.first()? + w.get(1)?) / 2.0)));
        g.extend(cuts.into_iter().map(|x| line((x, y), (x, d))));
    }
    g
}

/// Pin `i`'s x (1-based) in a row of pins or fuses spaced `pitch` apart.
fn pin_x(i: usize, pitch: f64) -> f64 {
    pitch * i.saturating_sub(1) as f64
}

/// A fuse bank of `n` ways (design 034 §3).
fn fuse_bank(n: usize) -> Vec<Primitive> {
    let xn = pin_x(n, 10.0);
    let mut g = vec![
        rect((-15.0, -10.0), (xn + 5.0, 15.0)),
        line((-10.0, 20.0), (-10.0, 10.0)),
        line((-10.0, 10.0), (xn, 10.0)),
        line((-10.0, -15.0), (-10.0, -5.0)),
        line((-12.0, -5.0), (-8.0, -5.0)),
    ];
    for i in 1..=n {
        let x = pin_x(i, 10.0);
        g.push(line((x, 10.0), (x, -15.0)));
        g.push(rect((x - 2.0, -3.0), (x + 2.0, 7.0)));
    }
    g
}

/// Graphics of the sized keys (fuse banks, connectors).
fn sized_graphics(key: StarterKey) -> Vec<Primitive> {
    if let Some(n) = key.fuse_bank_ways() {
        return fuse_bank(n);
    }
    Connector::of(key).map(Connector::graphics).unwrap_or_default()
}

/// Terminals of the sized keys.
fn sized_connection_points(key: StarterKey, t: &dyn Fn(&str, f64, f64) -> ConnectionPointDef) -> Vec<ConnectionPointDef> {
    if let Some(n) = key.fuse_bank_ways() {
        let mut v = vec![t("L+", -10.0, 20.0), t("M", -10.0, -15.0)];
        v.extend((1..=n).map(|i| t(&i.to_string(), pin_x(i, 10.0), -15.0)));
        return v;
    }
    Connector::of(key).map(|c| c.connection_points(t)).unwrap_or_default()
}

/// A socket pin (brief 044): its lead from (0, 5) and 034's cup, open downwards, centre (0, 0).
fn socket_pin() -> Vec<Primitive> {
    vec![line((0.0, 5.0), (0.0, 1.5)), half_circle_up((0.0, 0.0), 1.5, 6)]
}

/// A plug pin (brief 044): its lead from (0, −5) and the pin.
fn plug_pin() -> Vec<Primitive> {
    vec![line((0.0, -5.0), (0.0, -1.5)), rect((-0.5, -1.5), (0.5, 1.0))]
}

fn contact_no() -> Vec<Primitive> {
    vec![line((0.0, 10.0), (0.0, 4.0)), line((0.0, -10.0), (0.0, -4.0)), line((0.0, -4.0), (-4.0, 5.0))]
}

fn contact_nc() -> Vec<Primitive> {
    vec![line((0.0, 10.0), (0.0, 4.0)), line((0.0, -10.0), (0.0, -4.0)), line((0.0, 4.0), (3.0, 4.0)), line((0.0, -4.0), (4.0, 5.5))]
}

fn mcb() -> Vec<Primitive> {
    let mut g = contact_no();
    g.push(line((-1.5, 2.5), (1.5, 5.5)));
    g.push(line((-1.5, 5.5), (1.5, 2.5)));
    g
}

fn motor(phase: &str, leads: &[((f64, f64), (f64, f64))]) -> Vec<Primitive> {
    let mut g = vec![circle((0.0, 0.0), 10.0), text((-2.0, -0.5), 5.0, "M"), text((-2.5, -6.0), 3.0, phase)];
    g.extend(leads.iter().map(|(a, b)| line(*a, *b)));
    g.push(poly(&[(20.0, 15.0), (20.0, 0.0), (10.0, 0.0)], false));
    g
}

/// The graphics of starter `key` (brief 036: `terminalblockfuse` draws `fuseterminalblock`'s).
pub(crate) fn graphics_of(key: StarterKey) -> Vec<Primitive> {
    graphics(key)
}

pub(super) fn graphics(key: StarterKey) -> Vec<Primitive> {
    match key {
        K::Coil => vec![rect((-6.0, -3.0), (6.0, 3.0)), line((0.0, 3.0), (0.0, 10.0)), line((0.0, -3.0), (0.0, -10.0))],
        K::No => contact_no(),
        K::Nc => contact_nc(),
        K::Power3 => {
            let mut g = Vec::new();
            for x in [-10.0, 0.0, 10.0] {
                g.extend(shifted(contact_no(), x));
            }
            g.extend(dashed(-12.0, 8.0, 0.5));
            g
        }
        K::Mcb1 => mcb(),
        K::Mcb2 => {
            let mut g = mcb();
            g.extend(shifted(mcb(), 10.0));
            g.extend(dashed(-2.0, 8.0, 0.5));
            g
        }
        K::Fuse => vec![rect((-2.0, -5.0), (2.0, 5.0)), line((0.0, 10.0), (0.0, -10.0))],
        K::Psu => {
            let mut g = vec![
                rect((-15.0, -10.0), (15.0, 10.0)),
                line((-15.0, -10.0), (15.0, 10.0)),
                text((-13.0, 4.0), 3.5, "~"),
                text((8.0, -8.0), 3.5, "="),
            ];
            for x in [-10.0, 0.0, 10.0] {
                g.push(line((x, 15.0), (x, 10.0)));
            }
            for x in [-5.0, 5.0] {
                g.push(line((x, -15.0), (x, -10.0)));
            }
            g
        }
        K::Pbno => {
            let mut g = contact_no();
            g.extend(dashed(-8.0, -2.0, 0.5));
            g.push(poly(&[(-10.0, 3.0), (-8.0, 3.0), (-8.0, -2.0), (-10.0, -2.0)], false));
            g
        }
        K::Pbnc => {
            let mut g = contact_nc();
            g.extend(dashed(-6.0, 1.9, 0.5));
            g.push(poly(&[(-8.0, 3.0), (-6.0, 3.0), (-6.0, -2.0), (-8.0, -2.0)], false));
            g
        }
        K::Sel => {
            let mut g = contact_no();
            g.extend(dashed(-8.0, -2.0, 0.5));
            g.push(poly(&[(-10.0, 3.0), (-8.0, 3.0), (-8.0, -2.0), (-6.0, -2.0)], false));
            g
        }
        K::Lamp => vec![
            circle((0.0, 0.0), 4.0),
            line((-2.83, -2.83), (2.83, 2.83)),
            line((-2.83, 2.83), (2.83, -2.83)),
            line((0.0, 4.0), (0.0, 10.0)),
            line((0.0, -4.0), (0.0, -10.0)),
        ],
        K::Motor3 => motor("3~", &[((-10.0, 15.0), (-5.0, 8.66)), ((0.0, 15.0), (0.0, 10.0)), ((10.0, 15.0), (5.0, 8.66))]),
        K::Motor1 => motor("1~", &[((-5.0, 15.0), (-5.0, 8.66)), ((5.0, 15.0), (5.0, 8.66))]),
        K::Trafo => {
            // Where x = ±5 meets each circle of radius 6 around (0, ±4.5).
            let y = 4.5 + (36.0_f64 - 25.0).sqrt();
            vec![
                circle((0.0, 4.5), 6.0),
                circle((0.0, -4.5), 6.0),
                line((-5.0, 15.0), (-5.0, y)),
                line((5.0, 15.0), (5.0, y)),
                line((-5.0, -15.0), (-5.0, -y)),
                line((5.0, -15.0), (5.0, -y)),
            ]
        }
        K::TerminalBlock => vec![circle((0.0, 0.0), 1.5)],
        // Our own earth mark to the right of the ring (design 028 §2).
        K::TerminalBlockPe => {
            vec![circle((0.0, 0.0), 1.5), line((2.5, -1.2), (2.5, 1.2)), line((3.2, -0.8), (3.2, 0.8)), line((3.9, -0.4), (3.9, 0.4))]
        }
        // An arrow pointing away from its wire (design 022 §3.2).
        K::Continuation => vec![line((0.0, 0.0), (0.0, -5.0)), poly(&[(-1.5, -3.5), (0.0, -5.0), (1.5, -3.5)], false)],
        K::Di => vec![rect((-7.5, -2.5), (7.5, 2.5)), text((-2.5, -1.25), 2.5, "DI"), line((0.0, 2.5), (0.0, 10.0))],
        K::Do => vec![rect((-7.5, -2.5), (7.5, 2.5)), text((-2.5, -1.25), 2.5, "DO"), line((0.0, -2.5), (0.0, -10.0))],
        // ---- Brief 034 stage 1 (design 034 §3).
        K::Co => contact_co(),
        K::Swco => {
            let mut g = contact_co();
            g.extend(dashed(-8.0, 1.5, 0.5));
            g.push(line((-8.0, -2.5), (-8.0, 3.5)));
            g
        }
        K::Power4 => {
            let mut g = Vec::new();
            for x in [-15.0, -5.0, 5.0, 15.0] {
                g.extend(shifted(contact_no(), x));
            }
            g.extend(dashed(-17.0, 13.0, 0.5));
            g
        }
        K::Rcbo2 | K::Rcd2 => {
            let mut g = if key == K::Rcbo2 { mcb() } else { contact_no() };
            g.extend(shifted(contact_no(), 10.0));
            g.extend(dashed(-2.0, 8.0, 0.5));
            g.extend(rcd_release(5.0, 8.0, 0.0, 8.0));
            g
        }
        K::Rcd4 => {
            let mut g = Vec::new();
            for x in [0.0, 10.0, 20.0, 30.0] {
                g.extend(shifted(contact_no(), x));
            }
            g.extend(dashed(-2.0, 28.0, 0.5));
            g.extend(rcd_release(15.0, 18.0, 20.0, 28.0));
            g
        }
        K::Ssrin => vec![
            rect((-6.0, -3.0), (6.0, 3.0)),
            line((0.0, 3.0), (0.0, 10.0)),
            line((0.0, -3.0), (0.0, -10.0)),
            poly(&[(-2.0, 1.5), (2.0, 1.5), (0.0, -1.5)], true),
            line((-2.0, -1.5), (2.0, -1.5)),
        ],
        K::Ssrout => {
            let mut g = contact_no();
            g.push(poly(&[(2.0, -1.5), (5.0, -1.5), (3.5, 1.5)], true));
            g
        }
        K::Fuseterminalblock => vec![rect((-2.0, -5.0), (2.0, 5.0)), line((0.0, 10.0), (0.0, -10.0)), rect((-3.5, -7.5), (3.5, 7.5))],
        // ---- Brief 034 stage 2 (design 034 §4).
        K::Prox3 => vec![
            rect((-10.0, -5.0), (10.0, 5.0)),
            poly(&[(-7.0, 0.0), (-5.0, 2.0), (-3.0, 0.0), (-5.0, -2.0)], true),
            line((3.0, 2.5), (3.0, 1.5)),
            line((3.0, -2.5), (3.0, -1.5)),
            line((3.0, -1.5), (1.5, 2.0)),
            line((-5.0, 10.0), (-5.0, 5.0)),
            line((-5.0, -10.0), (-5.0, -5.0)),
            line((5.0, -10.0), (5.0, -5.0)),
        ],
        K::Floatno => levered(true, -4.0, 1.5, -5.5),
        K::Floatnc => levered(false, -4.0, 1.5, -5.5),
        K::Rollerno => levered(true, 3.0, 1.2, 4.2),
        K::Rollernc => levered(false, 3.0, 1.2, 4.2),
        K::Interlocknc => {
            let mut g = contact_nc();
            g.extend(dashed(-8.0, 1.9, 0.5));
            g.push(circle((-4.0, 0.5), 1.5));
            g.push(line((-5.0, 0.5), (-3.0, 0.5)));
            g.push(poly(&[(-3.6, 1.1), (-3.0, 0.5), (-3.6, -0.1)], false));
            g.extend(key_mark());
            g
        }
        K::Interlockno => {
            let mut g = contact_no();
            g.extend(dashed(-8.0, -2.0, 0.5));
            g.extend(key_mark());
            g
        }
        K::Valve => vec![
            rect((-6.0, -3.0), (6.0, 3.0)),
            line((0.0, 3.0), (0.0, 10.0)),
            line((0.0, -3.0), (0.0, -10.0)),
            line((6.0, 0.0), (8.0, 0.0)),
            poly(&[(8.0, 3.0), (14.0, -3.0), (14.0, 3.0), (8.0, -3.0)], true),
        ],
        K::Valve2 => vec![
            rect((-16.0, -3.0), (-4.0, 3.0)),
            rect((4.0, -3.0), (16.0, 3.0)),
            line((-10.0, 3.0), (-10.0, 10.0)),
            line((-10.0, -3.0), (-10.0, -10.0)),
            line((10.0, 3.0), (10.0, 10.0)),
            line((10.0, -3.0), (10.0, -10.0)),
            poly(&[(-3.0, 3.0), (3.0, -3.0), (3.0, 3.0), (-3.0, -3.0)], true),
            line((-4.0, 0.0), (-3.0, 0.0)),
            line((3.0, 0.0), (4.0, 0.0)),
        ],
        K::Tc => vec![
            poly(&[(-5.0, 10.0), (-5.0, 0.0), (0.0, -5.0)], false),
            poly(&[(5.0, 10.0), (5.0, 0.0), (0.0, -5.0)], false),
            line((5.5, 9.0), (5.5, 0.5)),
            circle((0.0, -5.0), 0.5),
            circle((0.0, -5.0), 0.33),
            circle((0.0, -5.0), 0.17),
        ],
        K::Heater => vec![
            rect((-2.5, -6.0), (2.5, 6.0)),
            line((-2.5, -3.0), (2.5, -3.0)),
            line((-2.5, 0.0), (2.5, 0.0)),
            line((-2.5, 3.0), (2.5, 3.0)),
            line((0.0, 6.0), (0.0, 10.0)),
            line((0.0, -6.0), (0.0, -10.0)),
        ],
        // ---- Brief 034 stage 3b (design 034 §6).
        K::Plcps => card_body(&[(-10.0, 25.0), (-5.0, 25.0), (0.0, 25.0), (-5.0, -25.0), (5.0, -25.0)]),
        K::Plccm => card_body(&[(-5.0, -25.0), (5.0, -25.0)]),
        K::Plccpu | K::Plcdi | K::Plcdq | K::Plcai | K::Plcao | K::Plcrtd | K::Plciol | K::Plcim | K::Plcpm => {
            card_body(&[(-5.0, 25.0), (5.0, 25.0)])
        }
        k @ (K::Chdi | K::Chdq | K::Chai | K::Chao | K::Chrtd | K::Chiol) => cell_graphics(k),
        // ---- Brief 044 (design 044 §3.2): connector pins, 034's cup and pin, no body.
        K::Pinpair => {
            let mut g = socket_pin();
            g.extend(plug_pin());
            g
        }
        K::Pins => socket_pin(),
        K::Pinp => plug_pin(),
        K::Fusebank4
        | K::Fusebank6
        | K::Fusebank8
        | K::Fusebank10
        | K::Fusebank12
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
        | K::Terminalblockfuse => crate::terminal_strips::terminal_blocks::graphics(key).unwrap_or_else(|| sized_graphics(key)),
    }
}

pub(super) fn connection_points(key: StarterKey) -> Vec<ConnectionPointDef> {
    let t = |k: &str, x: f64, y: f64| ConnectionPointDef { key: PointKey::def(k), label: Some(k.into()), at: p(x, y), accepts: Default::default() };
    let side = |k: PointKey, x: f64, y: f64| ConnectionPointDef { label: Some(k.to_string()), key: k, at: p(x, y), accepts: Default::default() };
    match key {
        K::Coil => vec![t("A1", 0.0, 10.0), t("A2", 0.0, -10.0)],
        K::No | K::Pbno | K::Sel | K::Floatno | K::Rollerno | K::Interlockno => vec![t("13", 0.0, 10.0), t("14", 0.0, -10.0)],
        K::Nc | K::Pbnc | K::Floatnc | K::Rollernc | K::Interlocknc => vec![t("11", 0.0, 10.0), t("12", 0.0, -10.0)],
        K::Power3 => vec![t("1", -10.0, 10.0), t("2", -10.0, -10.0), t("3", 0.0, 10.0), t("4", 0.0, -10.0), t("5", 10.0, 10.0), t("6", 10.0, -10.0)],
        K::Mcb1 | K::Fuse => vec![t("1", 0.0, 10.0), t("2", 0.0, -10.0)],
        K::Mcb2 => vec![t("1", 0.0, 10.0), t("2", 0.0, -10.0), t("3", 10.0, 10.0), t("4", 10.0, -10.0)],
        K::Psu => {
            vec![t("L", -10.0, 15.0), t("N", 0.0, 15.0), t("PE", 10.0, 15.0), t("+", -5.0, -15.0), t("-", 5.0, -15.0)]
        }
        K::Lamp => vec![t("X1", 0.0, 10.0), t("X2", 0.0, -10.0)],
        K::Motor3 => vec![t("U1", -10.0, 15.0), t("V1", 0.0, 15.0), t("W1", 10.0, 15.0), t("PE", 20.0, 15.0)],
        K::Motor1 => vec![t("U1", -5.0, 15.0), t("U2", 5.0, 15.0), t("PE", 20.0, 15.0)],
        K::Trafo => vec![t("1U1", -5.0, 15.0), t("1U2", 5.0, 15.0), t("2U1", -5.0, -15.0), t("2U2", 5.0, -15.0)],
        K::TerminalBlock | K::TerminalBlockPe => vec![t("1", 0.0, 0.0)],
        // No label: a continuation shows no connection point name.
        K::Continuation => vec![ConnectionPointDef { key: PointKey::def("1"), label: None, at: p(0.0, 0.0), accepts: Default::default() }],
        // Design 072 §3: a channel symbol's point is keyed by its role; the label is what it shows.
        K::Di => vec![ConnectionPointDef {
            key: PointKey::Role(ecad_model::ChannelRole::In),
            label: Some("I0.0".into()),
            at: p(0.0, 10.0),
            accepts: Default::default(),
        }],
        K::Do => vec![ConnectionPointDef {
            key: PointKey::Role(ecad_model::ChannelRole::Out),
            label: Some("Q0.0".into()),
            at: p(0.0, -10.0),
            accepts: Default::default(),
        }],
        // ---- Brief 034 stage 1: definition order is common, NC, NO (design 034 §2.3).
        K::Co | K::Swco => vec![t("11", 0.0, 10.0), t("12", 5.0, -10.0), t("14", 0.0, -10.0)],
        K::Power4 => vec![
            t("1", -15.0, 10.0),
            t("2", -15.0, -10.0),
            t("3", -5.0, 10.0),
            t("4", -5.0, -10.0),
            t("5", 5.0, 10.0),
            t("6", 5.0, -10.0),
            t("7", 15.0, 10.0),
            t("8", 15.0, -10.0),
        ],
        K::Rcbo2 | K::Rcd2 => vec![t("1", 0.0, 10.0), t("2", 0.0, -10.0), t("N1", 10.0, 10.0), t("N2", 10.0, -10.0)],
        K::Rcd4 => vec![
            t("1", 0.0, 10.0),
            t("2", 0.0, -10.0),
            t("3", 10.0, 10.0),
            t("4", 10.0, -10.0),
            t("5", 20.0, 10.0),
            t("6", 20.0, -10.0),
            t("N1", 30.0, 10.0),
            t("N2", 30.0, -10.0),
        ],
        K::Ssrin => vec![t("A1", 0.0, 10.0), t("A2", 0.0, -10.0)],
        K::Ssrout | K::Fuseterminalblock | K::Heater => vec![t("1", 0.0, 10.0), t("2", 0.0, -10.0)],
        // ---- Brief 034 stage 2: IEC 60947-5-2's numbers (1 supply +, 3 supply −, 4 output).
        K::Prox3 => vec![t("1", -5.0, 10.0), t("3", -5.0, -10.0), t("4", 5.0, -10.0)],
        K::Valve => vec![t("A1", 0.0, 10.0), t("A2", 0.0, -10.0)],
        K::Valve2 => vec![t("A1", -10.0, 10.0), t("A2", -10.0, -10.0), t("B1", 10.0, 10.0), t("B2", 10.0, -10.0)],
        K::Tc => vec![t("+", -5.0, 10.0), t("-", 5.0, 10.0)],
        // ---- Brief 034 stage 3b.
        K::Plcps => vec![t("L", -10.0, 25.0), t("N", -5.0, 25.0), t("PE", 0.0, 25.0), t("L+", -5.0, -25.0), t("M", 5.0, -25.0)],
        K::Plccpu | K::Plcdi | K::Plcdq | K::Plcai | K::Plcao | K::Plcrtd | K::Plciol => vec![t("L+", -5.0, 25.0), t("M", 5.0, 25.0)],
        // ---- Brief 045 (design 045 §3.1).
        K::Plcim | K::Plcpm => vec![t("L+", -5.0, 25.0), t("M", 5.0, 25.0)],
        K::Plccm => vec![t("P1", -5.0, -25.0), t("P2", 5.0, -25.0)],
        // Brief 045 (design 045 §4.1, FP2): a cell's points are keyed by role; the pin is the
        // label given when it joins a card.
        k @ (K::Chdi | K::Chdq | K::Chai | K::Chao | K::Chrtd | K::Chiol) => cell_shape(k)
            .map(|(_, pts)| {
                pts.iter()
                    .filter_map(|(role, x)| {
                        let role = ecad_model::ChannelRole::exact(role)?;
                        Some(ConnectionPointDef {
                            key: PointKey::Role(role),
                            label: Some(role.word().to_string()),
                            at: p(*x, 5.0),
                            accepts: Default::default(),
                        })
                    })
                    .collect()
            })
            .unwrap_or_default(),
        // ---- Brief 044: a pin's definition keys are its side keys.
        K::Pinpair => vec![side(SOCKET_KEY, 0.0, 5.0), side(PLUG_KEY, 0.0, -5.0)],
        K::Pins => vec![side(SOCKET_KEY, 0.0, 5.0)],
        K::Pinp => vec![side(PLUG_KEY, 0.0, -5.0)],
        K::Fusebank4
        | K::Fusebank6
        | K::Fusebank8
        | K::Fusebank10
        | K::Fusebank12
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
        | K::Terminalblockfuse => crate::terminal_strips::terminal_blocks::connection_points(key).unwrap_or_else(|| sized_connection_points(key, &t)),
    }
}

/// The local bounding box of a definition's graphics and terminals (text counted at its insertion
/// point and height). `(0,0)`–`(0,0)` for an empty definition.
pub fn bounds(d: &Definition) -> (Pt, Pt) {
    let mut lo = p(f64::INFINITY, f64::INFINITY);
    let mut hi = p(f64::NEG_INFINITY, f64::NEG_INFINITY);
    let mut add = |q: Pt| {
        if q.is_finite() {
            lo = p(lo.x.min(q.x), lo.y.min(q.y));
            hi = p(hi.x.max(q.x), hi.y.max(q.y));
        }
    };
    for g in &d.graphics {
        match g {
            Primitive::Line { a, b } => {
                add(*a);
                add(*b);
            }
            Primitive::Polyline { vertices, .. } => vertices.iter().for_each(|v| add(*v)),
            Primitive::Circle { center, radius } | Primitive::Arc { center, radius, .. } => {
                add(p(center.x - radius, center.y - radius));
                add(p(center.x + radius, center.y + radius));
            }
            Primitive::Text { at, height, value, .. } => {
                add(*at);
                add(p(at.x + height * 0.8 * value.chars().count() as f64, at.y + height));
            }
            Primitive::Point { at } => add(*at),
        }
    }
    for t in &d.connection_points {
        add(t.at);
    }
    if !(lo.is_finite() && hi.is_finite()) {
        return (p(0.0, 0.0), p(0.0, 0.0));
    }
    (lo, hi)
}

/// The point grid of the builder, the default of `symbol.point_grid` (design 048 §7.1, Q6).
pub const POINT_GRID_MM: f64 = 5.0;

/// Whether placed copy `rep` shows an older version than its lineage's newest (design 048 §8.4).
pub fn update_available(doc: &Document, rep: ecad_model::RepresentationId) -> Option<(DefinitionId, u64, u64)> {
    let r = doc.representations().get(&rep)?;
    let (d, _) = crate::devices::tags::rep_definition(doc, r)?;
    let o = d.origin.as_ref().filter(|o| o.library == PROJECT_SYMBOLS)?;
    let n = newest(doc, o.definition)?;
    (n.seq > o.library_seq).then_some((n.id, o.library_seq, n.seq))
}

/// A connector side's word in the connector tool's prompts.
pub fn side_word(side: Side) -> &'static str {
    match side {
        Side::Socket => "Socket",
        Side::Plug => "Plug",
        Side::Both => "Both",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Overseer ruling 4: `dashed` draws what it drew before brief 034, and `dashed_v` is the same
    /// pattern along y.
    #[test]
    fn dashed_links_are_unchanged_and_the_vertical_one_matches() {
        let ends = |g: &[Primitive]| {
            g.iter()
                .map(|x| match x {
                    Primitive::Line { a, b } => (a.x, a.y, b.x, b.y),
                    Primitive::Polyline { .. }
                    | Primitive::Arc { .. }
                    | Primitive::Circle { .. }
                    | Primitive::Text { .. }
                    | Primitive::Point { .. } => {
                        panic!("a dash is a line")
                    }
                })
                .collect::<Vec<_>>()
        };
        assert_eq!(ends(&dashed(-2.0, 8.0, 0.5)), [(-2.0, 0.5, -0.5, 0.5), (0.5, 0.5, 2.0, 0.5), (3.0, 0.5, 4.5, 0.5), (5.5, 0.5, 7.0, 0.5)]);
        assert_eq!(ends(&dashed(-6.0, 1.9, 0.5)).last(), Some(&(1.5, 0.5, 1.9, 0.5)));
        for (a, b, at) in [(-5.5, 0.5, 17.5), (-6.0, 1.9, 0.0), (0.0, 10.0, 3.0)] {
            let h: Vec<_> = ends(&dashed(a, b, at)).into_iter().map(|(x0, _, x1, _)| (at, x0, at, x1)).collect();
            assert_eq!(ends(&dashed_v(at, a, b)), h);
        }
    }
}
