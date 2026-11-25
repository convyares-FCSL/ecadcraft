//! Typed identities.
//!
//! Every object kind has its own ID type so the compiler keeps them apart. IDs are 128-bit values
//! allocated when an edit is made and recorded in the op; replay never allocates.

use std::fmt;

use serde::{Deserialize, Deserializer, Serialize, Serializer};

/// The value of an ID written as text: exactly 32 hex digits (either case), nothing else. No sign, no
/// surrounding space, no shorter form: `"+A1"`, `" 0a…"` and `"a1"` are not IDs. Every reader of an ID
/// from text (JSON, command parameters, stored keys, block names) goes through this.
///
/// # Example
/// ```rust
/// assert_eq!(ecad_model::hex_id("0000000000000000000000000000002a"), Some(42));
/// assert_eq!(ecad_model::hex_id("2a"), None);
/// assert_eq!(ecad_model::hex_id("+000000000000000000000000000002a"), None);
/// ```
pub fn hex_id(text: &str) -> Option<u128> {
    if text.len() != 32 || !text.bytes().all(|b| b.is_ascii_hexdigit()) {
        return None;
    }
    u128::from_str_radix(text, 16).ok()
}

macro_rules! typed_id {
    ($(#[$doc:meta])* $name:ident, $prefix:literal) => {
        $(#[$doc])*
        ///
        /// Serialised as 32 lower-case hex digits, so JSON readers without 128-bit integers
        /// (browsers) keep it exact.
        #[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
        pub struct $name(pub u128);

        impl Serialize for $name {
            fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
                s.serialize_str(&format!("{:032x}", self.0))
            }
        }

        impl<'de> Deserialize<'de> for $name {
            fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
                let text = String::deserialize(d)?;
                hex_id(&text).map(Self).ok_or_else(|| serde::de::Error::custom(format!("{text:?} is not an ID (32 hex digits)")))
            }
        }

        /// A hex string in a command's parameters.
        impl schemars::JsonSchema for $name {
            fn inline_schema() -> bool {
                true
            }

            fn schema_name() -> std::borrow::Cow<'static, str> {
                stringify!($name).into()
            }

            fn json_schema(_: &mut schemars::SchemaGenerator) -> schemars::Schema {
                schemars::json_schema!({"type": "string", "format": "hex-id", "pattern": "^[0-9a-fA-F]{32}$"})
            }
        }

        impl $name {
            /// Parse the 32-hex-digit form ([`hex_id`]).
            pub fn parse(text: &str) -> Option<Self> {
                hex_id(text).map(Self)
            }

            /// The full 32-hex-digit form.
            pub fn hex(self) -> String {
                format!("{:032x}", self.0)
            }
        }

        impl $name {
            /// Short form for messages and diagnostics (prefix and first 8 hex digits).
            pub fn short(self) -> String {
                format!(concat!($prefix, "-{:08x}"), (self.0 >> 96) as u32)
            }
        }

        impl From<u128> for $name {
            fn from(v: u128) -> Self {
                Self(v)
            }
        }

        impl fmt::Debug for $name {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                f.write_str(&self.short())
            }
        }

        impl fmt::Display for $name {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                f.write_str(&self.short())
            }
        }
    };
}

typed_id!(/// A page of the project.
    PageId, "pg");
typed_id!(/// A folder of pages.
    FolderId, "fld");
typed_id!(/// A layer.
    LayerId, "lyr");
typed_id!(/// A drawn item: a primitive or a symbol instance.
    ItemId, "itm");
typed_id!(/// A device.
    DeviceId, "cmp");
typed_id!(/// A connection point of a device.
    ConnectionPointId, "trm");
typed_id!(/// A representation (depiction) of a device, wire or cable.
    RepresentationId, "rep");
typed_id!(/// A wire.
    WireId, "cnd");
typed_id!(/// A cable.
    CableId, "cbl");
typed_id!(/// A route: a line drawn on a page.
    RouteId, "rte");
typed_id!(/// A drawing aid: a continuation or a location box.
    AidId, "aid");
typed_id!(/// A loose part: panel-only material on the BOM.
    LoosePartId, "lpt");
typed_id!(/// A net, stored on each of its wires: given when the net first appears and kept through
    /// edits.
    NetId, "net");
typed_id!(/// A conductor's own identity in the project: given when its wire is made, never reused
    ConductorId, "cdr");
typed_id!(/// An embedded definition.
    DefinitionId, "def");
typed_id!(/// A transaction.
    TxId, "tx");
typed_id!(/// An entry of the project's location list.
    LocationId, "loc");
typed_id!(/// An entry of the project's function list.
    FunctionId, "fn");
typed_id!(/// A link between two devices.
    LinkId, "lnk");
typed_id!(/// A project-defined user field.
    UserFieldId, "ufd");
typed_id!(/// A library (`.ecadlib`), stable across renaming its file.
    LibraryId, "lib");
typed_id!(/// A project, stable across renaming or moving its file (the file header records it).
    ProjectId, "prj");
typed_id!(/// A standard: kept when a project copies it.
    StandardId, "std");

/// Allocates fresh IDs.
///
/// # Semantics
/// A 128-bit counter-mode generator (SplitMix-style mixing of a seeded counter). Distinct seeds
/// give independent streams, so IDs from different sessions are, with overwhelming probability,
/// distinct. IDs are never reused: the counter only moves forward. The seed comes from the caller
/// so that tests are deterministic and the crate needs no clock or OS randomness.
///
/// # Example
/// ```rust
/// use ecad_model::{DeviceId, IdAllocator};
/// let mut ids = IdAllocator::seeded(7);
/// let a: DeviceId = ids.fresh();
/// let b: DeviceId = ids.fresh();
/// assert_ne!(a, b);
/// ```
#[derive(Debug, Clone)]
pub struct IdAllocator {
    seed: u64,
    counter: u64,
}

impl IdAllocator {
    /// A generator for `seed`.
    pub fn seeded(seed: u64) -> Self {
        Self { seed, counter: 0 }
    }

    /// A generator for continuing a project whose history ends with `last` (for example the last
    /// transaction ID). Reopening a project with the same session seed would otherwise restart a
    /// stream whose IDs are already in the document; mixing in `last` gives a fresh stream.
    pub fn seeded_after(seed: u64, last: u128) -> Self {
        Self::seeded(seed ^ mix((last as u64) ^ mix((last >> 64) as u64)))
    }

    /// The next ID, of any typed-ID kind.
    pub fn fresh<T: From<u128>>(&mut self) -> T {
        self.counter = self.counter.wrapping_add(1);
        let hi = mix(self.seed ^ mix(self.counter));
        let lo = mix(hi ^ self.counter.rotate_left(17) ^ 0x9E37_79B9_7F4A_7C15);
        T::from((u128::from(hi) << 64) | u128::from(lo))
    }
}

fn mix(mut z: u64) -> u64 {
    z = z.wrapping_add(0x9E37_79B9_7F4A_7C15);
    z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
    z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
    z ^ (z >> 31)
}
