//! Every integration test of this crate in one test executable: each module is a file here, so tests are
//! named `<module>::<test>` and `cargo test -p ecad-desktop <module>` runs one file's tests.

mod args;
mod folder;
mod startup;
mod startup_acceptance;
