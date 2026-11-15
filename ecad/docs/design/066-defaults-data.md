# Design 066: Defaults as data, and what a saved project means

Status: Draft for the owner (designer, 2026-11-15). Brief: `docs/briefs/066-defaults-data.md`.

**Goal (one sentence).** Every built-in default lives in a data file shipped with ECAD, not in Rust,
and a saved project reads exactly the same on any machine and any later build, whatever its
built-ins or company file say.

**Owner ruling (2026-11-15).** Built-in defaults must not be hard-coded; they move into a
configuration data file. The first value to change is the wire label cap height, 1.6 → 2.5 mm.

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
- **The hole (found by lane 060).** The built-in level is not in the project. A project stores only
  what differs from the built-in, so when the built-in changes, every project that relied on it
  changes how it reads. Two forms of it:
  1. Registry entries with no stored value at any level fall through to the built-in.
  2. Project settings that ecad-file **leaves out when equal to the built-in**
     (`crates/ecad-file/src/v1.rs:418-459`: `label_height`, `label_rule`, `label_min_length`, and
     likewise `tag_padding`, `wiring`, `sheet`, page number step, reference style). The registry
     reads them through "equal to the built-in means not set" (`defaults/text.rs:31-34`,
     `defaults/wires.rs:28-34, 54-57`), so a project that **chose** 1.6 on purpose would also move
     to 2.5. That is a second bug, not only a file-format gap.
- **New projects** record their settings in their first transaction ("New project", with
  `SetProjectSettings`; `crates/ecad-cadcraft/src/project.rs:1152-1170`). Replay starts from
  `Document::default()`. This is what makes the fix below cheap.

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
| Wire label cap height 1.6 | `ecad_model::wiring::DEFAULT_LABEL_HEIGHT`, read in 9 places | Already the entry `text.wire_label.height`; the constant goes. |
| Label rule `net`, shortest labelled wire 0 | `wiring::DEFAULT_LABEL_RULE`, `DEFAULT_LABEL_MIN_LENGTH` | Already entries `wiring.label_rule`, `wiring.label_min_length`. |

### 2.2 To data in a later step of this brief (066c), same mechanism

These are project settings with a built-in, left out of the file at that built-in, with the same
hole. Each becomes a registry entry with its value in the set (§3), and its settings field "not set"
instead of "equal to the built-in".

| What | Today |
| --- | --- |
| Tag number padding 2 | `ecad_model::doc::DEFAULT_TAG_PADDING` (and `devices::DEFAULT_PADDING`) |
| Wire number digits 2 | `wiring::DEFAULT_DIGITS` |
| Page number step 1 | `doc::DEFAULT_PAGE_NUMBER_STEP` |
| Tag dash `yes`, tag case `upper` | `tag::DEFAULT_DASH`, `DEFAULT_CASE` |
| Sheet A3 landscape, 8 × 6 grid | `default_sheet()` in ecad-file, `SheetSpec::default()` |
| Reference style, colour style `iec` | `default_style()`, `iec()` in ecad-file |
| Report table row height 5, header 7 mm, rows per sheet 44 | `generated.rs:49-50` `HEADER_MM`, `ROW_MM`; `outputs.rs:47` `ROWS_PER_SHEET`. Rows per sheet becomes **computed** from the sheet and the row height, not a third value. |

### 2.3 Defaults, but deferred (listed for `docs/deferred-work.md`)

