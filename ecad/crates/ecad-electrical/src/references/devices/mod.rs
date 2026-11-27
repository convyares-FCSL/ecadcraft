//! Device cross-references on a general reference index.
//!
//! # Semantics
//! - A **reference** is a text that shows another drawn item's place. A **source** shows a
//!   **target**'s place; both are representations. Sources and targets of one kind belong to one
//!   **group** ([`classify::Group`]): a cable's reference ends, or one device's symbols. A short rule table
//!   ([`index::RefIndex`]'s edges) decides which members of a group point at which:
//!   every cable reference end at every other end of its cable; every **child** (a symbol with
//!   contact poles) of a device at its **head** (its exclusive symbol: coil, MCB, motor…) and the
//!   head at every child. A device without a head has no references.
//! - Everything here is derived from the document and never stored: the index ([`index::RefIndex`]), the
//!   text under a contact (its head's place, [`table::contact_text`]) and the **contact mirror table**
//!   under a head (`table`): one row per pole, NO or NC, power or auxiliary, its labels and its
//!   place, with the poles the part offers that nobody drew (**spare**) and those drawn beyond the
//!   offer (**over**).
//! - What a part offers is its typed contacts ([`crate::devices::traits::Contacts`]; typed
//!   through `setprop`'s [`parse::CONTACTS_KEY`] in the grammar of [`parse::parse_set`]); where its table goes is
//!   a switching device's `contact_table` (typed through [`parse::MIRROR_KEY`], [`parse::parse_mirror`]), else
//!   the project's `contact_mirror`, else `auto`.
//! - Cable references keep their own derivation ([`crate::references::cables`]); this module only scopes
//!   them per commit ([`watch::Watch`]), so every cable text is exactly as before.
//!
//! # Update map
//! [`index::RefIndex`] is built whole on open, reload and branch switch, and kept per commit, undo, redo
//! and merge from the ops ([`index::RefIndex::groups_named`], [`index::RefIndex::refresh`]): only the groups an
//! op names are made again (O(group)). The texts and tables to redraw are worked out by
//! [`watch::Watch`], which derives the touched devices' texts before and after a commit:
//!
//! | Op | Devices whose texts are compared |
//! | --- | --- |
//! | `ModifyItem` / `RemoveItem` of an item of a component's representation | that component |
//! | the same of a conductor route | the components at the conductor's ends (a table under `auto` follows the rail its coil's wire meets) |
//! | `LinkRepresentation` / `UnlinkRepresentation` / `ModifyRepresentation` | the component it shows, or the components at its conductor's ends |
//! | `CreateWire`, `DeleteWire` (the commit's wire rewrite included) | the components at the ends |
//! | `AddConnectionPoint` / `RemoveConnectionPoint` | that component (its labels) |
//! | `CreateDevice` / `DeleteDevice` | that component |
//! | `ModifyDevice` changing its contacts or contact table | that component |
//! | `SetProperty` page number or sheet | the devices with a member on that page |
//! | page order (folder property, page and folder ops) | the devices with a member on a page whose book position changed |
//! | `EmbedDefinition` / `RemoveDefinition` | the components using it |
//! | `SetProjectSettings` changing `reference_style`, `sheet`, `length_unit` or `contact_mirror` | none derived: every page with a device member is redrawn or marked stale (the one wide case) |
//! | every other op | none |
//!
//! Cable references: `CreateCable`, `DeleteCable` and `ModifyCable` now scope to their cable ([`crate::references::cables::cables_touched_by`]); everything else is as before.
//!
//! This module records nothing and has no UI: commands commit through their JSON forms.

pub mod classify;
pub mod index;
pub mod parse;
pub mod places;
pub mod read;
pub mod table;
pub(crate) mod watch;
