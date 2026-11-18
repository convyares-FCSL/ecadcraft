//! The protection side of the rating checks (design 072 lane B; brief 057): the protective device a
//! rated subject is checked against, named by `protected_by` or found by the trace, with its In.

use super::*;
/// The device that protects a subject.
#[derive(Debug, Clone, PartialEq)]
pub struct Device {
    pub component: DeviceId,
    /// The fuse of a bank, or a strip's fuse position.
    pub fuse: Option<String>,
    /// `-Q01`, `-F01:3`.
    pub tag: String,
    /// In, when its rating reads.
    pub amps: Option<f64>,
    pub how: How,
}

/// The device of subject `s`: its `protected_by`, else the trace's.
pub(super) fn device_of(doc: &Document, s: RatedSubject, found: &Found, issues: &mut Vec<Issue>) -> Option<Device> {
    if let Some(named) = s.props(doc).and_then(|p| p.get(PROTECTED_BY)).filter(|v| !v.trim().is_empty()) {
        return match trace::named(doc, named) {
            Some((c, fuse)) => {
                let amps = trace::amps_of(doc, c, fuse.as_deref());
                let tag = trace::device_words(doc, c, fuse.as_deref());
                if amps.is_none() {
                    issues.push(Issue::DeviceUnread(c, trace::rating_text(doc, c, fuse.as_deref())));
                }
                if !trace::is_protective_component(doc, c) && fuse.is_none() {
                    issues.push(Issue::NamedDevice(tag.clone()));
                }
                Some(Device { component: c, fuse, tag, amps, how: How::Named })
            }
            None => {
                issues.push(Issue::DeviceMissing(named.chars().take(32).collect()));
                None
            }
        };
    }
    let mut devs: Vec<Device> = found
        .devices
        .iter()
        .map(|(c, fuse)| Device {
            component: *c,
            fuse: fuse.clone(),
            tag: trace::device_words(doc, *c, fuse.as_deref()),
            amps: trace::amps_of(doc, *c, fuse.as_deref()),
            how: How::Traced,
        })
        .collect();
    for d in &devs {
        if d.amps.is_none() {
            issues.push(Issue::DeviceUnread(d.component, trace::rating_text(doc, d.component, d.fuse.as_deref())));
        }
    }
    devs.sort_by(|a, b| b.amps.unwrap_or(0.0).total_cmp(&a.amps.unwrap_or(0.0)).then(a.tag.cmp(&b.tag)));
    if devs.is_empty() {
        issues.push(Issue::NoDevice(found.bounded));
        return None;
    }
    if let [first, second, ..] = devs.as_slice() {
        issues.push(Issue::TwoDevices(first.tag.clone(), second.tag.clone()));
    }
    devs.into_iter().next()
}