| What | Where | Why later |
| --- | --- | --- |
| House title block column widths and rows | `title_block/house.rs` (`DRAWING_MM`, `ROW_MM`…) | A title block is a structured layout (brief 055's designer), not a set of values; it becomes a data layout with libraries. |
| Cover page sizes | `front.rs` (`HEAD_MM`, `BODY_MM`, cell sizes) | Same: a page layout. |
| Terminal strip drawing look (box 8 × 24, lane pitches, stagger) | `strip_page.rs:52-74` | Look values and tuning are mixed in one block; separating them is its own small brief. |
| Drawing line weights (symbol 0.35, control wire 0.4, power 0.6, frame 0.5) | `ecad-tokens/src/line.rs` | Owner-ruled design tokens with a doc-sync test; a company line-weight table belongs with plot styles. |

### 2.4 Hard-coded text heights that bypass their style (bugs to fix here)

These draw text at a fixed height although a text style for that kind exists. A company that
changes the style sees no effect. Each reads its style instead (066a, no visible change at today's
values):

| Constant | Style it should read |
| --- | --- |
| `references.rs` `TEXT_HEIGHT_MM` 2.5 | `reference` (built-in 2.0: check which one pages draw today and keep that; question to the worker's report, not to the owner) |
| `xref.rs` `XREF_TEXT_MM` 2.5 | `reference` |
| `devices.rs` `TAG_HEIGHT` 3.5 | `tag` (2.5) — same check |
| `look.rs` `LABEL_HEIGHT` 2.5 | the style of what it labels |

If a value differs from its style today, the worker keeps what is drawn (adds a built-in style or
leaves the constant with a comment) and reports it; nothing on a page moves in 066a.

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

### 3.2 Built-in sets, numbered and frozen

The built-ins are kept as **numbered sets**. A set, once in a build that has saved a project, never
changes a value; changing a built-in means adding the next set. Every project names the set it was
made with (§5). This is the heart of the design: it lets a built-in change without changing any
existing project.

```
ecad/crates/<core crate>/defaults/        (§6)
  set-1/                                  the first set: every value, one file per area
    text.toml  wires.toml  print.toml  symbols.toml  terminals.toml  connectors.toml
    strip_pages.toml  bom.toml  styles.toml  continuations.toml  plc.toml  pages.toml  rating.toml
  set-2.toml                              only what set 2 changes from set 1
  sets.lock                               every (set, key, value) ever released; see §3.5
```

- **One file per area in set 1**, matching today's registry files, so lanes adding entries never
  edit the same file (the per-lane rule). The area files and `sets.lock` merge with `merge=union`.
- **A later set is one small file**: what it changes, and from which set.
- **A new key** (a lane adds an entry) goes into **set 1's** area file with its first value. No
  project can have read a key that did not exist, so adding it changes nothing; a project on any
  set reads the key's first value until a later set changes it.
- **All files are embedded at build time** (`include_str!`, listed in one Rust table), so wasm and a
  missing or damaged install both work, and the build never reads the disk for its built-ins.

### 3.3 Schema

Set 1, one area file (`set-1/text.toml`):

```toml
# Lane T (brief 031). Values only: kind, range, label and levels stay in the registry (§3.4).
[values]
"text.font"                 = "Barlow Semi Condensed"
"text.tag.show"             = "on"
"text.wire_label.height"    = 1.6      # millimetres
"text.run.gap"              = 15.0
"text.tag.gap"              = 0.6      # × tag height

[styles.text.tag]
height = 2.5
weight = "bold"

[styles.text.point_label]
height = 1.5
weight = "regular"
```

A later set (`set-2.toml`):

```toml
[set]
number   = 2
based_on = 1
why      = "Wire label cap height 1.6 → 2.5 mm (owner, 2026-11-15)."

[values]
"text.wire_label.height" = 2.5
```

Rules:

- Keys under `[values]` are the registry key, quoted, exactly as stored in a project and as
  `setdefault` takes them. Lengths are numbers in millimetres; choices and texts are strings; a
  number is a number. No units in values.
- `[styles.<catalogue>.<style id>]` sets a built-in style's field values. A style's name stays in
  code.
- The wire label style's height is the entry `text.wire_label.height` (one value, one place, as
  today); `styles.text.wire_label` does not repeat it.
- A set file's `[set]` table is required for sets 2 and later; set 1 has none.

### 3.4 What stays in code: the entry's description

**Recommendation: only the values move. Kind, range, levels, project source, label, group and brief
stay in the Rust `Entry`.** The range is the guard that keeps a hostile or mistaken value out
("hostile-input bounds stay constants"); if the data file could widen it, a company file could too.
Labels are UI words that will go through localisation, not through a defaults file. The `builtin:`
field of `Entry` goes away; code asks the set for the value.

### 3.5 The lock: a set never changes once released

`sets.lock` is a plain list, one line per value, in the order sets were made:

```
1 text.tag.show = "on"
1 text.wire_label.height = 1.6
2 text.wire_label.height = 2.5
```

A test rebuilds the list from the files and checks that **every line in the lock is still there,
unchanged**. New lines (a new key, a new set) are allowed and the test prints them for the worker
to append. A changed or removed line fails with: "set 1 changed `text.wire_label.height` (1.6 →
2.5): a released set never changes. Add the change to a new set." This turns "don't change a
built-in" from a habit into a check.

A set is **released** when a build that writes it is merged into `dev` (that is when Carl's own
projects start to name it). Until then, a lane's new set may still change on its branch.

## 4. Layers

Resolution keeps today's order, with one change at the bottom:

```
object → page → project → standard (the project's copy) → the project's built-in set
```

