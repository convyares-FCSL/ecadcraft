# Release 0.2.4: bring upstream across, tidy up, finish the improvement

Status: In progress (phase A)
Branch: `refactor/0.2.4`, cut from `dev`. Each lane works on its own `feature/0.2.4-<lane>` branch and is
merged into `refactor/0.2.4` with `--no-ff`; `cargo xtask ci` runs there. One PR goes into `dev` at the end.
Release ladder: 0.2.4 tidy-up → [0.2.5 features](release-0.2.5.md) → [0.2.6 UI](release-0.2.6.md) →
[0.2.7 format release](release-0.2.7.md) → 0.3.0, the functional core done well and the format freeze
([roadmap](../roadmap.md)).

## Goal

0.2.3 built the foundation. 0.2.4 brings upstream CADCraft across, finishes the tidy-up and fixes what the
0.2.3 end gate and two engineering reviews found, so the features of 0.2.5 build on a framework that needs
no more restructuring. Finding numbers (`#1xx` from the engineering reviews, `B`/`A` from the independent
review) are kept so the work can be traced; each is described here in full.

## Order

Items are numbered by when they were found, not by order. The order is:

- **A. Docs and sync prep:** #166, then #181 and #176–#182 (item 11), and item 6's schema version (B20).
- **B. Upstream sync** (item 1).
- **C. Bugs:** B1 and B2 (item 12) first, then #161, #165, #183, #163, #164 (item 11), then #162 and B17
  with the performance work (items 8 and 9). The agent items B8, B9 and B11 alongside (B9 needs item 1's
  command set).
