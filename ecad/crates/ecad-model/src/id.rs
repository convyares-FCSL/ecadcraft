//! Typed identities (principle P2).
//!
//! Every object kind has its own ID type so the compiler keeps them apart. IDs are 128-bit values
//! allocated when an edit is made and recorded in the op; replay never allocates (FILE-02).

use std::fmt;

use serde::{Deserialize, Deserializer, Serialize, Serializer};

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
                u128::from_str_radix(&text, 16).map(Self).map_err(serde::de::Error::custom)
            }
        }

        impl $name {
            /// Parse the 32-hex-digit form.
            pub fn parse(text: &str) -> Option<Self> {
                u128::from_str_radix(text.trim(), 16).ok().map(Self)
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
typed_id!(/// A layer.
    LayerId, "lyr");
typed_id!(/// A drawn item: a primitive or a symbol instance.
    ItemId, "itm");
typed_id!(/// A component.
    ComponentId, "cmp");
typed_id!(/// A terminal of a component.
    TerminalId, "trm");
typed_id!(/// A representation (depiction) of a component, conductor or cable.
    RepresentationId, "rep");
typed_id!(/// A conductor.
    ConductorId, "cnd");
typed_id!(/// A cable.
    CableId, "cbl");
typed_id!(/// An embedded definition.
    DefinitionId, "def");
typed_id!(/// A transaction.
    TxId, "tx");

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
/// use ecad_model::{ComponentId, IdAllocator};
/// let mut ids = IdAllocator::seeded(7);
/// let a: ComponentId = ids.fresh();
/// let b: ComponentId = ids.fresh();
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
