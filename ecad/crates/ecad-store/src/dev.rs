//! What only tests and dev tools build (feature `fault-injection`): never part of the product.
//!
//! - [`power`]: a redb storage backend that simulates power loss (reordered and torn writes since the last
//!   flush), for the project file's crash tests.

pub mod power;
