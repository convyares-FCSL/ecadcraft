# Brief 049: Continuation cross-references

Part of the [quick start](../quick-start.md). Continuation arrows themselves (placing them, their
name) are brief 022's.

## What this brief changed

**Branch `feature/continuation-refs` (brief 049)**.

| Where | What is there now |
| --- | --- |
| On the page | Every continuation arrow shows where its other ends are, past its tip and reading along the wire: `102-C-2 103-B-5`, in book order, at most three then `+n`; `?` for an end that cannot be located. Nothing is typed: move an arrow or renumber a page and every other end follows at once, also on the page on screen. The old text under the arrow's name is gone |
| Captions | A caption on arrows (`TO CONTROL SUPPLY`). Arrows on one page with the same caption and direction, no more than 20 mm apart, share one bracket with the caption under it; one arrow alone gets its own. The caption is display only |
| `contcaption` | Caption Continuations: select arrows, Enter, type the caption (Enter keeps the last one, `R` removes it). The JSON form is `contcaption {continuations: [...], caption: "..." or null}`. `place` and `setprop` take `caption` too |
| Inspector | A selected arrow shows **Caption** (editable) and **Other ends**, each with **Go to** |
| Hover | `Continuation 24V: 2 other ends`, then each one's place and page |
| Go to Reference | From an arrow or its text: the first other end; again: the next, wrapping |
| `xrefs` | Rows of kind `continuation`, one per arrow and other end |
| Settings ▸ Standards | **Continuations**: other ends shown before `+n` (3), gaps, the most space between arrows under one caption (20 mm), the bracket tick. Text styles: **continuation caption** (2.5 mm, regular) |

## Try it

1. New project. On page 1 draw a short vertical wire and, at its lower end, type
   `place {"symbol":"continuation","at":[<x>,<y>],"name":"24V"}` (or Place ▸ Continuation,
   name `24V`). Add a second page and a third, and place a `24V` arrow on each.
   Expected: each arrow shows the other two places past its tip, rotated along the wire.
2. On page 1 place two more arrows next to the first (5 mm apart), names `0V` and `L1`, with
   arrows of the same names on page 2.
3. Type `contcaption`, click the three arrows on page 1, Enter, type `TO CONTROL SUPPLY`, Enter.
   Expected: one bracket past the texts with `TO CONTROL SUPPLY` under it; one step in History,
   `Caption TO CONTROL SUPPLY`.
4. Move one of the three arrows 40 mm to the right. Expected: two brackets, the same caption
   under each. `u` puts it back under one.
5. Page 2's Properties: change its number to `110`. Back on page 1: every arrow that names page 2
   now reads `110-…`.
6. Hover an arrow: its other ends are listed. Right-click it ▸ Go to target (or `refgoto`):
   the first other end opens; again: the next.
7. Select the reference text and press Delete. Expected: `The continuation reference is derived;
   move or rename the continuation instead.`
