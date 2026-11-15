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
