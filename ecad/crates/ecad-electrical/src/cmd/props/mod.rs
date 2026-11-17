//! Each lane's rows of `setprop` (brief 072 "Lanes"): one file per lane, asked in this order
//! before [`super::prop::PROP_RULES`], so lanes moving keys into struct fields never edit the
//! shared table. The phase-end tidy-up folds them into it.

pub mod c;
pub mod d;

/// The lanes' row tables, in brief order.
pub static LANES: &[(&str, &[super::prop::PropRule])] = &[("072c", c::RULES), ("072d", d::RULES)];
