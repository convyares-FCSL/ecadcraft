# 044 Connector devices: how to try it

Brief 044 (`docs/briefs/044-connectors.md`, design `docs/design/044.md`). A plug and its socket
are one connector (`-X10`) that mates: pin 3 of the socket half (`-X10.S:3`) and pin 3 of the plug
half (`-X10.P:3`) are one net, on any pages.

Status: built by the worker; **not yet tried by the owner**. What is listed under "Checked by the
worker" ran as automated tests only.

## Connectors (by clicking)

1. Open a project with a page that has a sensor's three wires drawn vertically (any three wires
   do).
2. Press **P** until the Place flyout shows **Connector**, and click it (or type `connector` at the
   command line).
3. Answer the prompts with their chips, or Enter for the offered answer:
   - **Connector**: Enter for a new one (or type a connector's tag to add to it).
   - **Halves**: Both. **Type**: M12. **Pins**: 4. **Pin names**: Numbers. **Draw**: Pins.
4. Click left of the first wire and right of the third: the intent line names the three wires the
   line cuts before you click. Three pin pairs are placed, one on each wire; each wire is split and
   both parts keep their number (they are one net through the mate). Enter ends the tool.
5. Hover a pin: it reads `-X01.S:1` (or `.P:1`).
6. Add a page. Place ▸ Connector again, Enter for a new connector, **Halves** Socket, **Type**
   Dsub, **Pins** 9, **Draw** Row, click to place the socket half. Go back to the first page and run
   `connector` once more, typing the D-sub's tag at **Connector**: **Halves** offers Plug; click to
   place the plug half there.
7. Look at both halves: each half's tag shows the other half's place under it (`/2.C4` style).
8. Open Reports ▸ BOM: the D-sub has two rows, `-X0n.S` and `-X0n.P`, each with its own
   description (`D-sub socket, 9-way`, `D-sub plug, 9-way`).
9. Settings ▸ Standards, group **Connectors**: set the socket side name to `F` and the plug's to
   `M`. Every half tag, the hover, the wire list and the BOM show `.F` / `.M` at once.

## Commands

| Command | Does |
| --- | --- |
| `connector {halves?, family?, pins?, names?, layout?, at? \| line? \| wire?, device?, page?}` | Places a connector as rows (whole halves) or pin by pin across wires; one transaction |
| `place {symbol: socket4 \| plug4 \| … \| pinpair \| pins \| pinp, device?, pin?}` | Find Symbol's one-off placement; joins the connector named by `device` |
| `pinlabel {device, pin, to}` | Renames a pin on both halves; `{terminal, to: "next"}` moves one drawn pin to its half's next free pin |
| `setprop {component, key: "pins" \| "half.s.maker" \| …}` | The pin count and each half's description, maker, part number and location |

## Checked by the worker (automated)

`cargo test -p ecad-cadcraft lane_044`, `cargo test -p ecad-mcp lane_044`,
`cargo test -p ecad-model lane_044`: rows, pins across wires, mating across pages, refusals,
Checks rows, registry naming, BOM half rows, references, `pinlabel`, hostile input, the version-2
starters and the bridge-key check.

## Not in this brief

Crimps, housings and backshells per half (047), a connector plan report, cordsets, and a `mate`
command for halves drawn before 044 (they stay separate devices).