| Layer | Lives | Read when |
| --- | --- | --- |
| Built-in set *n* | In the ECAD build (embedded TOML) | Always; *n* is named by the project |
| Company (house) standard | A TOML file the company keeps; **copied into the project** as its standard | Only when a person loads it (§4.1). Never on open. |
| Project, page, object | In the project, as today | Always |

**A company file is never read on open.** It reaches a project only through an explicit, logged
edit (P8, "library changes reach a project only by an explicit edit"), exactly as brief 050a
designed the standard. That is what makes a project read the same on two machines with different
company files.

### 4.1 Where a company file comes from

- **File shape.** The same schema as a set, plus a header (§3.3):

  ```toml
  [standard]
  name     = "FCS house standard"
  based_on = "iec"

  [values]
  "text.tag.show" = "first"

  [styles.text.tag]
  height = 3.0
  ```

- **Desktop.** New project's standard picker lists the `.toml` files in the company standards
  folder: a folder chosen in Settings ▸ Standards ("Company standards folder"; default
  `%APPDATA%\ECAD\standards` on Windows, `$XDG_CONFIG_HOME/ecad/standards` on Linux). Project ▸
  Standard ▸ **Load from file…** reads any one file.
- **Browser (wasm).** No folder: **Load from file…** opens the browser's file picker. Nothing is
  read automatically.
- **Loading** shows the differences first (key, the project's value now, the file's value, how
  many objects use it: the Standards screen's existing "consequences said first") and applies them
  as one transaction (`SetStandard` and the style ops 050a already has). Undo reverts it.

This part is 066d and needs the owner's yes (§9, call 14): it is close to libraries (stage 3).

## 5. What a saved project means

### 5.1 The rule

**A project's reading depends only on what is in the project and on frozen sets.** Two things make
that true:

1. **The project names its built-in set.** `ProjectSettings` gains `defaults: u32`, the set number.
   - A new project records the newest set in its "New project" transaction (the same
     `SetProjectSettings` that already records the length unit). Replay of that history always
     gives the same set, on any build.
   - `ProjectSettings::default()` (the replay base, and the meaning of "absent" in a file) is
     **set 1**. A project saved before 066 has no `defaults` field and reads with set 1, which holds
     exactly today's built-ins. **Existing projects keep reading as they do today** (wire labels
     1.6) with no conversion work.
   - The field is always written; it is never left out.
2. **"Not set" is stored as not set, never as "equal to the built-in".** A settings field with a
   built-in (`wiring.label_height`, `label_rule`, `label_min_length` in 066b; the §2.2 list in 066c)
   becomes optional: absent means "use the set"; present means the project chose it, even when the
   value happens to equal a built-in. The registry's "equal to the default means not set" readers go.
   A project that chose 1.6 keeps 1.6 forever.

Why not "write every value into every file" instead: it makes every file carry about 150 values
nobody chose, makes "which values did this project choose" unanswerable (P7: unset must be
distinguishable), and still needs a rule for keys added after the file was saved. Naming the set
costs one number per project.

### 5.2 Moving a project to newer built-ins

Command **`usedefaults`** (`usedefaults` alone shows the project's set, the newest, and what would
change; `usedefaults <n>` moves to set *n*):

- Lists, per changed key, the value now, the value after, and how many objects use it.
- **Keep how it looks** (Toggle, on by default in the panel; `keep=on|off` in the command): for each
  changed key whose entry allows the project level, writes the current value as the project's own
  value in the same transaction, so nothing on a page moves; only future additions follow the new
  set. Keys without a project level are listed as "will change".
- One transaction: `SetProjectSettings { defaults }` plus the `SetProperty`/settings ops for kept
  values. Author recorded; undo reverts it; refused with agent writes off.
- In the UI: Settings ▸ Standards shows a line at the top, "Built-in defaults: set 1 (newest: 2)",
  read with `control::read_text`, and a `prim::Button` **Use newest defaults…** that opens the
  difference list (a `table` of key / now / after / used by) with the **Keep how it looks**
  `control::Toggle` and **Apply** / **Cancel** buttons. Element names: `standards.defaults_set`,
  `standards.use_newest`, `use_defaults.keep`, `use_defaults.apply`. Shown only when the project
  is not on the newest set.

Update map: the line and the button depend only on `ProjectSettings.defaults`; they change on a
`SetProjectSettings` op and on nothing else. The difference list is built when the dialog opens
(first show) and closed on any commit.

### 5.3 A project from a newer build

A build that does not know the project's set **refuses to open it**: "This project uses ECAD's
built-in defaults set 3; this ECAD knows sets 1–2. Open it with a newer ECAD." This follows the
existing rule that older builds refuse files using newer additions (decisions, 2026-11-09, briefs
019 and 022). It cannot read it "nearly right": that is the silent change this design removes.

