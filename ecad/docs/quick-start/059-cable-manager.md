# 059: the cable manager

Build cables in Project ▸ Cables before they are drawn, then assign them in the drawing
(brief `docs/briefs/059-cable-manager.md`, design `docs/design/059.md`).

## Commands (059a)

At the command line or through MCP's `execute`:

- `cablenew {"cores": "4G", "to": {"location": "+FIELD", "gland": "M20"}}`: a new cable with the
  next free `-W`, **not drawn** (no wires yet). Checks shows one Info row `-W01 is not in the
  drawing yet.` (a Warning in the release check), and no `core-open` rows for it.
- `cableedit {"cable": "-W01", "set": {"length": 12, "shield": "overall"}}`: one transaction,
  one undo step. `null` clears a field. A bad value (`"length": "x"`) is refused and nothing is
  written. Reducing the cores below those in use is refused, naming them.
- `cableremove {"cable": "-W01"}`: removes a cable that is not drawn. A drawn cable is refused with
  the pages it is drawn on (`-W01 is drawn on 2/D1. Delete it on its page.`).
- `cableslash {"from": …, "to": …, "cable": "-W01"}` on a cable that is not drawn makes the crossed
  wires its cores, in order, with the cores' labels (`1`, `2`, `3`); more wires than free cores
  is refused. On a drawn cable it continues the cable as before (032).

Checks gains `cable-not-drawn`, `cable-end-off-plan` (a drawn end away from that end's planned
location), `cable-bad-value`; an unwired core of a drawn cable now reads `core GNYE is not
assigned: slash or add a wire, or tick Not used.`

## Bench (agent-observed, medium tier, debug off, release build)

`ecad-bench --cables <medium.ecad>`: cablenew 2.09 ms, cableedit 0.08 ms, cableassign 2.41 ms,
cableslash 2.66 ms (500 cables). The rows time the whole command.

## Project ▸ Cables (059b): try it by clicking

1. Open a project, **Project ▸ Cables**. Click **+ Cable** (left of the filter): a new `-W` row
   appears, selected, badge `not drawn`, and the Details pane opens with the cursor in **Tag**.
   The footer counts `1 not drawn`.
2. In the Details pane, type a **Length** (`12`) and press Enter: one undo step. Type `4Q` in
   **Cores** and press Enter: the box turns red with the reason under it, nothing is written.
3. Pick a **Cable type**, a **Shield**, and each end's planned **Function** and **Location**
   (Ends). The list's From and To show the plan, `+FIELD (planned)`. The cable view shows the
   planned ends and grey cores marked `not assigned`.
4. The chevron beside **+ Cable** lists **New cable**, then the cables not drawn (Assign in the
   drawing switches to Draw on the page whose location matches the planned cabinet end).
5. **Remove** (footer) removes a cable that is not drawn at once. On a drawn cable it is refused:
   `-W01 is drawn on 2/D1. Delete it on its page.` with a Go to per page.
6. **Add a core** raises the offer by one (`4G` → `5G`).
7. Reports ▸ Cables: **Include not-drawn cables** (on by default) leaves them out for that view;
   `cablelist {"include_not_drawn": false}` does the same for a CSV.

Not done in 059b: Ins and Del keys on the list; the Tag text is focused but not selected; the
report setting is per view, not stored in the project.

## Assign in the drawing (059c)

- The slash tool's prompt now reads `Specify first point of cable slash or [Cable]:`. Type `Cable`,
  then a cable's tag (`-W03`), or Enter for **New** (the default): the slash then assigns that
  cable. On a cable not drawn the crossed wires become its cores in order; the preview line says
  `-> 3 wires onto -W03 (1, 2, 3)`.
- While Cable slash runs, the right panel shows **Cable**: New cable, then the cables not drawn
  (those planned on this page's location marked `matches`), then drawn cables not slashed on this
  page.
- **Assign in the drawing…** (the Details pane, + Cable's menu, the `cable-not-drawn` fix)
  switches to Draw on the page whose location matches the planned cabinet end and starts the
  slash with that cable picked: `Slash the wires of -W03 (4 cores).` Esc writes nothing.

Not done in 059c: Add to cable's own picker (it has no cable prompt today; `cableadd {cable}` takes
the free-core rule); after New cable the right panel does not jump to the new cable's Tag box; the
`matches` mark compares the page's location boxes, not the crossed wires' devices.
