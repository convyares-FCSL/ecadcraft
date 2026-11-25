//! Project symbols and the symbol search.

use super::*;

/// The library ID of symbols made in a project: one fixed ID, so a project
/// symbol pasted into another project stays a project symbol there.
pub const PROJECT_SYMBOLS: LibraryId = LibraryId(0x0000_ecad_0048_5ea1_0000_0000_0000_0001);

/// Most characters of a project symbol's key.
pub(crate) const MAX_KEY: usize = 16;

/// How a project symbol joins a device.
use ecad_model::Join;

/// Whether `d` is a symbol made or edited in a project (its origin is [`PROJECT_SYMBOLS`]).
pub fn is_project_symbol(d: &Definition) -> bool {
    d.origin.as_ref().is_some_and(|o| o.library == PROJECT_SYMBOLS)
}

/// The part a symbol of kind `k` plays in its device, with no poles of its own.
pub(super) fn part_of_kind(k: Kind) -> PartRole {
    match k {
        Kind::Fixed { exclusive: true } => PartRole::Head,
        Kind::Contact { .. } | Kind::ChangeOver => PartRole::Contact,
        Kind::Fixed { exclusive: false } => PartRole::Pole,
        Kind::TerminalBlock => PartRole::TerminalBlock,
        Kind::Channel(_) => PartRole::Channel,
        Kind::ConnectorHalf { .. } => PartRole::Half,
        Kind::Tagless => PartRole::Other,
    }
}

/// A project symbol's part in its device, declared when the symbol is
/// made: its derived starter's kind, else a head unless its defaults join it as a part.
/// A project symbol carries no poles of its own.
pub(crate) fn project_part_role(d: &Definition) -> PartRole {
    let kind = match d.derived_from.as_ref().and_then(starter_of_origin) {
        Some(s) => s.kind,
        None => Kind::Fixed { exclusive: join_of(d) != Join::Part },
    };
    part_of_kind(kind)
}

/// How project symbol `d` joins a device: `part` only when its defaults say so.
pub fn join_of(d: &Definition) -> Join {
    d.defaults.join.unwrap_or_default()
}

/// The starter an origin names (by library and ID).
pub fn starter_of_origin(o: &Origin) -> Option<&'static Starter> {
    (o.library == STARTER_LIBRARY).then(|| STARTERS.iter().find(|s| s.id == o.definition)).flatten()
}

/// Whether `key` is a valid symbol key: 1 to [`MAX_KEY`] letters, digits, `-`
/// and `_`, starting with a letter or digit.
pub(crate) fn valid_key(key: &str) -> bool {
    (1..=MAX_KEY).contains(&key.len())
        && key.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_')
        && key.bytes().next().is_some_and(|b| b.is_ascii_alphanumeric())
}

/// One embedded version of a project symbol.
#[derive(Debug, Clone, Copy)]
pub struct Version<'a> {
    pub id: DefinitionId,
    pub def: &'a Definition,
    /// Its lineage (`origin.definition`).
    pub lineage: DefinitionId,
    /// Its version (`origin.library_seq`).
    pub seq: u64,
}

impl<'a> Version<'a> {
    /// `def` as a project-symbol version, if it is one.
    pub fn of(id: DefinitionId, def: &'a Definition) -> Option<Self> {
        let o = def.origin.as_ref().filter(|o| o.library == PROJECT_SYMBOLS)?;
        Some(Version { id, def, lineage: o.definition, seq: o.library_seq })
    }

    /// Its key (`defaults.key`), if it has one.
    pub fn key(&self) -> Option<&'a str> {
        self.def.defaults.key.as_ref().map(|k| k.as_str())
    }
}

/// Every embedded project-symbol version (a pass over the project's definitions, which are few: the
/// symbols it uses, not its objects).
pub(crate) fn project_versions(doc: &Document) -> impl Iterator<Item = Version<'_>> {
    doc.definitions().iter().filter_map(|(id, d)| Version::of(*id, d))
}

/// The versions of `lineage`, oldest first.
pub fn versions_of(doc: &Document, lineage: DefinitionId) -> Vec<Version<'_>> {
    let mut v: Vec<Version<'_>> = project_versions(doc).filter(|x| x.lineage == lineage).collect();
    v.sort_by_key(|x| (x.seq, x.id));
    v
}

/// The newest version of `lineage` (highest version; the higher ID on a tie, from a merge).
pub fn newest(doc: &Document, lineage: DefinitionId) -> Option<Version<'_>> {
    versions_of(doc, lineage).pop()
}

/// The newest version of every lineage, by name then lineage.
pub fn project_symbols(doc: &Document) -> Vec<Version<'_>> {
    let mut by: BTreeMap<DefinitionId, Version<'_>> = BTreeMap::new();
    for v in project_versions(doc) {
        match by.get(&v.lineage) {
            Some(cur) if (cur.seq, cur.id) >= (v.seq, v.id) => {}
            _ => {
                by.insert(v.lineage, v);
            }
        }
    }
    let mut out: Vec<Version<'_>> = by.into_values().collect();
    out.sort_by(|a, b| a.def.name.cmp(&b.def.name).then(a.lineage.cmp(&b.lineage)));
    out
}

