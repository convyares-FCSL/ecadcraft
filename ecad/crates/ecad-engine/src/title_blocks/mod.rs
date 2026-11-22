//! The engine half of [`crate::title_block`]: a block turned into frame parts ([`layout`]) and DXF
//! import ([`dxf`], `cadcraft_io::read_dxf`).

pub mod dxf;
pub mod layout;
