# Brief 032: Wires and nets

Status: Approved (owner, 2026-11-12: design 032 §14's nine questions as recommended; FP-W1 to
FP-W4 decided by the overseer, design §13; every default this lane adds is registered in lane T's
defaults registry, design §4.5)
Branch: `feature/wires-nets`, from `origin/dev` `6729c8f1`.
Designer: **yes** (`docs/design/032.md`) · Size: M in all (Q6 S–M, Q8 M, Q4 M)
Roadmap: `docs/discussions/roadmap-review-2026-11.md` §3.2, v0.15 lane **W**: Q6 (wire join) →
Q8 (cable across pages) → Q4 (wire labels). Lanes T (031), P (033) and S (034) run beside it.

## Goal

Wires that look joined are joined, a cable keeps its identity where it crosses pages through
continuations, and wire labels stop piling up at junctions and on short drops.

## What exists today (checked on `6729c8f1`)

- `wire` (`commands.rs` `run_wire`) attaches an end that lands on a connection point, and puts a
  junction where an end lands on the **interior** of another wire. An end that lands on another
  wire's **loose end** is left loose: `plan_junction` refuses a route end, and `run_wire` drops the
  refusal (`let Ok(j) = … else { continue }`). The second wire gets a net and number of its own.
- On screen a loose end has a 6 px grey square (`LOOSE_ACI`, overlay only). Two loose ends at one
  point show the same square as one deliberate open end. Nothing is printed.
- The Checks list already has **Open end**, **Loose wire** and **Looks connected, no junction**
  (with **Add junction**). The repro produces all three rows; nobody looks at them while drawing.
- A cable is one tag with members (conductors). A second `cableslash` with the cable's tag is
  refused (`Tag -W04 is already used`). `cablecontinue` continues the *same* conductors onto
  another page (010) and refuses members attached at both ends, which is every core that ends on a
  continuation arrow. `MAX_REFERENCES_PER_CABLE` is 2.
- Wire labels: one per route per page, on its longest segment, placed clear of other labels and
  wires (`wires.rs` `labels_on_page_with`). Height from `wiring.label_height` (default 1.6 mm since 031 FP6; 1.8 before).

## Scope (in order)

1. **Q6: wire join (high bug, first).**
   - A new wire's end that lands on another wire's loose end **joins it**: the two become one wire
     (one conductor, one route, one label). An end on another wire's interior keeps today's
     junction. Where three or more ends meet, one junction joins them all. An existing loose end
     that lies on the new wire's interior also joins it, with a junction (owner Q2).
   - The way out: `join: false` on `wire` (`junction: false` stays as its old name), and a **Join**
     keyword in the Wire tool that turns joining off for the rest of the tool session.
   - Where joining is not possible (fixed numbers or potentials that disagree, a loop), the `wire`
     command is refused with the reason and the way out. It never draws a silently open end.
   - A new command, `joinwires {at}`, repairs ends that already meet (old files, moved wires). The
     Checks row **Looks connected** offers it as **Join**.
   - On screen, a loose end that lies on another wire's end or interior is marked in
     `ecad.check-warning` (yellow), unlike a deliberate open end.
   - **Regression test** (fails on `6729c8f1`): `wire [[0,0],[50,0]]`, then `wire
     [[50,0],[50,-20]]`, gives one conductor with one net.
2. **Q8: a cable across pages through continuations.**
   - `cableslash` with an existing cable's tag (or `cable` id) **continues that cable on this
     page**, when every wire it crosses is joined through a continuation pair to the free end of a
     core of that cable that is not drawn on this page yet. The crossed wires become members with
     those cores' labels. Otherwise it is refused with the reason, as today.
   - A core is then a **chain** of members joined through continuation pairs. Core counts, the
     cable list's from and to, and the Checks rows **Core used by two wires** and **More cores
     used than the cable offers** count chains, not members.
   - `cablecontinue`'s refusal for cores ending on continuations says what to do instead.
   - **No format change** (FP-W2). Cables with more than two cable references stay in M2
     (roadmap question 22).
3. **Q4: wire labels.**
   - A project setting chooses the rule: **each wire** (today) or **one per net per page**, and a
     **shortest labelled length**. A net is labelled at least once on every page it is drawn on.
   - Label height comes from lane T's text-style getter (031) once it lands; until then from
     `wiring.label_height`. Q4 starts on the defaults.

Each changed or new command (`wire`, `joinwires`, `cableslash`, `cablecontinue`, `wiresettings`)
has an MCP contract test in the acceptance criteria (design §12).

## Acceptance

Design 032 §12 lists the checks. In short: the regression test above and its siblings (both ends
joined, three-way, reverse T, `join: false`, refusals); the yellow mark and the Checks **Join**
fix; the pack's W04 case (a cable slashed on page 100 and on page 115 through continuation pairs:
one cable, its cores' from and to across both pages, no Checks rows); the label rule on a busbar
with 5 mm drops; the update maps' tests; the MCP contract tests; the bench rows in design §11.

## Not in scope

- Joining by moving or stretching a wire (grips, `move`): Checks and the yellow mark show it;
  `joinwires` repairs it. A later brief may join on move.
- Rewriting old files on open: they are repaired by the user (`joinwires`, Checks).
- More than two cable references, shields and pairs (M2, roadmap question 22).
- Wire types (M1 1.4), text placement and visibility (lane T, Q2 and Q3).
