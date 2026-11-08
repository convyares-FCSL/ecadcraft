# Brief 022: Connectivity and wire numbers

Status: Approved (owner, 2026-11-08: "Yes, start F4")
Branch: `feature/connectivity` (worktree `C:\dev\ecad-features\connectivity`), from
`feature/project-manager` (016 + 018). It is rebased onto `dev` once that merges, and onto 019's
references when that merges.
Designer: **yes** (`docs/design/022.md`) · Size: M–L
Roadmap: `docs/roadmap-v0.1.md`, F4. F5 (terminal strips) and F6 (lists) build on it.

## Goal

ECAD knows what is electrically connected. Nets (potentials) are worked out from what is drawn.
Conductors stay individual and are drawn as wired, while **the net carries the wire number** and
every conductor on it shows that number with its cross-section and colour (`24V-07 0.5 Blue`), as
in the owner's reference practice. Open ends and orphans are listed.

## What exists today

- Conductors with two ends attached to connection points (`AttachEnd`), components and
  connection points (018 devices), cables with cores, and cross-page cable references.
- Derived state is never stored (EEP-0002): nets and numbers must be derived, unless the design
  shows a stored override is required. A designer's manual number is a property, not a stored net.
- The rules in `ecad/CLAUDE.md` apply: "Derived views scale with what changed" (an update map,
  incremental), the glossary (connection point, terminal, net, conductor, wire, cable, core), and
  IEC as the default, never the law.

## Scope

1. **Net derivation.** A net is the set of connection points joined by conductors, including:
   - through cable cores, which carry their conductor across pages;
   - through cross-page references.

   Junction rules:
   - a T-junction needs a drawn dot to connect;
   - crossing lines never connect;
   - two conductors on one connection point connect.

   Where a device bridges internally (a terminal's two sides, a link between terminals), the
   definition says so. The update map must say what a net does for each op.
2. **Wire numbers.**
   - **Schemes:**
     - per potential, so every conductor on a net shares one number (the default);
     - sequential per page;
     - `page-column`.

     The scheme is a project setting, with prefixes per potential or voltage class (`24V-`, `0V-`,
     `L-`, `N-`, `PE`).
   - A designer can **fix a number by hand** on a net (stored as a property), and it then wins.
   - **Stability:** numbers must not renumber the whole project when one wire is added. The
     design defines it, for example "a new net takes the next free number; existing nets keep
     theirs".
3. **Conductor labels.** Each conductor shows its wire number, cross-section and colour:
   - placement as in the reference (beside the conductor, rotated with it), readable;
   - cross-section and colour are conductor properties, with defaults per potential (24 V blue,
     230 V brown) set in project settings;
   - IEC colours are the default, never enforced.
4. **Checks.** Open ends, conductors attached at one end only, nets with a single connection point,
   unconnected device connection points, and crossing lines that look connected but aren't.
   Listed in a panel or report (the designer decides where) with go-to.
5. **Properties panel and MCP.** Net and conductor properties shown and edited where 019's
   Properties panel will be; MCP can read nets and wire numbers and set a fixed number.
6. **Quick start:** a "Connectivity and wire numbers" section, with click-only steps on the demo
   project (`cargo xtask demo`), which gains wire numbers.

Out of scope: terminal strips and terminal plans (F5); wire lists and CSV (F6); PLC I/O lists.

## Format and ops

New ops and `.ecad` additions are allowed **only when required**. The designer and worker propose
them with reasons; the **overseer decides**. Prefer derived state plus `SetProperty`.

## Acceptance criteria

1. Nets on the demo project are correct: through devices, cables and cross-page references;
   T-junctions with and without a dot; crossings.
2. Every conductor shows its number, cross-section and colour; a net's conductors share the number.
3. Adding a wire never renumbers existing nets. A manual number survives edits.
4. The checks list finds each seeded fault and goes to it.
5. The large bench shows incremental net updates (no whole-project rebuild per edit), as the
   update map promises.
6. MCP reads nets and numbers and sets a fixed number.
7. Reference screenshots, approved by the owner.

## Needs owner check

- Approve the design (`docs/design/022.md`).
- A click-only trial on the demo project.
