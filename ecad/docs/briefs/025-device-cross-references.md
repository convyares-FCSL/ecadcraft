# Brief 025: Device cross-references on a general reference index

Status: Approved (owner, 2026-11-11; design 025 §12 answered as recommended, FP1–FP3 approved by
the overseer, see design §13)
Branch: `feature/cross-references` (worktree `C:\dev\ecad-features\connectivity`, folder name
historical), from `dev` `af2d41eb`.
Designer: **yes** (`docs/design/025.md`) · Size: M
Roadmap: `docs/roadmap-v0.1.md` F3, lane B (after F2, before 028 terminal strips). Agreed first step:
`docs/features.md`, "a general reference index" (owner decision, 2026-11-08).

## Goal

A contact says where its coil is (`110-B-5`) and a coil says where its contacts are, in a contact
mirror table under it, including the contacts its part offers that nobody has drawn yet. The
texts stay live. They are built on one reference index that runs from a drawn item to the
references that show its place, so cable references, coil and contacts, and parent and child
parts all use the same machinery.

## What exists today

- Cable references (010): a reference end is a representation with role `reference`. Its text is
  derived by `ecad_model::derive::cable_references(_of)` in the project's `ReferenceStyle` (019:
  `page-column-row` by default). Commits are scoped per cable touched
  (`references::cables_touched`, `Snapshot`, brief 014).
- Devices (018): one component, many representations. The first symbol has role `main`; joined
  contacts carry their symbol key as their role. The starter catalogue (`symbols.rs`) knows each
  symbol's kind: `Contact { make }`, `Fixed { exclusive }` (coil, MCB, fuse, PSU, lamp, motors,
  transformer), PLC channel and terminal. Joined contacts get IEC labels (`13/14`, `21/22`).
- Checks tab (022) with per-subject updates and go-to. Properties panel (019) with a device
  section. Hover lens (018) with an `Also:` line.
- Indexes in `ecad_model::index`: `subject_reps`, `page_reps`, `terminal_reps`, `tags` and others.

## Scope

1. **The general reference index.** A derived, never-saved index from each target (a drawn
   representation) to the sources that show its place, and back. It is type-agnostic: it knows
   groups and edges, while a short rule table decides which members of a group point at which.
   Cable references move onto it with no visible change. Its update map is written per op
   (design §6).
2. **Coil and contact references.** Under each contact, its coil's place in the project's
   reference style. Under each coil, a **contact mirror table**, a derived graphic with one row
   per contact pole: NO or NC, power or auxiliary, its terminals and where it is drawn, including
   the offered contacts no one has drawn yet. The designer can move the table or turn it off per
   device. A project default is proposed.
3. **Contact counts.** A device with more contacts of a kind drawn than its part offers is listed
   in the Checks tab with go-to. It is never refused.
4. **Parent and child.** MCB with an auxiliary contact works through the same rule in this brief.
   A PLC card with channels needs a card symbol and is a later step (design §8 says why).
5. **Read it everywhere.** Hover, the Properties panel, Go to Reference and MCP (`xrefs`,
   `contacts`) show references. Quick start section "5L. Cross-references".

Out of scope: terminal strips (028), tag numbering and renumbering (026; tag assignment is not
touched), outputs and printing (024), continuations on the index (design Q9), a PLC card symbol.

## Format and ops

No new op. The design proposes (the **overseer decides**):
- component properties `contacts` (what the part offers, e.g. `3P+1NO+1NC`) and `mirror` (table
  placement), both through the existing `SetProperty`;
- an optional `ProjectSettings.contact_mirror` default, through `SetProjectSettings`, left out of
  the file at its default.

The index is derived and never serialised. Old files open unchanged and gain the texts and tables
on first draw.

## Acceptance criteria

1. On a project with `-K1` (coil on page 100, power contacts on 100, a NO and an NC on 105), each
   contact shows the coil's place (`100-B-3`) and the coil shows a mirror table listing all five
   poles with their places. Renumbering page 105 to `106` changes only the table's rows that name
   it, in the same commit.
2. Setting `contacts` to `3P+1NO+1NC` adds no rows for poles already drawn; setting it to `3P+2NO+1NC`
   adds a spare row `23/24 NO —`. Joining a third NO lists `-K1` in Checks as too many contacts,
   with go-to. Nothing is refused.
3. Moving the table, setting it to `off` and back, and moving the coil (the table follows) are each
   one transaction and one `u`.
4. Every existing cable-reference test passes unchanged, and `cablerefs` output is byte-identical
   on the 010, 019 and 022 fixtures.
5. MCB with an auxiliary NC: the NC shows the MCB's place and the MCB shows a one-row table.
6. Hover, Properties and Go to Reference show and follow references both ways.
7. MCP contract tests (brief 011 rule): `setprop` with `contacts` and `mirror`, `mirrormove` and
   `place` join labels record `Author::Agent(<client>)` and are refused with writes off; `xrefs`,
   `contacts` and `refgoto` work with writes off and record nothing.
8. The large bench reports the design §7 rows. Index build stays under 10 ms on open, a coil move
   stays under 1 ms beyond today's commit, and no ordinary edit rebuilds or scans the index.
9. Save and reopen keep `contacts` and `mirror`. An old file opens unchanged.
10. Reference screenshots, approved by the owner.

## Needs owner check

- Approve the design (`docs/design/025.md`) and answer its questions (§12).
- A click-only trial: quick start 5L.
