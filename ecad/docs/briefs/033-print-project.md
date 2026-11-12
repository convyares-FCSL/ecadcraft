# Brief 033: Print and project

Status: Draft (designer, 2026-11-12; waiting on the owner's answers to design 033 §12 and the
overseer's decision on FP1–FP3)
Branch: `feature/print-project`, from `origin/dev` `6729c8f1`
Designer: **yes** (`docs/design/033.md`) · Size: M–L (three S–M parts)
Roadmap: `docs/discussions/roadmap-review-2026-11.md` §3.2, v0.15 lane **P**: Q5 print quality,
Q7 pack quick fixes, Q6's `table` part. Owner answer Q23 (pending revision) accepted as
recommended. Findings: §6.2 (drawing-pack test, stage 2).
Lanes beside it: T 031 text styles, W 032 wires and nets, S 034 component library. This lane's
files and its call-site touches in other lanes' files are listed in design §0.

## Goal

The printed book is small, every page is readable on paper, and the project prints the way a real
pack is numbered, tagged and revised.

## What exists today (`origin/dev` `6729c8f1`)

- Print Book (024) projects every page onto one scratch drawing and calls CADCraft's plotter once
  with a window per sheet. The plotter writes the **whole** model into every page and clips it,
  so a 30-sheet book is 32 MB and grows with sheets² (design §0.1, measured).
- Missing values in generated tables are a `?` in ACI 2: yellow when printed with screen
  colours.
- Contact mirror tables (025) go below or beside their coil with no check against the frame, so
  a coil low on the page puts its table on the title block.
- Tags are stored in upper case with a leading `-`. Identity (`tag::key`) ignores both.
- Revisions have six fixed fields. The Rev box shows the newest issued rev even when the drawing
  has changed since.
- A generated page's continuation sheets all carry the page's number.
- `table` is CADCraft's command. On an ECAD page it is refused with "object of this kinds are not
  supported on ECAD pages yet".

## Scope

1. **Print quality (Q5).**
   a. **PDF size:** each PDF page holds only what lies in its window. This is a small generic
      culling patch in CADCraft's plotter (`crates/io/src/pdf.rs`, possibly `crates/render`) and
      needs owner approval (design Q1). Targets: bytes grow linearly with sheets, and the 30-sheet
      pack prints under 5 MB. Pages look identical.
   b. **Missing data:** `?` in a thin box; yellow on screen, black when printed in either colour
      mode (design §3).
   c. **Contact tables:** an automatic table never overlaps the title block. It moves right of the
      coil, else lifts. A table the user placed stays where it is, with a Checks warning (design
      §4).
2. **Pack quick fixes (Q7).**
   a. **Tag style:** project settings for the leading `-` and for upper or typed case, applied when
      a tag is written. Changing the dash rewrites existing tags in one undoable transaction
      (design §8).
   b. **Revision columns per project:** a column list with custom fields `rev.<n>.<field>`, headers
      and widths, used by the title block, the Revisions window and the revision list page (design
      §7).
   c. **Pending-revision marker:** derived from the undo stack after the last issue. Style `D*`
      (default), `E (draft)` or off. Shown in the Rev box and on the cover, and noted by Print Book
      (design §6).
   d. **Continuation sheets numbered in sequence:** `400, 401, 402`, or `BOM, BOM2` for a number
      without digits, for every generated kind. These numbers count as taken, and a clash from a
      growing table is a Checks warning (design §5).
3. **`table` (Q6).** Refused before anything is written, in clean words that offer a way out. The
   broken plural is fixed for every refused kind (design §11).
4. **Text sizes** for the frame, title block, generated tables and contact tables are read
   through lane T's text-style getter (brief 031). Until it lands, a local stub returns today's
   sizes (design §1.2).
5. **Update maps** for every changed view (design §9); **MCP contract tests** for every new or
   changed command; the `print25` bench before and after.

## Acceptance criteria

Design §15, AC1–AC15, including:

- **MCP contract tests** (through `ecad-mcp`'s `execute`, as `mcp_contracts.rs` does). Each one
  checks the transaction recorded, its author `Author::Agent(<client>)`, and the refusal with writes
  off:
  - `projectprops` with `tag_dash`/`tag_case` (AC11), `revision_columns` (AC10) and
    `revision_pending` (AC9);
  - `revision` with custom `fields` (AC10);
  - `newpage`/`pageprops` refusals of a continuation number (AC8);
  - the `contacts` reply's `table` (AC5, AC6);
  - `table`'s refusal (AC13).
- A pixel-identical check of printed pages before and after culling (AC2). An upstream test that
  a page's content stream holds nothing from outside its window (AC1).
- Hostile settings refused or read as absent, never a panic (AC15). Every crash fix lands with
  its test.
- Old (030) projects open and print as before, apart from the pending rule (AC14).
- `docs/quick-start.md` updated in the same change.

## Testing (owner decisions, 2026-11-12)

Fast scoped tests only: `cargo xtask test-changed` before each commit, and clippy before each
push. No `--include-ignored`. The tester covers this brief: the culling patch is outside `ecad/`
and the tag rewrite is a bulk op. The tester runs only its acceptance tests and the fast tests
of the crates the change touched. The print bench uses the 25-page project, release build. The
upstream patch also needs the root CADCraft build and clippy, and the tests of `cadcraft-io` (and
`cadcraft-render` if touched).

## Open before approval

- Owner answers to design §12 Q1–Q9 (Q1, the upstream patch, is a gate).
- Overseer decision on FP1–FP3 (design §10).
- Lane T's names for the text kinds in design §1.2.
- Order with lane T for the three tag call sites in `devices.rs`, and with lane S for the two
  hunks in `convert.rs` (design §0).

## Later (not in this brief)

- The same missing-data mark for `?` on schematic pages (lane T, design FU1).
- Placed tables and a fuse list: report generator (roadmap 2.9).
- Contents columns for revision and date; contact-table free text per contact; user-defined title
  blocks.
