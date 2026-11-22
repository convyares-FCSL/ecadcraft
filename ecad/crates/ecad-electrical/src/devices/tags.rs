//! Tag numbering and the words about a device (§2).

use super::*;

/// The highest number each tag prefix has had, from the project's history.
///
/// # Semantics
/// Built from the ledger's log on first use and then only from transactions added since
/// ([`TagHigh::catch_up`]), so its cost follows the change. Every tag a transaction wrote or
/// removed counts, undone ones included, so the next automatic number is never one a device has
/// had (owner, 2026-11-08: highest + 1, deleted tags are not reused).
#[derive(Debug, Clone, Default)]
pub struct TagHigh {
    seen: usize,
    high: BTreeMap<String, u64>,
    /// The high-water mark at the ledger's base (a version 2 snapshot carries it, brief 021): the
    /// history before the base is never read for it.
    base: tag::HighWater,
    /// Provisional numbers given out since the last transaction (brief 026), so two new devices of
    /// one edit never share one; cleared when a transaction is read.
    prov: BTreeMap<String, u64>,
}

impl TagHigh {
    /// Starting from `base`, the high-water mark at the ledger's base (empty for a ledger
    /// replayed from the root).
    pub fn starting_at(base: tag::HighWater) -> Self {
        Self { seen: 0, high: base.clone(), base, prov: BTreeMap::new() }
    }

    /// Read the transactions added to `ledger`'s log since the last call.
    pub fn catch_up(&mut self, ledger: &Ledger) {
        let log = ledger.log();
        if self.seen > log.len() {
            *self = TagHigh::starting_at(self.base.clone());
        }
        for tx in log.get(self.seen..).unwrap_or_default() {
            tag::note_ops(&mut self.high, &tx.ops);
        }
        if self.seen != log.len() {
            self.prov.clear();
        }
        self.seen = log.len();
    }

    /// The highest number of each prefix (the high-water mark), as read so far.
    pub fn marks(&self) -> &tag::HighWater {
        &self.high
    }

    /// The next automatic tag for `prefix` (brief 026): provisional (`-K?3`) when the project has
    /// provisional tags on, else [`TagHigh::next`].
    ///
    /// # Errors
    /// As [`TagHigh::next`].
    pub fn next_auto(&mut self, ledger: &Ledger, prefix: &str) -> Result<tag::Given, CmdError> {
        self.catch_up(ledger);
        self.next_auto_in(ledger.document(), prefix)
    }

    /// The tag a new object gets: the typed one, `-K?` as the next provisional number, or the next
    /// automatic one of `prefix` when none was typed.
    ///
    /// # Errors
    /// As [`TagHigh::next`].
    pub fn given(&mut self, ledger: &Ledger, typed: Option<tag::Entry>, prefix: &str) -> Result<tag::Given, CmdError> {
        self.catch_up(ledger);
        let doc = ledger.document();
        match typed {
            Some(tag::Entry::Tag(g)) => Ok(g),
            Some(tag::Entry::Next(p)) => Ok(tag::Given::new(self.next_provisional_in(doc, &p), tag::TagState::Provisional)),
            None => self.next_auto_in(doc, prefix),
        }
    }

    /// [`TagHigh::next_auto`] without reading the log first.
    ///
    /// # Errors
    /// As [`TagHigh::next`].
    pub fn next_auto_in(&mut self, doc: &Document, prefix: &str) -> Result<tag::Given, CmdError> {
        if ecad_model::naming::numbering::provisional_on(doc) {
            Ok(tag::Given::new(self.next_provisional_in(doc, prefix), tag::TagState::Provisional))
        } else {
            self.next_in(doc, prefix).map(|t| tag::Given::new(t, tag::TagState::Automatic))
        }
    }

    /// The stored text of the next provisional tag of `prefix` (`-K3`, shown `-K?3`): the highest provisional number in use, or
    /// given out in this edit, + 1. Counts as given.
    pub fn next_provisional_in(&mut self, doc: &Document, prefix: &str) -> String {
        let p = prefix.trim_start_matches('-').to_ascii_uppercase();
        let in_use = tag::parse(&ecad_model::naming::numbering::next_provisional(doc, &p)).map_or(1, |(_, n)| n);
        let n = in_use.max(self.prov.get(&p).map_or(0, |g| g.saturating_add(1)));
        self.prov.insert(p.clone(), n);
        crate::defaults::print::tag_style(doc).provisional(&p, n)
    }

    /// The automatic tag a new device of `prefix` would get, without using it up (prompts).
    ///
    /// # Errors
    /// As [`TagHigh::next`].
    pub fn peek_auto(&self, doc: &Document, prefix: &str) -> Result<String, CmdError> {
        if ecad_model::naming::numbering::provisional_on(doc) {
            Ok(tag::shown(&ecad_model::naming::numbering::next_provisional(doc, prefix), tag::TagState::Provisional))
        } else {
            self.peek(doc, prefix)
        }
    }

