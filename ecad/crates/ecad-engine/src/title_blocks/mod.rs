//! The engine half of [`ecad_model::objects::title_block`]: a block turned into frame parts ([`layout`]) and DXF
//! import ([`dxf`], `cadcraft_io::read_dxf`).

pub(crate) mod commands;
pub mod dxf;
pub mod layout;