### 5.4 Format changes (for the overseer; not decided here)

| # | Change | Old files | Older builds |
| --- | --- | --- | --- |
| F1 | `ProjectSettings.defaults: u32`, always written; absent reads as 1 | Read with set 1 = today's built-ins | Refuse a file that has it (unknown field), as with 019/022 additions |
| F2 | `wiring.label_height`, `label_rule`, `label_min_length` become optional: absent = from the set. Written only when the project set them | Absent today meant "the built-in" = set 1's value: same reading | Same as F1 |
| F3 (066c) | The §2.2 settings fields get the same treatment, and become registry entries | Same as F2 | Same as F1 |

No op changes: `SetProjectSettings` already carries the whole settings struct. Both v1 and v2
encodings carry `ProjectSettings` through the same serde type, so one change covers both.

## 6. Where it lives (lane 065 crate map)

The registry is domain, not engine: **it moves to Core.** Concretely:

- The `Entry` / `Kind` / `Value` / `Level` types, `parse_value`, the embedded sets, the set loader
  and validator, and `resolve` move to the Core crate that lane 065's map gives domain rules
  (`ecad-electrical` if it sits below `ecad-file`; otherwise `ecad-model`, module `defaults`).
  ecad-model and ecad-file need the set's values (`ProjectSettings::default()`, the "absent" reading),
  so it must sit at or below them. It never depends on a `cadcraft-*` crate.
- The style-existence check (`Kind::Style` naming a project style) needs the style catalogues; it
  stays a wrapper in ecad-cadcraft (`parse_in`) until the catalogues move too.
- `toml` becomes a dependency of that Core crate (wasm-safe; run `cargo xtask wasm`).
- If 065 has not landed when 066 starts, 066a moves the values to data **in place** in ecad-cadcraft
  and the crate move happens with 065's moves; the step order (§8) does not depend on it.

## 7. Validation: hostile or broken files never crash

One loader reads a set file, a company file, or (later) a library's standard. It returns the values
it accepts and a list of problems; it never panics and never returns a value outside an entry's
range.

| Problem | What happens |
| --- | --- |
| File larger than 256 KiB, or nesting deeper than 16 | Refused before parsing (cheap scan). |
| Not TOML (syntax error) | Whole file refused; one problem with line and column. |
| Unknown key | Ignored; one problem per key, first 50 listed then "and *n* more". |
| Wrong kind (`"2.5"` for a length, `2` for a choice) | That value refused; the layer below is used. |
| Out of range, not finite, unknown word, control characters, too long | That value refused; the layer below is used. |
| A set file that names an unknown `based_on`, or a gap in numbering | Test failure (built-ins); refused (company). |

- **Built-in sets** are checked by tests (§8), so a shipped build always has valid sets. At run time
  a built-in that still fails to load (not reachable in a tested build) gives a Checks row
  `defaults-damaged` on the project and each affected entry uses its kind's safe value (the range's
  low end, the first choice); never a crash.
- **Company files** show their problems in the load dialog before anything is applied, and refused
  values are not applied. Loading a file with problems still loads its good values, after the
  person sees the list.
- **Stored project values** keep today's rule: one that does not parse counts as absent.
- **Errors.** The loader's error type is a `thiserror` enum in the Core crate (`DefaultsError`:
  `TooLarge`, `TooDeep`, `Syntax { line, column, message }`, `UnknownKey`, `WrongKind`,
  `OutOfRange`, `UnknownSet`), each carrying the file name and the byte span the `toml` parser
  reports. That is what `miette` needs to underline the place in the file, so lane 061's later
  `miette::Diagnostic` derive adds no reshaping. This brief does not add `miette`.

## 8. Steps for the worker (each one green)

| Step | What | Visible change |
| --- | --- | --- |
| **066a** | Set 1 as TOML: every registry built-in and style built-in value moves into `set-1/*.toml`; the loader, validator and `DefaultsError`; `Entry.builtin` goes; `resolve` reads set 1; `sets.lock` written. §2.4's hard-coded heights read their styles. `DEFAULT_LABEL_HEIGHT` and friends removed (readers ask the registry). | None: same values. |
| **066b** | Named sets and the saved-file rule: F1 and F2; "equal to built-in" readers go; `set-2.toml` changes `text.wire_label.height` to 2.5; new projects record set 2; `usedefaults` command, its MCP contract, and the Standards line and dialog (§5.2). | New projects' wire labels 2.5. Existing projects unchanged. |
| **066c** | F3: §2.2's settings defaults become registry entries with values in set 1, fields optional; report rows per sheet computed. | None. |
| **066d** | Company standard file: load from file / folder, difference list, one transaction (§4.1). Only with the owner's yes (call 14). | New. |

