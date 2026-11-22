//! Load current (design 057 §4.3): the interface the Power brief (0.2.3) fills.
//!
//! 057 ships [`TypedLoads`]: a subject's own `load` property. Power adds its own source behind
//! [`LoadSource`] and composes typed → Power: a typed load wins (Q4).

use ecad_model::{Document, Op};

use crate::circuits::RatedSubject;

/// Where a load came from.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LoadFrom {
    Typed,
    Power,
}

/// A running design current (Ib).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Load {
    pub amps: f64,
    pub from: LoadFrom,
}

/// A source of design currents.
pub trait LoadSource {
    /// The running design current of a rated subject, if known.
    fn load(&self, doc: &Document, s: RatedSubject) -> Option<Load>;
    /// Which subjects' loads an op may change (for the update map).
    fn touched(&self, doc: &Document, op: &Op) -> Vec<RatedSubject>;
}

/// The subject's own `load` property (A, design 057 §6.1).
#[derive(Debug, Clone, Copy, Default)]
pub(crate) struct TypedLoads;

impl LoadSource for TypedLoads {
    fn load(&self, doc: &Document, s: RatedSubject) -> Option<Load> {
        let (_, load) = s.inputs(doc);
        load.map(|amps| Load { amps, from: LoadFrom::Typed })
    }

    fn touched(&self, _doc: &Document, op: &Op) -> Vec<RatedSubject> {
        match op {
            Op::ModifyWire { id, before, after } if before.load != after.load => vec![RatedSubject::Wire(*id)],
            Op::ModifyCable { id, before, after } if before.load != after.load => vec![RatedSubject::Cable(*id)],
            _ => Vec::new(),
        }
    }
}
