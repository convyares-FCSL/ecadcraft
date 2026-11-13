# Brief 042: Inspector, Settings and the Standards screen (U4c)

Status: Draft (U0 designer, 2026-11-13), for the overseer and the owner.
Branch: `feature/ui-inspector`, from `origin/dev` after `fix/ui-followups` has merged.
Designer: done (`docs/design/ui-architecture.md` §11; the look and rules are spec §1 principle 7,
§2 "Settings is a place you visit", §5 "What you set and what is read", the push-back's item 8,
screen e) · Size: L, in four steps
Roadmap: M1 **1.13(b)** (the Standards screen) and the inspector part of **1.5**. Lane **U4c**;
runs beside 040, 041 and 043.
Inputs: spec §2, §3, §5; screen e (Settings ▸ Standards: one table of every default with an origin
badge, the chain on the right, change at a level or reset, consequences said first); owner answers
4 (page-level values only for function, location and sheet) and 6 (K as a project override of
IEC's Q); design 035 §3.2 (Agent access "until the UI shell gives settings a home"); U0 O7.

## Goal

Every default arrives filled in, shows where it came from, and is changed where it is shown: the
inspector badges each value's origin, and one Settings ▸ Standards screen lists every default with
its chain and changes it at the level chosen, with no scattered dialogs.

## What exists today (`dev` after #74)

- **Registry** (031, extended by 032–036): `ecad_cadcraft::defaults` with `Entry { key, label,
  group, kind, builtin, levels, project }`, `resolve(doc, entry, Scope) -> Resolved { value, from:
  Source }`, `Source::{Builtin, Standard, Project, Page, Object, Symbol}`, `all()`; the read-only
  `defaults` command. `Level::Standard` is **reserved** ("no source until the Standards brief"):
  named standards (roadmap 1.13(a), the style model) are not built.
- Page level is allowed only by the text **show** entries (`text.*.show`, levels SPG / SPGO),
  which is 031's per-page show and hide (spec §3 keeps it: "Show or hide per text at symbol, page or
  project level"). No value entry allows page level, which already matches owner answer 4.
- **Right panel** (`shell/right.rs`): the ECAD properties section (`sheets_ui::section`), unchanged
  from CADCraft's Properties palette; no origin badges.
- **Settings** (`shell/mod.rs` `settings_overlay`): an `egui::Window` with only the developer
  switch. Project settings live in two popouts of `sheets_ui` (`Popout::Info`, `Popout::Settings`,
  with 031's Text section from `text_ui.rs`). Their grow-every-frame defect (038 open point 4) is
  fixed on `fix/ui-followups` (`e415a693`): the scroll area sizes to its content
  (`min_scrolled_height`). That branch also sets the first window to 1680 × 940, growing to the
  largest size that fits the monitor, and keeps a user's own size (038 open point 5).
- **Dependency:** this brief starts after `fix/ui-followups` merges, because it moves the same
  popout code (`sheets_ui.rs` `ProjectProps`) and must keep that sizing.
- **Agent access** (035) is a row in the Developer section, shown in the Text panel's Commands tab
  (`developer.rs`).
- The status bar shows the standard's name (`shell/status_bar.rs`).

## §0 Files touched

● owned by this lane; ○ shared, with the hunk named.

| File | Change |
| --- | --- |
| ● `ecad/apps/ecad-trial/src/settings/` (new: `mod.rs`, `standards.rs`, `project.rs`, `text.rs`, `agent.rs`, `display.rs`, `developer.rs`) | Settings as a place: a full-window overlay over the main view and panels, closes back to where you were; its sections |
| ○ `ecad/apps/ecad-trial/src/shell/mod.rs` (`settings_overlay` removed; one call to `crate::settings::show`) | |
| ○ `ecad/apps/ecad-trial/src/lib.rs` (one `pub mod settings;` line; `open_project_popout` redirects to Settings sections) | Not the go-to code around `goto_symbol` / `goto_symbols`, which `fix/ui-followups` changed |
| ● `ecad/apps/ecad-trial/src/shell/right.rs`, `sheets_ui.rs`, `text_ui.rs` | Inspector: the properties section with origin badges and set-versus-read; the two popouts become Settings sections, keeping `fix/ui-followups`' content-sized scroll area |
| ● `ecad/apps/ecad-trial/src/developer.rs` | The Agent access row moves to Settings ▸ Agent access (035's element names kept) |
| ● `ecad/apps/ecad-trial/src/shell/status_bar.rs` | "Standard: IEC" opens Settings ▸ Standards |
| ● `ecad/apps/ecad-trial/src/lists/standards.rs` (new column set) | The Standards table on U3's component |
| ○ `ecad/apps/ecad-trial/src/lists/mod.rs` (one `pub mod standards;` line) | |
| ● `ecad/crates/ecad-cadcraft/src/default_cmds.rs` (new) | `setdefault {key, level: project\|page\|object, page?, component?, representation?, value \| reset: true}`: writes through the same properties and settings the registry reads (no new op, no new stored field); refuses a level the entry does not allow, in the designer's words |
| ○ `ecad/crates/ecad-cadcraft/src/lanes.rs` (one line `("042", crate::default_cmds::specs),`) and `lib.rs` (one `mod` line) | |
| ○ `ecad/crates/ecad-cadcraft/src/defaults/mod.rs` (read-only helpers only: `chain(doc, entry, scope) -> Vec<(Level, Option<Value>)>`, `users(doc, entry) -> usize`) | No change to entries or resolution |
| ● `ecad/crates/ecad-cadcraft/tests/all/lanes/lane_042.rs`, `ecad-trial/tests/all/lanes/lane_042.rs` | Tests; one `mod lane_042;` line in each `lanes/mod.rs` |
| ● `ecad/docs/quick-start/042-ui-inspector-standards.md` | How to try it |

Not touched: the canvas (040; its sheet choice gets a control in Settings ▸ Display only if 040 is
in `dev`, otherwise later), tools and wheel (041), the banner, History and `agent/` (043),
named standards and the Standard level (1.13(a)), any entry's levels.

## Scope, in order

1. **Settings, a place you visit** (spec §2): opened from the project menu, the status bar's
   standard, any origin badge (at that key) (no key of its own: spec §5's key table has none); a left list of sections (Standards,
   Project, Text styles, Display, Agent access, Developer) and a body; Esc or Close returns to
   where you were. The two popouts become the Project and Text styles sections, contents
   unchanged, keeping the content-sized scroll area from `fix/ui-followups` (its regression test
   in `tests/all/sheets_ui.rs` keeps passing against the sections). Agent access moves from Developer to its own section (O7); the Commands
   tab no longer shows it.
2. **`setdefault`** (no op, no format change): the one writer for the Standards screen and the
   inspector's "change at level" and reset. Table-driven contract tests: each level each entry
   allows; refusals for a level it does not; reset removes the override and `resolve` falls back
   one level; one transaction per call, undoable.
3. **Standards screen** (screen e) on the U3 table: one row per registry entry (key, label, group,
   resolved value, origin badge); filter by group and by "overridden"; the detail on the right shows
   the chain built-in → standard → project → page → object with the level in use highlighted;
   **a level is shown only when it holds a value or the entry allows it** (push-back 8), and page
   level only where the entry allows it (owner answer 4: values never; show and hide yes). Change at
   a level, or reset, through `setdefault`. **Consequences said first**: before applying, the detail
   states how many objects use the value and what changes for them (for a tag prefix: "3 devices
   use K; fixed and released tags keep their names; new devices get Q", spec push-back 2); computed
   by `users`, not by applying. The text styles (031) appear as the first **catalogue** (one row per
   kind with height, weight, font, shown); other catalogues of roadmap 1.13 (wire categories,
   cable types, location and box styles, title blocks, part sets) are added as column sets by the
   briefs that build their models.
4. **Inspector** (right panel): the selected object's values, or the page's when nothing is
   selected. Each value that comes from the registry carries a badge for its source (built-in,
   standard, project, page, object, symbol, "from box"); clicking it opens Settings ▸ Standards at
   that key; an overridden value has Reset. Values set here look like fields; values read from the
   drawing are grey text that go to where they are set (spec §5). Hidden texts' rows say
   "Shown on the page" with their level (spec §3). The list of the device's other placements uses
   040's `canvas::related` when it is in `dev`, otherwise the existing device list; its go-to runs
   `Trial::goto_symbols` (from `fix/ui-followups`).

## Out of scope

Named standards and switching IEC / US (1.13(a), owner question Q1), the Standard level as a
writable source, wire categories and other catalogues' models (1.4, 2.6, 2.7), part sets (M2),
the agent's ask-first mode (owner answer 3: a setting later), the banner and History (043).

## Update map

| View | Changed by | Does |
| --- | --- | --- |
| Inspector | `props_changes_since` containing the selection; selection change | Re-read the selected object's rows and their sources |
| Standards table | A commit writing a registry key (project property, page property, object property in the registry's key areas, or the settings the registry reads) | Patch the rows of the touched keys; tint changed cells |
| Settings ▸ Project, Text styles | As their popouts today | Unchanged |
| Settings ▸ Agent access | 035's access state changes | Re-read the row |

Every other op: nothing; the Standards table does not re-resolve all entries on an ordinary
commit.

## Acceptance criteria

1. **Settings** opens from the project menu, the status bar's standard and an origin badge, and
   Esc returns to the same workspace, page and selection (kittest). The Project and Text styles
   sections keep their 031 / 019 tests passing, and `fix/ui-followups`' sizing test passes against
   them.
2. **Agent access** works from Settings ▸ Agent access with 035's element names; 035's tests pass
   with the new location; the Commands tab no longer has the row.
3. **`setdefault`** contract tests pass; MCP can call it like any command; a refused level gives
   the reason and the allowed levels.
4. **Standards**: every `defaults::all()` entry has one row; setting `symbol.coil.prefix` (the contactor
   coil's prefix, written to the project's `tag_prefixes` setting) at project level shows a Project
   badge and the chain with the built-in value below; the consequence line appears before applying; reset returns to built-in.
5. **Inspector**: selecting a tag text shows its height with a Project or Built-in badge; clicking
   it opens Standards at `text.tag.height`; read values are grey and go to their source.
6. **Live**: a `textstyle` call from the Commands tab patches the text rows of the Standards table
   (counter: no rebuild) and tints them; a commit touching no registry key leaves the table alone.
7. **Lessons from U2/U3**: no per-frame O(entries) copies in the Standards table (borrowed rows,
   labels only for accessibility); the overlay hides the page, so nothing on the hidden page answers
   hover (`Shell::over_shown_page`); every key the brief names is pressed in a test.

## Tester

None by default: `setdefault` writes existing properties through existing ops, with no format or
store change. If the overseer reads `setdefault` as a new op surface, the tester runs criterion 3
only.

## Owner question

1. **Q1. Named standards before or after this screen?** Roadmap 1.13 splits (a) the style model
   (named standards such as IEC and US, the Standard level, stored per project) from (b) this
   screen. (a) is not built. **Recommend:** build the screen now over the registry as it is, with
   the standard shown as "IEC (built in)" and read-only; 1.13(a) then adds the Standard level and
   the IEC / US switch without changing the screen.
## Owner check

Change the contactor letter in Standards and watch what the screen says before applying it.