    /// The next automatic tag for `prefix`: its highest number + 1, skipping (boundedly) any tag a
    /// device holds now.
    ///
    /// # Errors
    /// The prefix has no number left.
    pub fn next(&mut self, ledger: &Ledger, prefix: &str) -> Result<String, CmdError> {
        self.catch_up(ledger);
        self.next_in(ledger.document(), prefix)
    }

    /// [`TagHigh::next`] without reading the log first (the caller caught up). The number given
    /// counts as used, so a second call in the same edit gives the next one.
    ///
    /// # Errors
    /// As [`TagHigh::next`].
    pub fn next_in(&mut self, doc: &Document, prefix: &str) -> Result<String, CmdError> {
        let (prefix, n, t) = self.first_free(doc, prefix)?;
        self.high.insert(prefix, n);
        Ok(t)
    }

    /// The tag [`TagHigh::next_in`] would give, without using it up and without reading the log
    /// (for prompts, previews and pick lists, which run every frame). The project keeps its copy
    /// caught up on every change, so this never reads the log.
    ///
    /// # Errors
    /// As [`TagHigh::next`].
    pub fn peek(&self, doc: &Document, prefix: &str) -> Result<String, CmdError> {
        self.first_free(doc, prefix).map(|(_, _, t)| t)
    }

    fn first_free(&self, doc: &Document, prefix: &str) -> Result<(String, u64, String), CmdError> {
        let prefix = prefix.trim_start_matches('-').to_ascii_uppercase();
        let first = self.high.get(&prefix).copied().unwrap_or(0).saturating_add(1);
        // Bounded: a hostile file may hold many tags above the highest the log shows.
        for n in (first..=tag::MAX_TAG_NUMBER).take(10_000) {
            let t = tag::format(&prefix, n, crate::defaults::print::tag_style(doc));
            // Devices and cables share one namespace (brief 026).
            if !doc.tag_taken(&t) {
                return Ok((prefix, n, t));
            }
        }
        Err(CmdError::refused(format!("No tag number is left for -{prefix}; type a tag.")))
    }
}

// ---------------------------------------------------------------- words

/// A component's tag as stored.
pub fn tag_of(doc: &Document, c: DeviceId) -> Option<&str> {
    doc.devices.get(&c)?.tag()
}

/// A component's tag as drawn (a provisional one with its `?`).
pub fn tag_shown_of(doc: &Document, c: DeviceId) -> Option<String> {
    doc.devices.get(&c)?.tag_shown()
}

/// A component's tag as drawn, or `(no tag)`.
pub fn tag_words(doc: &Document, c: DeviceId) -> String {
    tag_shown_of(doc, c).unwrap_or_else(|| "(no tag)".into())
}

/// The starter symbol a representation shows, when it shows one.
pub fn rep_starter(doc: &Document, r: &Representation) -> Option<&'static Starter> {
    let item = doc.items.get(r.items.first()?)?;
    let ItemKind::Symbol(inst) = &item.kind else { return None };
    symbols::catalogue::of_definition(doc.definitions.get(&inst.definition)?)
}

/// The definition a representation's symbol shows.
pub fn rep_definition<'a>(doc: &'a Document, r: &Representation) -> Option<(&'a Definition, Placement)> {
    let item = doc.items.get(r.items.first()?)?;
    let ItemKind::Symbol(inst) = &item.kind else { return None };
    Some((doc.definitions.get(&inst.definition)?, inst.placement))
}

/// A device's description: its `description`, else its first symbol's name, else `no description`.
pub fn description(doc: &Document, c: DeviceId) -> String {
    if let Some(d) = doc.devices.get(&c).and_then(|x| x.description.as_ref()) {
        return d.to_string();
    }
    doc.representations_of(Subject::Device(c))
        .find_map(|(_, r)| rep_definition(doc, r).map(|(d, _)| d.name.clone()))
        .unwrap_or_else(|| "no description".into())
}

/// 1-based page numbers a device is drawn on, in page order, each once.
pub fn page_numbers(doc: &Document, c: DeviceId) -> Vec<usize> {
    let pages: BTreeSet<PageId> = doc.representations_of(Subject::Device(c)).map(|(_, r)| r.page).collect();
    doc.page_order.iter().enumerate().filter(|(_, p)| pages.contains(p)).map(|(i, _)| i + 1).collect()
}

/// `pages 1, 2` or `page 1`.
pub fn pages_words(doc: &Document, c: DeviceId) -> String {
    let n = page_numbers(doc, c);
    let list = n.iter().map(usize::to_string).collect::<Vec<_>>().join(", ");
    if n.len() == 1 { format!("page {list}") } else { format!("pages {list}") }
}

/// The page of a device's first representation in page order.
fn first_page(doc: &Document, c: DeviceId) -> Option<PageId> {
    let pages: BTreeSet<PageId> = doc.representations_of(Subject::Device(c)).map(|(_, r)| r.page).collect();
    doc.page_order.iter().find(|p| pages.contains(p)).copied()
}

