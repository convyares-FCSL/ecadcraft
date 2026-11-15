# Brief 059: The cable manager

Status: Approved (owner, 2026-11-15: design §14 answered; overseer, 2026-11-15: FD1–FD5
approved). The worker starts after #108 (057) merges.
Branch: `feature/cable-manager`, from `origin/dev` `22c924d1` (052 merged, #105). Starts after
057 (`feature/ratings`) merges into `dev`: it builds on 057's Cables tab, Details pane, cable view,
core table, `cable` catalogue and `core-open`.
Designer: **yes** (`docs/design/059.md`) · Size: L, in four parts (059a M, 059b M, 059c M, 059d S)
Roadmap: 2.7 (cable manager). Lanes: C (`cable_cmds.rs` new, `slash.rs`, `references.rs`,
`styles/cable.rs`, `checks.rs`), U (`lists/cables.rs`, `lists/detail.rs`, `lists/cable_view.rs`,
the slash and Add to cable tools).

## Goal

Project ▸ Cables becomes the place where cables are **built**, before they are drawn (owner,
2026-11-15): "a good place to build your cables, then you assign them in the drawing".

- **+ Cable** creates a cable with no wires yet (the next free `-W`, ready to edit): cable type,
  cores, length, each end's planned location and function, glands. Its menu also offers the
  not-drawn cables, to assign one in the drawing.
- **Every field is editable** in the Details pane: type, cores, length, glands, shield, maker,
  part number, description, and each end's planned location and function.
- **Remove** deletes a cable that is not drawn. A drawn cable is **refused**, with a **Go to** for
  each page it is drawn on (owner, Q1).
- **Assign in the drawing.** The slash tool and Add to cable pick **New cable** (the next free `-W`
  at once) or an existing cable from a list. Picking a cable that is not drawn yet makes the slashed wires
  its cores, in order, with its core labels.
- **Cores not assigned show as such**: in the core table, the cable view and the Checks.
- **Gland sizes** at each end, shown in the list, the Details pane and the cable view (today:
  "gland —").

## Why

- Engineers write the cable schedule before, or alongside, the schematic: cable types, cores and
  lengths come from the site survey and the BOM, not from the drawing.
- Today a cable exists only after a slash. Its type, length and glands can be set only after it is
  drawn, and only through Set property.
- The model already allows it: a `Cable` with no members is valid, and `cableadd` / `cableslash`
  already take an existing cable. What is missing is a way to make one without wires, a way to
  pick it while drawing, and a rule for a cable that is not drawn yet. Today its offered cores
  would all be `core-open` errors.

## Scope

- No new op and no new record field. A cable that is not drawn is a `Cable` with no members and
  no representation. New data are cable properties and `cable` catalogue fields (design §13,
  FD1–FD5, approved). A cable stays a `Cable`, not a device (Q10). Each **end** has its own planned
  location and function, as entry keys of 052's lists (Q4).
- New commands `cablenew`, `cableedit`, `cableremove`, with typed parameter structs
  (`#[serde(deny_unknown_fields)]`) and an error enum, `CableError` (code review 2026-11, findings
  2 and 3). `cableslash` and `cableadd` gain the assign rule.
- Checks: `cable-not-drawn` (Info; Warning in the release check), `cable-end-off-plan` (a drawn end
  differs from that end's plan), `no-gland`, `cable-bad-value`, and `core-open` adjusted (not raised
  for a cable that is not drawn; "not assigned" wording for unwired cores). Reducing cores below
  those assigned is refused (Q6).
- The cable report includes not-drawn cables by default, and the choice is offered when the report
  is generated (Q3).

## Out of scope

- Gland sizes computed from the cable's outer diameter and a gland table (Q5, option C), cable
  routing and tray fill, cable pull lengths from a layout, ferrules, and importing a cable schedule
  from CSV (the toolbar's Import stays the report import of 057).
- Making a cable a `Component` (owner, Q10: a cable is a different thing from a device).

## Parts

| Part | What | Size | Tester |
| --- | --- | --- | --- |
| 059a | Model rules, `cable_cmds.rs` (`cablenew`, `cableedit`, `cableremove`), `CableError`, the assign rule in `cableslash` / `cableadd`, checks, MCP contract tests | M | Yes (it changes `cableslash`'s behaviour) |
| 059b | Cables tab: + Cable (new or assign), editable Details pane, Remove (refused when drawn, with Go to), the not-drawn state in the list, cable view and core table, the report option | M | No (reviewer) |
| 059c | Assign in the drawing: the cable picker on the slash tool and on Add to cable; the "Assign in the drawing" fix | M | Yes (interactive slash) |
| 059d | Glands: per-end sizes, cable-type default, `no-gland`, list and cable view | S | No |

059a first; 059b and 059c can then run in parallel (different files); 059d after 059b.

## Acceptance

Design §11 lists the checks. In short:

1. `cablenew {}` makes `-W<next>` with no members in one transaction, `Create cable`. It shows in
   Project ▸ Cables as **not drawn**, with one `cable-not-drawn` Info row (Warning in the release
   check) and no `core-open` rows.
2. `cableedit` sets each field and refuses a bad value with `CableError::BadValue`, writing nothing.
   Reducing the cores below those assigned is refused with `CableError::CoresInUse`, naming the
   cores in use. Each edit is one transaction and one undo step.
3. `cableremove` on a cable that is not drawn deletes it. On a drawn cable it is refused with
   `CableError::Drawn`, naming the pages; the UI offers a **Go to** for each.
4. `cableslash {from, to, cable}` on a cable that is not drawn makes the crossed wires its members
   in crossing order, with core labels from its offer, plus a slash; more wires than free cores is
   refused (Q6). On a drawn cable it continues the cable as today (032).
5. The slash tool's **Cable** picker lists not-drawn cables first, and picking one does 4. Esc
   writes nothing.
6. Glands show in the list (`M20, M20`), the Details pane's Ends and the cable view.
7. MCP contract tests for the three new commands and for the assign rule: transaction, author
   `Author::Agent(<client>)`, and refusal with writes off.
8. Bench rows `cablenew`, `cableedit`, `cableassign` (design §12) within budget on the large
   project.

## Owner answers

Design §14 records the ten answers (owner, 2026-11-15).
