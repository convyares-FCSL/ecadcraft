# D2: first builds off the UI thread, and edits that scale with what changed

What changed, and how to try it by clicking. (Written by the agent; nothing here is owner-tried yet.)

- **Checks, Reports and the Release dialog no longer freeze on first show.** The Checks list and
  the report models are built on the background pool the first time the project panel needs them.
  Open a large project and open Checks, Reports (any tab) or Publish ▸ Release: the surface says
  "Running…" and fills in on a later frame. An edit made while it runs does not apply the old
  result; the build starts again for the new state. The Release dialog for an issued revision
  (Revisions ▸ Release on an issued row) reads that revision's state on the spot (about 50 ms on
  the large project) and works out its report models and check on the pool, saying "Running…"
  meanwhile (about 2 s on the large project, formerly a freeze of that length). The pending
  revision's dialog (Publish ▸ Release) sorts and groups its rows on the pool too, keeps only the
  rows it shows (5 a group, with the group's count) and works out their places in words when the
  result lands: on the large project (about 30 000 rows) the UI thread spends about 11 ms, formerly
  a 2.7 s freeze. After an edit the dialog keeps the last view until the new one lands.
- **Edits that touched a device, a layer, a definition or a link are fast again on big projects.**
  The references view now says exactly which cables an edit can reach (`cables_touched_by` returns
  `Touch::Cables` or `Touch::All(reason)`); a device, strip or connector name reaches none.
  `cargo xtask layers` counts every "everything" fallback (`WIDE` in `views_guards.rs`).
- **Files of another format version open read-only and say so** ("older format version (file 1,
  this build 2)"); "damaged" is only for corruption at this build's version. `SCHEMA_VERSION` is 2;
  every format change bumps it (a golden-hash pin fails otherwise). Files written before this
  build must be written again.
- Headless harnesses (`App::new`) still build the views on the calling thread; only the desktop
  (`App::start`) uses the pool.
