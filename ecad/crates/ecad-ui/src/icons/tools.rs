//! The seven tool groups of the Draw workspace (spec §2). Tool icons echo the schematic.

use super::Pen;

/// Select: an arrow pointer.
pub fn select(p: &mut Pen) {
    p.poly(&[(4.0, 2.0), (4.0, 13.0), (7.0, 10.0), (9.0, 14.0), (10.5, 13.3), (8.6, 9.4), (12.5, 9.4)], true);
}

/// Wire: a line between two end rings.
pub fn wire(p: &mut Pen) {
    p.circle(2.75, 8.0, 1.75);
    p.line((4.5, 8.0), (11.5, 8.0));
    p.circle(13.25, 8.0, 1.75);
}

/// Place: a symbol box with a connection point ring on its lead.
pub fn place(p: &mut Pen) {
    p.rect(4.0, 1.5, 12.0, 9.5);
    p.line((8.0, 9.5), (8.0, 11.75));
    p.circle(8.0, 13.25, 1.5);
}

/// Cable: a slash across three lines.
pub fn cable(p: &mut Pen) {
    for y in [5.0, 8.0, 11.0] {
        p.line((1.5, y), (14.5, y));
    }
    p.line((5.0, 14.0), (11.0, 2.0));
}

/// Draw: an arc over a line (plain geometry).
pub fn draw(p: &mut Pen) {
    p.arc(8.0, 10.0, 5.5, 180.0, 360.0);
    p.line((1.5, 13.0), (14.5, 13.0));
}

/// Box: a dashed box (a location or function box).
pub fn boxed(p: &mut Pen) {
    let (x0, y0, x1, y1) = (2.0, 3.0, 14.0, 13.0);
    p.dashed((x0, y0), (x1, y0), 2.0, 1.5);
    p.dashed((x1, y0), (x1, y1), 2.0, 1.5);
    p.dashed((x1, y1), (x0, y1), 2.0, 1.5);
    p.dashed((x0, y1), (x0, y0), 2.0, 1.5);
}

/// Text: a letter T.
pub fn text(p: &mut Pen) {
    p.line((3.0, 3.0), (13.0, 3.0));
    p.line((8.0, 3.0), (8.0, 13.0));
    p.line((6.0, 13.0), (10.0, 13.0));
}
