//! The four workspaces (spec §2).

use super::Pen;

/// Draw: a page with a wire on it.
pub fn draw(p: &mut Pen) {
    p.rect(3.0, 1.5, 13.0, 14.5);
    p.circle(6.0, 8.0, 1.0);
    p.line((7.0, 8.0), (9.0, 8.0));
    p.circle(10.0, 8.0, 1.0);
}

/// Lists: rows with bullets.
pub fn lists(p: &mut Pen) {
    for y in [4.0, 8.0, 12.0] {
        p.dot(3.0, y, 1.1);
        p.line((6.0, y), (14.0, y));
    }
}

/// Library: shelves of symbols.
pub fn library(p: &mut Pen) {
    p.line((1.5, 7.0), (14.5, 7.0));
    p.line((1.5, 14.0), (14.5, 14.0));
    p.rect(3.0, 2.0, 6.0, 5.5);
    p.circle(10.5, 3.75, 1.75);
    p.rect(3.0, 9.0, 6.0, 12.5);
    p.poly(&[(8.5, 12.5), (10.5, 9.0), (12.5, 12.5)], true);
}

/// Publish: a printer with a sheet coming out.
pub fn publish(p: &mut Pen) {
    p.rect(5.0, 1.5, 11.0, 5.5);
    p.rect(1.5, 5.5, 14.5, 11.0);
    p.poly(&[(5.0, 11.0), (5.0, 14.5), (11.0, 14.5), (11.0, 11.0)], false);
    p.line((6.5, 12.75), (9.5, 12.75));
}
