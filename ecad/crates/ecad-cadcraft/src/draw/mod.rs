//! The engine half of modules whose rules go to `ecad-electrical` (design 065 §3, cut 5): building
//! CADCraft entities, text styles and fonts from what the rules decide. One module per domain
//! module; the projection (`project`, `verify`, `translate`) calls them.

pub mod text_style;
