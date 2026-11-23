//! The rules of `place`: which device a placed
//! symbol joins, or which tag a new one takes, for each form of `place`, and the checks of a
//! tagless symbol's name and caption. Typed input in, a target or a refusal out.
//!
//! Excludes: building the symbol's ops (each form's own planner: [`crate::devices::placing::plan_place`],
//! the strip, card and connector planners), committing and answering in JSON (the engine crate's
//! `place` shim).

use ecad_model::naming::tag::{self, Style};
use ecad_model::{DeviceId, Document, Noun};

use crate::devices::{self};

use crate::devices::placing::Target;
use crate::error::CmdError;
use crate::symbols::{Side, Starter};

/// `device` read as a hex ID (`None` when not given).
///
/// # Errors
/// Text that is not a hex ID.
pub fn device_id(device: Option<&str>) -> Result<Option<DeviceId>, CmdError> {
    match device {
        None => Ok(None),
        Some(t) => u128::from_str_radix(t.trim(), 16).map(|n| Some(DeviceId::from(n))).map_err(|_| CmdError::input("`device` (hex id) is required")),
    }
}

/// Refuse `device` and `tag` together: `tag` names a new `what`, `device` joins one.
///
/// # Errors
/// Both given.
pub fn one_of(device: bool, tag: bool, what: &str) -> Result<(), CmdError> {
    match device && tag {
        true => Err(CmdError::input(format!("`tag` is for a new {what}; `device` joins an existing one"))),
        false => Ok(()),
    }
}

fn style(doc: &Document) -> Style {
    crate::defaults::print::tag_style(doc)
}

/// What a starter symbol `place`d makes: another symbol of `device`, a new device
/// tagged `tag`, or a new device with the next automatic tag.
///
/// # Errors
/// No such device; a connector half's tag; a tag that does not read.
pub fn device_target(doc: &Document, device: Option<DeviceId>, tag: Option<&str>) -> Result<Target, CmdError> {
    match (device, tag) {
        (Some(c), _) if doc.devices().contains_key(&c) => Ok(Target::Join(c)),
        (Some(_), _) => Err(CmdError::NotFound(Noun::Device)),
        (None, Some(t)) => {
            if let Some(Ok((c, Some(side), _))) = crate::connectors::resolve(doc, t) {
                return Err(CmdError::refused(format!(
                    "{} is the {} half of {}. Pick another tag.",
                    t.trim(),
                    side.word(),
                    devices::tags::tag_words(doc, c)
                )));
            }
            Ok(Target::New(Some(tag::enter(t, style(doc))?)))
        }
        (None, None) => Ok(Target::New(None)),
    }
}

/// What a project symbol `place`d joins (pack re-run, bug 2): `device`, or the
/// device that already holds the typed tag (a tag is an identity), else a new device with that
/// tag. Devices and cables share one namespace.
///
/// # Errors
/// A tag that does not read, or a cable's.
pub fn project_target(doc: &Document, device: Option<DeviceId>, tag: Option<&str>) -> Result<(Option<DeviceId>, Option<tag::Entry>), CmdError> {
    let Some(t) = tag else { return Ok((device, None)) };
    free_or_held(doc, tag::enter(t, style(doc))?, "device")
}

/// The device that holds the typed tag, or the typed tag to give a new one.
fn free_or_held(doc: &Document, typed: tag::Entry, noun: &str) -> Result<(Option<DeviceId>, Option<tag::Entry>), CmdError> {
    let tag::Entry::Tag(given) = &typed else { return Ok((None, Some(typed))) };
    match devices::tags::holder_of(doc, &given.key(), None) {
        Some(c) => Ok((Some(c), None)),
        None if !doc.cables_with(&given.key()).is_empty() => {
            Err(CmdError::refused(format!("Tag {} is a cable's; choose another tag for a {noun}.", given.shown())))
        }
        None => Ok((None, Some(typed))),
    }
}

/// The strip a level symbol joins: `device`, or the strip tagged `tag`, else a
/// new strip with that tag (`None`, `None`: a new strip with the next tag).
///
/// # Errors
/// No such device; a device that is not a strip; a tag that does not read, or a cable's.
pub fn strip_target(doc: &Document, device: Option<DeviceId>, tag: Option<&str>) -> Result<(Option<DeviceId>, Option<tag::Entry>), CmdError> {
    let not_strip = |c: DeviceId| {
        CmdError::refused(format!(
            "{} is not a terminal strip. Name a strip (-X01), or leave it out for a new one.",
            devices::tags::tag_words(doc, c)
        ))
    };
    match (device, tag) {
        (Some(c), _) if !doc.devices().contains_key(&c) => Err(CmdError::NotFound(Noun::Device)),
        (Some(c), _) if crate::terminal_strips::strip_of(doc, c).is_none() => Err(not_strip(c)),
        (Some(c), _) => Ok((Some(c), None)),
        (None, Some(t)) => {
            let typed = tag::enter(t, devices::padding(doc))?;
            if let tag::Entry::Tag(given) = &typed
                && let Some(c) = devices::tags::holder_of(doc, &given.key(), None)
            {
                return if crate::terminal_strips::strip_of(doc, c).is_some() { Ok((Some(c), None)) } else { Err(not_strip(c)) };
            }
            free_or_held(doc, typed, "strip")
        }
        (None, None) => Ok((None, None)),
    }
}

