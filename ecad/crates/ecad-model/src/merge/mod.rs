//! Three-way merge of two branches' states (brief 020b, design 020 §5–§6).
//!
//! # Semantics
//! - The inputs are three states (the common **ancestor**, the **target** the merge is appended
//!   to, the **source** merged from) and the **scope**: every object either side's steps named
//!   since the fork. Only objects in the scope are compared, so the cost follows what changed,
//!   not the project's size (an object outside it is the same in all three states).
//! - Per object and per property (design §5.1): changed on one side only → that side; changed
//!   alike on both → kept; changed differently → a **value conflict**; removed on one side and
//!   changed on the other → a **removed conflict**. Added on one side → added.
//! - Rules the result must keep are conflicts too, each with a choice that satisfies it, so the
//!   result always passes the ledger's check (design §5.3): an object kept that refers to one
//!   removed (**needed**: bring it back, or leave it unconnected / remove what uses it), two
//!   devices with one tag and two pages with one number (renumber the source's or the
//!   target's), two sibling folders with one name (rename one), page or folder order changed
//!   differently on both sides.
//! - Nothing is preselected (owner answer 7): an unanswered conflict leaves the target's side in
//!   the provisional result and is listed in [`Plan::unanswered`]; [`Plan::ops`] is empty until
//!   every conflict has a choice. Never a dead end: every conflict offers at least two choices.
//! - The result is ordinary ops from the target's state ([`crate::document::diff::ops_between_in`]) over
//!   the scope and what the rules added to it.
//! - Version names are the target's (they are branch-local history); project properties are
//!   merged only where `keep_project_key` allows (revision fields are never merged into the
//!   release line, owner answer 10).
//!
//! - A tag conflict also offers **Make them one** (brief 026, FP5, [`Choice::MakeOne`]): the
//!   source's device joins the target's through [`Inputs::join`] (018's device merge), or the
//!   source's cable's members join the target's cable; the target's property values are kept.
//!   When they cannot be one, the choice is left out and [`Plan::unavailable`] says why.
//!   Devices and cables share one tag namespace. Provisional tags (`-K?1`) added on both sides are
//!   not conflicts: the source's gets the next provisional number ([`Plan::provisional_renumbered`]).
//!
//! Excluded: "combine" for sibling folders (the rename choice covers it); an inline editor for
//! anything but property values.

pub mod apply;
mod conflicts;
mod rules;
pub mod state;
#[cfg(test)]
mod tests;
