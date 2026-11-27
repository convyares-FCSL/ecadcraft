//! The file as a container: writing, crash and recovery, format versions, strict `meta`.

mod crash;
mod damaged_pages;
pub(crate) mod files;
mod format_version;
mod recovery;
mod strict_meta;
