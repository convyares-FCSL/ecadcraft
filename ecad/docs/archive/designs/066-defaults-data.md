Historical record (2026-11-15). Not current; see [design/defaults.md](../../design/defaults.md).

# Design 066: Defaults as data, and what a saved project means

Status: **Approved** (owner, 2026-11-15: §9 answered; overseer, 2026-11-15: format changes F2 and
F3 approved). Brief: `docs/briefs/066-defaults-data.md`.

**Goal (one sentence).** Every built-in default lives in a data file shipped with ECAD, not in Rust.
A value a project chose stays chosen, and a project reads the same on any two machines whatever
company files they hold.

**Owner rulings (2026-11-15).** Built-in defaults must not be hard-coded; they move into
configuration data. The first value to change is the wire label cap height, 1.6 → 2.5 mm. ECAD is
pre-alpha: there is **one** built-in set, edited freely, and every project reads the current
built-ins. Existing projects show 2.5 mm labels the next time they are opened. Versioning the
built-ins waits for the v1 format freeze (§10). ECAD is desktop-only.

## 1. Where we are today

- **The registry.** `crates/ecad-cadcraft/src/defaults/` holds about 150 entries in 13 per-area
  files (`text.rs`, `wires.rs`, `print.rs`…). Each `Entry` is a Rust static: key, label, group,
  brief, kind (length / number / choice / text / parsed / style, with its range), **built-in
  value**, the levels that may override it, and where the project level is stored. Code reads
  `resolve(doc, entry, scope)`: object → page → project → standard → built-in.