- **D. Typed commands** (item 2, with #148). Also decided here: whether command declarations move from
  ecad-electrical into the engine, if typed commands make it cheap. Splitting merge and connectivity out of
  ecad-model waits for 0.3.0, and only if build times justify it.
- **E. Guards and debt:** item 5, the rest of item 7, item 11's #167–#175.
- **F. The rest:** items 3, 4 and 10, then #150 (tests by function) last, the largest.

## 1. Upstream CADCraft sync

Last sync: CADCraft 0.19.0 (`ecad/UPSTREAM.md`). Upstream `main` is several hundred commits ahead at 0.21.0,
and about half of the files ECAD patches have changed upstream (heaviest: ui-egui `canvas.rs` and `lib.rs`,
engine `cmd/file.rs` and `session/interactive.rs`, fonts `mtext.rs`). The sync's own policy draft is
[upstream-sync-policy.md](upstream-sync-policy.md).

1. Fetch upstream by URL (no new remote in the repository config). Pick the target: the latest release tag,
   or `main` if no tag covers it, and say which and why.
2. Seam review first: for each file in `ecad/UPSTREAM-PATCHES.md` (patched files and the files ECAD compiles
   unchanged), what upstream changed and whether the patch still applies, needs re-doing or is no longer
   needed (upstream may now offer the hook). Also new upstream crates or dependencies, a Rust toolchain
   change (ECAD pins one in `ecad/rust-toolchain.toml`) and changes to the `cadcraft-web` wasm build. Report
   before merging.
3. Merge, re-apply the seams, update `UPSTREAM.md`'s `Commit:` line and `UPSTREAM-PATCHES.md`; `cargo xtask
   patches` green.
4. Build, full suite, medium bench against `dev` before the sync, and a side-by-side look at everything
   drawn through the seams (canvas, text and mtext, PDF). Report.
5. List the `ECAD-SEAM` hooks generic enough to offer upstream; no upstream PR without the owner's go.
6. From then on the upstream check is weekly, so syncs stay small.

Settled with the sync: where the UI fonts live. The interface and sheet fonts (IBM Plex Sans, Barlow Semi
Condensed) are in `ecad/assets/fonts/`, while the house rule puts font files in the craft-fonts repository,
which is upstream's. Decide which stands, and move the fonts or record the exception in
[design/decisions.md](../design/decisions.md).

## 2. Typed commands end to end

Every command is typed end to end, as in a production RPC protocol: one definition per command with its id,
its params type and its reply type, shared by the engine, the app and MCP. JSON is only the wire format;
nothing outside the transport edge handles a raw `serde_json::Value`. Takes in #101 (a text check that
replies are built from structs) and #148 (session commands are written twice, once in `TYPED` and once in
`specs()`, 14 of them declared centrally; shared form plumbing sits under `properties/`; the agent policy's
hand lists duplicate each generic command's `ACCESS` and `IO`: one `SessionEntry` list like
`generic::Entry`, with declarations that carry their parameter-conditional reads).

1. **Trait shape first.** Both command traits (the engine's `Command`, electrical's `Generic`) require
   `type Params` and `type Reply`, with no default. Params and replies are documented pub structs or enums
   (`Serialize + Deserialize + JsonSchema`; their doc comments are the help). A bound or marker trait stops
   `Value` being used as a reply. The outcome carries the typed reply, serialised only at the edge (generic
   runner, MCP, agent bridge). Refusals stay a typed `CmdError`. The app calls
   `call::<P>(params) -> Result<P::Reply, CmdError>` and builds params as structs: no `json!` for commands
   in ecad-app. MCP's describe shows both schemas. Show the trait shape, the call signature and one finished
   area (wires) with line counts, and wait for approval before the rest.
2. **Then area by area,** one commit per feature folder in the source layout's order (wires, cables,
   terminal strips, PLC, references, devices, pages, reports, styles, title blocks, history, the rest): the
   area's commands typed on both traits, replies built from the structs, the app reading the structs, the
   MCP tests and the docs touched updated in the same commit.
3. Replies agents see may change shape (pre-alpha, no compatibility). List every changed reply key and
   command, and update the agent and MCP docs.
4. When every area is done, no `json!` builds a command reply anywhere and ecad-app holds no engine-reply
   `Value`; the developer guide's "adding a command" shows the new pattern. Say whether #101's text check is
   now redundant.
5. **A command reference made from the schemas,** generated into `docs/agents/commands.md` with a doc-sync
   test that fails when it is stale, and a `help <command>` command that shows one command's entry. Today
   there is no `help` command; the reference an agent reads comes from MCP `list_commands` and the control
   channel's `engine.commands`.

Measure lines added and removed per area; build and fast tests after each area, the MCP tests too.

## 3. UI string table, English only

On-screen text is keyed in one table; languages for it come in 0.2.5.

## 4. Rating fields

- **A generic rating reader:** today each device group's reader maps the words of a typed rating to its
  fields by hand, and a test makes sure every nameplate field of a group's list is read. The reader takes
  each value to a field by the field's type, so a new field needs no reader code.
- **List fields by `setprop`:** a power supply's `voltages`, a switching device's `control` and a fuse bank's
  `outputs` are read only on the Device tab and set through the whole rating text (or `rating.<n>` per
  fuse). They get list-typed values of their own.
- **One BOM value only when all agree:** a BOM row's article number and its rating columns show the most
  common value of the row's devices. A row shows a value only when all its devices agree, and flags the
  devices that differ.
- With it, #173: the hand lists a new device group has to copy (the rating-text groups, seven template
  statics, the app's labels) become one list each.

## 5. Guard follow-ups

Each ends with its guard at zero, with no counts kept.

- **Typed errors in the engine and the app** (with B13): the engine still has about 145 `Result<_, String>`
  lines and 35 `Err(format!(..))`, the app about 57 and 6. Move them to the crate's error type, open, save
  and branch first; then the text-error ban covers both crates like the others.
- **Stored text fields:** 11 fields in ecad-model's stored structs are still `String` (potential `name` and
  `prefix`, `unclassified_prefix`, `label_rule`; a definition's and two drawing names, `linetype`; two
  device labels; the ledger's `label`). Type them as `Text<N>` or a typed value (a format change: bump the
  schema version); then guard G13's entry is removed and G13 is a hard zero.
- **Raw egui controls:** 13 app panels made before the components still use `egui::TextEdit` or `ComboBox`
  (`RAW_CONTROL_FILES` in xtask). Convert them to `ecad_widgets::control`; an entry goes when its file has
  none left, and at none the list is deleted and the ban is total.
- **Literal sizes:** about 260 lines in 39 app files write a space, a stroke width or a size as a number
  (`add_space(4.0)`, `Stroke::new(1.0, ..)`, `vec2(12.0, ..)`). After a token pass gives each a token, the
  literal-size ban returns as a hard zero.

## 6. One schema version (B20)

`ecad_file::SCHEMA_VERSION` is the file's record version, checked on open and bumped with the format;
`ecad_model::SCHEMA_VERSION` was never bumped and is read only by the dev snapshot, and `UPSTREAM.md` named
it. Delete the model's and point the snapshot and `UPSTREAM.md` at ecad-file's.

## 7. From the second engineering review

Before any 0.2.5 feature builds on them:

- **#135** The Device tab's Identity and Parts sections loop over hard-coded key lists with their own labels,
  so a new row from electrical never shows. Walk electrical's rows by place and use their labels; the app
  keeps only the ordering.
- **#141** One rating grammar owned by the model: the `once`/`within`/`tokens` helpers are copied into each
  rating reader, and the motor field writer and the rating reader accept different words.
- **#142** Strip checks (now in reports) into `terminal_strips/`, cable cores (now in terminal strips) into
  `cables/`. With #175: cable forms (in `properties/forms/`) and PLC card placement (in
  `devices/interact.rs`) into their features.
- **#138** Stored terminal-block starter keys (`term2.1`, `Termlnpe`, `Termsensor`…) abbreviate "terminal";
  rename them, with a schema bump.
- **#139** Disk errors out of electrical's `CmdError` (`File`, `Binding`, `Path` wrap ecad-file errors that
  nothing in electrical builds): an engine-side error, or a map at the runner edge.
- **#140** Typed "not found": seven "no such …" refusals are still free text, and agent-visible JSON keys
  and model error strings use old words (component, conductor, terminal). `NotFound(Noun::…)`; `wire` and
  `device` keys.
- **#137** The OS boundary: the app reads settings folders from the environment through the engine,
  ecad-file checks `is_file` outside ecad-store, and the core OS guard misses `is_file`, `exists` and
  `metadata`. The host supplies the settings folder; extend the guard. With **#168**: the guard covers the
  app too, and the app host's hand-made `utc_time` uses chrono.
- **#144** A dead `pub use` of four electrical constants in the engine's sheet commands; delete it and move
  the status words to electrical.
- **#155** ecad-store's hand-rolled calendar conversion uses chrono.
- **#154** Compatibility shims out (`tag_rule`, `tag_marks_current`, the old device form), with a bump.
- **#156** The power-loss simulator (`ecad-store`'s `power` module) goes behind a fault-injection feature;
  only tests use it.
- **#132 and #134** One docs-truth pass: sentences the history sweep cut short, history the guard misses
  (no number, outside `src/`, in string literals and `Cargo.toml` comments), and docs that name the wrong
  thing. The history guard reads string literals and `Cargo.toml` comments too, and `cargo doc -D warnings`
  joins `xtask lint` (B14: 277 warnings and 82 broken links today).
- **#148** with item 2.
- **#150** Tests by function, per feature, with a guard on test paths: about 1,330 history comment lines in
  tests, `_acceptance` files beside plain ones, step-numbered test names, `lanes/` and `tester_*` folders in
  bench and inspector, store tests outside `tests/all/`, pure rule tests still in the engine. The largest
  item; last.
- **#183** `place {at}` with no symbol reuses any definition whose display name is "Two-point coil", so a
  user's own definition of that name is placed as the coil, and older projects gain a second embedded coil.
  Find the built-in coil by a fixed key, with a test for the name collision.

The rest of that review's 0.3.0 list stays at 0.3.0 or is done when its code is touched; the word "look"
(#158, against the glossary's "draw, never look") is the owner's call.

## 8. Redraw only what changed

Every edit re-projects the whole page on screen (about 1 ms); by design today. The product path is flat in
project size (release, no verify: 1.09 ms at 200 wires, 1.27 ms at 3,200 wires on 80 pages). With the test
build's whole-project check per commit, each edit costs about 0.1 ms more for each landed wire already in the
project (`wire` 1.16, 4.46, 18.6 s at 100, 200, 400 wires; `setstyle` and `category` alike), so something
does a full pass on every commit. Profile to find the pass, then make it incremental through the view's
update map. Target: per-edit cost flat in project size. Stacked lines 0.01 mm apart cost a further ~4.5× in
`wire` (the near-ends check); look at it in the same pass. A dense single page (100 to 800 wires) is not a
target on its own; redrawing only what changed removes its cost.

## 9. From the 0.2.3 end gate

- Bench slowdowns against `dev` before 0.2.3 on medium (nets 6.0 → 14.5 ms, terminal-block update 1.4 →
  6.1, grid change 3.7 → 6.3, finalise 30 → 46, net update 0.6 → 1.1). The generated workloads differ (5,992
  against 1,003 check rows), so first make the bench generate the same project on both.
- Large tier over target: nets churned 110 ms (≤ 100), net update 2.6 (≤ 2), cross-reference build 11.5
  (< 10), strip build 52 (< 50), I/O list update 7.3 (< 5), PLC frame update 8.2 (~8).
- The six pixel checks under `ECAD_TESTER_SHOTS` that fail on `dev` too.
- Demo overlaps: the wire label `N-01 2.5 BU` over the second `-X01` tag; continuation references `2/C1 3/D1`
  run into the revision block's Date column (fixed here if a layout bug, in 0.2.6 if styling).
- `setstyle` cost against the number of categories; other `dashed_line` callers against hostile geometry.
- With them, **#162** (the refusal snapshot clones the designation-uses index on every generic command:
  measure, then fix) and **B17** (`Ledger::preview` clones the whole document, so a wire draws twice; use
  `with_applied`; the clipboard keeps eight whole copies).

## 10. Project-name menu duplicates

In the project-name menu (top left), Project info and Project settings open the same Settings window as the
Settings entry below them, and Revisions overlaps History (the Rev pill). One entry each.

## 11. From the third engineering review

New findings, all in this release:

- **Bugs and risks:**
  - **#161** The generic runner writes a command's files before the edit commits, so a refused reply can
    leave files on disk (M).
  - **#162** with item 9.
  - **#163** Report columns (wire, cable, BOM) are parallel hand lists and `rows_json` zips them unchecked:
    one column type per report (M, before paged reports).
  - **#165** The page and folder diff allocates n × m from file input, so a hostile file can abort it: a
    longest-increasing-subsequence diff or a cap, with a hostile-file test (S).
  - **#183** with item 7.
- **Sync prep, first:** **#166** ecad-app compiles upstream's `control_server.rs` by `#[path]`, and no
  upstream file listed it, so the sync's seam review would miss it. It is listed in
  `UPSTREAM-PATCHES.md`; later the desktop host starts the listener and hands the app a channel.
- **Debt and guards:**
  - **#164** The golden fixture must cover every device group and kind.
  - **#167** Four wildcard-match lint opt-outs left from the 0.2.3 move.
  - **#168** with #137.
  - **#169** Process-wide state in shipped code (`HOST`, `PROCESS_DIR`, `SWITCH`, `TEXT_PASSES`).
  - **#170** The engine writes to disk directly while the rule says only ecad-store does: decide and align
    rule, guard and code.
  - **#171** The `test-ops` feature's delete condition is met: delete `apply_ops` and the feature
    (`access.rs`); make each test case that applies ops through it with a command.
  - **#172** The dark sheet is inferred by comparing colours: read the typed sheet.
  - **#173** with item 4.
  - **#174** `deny(clippy::indexing_slicing)` on ecad-file and the rating parsers, which read hostile text.
  - **#175** with #142.
- **Docs** (the rule: work not started or in progress is a draft in `docs/briefs/`, finished work is in the
  archive, unneeded work is deleted):
  - **#181** `ecad/CLAUDE.md`, `README.md` and `UPSTREAM-PATCHES.md` carry history, machine paths and stale
    names: one pass, before the sync edits `UPSTREAM-PATCHES.md`.
  - **#176** Paged reports are 0.2.6; the roadmap matches, and its open question on report sheets as pages
    lives in the paged-reports brief.
  - **#177** The archive index's row for paged reports points to `docs/briefs/`.
  - **#178** The never-built ecadlib contract draft is deleted (superseded); the binary-encoding proposal
    and the upstream sync policy move to `docs/briefs/` as drafts.
  - **#179** The release plans are release briefs in `docs/briefs/`; the roadmap's deferred specs are draft
    briefs, and the roadmap is one line per item.
  - **#180** The future parts of `design/ui.md`, `design/defaults.md` and `design/layout.md` move into
    briefs.
  - **#182** `docs/README.md` names `docs/briefs/`.

## 12. From the independent engineering review

- **Phase C bugs, first:**
  - **B1** A damaged file crashes ECAD inside redb (288 of 3,000 mutated files; a zeroed 4 KiB page in 47%
    of cases). Catch the unwind around every redb entry (open, reads, Verify, lazy reads after open, and
    B10: a redb panic or a read error after open must not end a session with unsaved edits), map it to
    damage or an open error, and never reuse the handle. Keep a small crash corpus as a test, with a seeded
    fuzz in the slow set, and report upstream (M).
  - **B2** An older-format file opens as 4 of its 18 transactions, the report hides the damage, and Save
    As keeps the truncated copy. Refuse, or say "4 of 18 read", and offer no Save As for a partial load of
    another format; fix the test that pins today's behaviour (S).
- **Agents:**
  - **B8** The agent's nets come from a second, simpler builder (wire ends only), not `Connectivity`: one
    builder and a differential test (S).
  - **B9** The agent gate is a denylist over a growing upstream command set: an allowlist (ECAD commands and
    reviewed drawing commands), `lisp` and `appload` in `NEVER_COMMANDS`, and a test that fails when a sync
    adds an unclassified command (M, with item 1).
  - **B11** `list_commands` hands an agent 602 commands (246 KB) and shows refused ones as enabled: default
    to ECAD's commands and the allowed set, and mark each never, read or write (S).
- **Guards:**
  - **B12** Property tests (an op and its inverse, incremental `Connectivity` equal to a full build,
    `merge(a, a) = a`, decode never panics, canonical form idempotent) and a `cargo fuzz` target on open (M).
  - **B14** = #132's `cargo doc -D warnings`.
  - **B16** `cargo deny` (licences and advisories) in `xtask lint`; the hostile-file path runs C code (S).
  - **B18** A bench ratio gate on medium, and the small tier on pull requests (S).
- **Performance:** **B17** with item 9.
- **Docs:** **B15** The roadmap disagreed with the release ladder, including on when the format freezes; it
  now follows the ladder and has a "Format freeze at 0.3.0" section (with #179). **B20** = item 6.
- Already planned: **B13** with item 5.

Placed elsewhere: the format items (B3–B6, A1) in [the format release](release-0.2.7.md), where A2 and A3
are decided too; A4 in phase D; B7 (recovery journal) and A5 (the agent tool set) in
[0.2.5](release-0.2.5.md); B19 (open off the UI thread) in [0.2.6](release-0.2.6.md).
