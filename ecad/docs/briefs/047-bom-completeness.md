# Brief 047: BOM completeness

Status: Draft (designer, 2026-11-13; owner questions in design 047 §15, format proposals in §14)
Branch: `feature/bom-completeness`, from `origin/dev` `be6f4413`.
Designer: **yes** (`docs/design/047.md`) · Size: M
Roadmap: `docs/discussions/roadmap-review-2026-11.md` M1 item **1.12**, lane U, after 1.8 (graphical
strip pages, brief 046). Builds on 024 (the BOM, its grouping and the BOM-by-location page), 030
(end plates and jumper bars on the BOM), 034 (fuse banks, `rating`, default prefixes), 036 (fuse
positions on strips) and 031 (the defaults registry). Later items that take over from it: **2.3
parts catalogue** (a line points at a catalogue part) and **2.9 report generator** (the BOM becomes
a shipped report definition).

## Goal

The BOM lists everything a panel builder orders, as the drawing-pack test showed a real pack does:
each row can carry the company's internal article number, material with no device (trunking and
DIN rail in metres, crimps) has rows of its own, and a device's add-on parts (an aux contact block,
a CPU's memory card, a bank's fuse inserts) appear as rows under its tag.

## What exists today (`origin/dev` `be6f4413`)

- `outputs.rs`: the BOM model groups **devices** (components with a tag and no `kind`) by
  maker, part number and description, by location or by part (024 §6.1). Strips give one row per
  terminal type (028, 036) and one per placed accessory part, jumper bars by link (030). Qty is a
  whole count. Columns: Location, Qty, Tags, Description, Maker, Part number.
- The BOM-by-location page (`generated.rs`) draws Qty, Tags, Description, Maker, Part number.
- A device has one part: its `maker`, `part_number` and `description` properties.
- No article number anywhere, no row without a tag, no second part per device. Fuse banks and fuse
  positions carry `rating.<n>` (034, 036); their inserts are not listed ("1.12's", 034 §9 and
  036 §7.2).
- The registry (`defaults/`) has no BOM entries.

## Scope

1. **Article number.** An `article_number` beside every part number the BOM reads: a device's
   main part, each part line, each item, and the strip parts of 028, 030 and 036. A BOM column that
   is **off until the user adds it** (UI owner answer 12), as a registry entry.
2. **Items** (untagged rows): trunking and DIN rail in metres, crimps, ferrules, cable ties, any
   material with no device. Each item has its own location, quantity and unit (`pcs` or `m`).
3. **Part lines** (assembly rows): a device's add-on parts, `part.<n>.*` properties, each with
   maker, order code, description, article number, quantity and, for fuse inserts and later
   crimps, the fuses or positions it serves. Kits insert several lines carrying the kit's name.
4. **The part set** of the UI spec is the main part plus the part lines, in order, so the parts
   catalogue (2.3) later fills the same fields and the report generator (2.9) reads the same rows.
5. **Report layout:** lines merged with equal parts (flat, the default) or listed under their
   device (nested), a registry entry. A Unit column in CSV; metres shown in Qty.
6. **Checks:** a part line or item with no order code, a fuse with no insert, one order code with
   two article numbers. Each has a fix or go-to; nothing is refused.
7. **Commands and MCP:** `partline`, `bomitem`, `article`; `bom` returns the new fields.
8. **No new panel.** The Reports window's BOM tab gains a column toggle and **+ Item**; the
   inspector lists a device's part set read-only. Editing a part set in a panel waits for 2.3.

Out of scope: the parts catalogue and choosing from it (2.3); saving a set as a kit (2.3); counts
derived from the drawing (crimps per wire end, ferrules per cross-section, rail and duct length from
a panel layout: 1.4, 3.1); a part-set editor panel; cables on the BOM (024 Q6); XLSX (3.4).

## Format and ops

**No new op and no format change** (design §14, FP1–FP6, for the overseer): items are tagless
components with `kind: "bom_item"` and no representation (024 FP2's pattern); part lines, article
numbers and the BOM defaults are properties. Pre-v1: no compatibility work.

## Acceptance criteria

The full table is design §12. In short:

1. `article_number` on a device, a part line, an item and a strip part shows in the BOM's Article
   number column once `bom.column.article` is `on`, in the window, the BOM page and the CSV; off,
   nothing shows and the CSV has no such column.
2. `bomitem` adds an item (one transaction, one `u`); `3.6 m` of trunking in `+CP1` and `2 m` of
   the same part in `+CP1` give one row of `5.6 m`; an item with no location takes the project's.
3. `partline` adds, edits and removes lines; an aux-block line on `-K03`…`-K06` gives one row
   `-K03, -K04, -K05, -K06`, Qty 4; a kit adds its lines in one transaction, each carrying the kit.
4. A fuse bank line with `for: "1,2,4"` counts 3 and lists `-F01:1, -F01:2, -F01:4`; a fuse with
   no insert is counted by the Checks row `fuses have no insert`.
5. `bom.lines = nested` lists each device's lines under it, unmerged; flat merges them by part.
6. Every Checks row of design §9 has a fix or go-to.
7. MCP contract tests (brief 011 rule) for `partline`, `bomitem` and `article`: the transaction,
   `Author::Agent(<client>)`, refused with writes off; `bom` works with writes off.
8. Update maps (design §10): a part-line edit changes only its device's rows; the bench rows hold.
9. Save and reopen keep everything.

## Tester

**Not needed** by the CLAUDE.md rule: no `.ecad` format or op change, no movement, no store or
locking. The reviewer covers the PR into `dev` (design §13).

## Needs owner check

- The questions in design §15 and the overseer's rulings on §14.
- A click-only trial on the demo (quick start `docs/quick-start/047-bom-completeness.md`).