- **Style built-ins.** Each style catalogue (`styles/text.rs`, `wire.rs`, `cable.rs`, `frame.rs`…)
  has built-in styles whose field values (a text style's cap height and weight, for example) are
  Rust literals too.
- **The standard is already a copy inside the project** (brief 050a, P8): `Document.standard` holds
  its header, its registry values and its style records. Nothing outside the project is read.
- **A chosen value can be lost (found by lane 060).** ecad-file **leaves out project settings
  equal to the built-in** (`crates/ecad-file/src/v1.rs:418-459`: `label_height`, `label_rule`,
  `label_min_length`, and likewise `tag_padding`, `wiring`, `sheet`, page number step, reference
  style). The registry reads them through "equal to the built-in means not set"
  (`defaults/text.rs:31-34`, `defaults/wires.rs:28-34, 54-57`). So a project that **chose** 1.6
  on purpose looks exactly like one that never chose, and moves to 2.5 with everyone else. That
  is the bug this design fixes. Projects that never chose follow the built-in, as intended
  pre-alpha.

## 2. Inventory: what moves to data and what stays code

The rule: **a value a company might reasonably want different, and which changes how a drawing
looks or reads, is a default → data. A bound that guards against hostile input, a contract other
data depends on, an algorithm's tuning, or the app's own UI size stays a constant.** The registry
already says this for hostile-input bounds (`defaults/mod.rs:16-17`).

### 2.1 To data in this lane

| What | Today | Notes |
| --- | --- | --- |
| Every registry entry's built-in value (about 150) | `builtin:` field of each `Entry` | Values only; see §3.3 for the rest of the entry. |
| Built-in style field values (text, wire, cable, location, page, rating table, checks) | `BUILTINS` slices in `styles/*.rs` | `frame` styles' title-block bodies excepted (§2.3). |
| Wire label cap height 1.6 → **2.5** | `ecad_model::wiring::DEFAULT_LABEL_HEIGHT`, read in 9 places | Already the entry `text.wire_label.height`; the constant goes. |
| Label rule `net`, shortest labelled wire 0 | `wiring::DEFAULT_LABEL_RULE`, `DEFAULT_LABEL_MIN_LENGTH` | Already entries `wiring.label_rule`, `wiring.label_min_length`. |

### 2.2 To data in a later step of this brief (066c), same mechanism

Project settings with a built-in, left out of the file at that built-in, with the same "chosen
value lost" bug. Each becomes a registry entry with its value in the data, and its settings field
"not set" instead of "equal to the built-in".

| What | Today |
| --- | --- |
| Tag number padding 2 | `ecad_model::doc::DEFAULT_TAG_PADDING` (and `devices::DEFAULT_PADDING`) |
| Wire number digits 2 | `wiring::DEFAULT_DIGITS` |
| Page number step 1 | `doc::DEFAULT_PAGE_NUMBER_STEP` |
| Tag dash `yes`, tag case `upper` | `tag::DEFAULT_DASH`, `DEFAULT_CASE` |
| Sheet A3 landscape, 8 × 6 grid | `default_sheet()` in ecad-file, `SheetSpec::default()` |
| Reference style, colour style `iec` | `default_style()`, `iec()` in ecad-file |
| Report table row height 5, header 7 mm, rows per sheet 44 | `generated.rs:49-50` `HEADER_MM`, `ROW_MM`; `outputs.rs:47` `ROWS_PER_SHEET`. Rows per sheet becomes **computed** from the sheet and the row height, not a third value. |

### 2.3 Defaults, but deferred (owner, 2026-11-15, call 16)

| What | Where | Why later |
| --- | --- | --- |
| House title block column widths and rows | `title_block/house.rs` (`DRAWING_MM`, `ROW_MM`…) | A title block is a structured layout (brief 055's designer), not a set of values; it becomes a data layout with libraries. |
| Cover page sizes | `front.rs` (`HEAD_MM`, `BODY_MM`, cell sizes) | Same: a page layout. |
| Terminal strip drawing look (box 8 × 24, lane pitches, stagger) | `strip_page.rs:52-74` | Look values and tuning are mixed in one block; separating them is its own small brief. |
| Drawing line weights (symbol 0.35, control wire 0.4, power 0.6, frame 0.5) | `ecad-tokens/src/line.rs` | Owner-ruled design tokens with a doc-sync test; a company line-weight table belongs with plot styles. |

### 2.4 Drawn text reads its style (066d; owner, 2026-11-16)

Owner ruling: drawn text never ignores its style. Every place drawing or layout code puts text on
a page reads its `TextKind`'s `TextStyle` (height, weight, font through `style_name`, colour);
no literal heights, weights or font names remain outside `styles/`, `defaults/`, the text style
modules and the TOML. `cargo xtask layers` enforces it at **0** (`drawn_text_violations`).

| Text | Where | Kind | Before 066d |
| --- | --- | --- | --- |
| Contact cross-reference, connector member references | `xref.rs` | Reference | `XREF_TEXT_MM` 2.5, literal "Standard" |
| Contact table rows | `xref.rs` | Table | style height, literal "Standard" |
| Continuation other-end reference | `continuations.rs` | Reference | literal "Standard" |
| Continuation caption | `continuations.rs` | ContinuationCaption | literal "Standard" |
| PLC card name block, type text | `plc/look.rs` | Tag / PartNumber | literal "Standard" (bold tag drawn regular) |
| PLC card caption | `plc/mod.rs` | PartNumber | Description height, PartNumber style |
| Cable marker text height (pick box) | `references.rs` | Reference | `TEXT_HEIGHT_MM` 2.5 |
| Cable look layout: label spacing, box pitch and padding, flags, slash labels | `look.rs`, `words.rs` | CableText | `LABEL_HEIGHT` 2.5 |
| Free text | `convert.rs` | Free (font, weight; height and colour the user's) | literal "Standard" |
| "PAGE UNAVAILABLE" placeholder | `project.rs` | Free (overseer ruling 3) | 5.0, "Standard" |
| Location label in a box style of its own | `draw/locations.rs` | LocationLabel, the box's style as a whole | literal bold/regular names |
| Strip page terminal label | `strip_page.rs` | PointLabel | Table |
| Strip page location in the tag box | `strip_page.rs` | LocationLabel | Table |
| House title block: Page, Rev, Title, Drawing no., Project, values, company line | `title_block/house.rs` | TitlePage, TitleRev, TitleMain, TitleValue, TitleCaption weights | literal `bold: true` |
| Cover block titles, body, column heads, customer line | `draw/front.rs` | Cover × `[layout]` factors 0.5, 0.35, 0.24, 0.38 (ruling 5) | literals 5.0, 3.5, 2.4, 3.8 |
| Device texts, wire labels, report tables, I/O list, rack tiles, frame grid, title captions, cover heading, cable texts | various | their kinds | already read their styles |

**Reference default:** 2.5 mm (ruling 6), one value in `text.toml`: contact cross-references, on
almost every schematic page, keep their size; continuation references and cable reference ends
move from 2.0 to 2.5.

**Exempt by location** (the guard's `DRAWN_TEXT_EXEMPT`): symbol definitions and builders
(`ecad-electrical/src/symbols*`: glyph text is line art in symbol units, ruling 1), text inside a
title block logo (graphics, ruling 2), user and DXF-imported title block items (their sizes are user
data, ruling 4; `draw/title_block/dxf.rs`), screen-only overlays (`interact/`, pixels), commands,
and the style modules themselves. Base text entities are built with
`text_style::PLACEHOLDER_STYLE`, which every drawn text overwrites with its kind's style.

Not changed: the revision rows' fill is a named fit bound (`ROW_FILL`), not a size; the strip
tag's size is the TableHeading style times the registry's `strippage` tag factor (data).

### 2.5 Stays code

Bounds and guards (every `MAX_*`, `MIN_*`, `LABEL_HEIGHTS`, an entry's range), command result limits
(`DEFAULT_LIMIT`, `DEFAULT_ROWS`, `DEFAULT_BAR`), the agent port, grids that symbols are drawn on
(`SYMBOL_GRID_MM`, `POINT_GRID_MM`, `checks::GRID_MM`: changing one breaks every library symbol),
sheet frame geometry in `ecad_model::sheet` (margins, title block band: every coordinate depends on
it), font metrics (`CAP_HEIGHT_EM`), the store's snapshot interval, UI sizes (`TILE_MIN_HEIGHT`,
agent outline), strings and keys.

## 3. The data files

### 3.1 Why TOML

TOML is what Rust projects use for configuration, a maintained parser (`toml` 1.x) is already in
both lock files through CADCraft, and it is plain enough for a person to edit; the YAML crates for
serde are no longer maintained.

### 3.2 One built-in set, one file per area, embedded

```
ecad/crates/ecad-electrical/defaults/
  text.toml  wires.toml  print.toml  symbols.toml  terminals.toml  connectors.toml
  strip_pages.toml  bom.toml  styles.toml  continuations.toml  plc.toml  pages.toml  rating.toml
```

- **One file per area**, matching today's registry files, so lanes adding entries never edit the
  same file (the per-lane rule). A new entry's value goes into its area's file.
- **Edited freely** (owner, 2026-11-15): changing a value is editing its line. Every project that has
  not chosen its own value follows the change on its next open.
- **Embedded at build time** (`include_str!`, listed in one Rust table, one line per area). The
  build never reads the disk for its built-ins, and a damaged or partial install still has them.

### 3.3 Schema

One area file (`text.toml`):

```toml
# Lane T (brief 031). Values only: kind, range, label and levels stay in the registry (§3.4).
[values]
"text.font"                 = "Barlow Semi Condensed"
"text.tag.show"             = "on"
"text.wire_label.height"    = 2.5      # millimetres (owner, 2026-11-15: was 1.6)
"text.run.gap"              = 15.0
"text.tag.gap"              = 0.6      # × tag height

[styles.text.tag]
height = 2.5
weight = "bold"

[styles.text.point_label]
height = 1.5
weight = "regular"
```

Rules:

- Keys under `[values]` are the registry key, quoted, exactly as stored in a project and as
  `setdefault` takes them. Lengths are numbers in millimetres; choices and texts are strings; a
  number is a number. No units in values.
- `[styles.<catalogue>.<style id>]` sets a built-in style's field values. A style's name stays in
  code.
- The wire label style's height is the entry `text.wire_label.height` (one value, one place, as
  today); `styles.text.wire_label` does not repeat it.

### 3.4 What stays in code: the entry's description

**Only the values move. Kind, range, levels, project source, label, group and brief stay in the
Rust `Entry`.** The range is the guard that keeps a hostile or mistaken value out
("hostile-input bounds stay constants"); if a data file could widen it, a company file could too.
Labels are UI words that will go through localisation, not through a defaults file. The `builtin:`
field of `Entry` goes away; code asks the loaded data for the value.

## 4. Layers

Resolution keeps today's order:

```
object → page → project → standard (the project's copy) → built-in (the embedded TOML)
```

| Layer | Lives | Read when |
| --- | --- | --- |
| Built-in | In the ECAD build (embedded TOML) | Always; the current build's values |
| Company (house) standard | Copied into the project as its standard (brief 050a) | Through the project only. A company file on the machine is **never** read on open. |
| Project, page, object | In the project, as today | Always |

**A project never silently picks up a machine's company file.** A company standard reaches a project
only through an explicit, logged edit (P8: "library changes reach a project only by an explicit
edit"), as brief 050a designed the standard. So a project reads the same on two machines with
different company files. Loading a company standard from a TOML file waits for libraries
(stage 3; owner, call 14) and is listed in `docs/deferred-work.md`.

(Two machines with **different ECAD builds** may read a project differently where it relies on a
built-in that changed between them. That is accepted pre-alpha; versioned built-ins are deferred to
the v1 format freeze, §10.)

## 5. What a saved project means

**"Not set" is stored as not set, never as "equal to the built-in".** A settings field with a
built-in becomes optional:

- absent: the project follows the built-in (or its standard), and moves when the built-in changes;
- present: the project chose it, even when the value equals the built-in today. It never moves on
  its own.

This covers `wiring.label_height`, `label_rule`, `label_min_length` in 066b (F2) and the §2.2 list
in 066c (F3). The registry's "equal to the default means not set" readers go
(`defaults/text.rs` `wire_label_height`, `defaults/wires.rs` `label_rule_setting`,
`label_min_length_setting`). A project that chose 1.6 keeps 1.6; a project that never chose shows
2.5. Clearing a chosen value (the Standards screen's existing "use default") stores absent.

### 5.1 Format changes (approved by the overseer, 2026-11-15)

| # | Change | Old files | Older builds |
| --- | --- | --- | --- |
| F2 | `wiring.label_height`, `label_rule`, `label_min_length` become optional: absent = built-in. Written only when the project set them | Absent meant "the built-in"; it still does (the built-in is now 2.5). A pre-066 project that chose 1.6 wrote nothing, so it cannot be told apart and shows 2.5: accepted pre-alpha | A present field reads as today |
| F3 (066c) | The §2.2 settings fields get the same treatment and become registry entries | Same as F2 | Same as F2 |

No op changes: `SetProjectSettings` already carries the whole settings struct. Both v1 and v2
encodings carry `ProjectSettings` through the same serde type, so one change covers both. Replay of
an old history is unchanged except where it relied on a built-in.

## 6. Where it lives

Crate map approved (lane 065): the registry and its TOML go into **ecad-electrical** (Core, no OS
access), which lane 067 is creating on `feature/crates`.

- The `Entry` / `Kind` / `Value` / `Level` types, `parse_value`, the embedded TOML, the loader and
  validator, and `resolve` move to ecad-electrical. ecad-model and ecad-file need built-in values
  only through the registry (the settings fields' "absent" now needs no constant), and
  ecad-electrical never depends on a `cadcraft-*` crate.
- The style-existence check (`Kind::Style` naming a project style) needs the style catalogues; it
  stays a wrapper in ecad-cadcraft (`parse_in`) until the catalogues move too.
- `toml` becomes a dependency of ecad-electrical.
- The 066 worker starts after 067 lands in `refactor/0.2.3`, branching from `refactor/0.2.3`.

## 7. Validation: broken or hostile data never crashes

One loader reads the built-in files now, and a company standard file later (stage 3). It returns
the values it accepts and a list of problems; it never panics and never returns a value outside an
entry's range.

| Problem | What happens |
| --- | --- |
| File larger than 256 KiB, or nesting deeper than 16 | Refused before parsing (cheap scan). |
| Not TOML (syntax error) | Whole file refused; one problem with line and column. |
| Unknown key | Ignored; one problem per key, first 50 listed then "and *n* more". |
| Wrong kind (`"2.5"` for a length, `2` for a choice) | That value refused; the layer below is used. |
| Out of range, not finite, unknown word, control characters, too long | That value refused; the layer below is used. |
| An entry with no value in any file | Test failure. |

- **Built-in files** are checked by tests (§8), so a shipped build always has valid data. At run
  time a built-in that still fails to load (not reachable in a tested build) gives a Checks row
  `defaults-damaged` on the project, and each affected entry uses its kind's safe value (the range's
  low end, the first choice); never a crash.
- **Stored project values** keep today's rule: one that does not parse counts as absent.
- **Errors.** The loader's error type is a `thiserror` enum in ecad-electrical (`DefaultsError`:
  `TooLarge`, `TooDeep`, `Syntax { line, column, message }`, `UnknownKey`, `WrongKind`,
  `OutOfRange`, `Missing`), each carrying the file name and the byte span the `toml` parser
  reports. That is what `miette` needs to underline the place in the file, so lane 061's later
  `miette::Diagnostic` derive adds no reshaping. This brief does not add `miette`.

## 8. Steps for the worker (each one green)

| Step | What | Visible change |
| --- | --- | --- |
| **066a** | The built-ins as TOML: every registry built-in and style built-in value moves into `defaults/<area>.toml` in ecad-electrical; the loader, validator and `DefaultsError`; `Entry.builtin` goes; `resolve` reads the loaded data. §2.4's hard-coded heights read their styles. `DEFAULT_LABEL_HEIGHT`, `DEFAULT_LABEL_RULE`, `DEFAULT_LABEL_MIN_LENGTH` removed (readers ask the registry). Values unchanged (wire label still 1.6). | None. |
| **066b** | F2 and the saved-file rule (§5); the "equal to the built-in" readers go. Then change `text.wire_label.height` to **2.5** in `text.toml`: the first value changed as data, and the proof that it works. | Wire labels 2.5 in every project that did not choose a height. |
| **066c** | F3: §2.2's settings defaults become registry entries with values in the TOML, fields optional; report rows per sheet computed. | None. |

Tests (table-driven, `tests/all/lanes/lane_066.rs` in the crates touched):

- **Data complete and valid:** every registry entry has a value; every key in every file is a
  registry entry; every value is valid for its kind (replaces `every_builtin_is_valid_for_its_kind`);
  every built-in style field value is valid for its field.
- **No visible change after 066a:** the screenshot and projection tests already on the branch pass
  unchanged.
- **Round trip** (v1 and v2): a project with a chosen `label_height = 1.6` and one with none save and
  reopen with the same settings and the same resolved value for every entry.
- **Chosen equals built-in survives:** with the built-in at 2.5, a project that chose 1.6 reads 1.6,
  and a project that chose 2.5 still has the field present (it does not become "not set").
- **Not chosen follows:** a project with no chosen height reads 2.5 after 066b.
- **Two machines, same read:** the same project opened with two different company standard files
  present on the machine (and none) resolves every entry identically and projects the same page.
- **Hostile files** (through the loader, as the company file will use it): empty, binary junk,
  1 MiB, 10 000-deep `[[[[`, `"text.tag.show" = 7`, `"text.wire_label.height" =
  nan`/`inf`/`1e308`/`-1`/`"2.5"`, unknown keys ×10 000, control characters, a 70 000-character
  string: each refused with the right `DefaultsError`, no panic, resolution falls through.
- **Bench:** the loaded data is parsed once (on first use) into a table indexed by entry; resolving
  is a lookup by index, no hashing by key on the hot path. The medium project's text-layout bench
  shows no regression.

No new command, so no new MCP contract; `setdefault` and `defaults` keep their tests and behaviour
(`defaults` lists the same values, now from the data).

## 9. Owner decisions (approved, owner and overseer, 2026-11-15)

### Going ahead

1. Defaults are data in TOML files, one per area, built into ECAD so the build never reads the disk
   for them and a damaged install still works.
2. Only the values move to data; each default's allowed range and its label stay in code, so no
   file can widen what is allowed.
3. One built-in set, edited freely. Every project reads the current built-ins; existing projects
   show 2.5 mm wire labels on their next open.
4. A value a project chose is kept even when it equals a built-in; "not set" is stored as not set
   (F2, F3; overseer approved).
5. A project never picks up a machine's company file on its own; a company standard reaches it only
   as an explicit, undoable edit. A project reads the same on two machines.
6. A broken or hostile file never crashes ECAD: bad values are refused, listed, and the next layer's
   value is used. Error types are ready for miette (lane 061) later.
7. The registry and its TOML go into ecad-electrical (lane 067).
8. Four hard-coded text heights that ignore their text style are fixed to read the style, keeping
   today's look.

### Answered

12. **Existing projects and 2.5:** no, they don't keep 1.6. Pre-alpha: one set, every project reads
    the current built-ins.
13. **When a set freezes:** deferred with numbered sets to the v1 format freeze (§10).
14. **Company standard file:** waits for libraries (stage 3).
15. **"Keep how it looks":** deferred with numbered sets (§10).
16. **Later moves to data** (§2.3): deferred as listed.

## 10. Deferred to the v1 format freeze (owner, 2026-11-15)

Recorded in `docs/deferred-work.md`. At the freeze, a built-in change must no longer move saved
projects. The design worked out for that, kept here for then:

- **Numbered, frozen built-in sets.** Changing a built-in means adding a set; each project names its
  set in `ProjectSettings.defaults` (was F1), recorded in the "New project" transaction (which
  already records the length unit), so replay is stable; absent = the first set.
- **`sets.lock`** and a test that fails when a released value changes. Open question then: does a
  set freeze when it reaches `dev` or at a release (was call 13)?
- **`usedefaults`** to move a project to newer built-ins, with **Keep how it looks** (was call 15),
  and a Standards line "Built-in defaults: set *n* (newest: *m*)".
- **A build that doesn't know a project's set refuses to open it**, as older builds refuse other
  newer additions.
