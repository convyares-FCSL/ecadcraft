# Release 0.2.5: features

Status: Draft, not started (the feature list is still to come)
Release ladder: [0.2.4 tidy-up](release-0.2.4.md) → 0.2.5 features → [0.2.6 UI](release-0.2.6.md) →
[0.2.7 format release](release-0.2.7.md) → 0.3.0 ([roadmap](../roadmap.md)).

## Goal

New features on the framework 0.2.4 leaves. The full feature list is to come; the items below are placed here
so far.

## Rule from this release

New electrical interactions are ECAD tools that emit ops directly, not CADCraft drawing diffs translated
afterwards (A3). Existing tools are not rewritten for it.

## Items

1. **Recovery journal for unsaved work (B7).** Unsaved work has no journal today, so a crash loses everything
   since the last save. Append pending transactions to a side journal every few seconds or transactions; on
   the next open, offer to replay it (the hash chain validates it). A crash should not lose an afternoon's
   work.
2. **An ECAD-domain agent tool set (A5).** Typed reads (project, nets from `Connectivity`, devices,
   references) and typed intent commands with `dry_run`, which returns the ops and a diff before committing,
   so agent behaviour can be tested without CADCraft. Builds on 0.2.4's agent allowlist and trimmed
   `list_commands` (B9, B11).
3. **Languages for the UI string table.** 0.2.4 keys every on-screen text in one English table; this release
   adds the languages.

Not yet placed in any release ([roadmap](../roadmap.md)): the panel layout page, power, tests and FAT,
network, title blocks built from elements, and the [potentials editor](potentials-editor.md).