Tests (table-driven, `tests/all/lanes/lane_066.rs` in the crates touched):

- **Sets complete and valid:** every registry entry has a value in set 1; every key in every set is
  a registry entry; every value is valid for its kind (replaces `every_builtin_is_valid_for_its_kind`).
- **Lock:** §3.5, including a deliberately changed set-1 value in a test fixture failing with the
  message.
- **Round trip:** a project with `defaults = 2`, a chosen `label_height = 1.6`, and one without a
  chosen height saves and reopens (v1 and v2) with the same settings and the same resolved value for
  every entry.
- **Old file reads the same:** a pre-066 fixture (no `defaults`, no `label_height`) resolves every
  entry to set 1's value; wire labels draw at 1.6.
- **Chosen equals built-in survives:** a project that set 1.6 on set 1 moves to set 2 and still
  reads 1.6.
- **Replay:** a "New project" history recorded with set 2 replays to set 2; a history without the
  field replays to set 1.
- **Two machines, same read:** open the same project with two different company standards folders
  (and none): every entry resolves identically, and the projected page is identical.
- **Hostile files:** empty, binary junk, 1 MiB, 10 000-deep `[[[[`, `"text.tag.show" = 7`,
  `"text.wire_label.height" = nan`/`inf`/`1e308`/`-1`/`"2.5"`, unknown keys ×10 000, control
  characters, a 70 000-character string: each refused with the right `DefaultsError`, no panic,
  resolution falls through.
- **Newer set refused:** a file naming set 99 is refused with the message of §5.3.
- **`usedefaults`:** with `keep=on` nothing on the page moves; with `keep=off` the wire labels
  become 2.5; one transaction, undo reverts; MCP `execute` records `Author::Agent(<client>)` and is
  refused with writes off.
- **Bench:** resolving is a lookup by entry index in the project's set (no hashing by key on the hot
  path); the medium project's text-layout bench shows no regression.

## 9. Owner decisions

### Going ahead (follows the rulings)

1. Defaults are data in TOML files shipped inside ECAD and built into it, so the browser version
   and a damaged install still work.
2. Only the values move to data; each default's allowed range and its label stay in code, so no
   file can widen what is allowed.
3. Built-ins are kept as numbered sets that never change once used; changing a default means
   adding a new set, and a check fails if anyone edits an old one.
4. Every project names its set. New projects get the newest one; projects saved before this keep
   today's values (wire labels stay 1.6) with no conversion.
5. A value a project chose is kept even when it equals a built-in; "not set" is stored as not set.
6. A company file is never read when a project opens; it reaches a project only when someone loads
   it, as one undoable edit. That is what makes a project read the same on every machine.
7. A broken or hostile file never crashes ECAD: bad values are refused, listed, and the next layer's
   value is used. Error types are ready for the nicer error display (lane 061) later.
8. An ECAD that does not know a project's set refuses to open it, as with other newer additions.
9. The registry moves to the core crates in lane 065's map; until then it changes in place.
10. Four hard-coded text heights that ignore their text style are fixed to read the style, keeping
    today's look.
11. For the overseer: format changes F1–F3 (§5.4).

### Needs your call

12. **Existing projects and the 2.5 wire label.** Projects saved before this keep 1.6 (the point
    of the design). They can move with **Use newest defaults**, keeping their look or not.
    *Recommendation:* keep 1.6 for existing projects; move them by hand when you want.
13. **When a set freezes.** A set freezes once it reaches `dev`, so your trial projects never shift
    under you; changing a default afterwards always means a new set, even before v1.
    *Recommendation:* freeze at `dev`. (Freezing only at a release would mean projects made on dev
    builds can still shift.)
14. **The company standard file (066d) now or with libraries.** Loading a house standard from a
    TOML file is small and uses the same checker, but it overlaps with libraries (stage 3, not yet
    approved), where a standard will also come from a `.ecadlib`.
    *Recommendation:* build 066a–066c now; hold 066d until you approve stage 3, or approve it now as
    a TOML-only loader that libraries later reuse.
15. **"Keep how it looks" on by default** when moving a project to newer defaults.
    *Recommendation:* on: moving should never surprise; switch it off to take the new look.
16. **Later moves to data** (§2.3: title block columns, cover sizes, terminal strip drawing look,
    line weights): recorded as deferred work, each its own brief. *Recommendation:* defer as listed.
