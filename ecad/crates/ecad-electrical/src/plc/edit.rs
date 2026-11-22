//! Changing a PLC device's typed data (design 072 §3) inside a transaction being built: every
//! change of a station's or module's fields is one [`Op::ModifyDevice`] recording the whole device
//! before and after.
//!
//! # Semantics
//! - [`current`] is the device as the ops built so far leave it, so a later change's `before` is
//!   always what the store will hold when it applies.
//! - [`modify`] adds the change: into the device's own `CreateDevice` or `ModifyDevice` when that
//!   is the last op changing its record, else as a new `ModifyDevice`. Nothing is added when
//!   nothing changes.
//!
//! Excludes: what a change means (the commands and `setprop` rows decide that).

use ecad_model::{Device, DeviceId, DeviceType, Document, ObjectRef, Op};

use ecad_model::objects::plc::{PlcModule, PlcRack};

/// Whether `op` changes device `c`'s record.
fn changes(op: &Op, c: DeviceId) -> bool {
    match op {
        Op::CreateDevice { id, .. } | Op::DeleteDevice { id, .. } | Op::ModifyDevice { id, .. } | Op::SetDeviceDefinition { id, .. } => *id == c,
        Op::SetProperty { object: ObjectRef::Device(x), .. } => *x == c,
        _ => false,
    }
}

/// Device `c` as `doc` with `ops` applied would hold it; `None` when it does not exist then.
pub fn current(doc: &Document, ops: &[Op], c: DeviceId) -> Option<Device> {
    let mut d = doc.devices.get(&c).cloned();
    for op in ops.iter().filter(|op| changes(op, c)) {
        match op {
            Op::CreateDevice { device, .. } => d = Some(device.clone()),
            Op::DeleteDevice { .. } => d = None,
            Op::ModifyDevice { after, .. } => d = Some(after.clone()),
            Op::SetDeviceDefinition { after, .. } => {
                if let Some(x) = d.as_mut() {
                    x.definition = *after;
                }
            }
            Op::SetProperty { key, after, .. } => {
                if let Some(x) = d.as_mut() {
                    match after {
                        Some(v) => x.properties.insert(key.clone(), v.clone()),
                        None => x.properties.remove(key),
                    };
                }
            }
            _ => {}
        }
    }
    d
}

/// Change device `c`'s type data with `f`, adding the change to `ops`. `false` when `c` does not
/// exist (as `ops` leave it).
pub fn modify(doc: &Document, ops: &mut Vec<Op>, c: DeviceId, f: impl FnOnce(&mut DeviceType)) -> bool {
    let Some(before) = current(doc, ops, c) else { return false };
    let mut after = before.clone();
    f(&mut after.device_type);
    if after == before {
        return true;
    }
    match ops.iter_mut().rev().find(|op| changes(op, c)) {
        Some(Op::CreateDevice { device, .. }) => *device = after,
        Some(Op::ModifyDevice { before: b, after: a, .. }) => {
            if *b == after {
                // Back where it was: the change undoes itself.
                if let Some(i) = ops.iter().rposition(|op| changes(op, c)) {
                    ops.remove(i);
                }
            } else {
                *a = after;
            }
        }
        _ => ops.push(Op::ModifyDevice { id: c, before, after }),
    }
    true
}

/// Change module `c`'s data with `f`; `false` when `c` is not a module.
pub fn module(doc: &Document, ops: &mut Vec<Op>, c: DeviceId, f: impl FnOnce(&mut PlcModule)) -> bool {
    if !current(doc, ops, c).is_some_and(|d| matches!(d.device_type, DeviceType::PlcModule(_))) {
        return false;
    }
    modify(doc, ops, c, |t| {
        if let DeviceType::PlcModule(m) = t {
            f(m);
        }
    })
}

/// Change station `c`'s data with `f`; `false` when `c` is not a station.
pub fn station(doc: &Document, ops: &mut Vec<Op>, c: DeviceId, f: impl FnOnce(&mut PlcRack)) -> bool {
    if !current(doc, ops, c).is_some_and(|d| matches!(d.device_type, DeviceType::PlcRack(_))) {
        return false;
    }
    modify(doc, ops, c, |t| {
        if let DeviceType::PlcRack(r) = t {
            f(r);
        }
    })
}

/// Device `c`'s module data, if it is a module.
pub fn module_of(doc: &Document, c: DeviceId) -> Option<&PlcModule> {
    match &doc.devices.get(&c)?.device_type {
        DeviceType::PlcModule(m) => Some(m),
        _ => None,
    }
}

/// Device `c`'s station data, if it is a station.
pub fn station_of(doc: &Document, c: DeviceId) -> Option<&PlcRack> {
    match &doc.devices.get(&c)?.device_type {
        DeviceType::PlcRack(r) => Some(r),
        _ => None,
    }
}
