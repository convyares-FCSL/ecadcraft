# Defaults: built-in values as data, and what a saved project means

A default is any value a project uses until someone chooses another: a text height, a show rule, a
gap, a wire label rule, a sheet size, a tag's padding. This note says where defaults live, how a
value is resolved, and what a project stores. The user's side (Standards, project defaults) is in
[../user/styles.md](../user/styles.md); the list of entries is what the `defaults` command prints.

## The rule for what is a default

A value a company might reasonably want different, and which changes how a drawing looks or reads,
is a **default** and lives in data. These stay constants in code, beside what they guard:

- bounds that guard against hostile input (every `MAX_*` and `MIN_*`, an entry's allowed range);
- contracts other data depends on: the grids symbols are drawn on (5 mm, `SYMBOL_GRID_MM`,
  `POINT_GRID_MM`: changing one breaks every library symbol) and the sheet frame's geometry;
- an algorithm's tuning and font metrics;
- the app's own screen sizes (those are the UI kit's tokens);
- words and keys.

## One entry per default, values in TOML

- **The registry** (`ecad-model/src/registry/`) is the mechanism: an `Entry` has a stable key
  (`text.tag.height`, never renamed once released), words for the Standards screen, a kind with its
  range (length, number, choice, switch, text, parsed text, style), the levels that may override it,
  and where the project level is stored.
- **Each crate declares the entries it knows**, with their built-in values in its own data set:
  electrical the electrical ones, drawing-style the look ones, ecad-model the project settings it
  computes with (tag padding, dash and case). `ecad-drawing-style` assembles them into one
  `REGISTRY`; the engine passes it to commands and views.
- **The values are TOML files**, one per area, embedded at build time with `include_str!`:
  `ecad-electrical/defaults/*.toml`, `ecad-drawing-style/defaults/{text,styles}.toml`,
  `ecad-model/defaults/{settings,colours}.toml`. The build never reads the disk for its built-ins,
  so a damaged or partial install still has them.
- **Only values live in data.** Kind, range, levels and words stay in the Rust `Entry`, so no data
  file can widen what is allowed. Keys under `[values]` are registry keys exactly as a project
  stores them and as `setdefault` takes them; lengths are millimetres with no unit in the value;
  `[styles.<catalogue>.<id>]` sets a built-in style's field values.

Why TOML: it is what Rust projects use for configuration, it is plain enough for a person to edit,
and its parser is already in the dependency tree.

Why per area: a new entry's value goes into its area's file, so work on different areas never edits
the same file.

## How a value is resolved

Narrowest level first, only the levels the entry allows:

```
object → page → project → standard (the project's own copy) → built-in
```

A stored value that does not parse, is out of range, or names a style that does not exist counts
as absent, and the next level is used. So a hostile file never gives a value outside the entry's
range. Code asks `resolve` (or a typed reader over it) and never writes the number where it uses
it. The data is parsed once, on first use, and each entry keeps its value in a slot, so resolving
is a field read, not a lookup by key.

## What a saved project means

- **"Not set" is stored as not set, never as "equal to the built-in".** A project setting with a
  built-in is optional in the file: absent means the project follows the built-in (or its standard)
  and moves when the built-in changes; present means the project chose it, even when the value
  equals the built-in today, and it never moves on its own.
- **One built-in set, edited freely, until v1.** Every project reads the current build's
  built-ins, so a project that relies on a built-in follows a change on its next open. Two
  different builds may draw such a project differently; that is accepted before v1.
- **A project never picks up a machine's files.** A company standard reaches a project only as a
  copy inside it (`Document.standard`, changed by the `SetStandard` op): an explicit edit that is
  recorded and can be undone. No company file is read on open, so a project reads the same on any
  two machines with the same build.

## Broken or hostile data never crashes

One loader (`ecad-model/src/registry/data.rs`) reads a data file and returns the values it accepts
and a list of problems. It never panics and never returns a value outside an entry's range:

| Problem | What happens |
| --- | --- |
| File larger than 256 KiB, or nested deeper than 16 | Refused before parsing |
| Not TOML | Whole file refused, with line and column |
| Unknown key | Ignored and listed (the first 50, then a count) |
| Wrong kind, out of range, not finite, unknown word, control characters, too long | That value refused; the level below is used |

Tests check every shipped file, so a tested build always loads its built-ins. A built-in that still
fails to load takes its kind's safe value and the Checks list shows `defaults-damaged`. The errors
carry the file and the byte span, ready for a report that underlines the place.

## Drawn text reads its style

Every text that drawing or layout code puts on a page takes its height, weight, font and colour
from its text kind's style. No literal text height, style name or text-size constant remains in the
drawing crates outside the places that define styles and their data. Exempt by location: symbol
definitions (their glyph text is line art in symbol units), screen-only overlays (pixels),
user-imported title block items (their sizes are the user's data) and the style modules
themselves. `cargo xtask layers` enforces this at zero (`dev/xtask/src/drawn_text.rs`).

## Not built yet

Loading a company standard from a file on disk waits for libraries. Numbered, frozen built-in sets
(so a built-in change no longer moves saved projects) wait for the v1 format freeze; see
[../roadmap.md](../roadmap.md).