/// `-K01 (Contactor / relay coil, page 1 "Supply")`.
pub fn holder_words(doc: &Document, c: DeviceId) -> String {
    let page = first_page(doc, c).map(|p| format!(", {}", crate::references::cables::page_words(doc, p))).unwrap_or_default();
    format!("{} ({}{page})", tag_words(doc, c), description(doc, c))
}

/// The device other than `except` that holds tag identity `k` (the same tag), if any.
pub fn holder_of(doc: &Document, k: &tag::Key, except: Option<DeviceId>) -> Option<DeviceId> {
    doc.components_with(k).into_iter().find(|c| Some(*c) != except)
}

/// What holds a tag: a device or a cable (brief 026, one namespace).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Holder {
    Device(DeviceId),
    Cable(ecad_model::CableId),
}

impl Holder {
    /// The holder as an object reference.
    pub fn object(self) -> ecad_model::ObjectRef {
        match self {
            Holder::Device(c) => ecad_model::ObjectRef::Device(c),
            Holder::Cable(c) => ecad_model::ObjectRef::Cable(c),
        }
    }
}

/// The device or cable other than `except` holding tag identity `k`, devices first.
pub(crate) fn tag_holder(doc: &Document, k: &tag::Key, except: Option<ecad_model::ObjectRef>) -> Option<Holder> {
    doc.components_with(k)
        .into_iter()
        .map(Holder::Device)
        .chain(doc.cables_with(k).into_iter().map(Holder::Cable))
        .find(|h| Some(h.object()) != except)
}

/// A cable's tag as stored.
pub fn cable_tag_of(doc: &Document, k: ecad_model::CableId) -> Option<&str> {
    doc.cables.get(&k)?.tag()
}

/// A cable's tag as drawn.
pub fn cable_tag_shown_of(doc: &Document, k: ecad_model::CableId) -> Option<String> {
    doc.cables.get(&k)?.tag_shown()
}

/// `-K03 (Contactor / relay coil, 101-C-1)` or `-W01 (cable, 3 wires)`.
pub(crate) fn holder_words_of(doc: &Document, h: Holder) -> String {
    match h {
        Holder::Device(c) => holder_words(doc, c),
        Holder::Cable(k) => {
            let n = doc.cables.get(&k).map_or(0, |c| c.cores.len());
            format!("{} (cable, {n} wire{})", cable_tag_shown_of(doc, k).as_deref().unwrap_or("(no tag)"), if n == 1 { "" } else { "s" })
        }
    }
}

/// The prefix of new cable tags: the project's `cable_prefix`, else `W` (brief 026, design 072 F5).
pub fn cable_prefix(doc: &Document) -> String {
    doc.settings.cable_prefix.as_ref().map_or_else(|| "W".to_string(), |p| p.to_string())
}

/// The tag a new cable gets (brief 026, design §6): `typed` as stored (normalised; `-W?` is the next
/// provisional), else the next automatic one (`-W01`, or `-W?1` with provisional tags on).
///
/// # Errors
/// A typed tag that cannot be stored, or that a device or cable holds (with the next free one).
pub fn new_cable_tag(ledger: &Ledger, high: &mut TagHigh, typed: Option<&str>) -> Result<tag::Given, CmdError> {
    high.catch_up(ledger);
    let doc = ledger.document();
    match typed {
        Some(t) => {
            let given = match tag::enter(t, crate::defaults::print::tag_style(doc))? {
                tag::Entry::Next(p) => return Ok(tag::Given::new(high.next_provisional_in(doc, &p), tag::TagState::Provisional)),
                tag::Entry::Tag(g) => g,
            };
            if let Some(h) = tag_holder(doc, &given.key(), None) {
                let next = tag::prefix_of(&given.text).and_then(|p| high.peek(doc, &p).ok());
                let way = next.map_or_else(|| "Choose another tag.".to_string(), |n| format!("Choose another tag (the next free one is {n})."));
                return Err(CmdError::refused(format!("Tag {} is already used by {}. {way}", given.shown(), holder_words_of(doc, h))));
            }
            Ok(given)
        }
        None => high.next_auto_in(doc, &cable_prefix(doc)),
    }
}

/// The refusal of a duplicate, with the ways forward.
pub(crate) fn duplicate_words(doc: &Document, t: &str, holder: DeviceId, way: &str) -> String {
    format!("Tag {t} is already used by {}. {way}", holder_words(doc, holder))
}

/// The noun of an exclusive kind, for "already has a …".
pub(super) fn noun(s: &Starter) -> &'static str {
    match s.key {
        K::Coil => "coil",
        K::Mcb1 | K::Mcb2 => "MCB",
        K::Fuse => "fuse",
        K::Psu => "power supply",
        K::Lamp => "lamp",
        K::Motor3 | K::Motor1 => "motor",
        K::Trafo => "transformer",
        _ => "head symbol",
    }
}

/// The noun of head representation `h`, for "already has a …" and "has two …s".
pub(crate) fn head_noun(doc: &Document, h: RepresentationId) -> &'static str {
    doc.representations.get(&h).and_then(|r| rep_starter(doc, r)).map_or("head symbol", noun)
}
