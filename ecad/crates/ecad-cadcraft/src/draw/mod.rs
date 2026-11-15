//! The engine half of modules whose rules go to `ecad-electrical` (design 065 §3, cuts 5 and 6):
//! building CADCraft entities, text styles and fonts from what the rules decide, and the glue that
//! runs a rule against a live [`crate::Project`]. One module per domain module; the projection
//! (`project`, `verify`, `translate`) and the commands call them.

pub mod checks;
pub mod front;
pub mod generated;
pub mod proposals;
pub mod text_style;
pub mod title_block;