/// The card a channel symbol joins: `device`, or the device tagged `tag`, else a
/// new card with that tag (`None`, `None`: a new card with the next tag).
///
/// # Errors
/// No such device; a tag that does not read.
pub fn card_target(doc: &Document, device: Option<DeviceId>, tag: Option<&str>) -> Result<(Option<DeviceId>, Option<tag::Entry>), CmdError> {
    match (device, tag) {
        (Some(c), _) if doc.devices().contains_key(&c) => Ok((Some(c), None)),
        (Some(_), _) => Err(CmdError::NotFound(Noun::Device)),
        (None, Some(t)) => {
            let typed = tag::enter(t, style(doc))?;
            Ok(match &typed {
                tag::Entry::Tag(g) => match devices::tags::holder_of(doc, &g.key(), None) {
                    Some(c) => (Some(c), None),
                    None => (None, Some(typed)),
                },
                tag::Entry::Next(_) => (None, Some(typed)),
            })
        }
        (None, None) => Ok((None, None)),
    }
}

/// The connector a half or pin symbol of `side` joins: `device` (an ID or a
/// tag), or the connector a half's tag (`-X01.P`) or a tag names, else a new connector with that
/// tag.
///
/// # Errors
/// A device that cannot be found; both `device` and `tag`; the other half's tag; a cable's tag; a
/// device that is not a connector.
pub fn connector_target(
    doc: &Document, side: Side, device: Option<&str>, tag: Option<&str>,
) -> Result<(Option<DeviceId>, Option<tag::Entry>), CmdError> {
    let mut found = crate::connectors::commands::device(doc, device)?;
    one_of(device.is_some(), tag.is_some(), "connector")?;
    let mut new_tag = None;
    if let Some(t) = tag
        && let Some(named) = crate::connectors::resolve(doc, t)
    {
        // A half's tag (`-X01.P`) names its connector and that half.
        let (c, half, _) = named?;
        if let Some(h) = half
            && side != Side::Both
            && h != side
        {
            return Err(CmdError::refused(format!(
                "{} is the {} half of {}; this symbol is a {} half. Name -{}'s {} half, or Enter for a new connector.",
                t.trim(),
                h.word(),
                devices::tags::tag_words(doc, c),
                side.word(),
                devices::tags::tag_words(doc, c).trim_start_matches('-'),
                side.word()
            )));
        }
        found = Some(c);
    } else if let Some(t) = tag {
        let (c, typed) = free_or_held(doc, tag::enter(t, style(doc))?, "device")?;
        found = c.or(found);
        new_tag = typed;
    }
    if let Some(c) = found
        && !crate::connectors::connector_of(doc, c).is_some_and(|x| x.mating)
    {
        return Err(CmdError::refused(crate::connectors::not_a_connector(doc, c)));
    }
    Ok((found, new_tag))
}

/// A tagless symbol's name and caption as `place` takes them: a
/// continuation is part of no device, needs a name and may have a caption.
///
/// # Errors
/// `device` or `tag` given; a name or caption that does not read; no name for a continuation; a
/// caption on anything but a continuation.
pub fn tagless(starter: &Starter, joins: bool, name: Option<&str>, caption: Option<&str>) -> Result<(Option<String>, Option<String>), CmdError> {
    if joins {
        return Err(CmdError::refused(format!("A {} has no tag and is not part of a device.", starter.name.to_lowercase())));
    }
    let name = match name {
        Some(n) => Some(crate::wires::valid_name(n)?),
        None => None,
    };
    if starter.key == crate::symbols::StarterKey::Continuation && name.is_none() {
        return Err(CmdError::input("`name` is required for a continuation (the potential or signal it carries, e.g. 24V)"));
    }
    // A continuation may be placed with its caption.
    let caption = match caption {
        None => None,
        Some(_) if starter.key != crate::symbols::StarterKey::Continuation => {
            return Err(CmdError::refused(format!("Only continuation arrows have a caption; that is a {}.", starter.name.to_lowercase())));
        }
        Some(t) => crate::continuations::caption_text(Some(t))?,
    };
    Ok((name, caption))
}