/// The project symbol whose newest version has key `key` (case-insensitive), as that version.
pub fn project_by_key<'a>(doc: &'a Document, key: &str) -> Option<Version<'a>> {
    let k = key.trim();
    project_symbols(doc).into_iter().find(|v| v.key().is_some_and(|x| x.eq_ignore_ascii_case(k)))
}

/// Whether some version of a project symbol has key `key` (case-insensitive).
pub(crate) fn has_project_key(doc: &Document, key: &str) -> bool {
    let k = key.trim();
    project_versions(doc).any(|v| v.key().is_some_and(|x| x.eq_ignore_ascii_case(k)))
}

/// The op that keeps the project's tag prefixes on project symbols true after
/// `ops`: a prefix whose key no version holds any more moves to the new key of the symbol that had
/// it (a rename), or is cleared (its versions removed). `None` when nothing changes.
///
/// # Semantics
/// Reads only the project's symbol versions and `ops`' `EmbedDefinition`/`RemoveDefinition`, so
/// it goes last in the plan. A rename onto a key that already has a prefix keeps that one.
pub(crate) fn prefix_follow(doc: &Document, ops: &[Op]) -> Option<Op> {
    use ecad_model::naming::prefix::PrefixOf;
    let removed: BTreeSet<DefinitionId> = ops.iter().filter_map(|o| if let Op::RemoveDefinition { id, .. } = o { Some(*id) } else { None }).collect();
    let embedded: Vec<Version<'_>> =
        ops.iter().filter_map(|o| if let Op::EmbedDefinition { id, definition } = o { Version::of(*id, definition) } else { None }).collect();
    let lower = |k: &str| k.trim().to_ascii_lowercase();
    let mut after: BTreeSet<String> = project_versions(doc).filter(|v| !removed.contains(&v.id)).filter_map(|v| v.key().map(lower)).collect();
    after.extend(embedded.iter().filter_map(|v| v.key().map(lower)));
    let before = doc.settings().clone();
    let mut next = before.clone();
    for (of, prefix) in &before.tag_prefixes {
        let PrefixOf::Project(k) = of else { continue };
        if after.contains(&lower(k)) {
            continue;
        }
        next.tag_prefixes.remove(of);
        let lineages: BTreeSet<DefinitionId> =
            project_versions(doc).filter(|v| v.key().is_some_and(|x| x.eq_ignore_ascii_case(k.trim()))).map(|v| v.lineage).collect();
        let renamed = embedded.iter().filter(|v| lineages.contains(&v.lineage)).max_by_key(|v| v.seq).and_then(|v| v.key());
        if let Some(new) = renamed.filter(|k| PrefixOf::project_form(k)).map(|k| PrefixOf::Project(k.into()))
            && !before.tag_prefixes.contains_key(&new)
            && !next.tag_prefixes.contains_key(&new)
        {
            next.tag_prefixes.insert(new, prefix.clone());
        }
    }
    (next != before).then_some(Op::SetProjectSettings { before, after: next })
}

/// The key a symbol's registry entries use: a starter's catalogue key, else a project symbol's own.
pub fn registry_key(d: &Definition) -> Option<String> {
    if let Some(s) = of_definition(d) {
        return Some(s.key.to_string());
    }
    d.defaults.key.as_ref().map(|k| k.to_string())
}

/// Whether `d` is a connector half or pin: a connector starter whose bridges join the
/// side keys.
pub fn is_mating(d: &Definition) -> bool {
    of_definition(d).is_some_and(|s| matches!(s.kind, Kind::ConnectorHalf { .. }))
        && d.bridges.iter().any(|g| g.contains(&SOCKET_KEY) && g.contains(&PLUG_KEY))
}

/// Whether bridge key `k` of definition `d` is one a placement can write:
/// one of the definition's own terminal keys, a side key on a connector half, or a level
/// key of its multi-level terminal type.
pub fn bridge_key_ok(d: &Definition, k: &PointKey) -> bool {
    if d.connection_points.iter().any(|t| t.key == *k) {
        return true;
    }
    match of_definition(d) {
        Some(s) if matches!(s.kind, Kind::ConnectorHalf { .. }) => *k == SOCKET_KEY || *k == PLUG_KEY,
        Some(s) => crate::terminal_strips::terminal_blocks::of_starter(s.key).is_some_and(|(t, _)| t.bridges.iter().any(|g| g.contains(k))),
        None => false,
    }
}

/// Most characters of a search query that are used.
pub const MAX_QUERY: usize = 60;

/// The symbols matching `query`: a substring of the key, name or an alias, or the
/// tag prefix with or without `-`. An empty query lists every symbol, in table order.
pub fn search(query: &str) -> Vec<&'static Starter> {
    let q: String = query.trim().chars().take(MAX_QUERY).collect::<String>().to_lowercase();
    if q.is_empty() {
        return STARTERS.iter().collect();
    }
    let bare = q.trim_start_matches('-');
    STARTERS
        .iter()
        .filter(|s| {
            s.key.word() == q
                || s.name.to_lowercase().contains(&q)
                || (!s.family.word().is_empty() && s.family.word().contains(q.as_str()))
                || s.aliases.iter().any(|a| a.contains(q.as_str()))
                || (!bare.is_empty() && s.prefix().eq_ignore_ascii_case(bare))
        })
        .collect()
}
