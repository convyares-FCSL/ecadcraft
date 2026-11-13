# Brief 049: Continuation cross-references

Status: Draft (designer, 2026-11-13). Waits on the owner questions in design §13 and the overseer's
FP1–FP5 (design §12).
Branch: `feature/continuation-refs`, from `origin/dev` `f26c8f53`.
Designer: **yes** (`docs/design/049.md`) · Size: M (the roadmap's estimate)
Roadmap: `docs/discussions/roadmap-review-2026-11.md` M1 item **1.9**, lane **D**, after 1.3 PLC
(brief 045). Builds on 022 (continuations join nets by name), 025 (the general reference index,
`RefIndex`, Go to Reference, `xrefs`), 031 (text kinds, show and hide, the defaults registry) and
044 (the latest reference rule added to the index). Owner answer Q21 (decided): a caption is a
property of the continuation, and the arrows that share it are grouped under it; no new object.

**Words.** In this brief a *continuation* is 022's arrow that joins a net to every other arrow
with the same name. It is not 033's continuation **sheet** (a generated report's pages 400, 401).
The design calls the arrows **continuation arrows**, and their partners **other ends**.

## Goal

Every continuation arrow shows where its other ends are (`102-C-2`), worked out live from the
reference index and never typed, and arrows that go to the same place can be grouped under one
caption (`TO CONTROL SUPPLY`), as in a real drawing pack.

## What exists today (`origin/dev` `f26c8f53`)

- 022 draws, beside each arrow, its name and its other places (`wires::continuation_texts`,
  `other_places`): up to three, then `+n`, horizontal, under the name. The name is hidden by
  default since 031 (`text.continuation_name.show` = `off`).
- Continuations are **not** on 025's reference index: no Go to Reference, no `xrefs` rows, no
  hover lines about other ends.
- Their update map is the widest left in the app: creating, deleting or renaming a continuation
  makes `project::pages_touched` return "any page". A moved arrow, or a renumbered page, is not
  scoped at all: from reading the code, the other ends' texts on the page on screen are not
  redrawn until the page is shown again (design §0, finding F1; not run).
- `other_places` builds a page-order map for every arrow it derives (O(pages) each).
- No caption: the pack redraw typed captions as free text, which does not move with the arrows.

## Scope

1. **Continuations on the reference index.** A group per continuation name; each arrow is a member
   and shows every other member (implicit edges, so a large group costs O(members), not O(n²)).
2. **What an arrow shows.** Its other ends' places in the project's reference style, in book order,
   past the arrow tip and reading along the wire; up to three, then `+n`; `?` for an end that
   cannot be located. Derived, never stored, never editable as text.
3. **Captions.** A component property `caption` on a continuation. Arrows on one page with the same
   caption and direction, next to each other, are drawn under one bracket with the caption text.
   One arrow with a caption gets its own bracket.
4. **Setting captions.** A `contcaption` command (JSON and interactive: select arrows, type the
   caption), `place` and `setprop` accept `caption`, and the inspector shows a **Caption** row. No
   new panel.
5. **Reading.** Hover lines, Properties' **Other ends** list with Go to, `refgoto` from an arrow or
   its text, and `xrefs` rows of kind `continuation`.
6. **Update maps** for the index, the texts and the caption brackets (design §8): every
   continuation case of `pages_touched` becomes a page set; nothing rescans the project.
7. **Defaults** through the registry: a new area `continuation` and a text kind
   `continuation_caption`.
8. **MCP** contract tests for `contcaption` and for `place` with a caption; **bench** rows.

## Later (not designed now)

- An incoming arrow drawn differently from an outgoing one (direction): a starter variant for lane
  S, with no index change (design §13 Q8).
- A continuation list report (every name, its arrows and captions): the report generator (2.9).
- Continuing a cable across pages through continuations (Q8, v0.15) is unchanged by this brief.

## Format and ops

No new op and no record field. Design §12 lists FP1–FP5 for the overseer: the component property
`caption`; the registry area `continuation` and the text kind `continuation_caption`; a rotation
on `xref::Shape::Text`; a `Group::Continuation` on `RefIndex` with implicit edges; and moving the
other-end texts from `wires.rs` (lane W) to lane D's `xref.rs` and a new `continuations.rs`.

## Acceptance criteria

The full table is design §10. In short:

1. Three arrows named `24V` on pages 101, 102 and 103: each shows the other two places in book
   order (`102-C-2 103-B-5`), past its tip and reading along the wire; a fifth arrow of the name
   shows `+1` after three.
2. Moving the arrow on page 102 by JSON while page 101 is on screen redraws page 101's text at once
   (fails before the fix: F1). Renumbering page 102 does the same.
3. Renaming an arrow, creating one or deleting one redraws only the pages of the old and new name
   groups; `pages_touched` no longer returns "any page" for these ops.
4. `contcaption {continuations: [a, b, c], caption: "TO CONTROL SUPPLY"}` on three adjacent arrows
   draws one bracket and the caption; moving one 40 mm away splits it into two brackets with the
   same caption; `caption: null` removes it. One transaction each, one `u`.
5. `refgoto` from an arrow or its text goes to its first other end, then the next on repeat; hover
   and Properties list every other end; `xrefs {page}` lists kind `continuation` rows.
6. Editing the derived text is refused with `The continuation reference is derived; move or rename
   the continuation instead.`
7. Registry defaults (places shown, gaps, run gap, caption text size) change every arrow that
   inherits them, redrawing only pages with continuations.
8. Hostile input never panics: a 10 000-character caption, control characters, 10 000 arrows of one
   name, a caption on a non-continuation.
9. MCP contract tests for `contcaption` and `place` with `caption`; refused with writes off.
10. Bench rows within target (design §9); verify-mode equality for the index with continuations.

## Tester

**No** (design §11): no format, op, movement, store or `translate.rs` change. The worker's lane
tests cover §10, the failing-first test for F1 included, and the reviewer checks the update map
once on the feature PR. The owner may ask for one (question Q11).

## Needs owner check

The owner questions in design §13, and a click-only trial (quick start `049-continuation-refs.md`):
draw a 24 V supply on one page with two arrows to two other pages, group them under `TO CONTROL SUPPLY`, then move an arrow and renumber a page and watch the other ends follow.
