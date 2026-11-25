# Wires

A **wire** joins two connection points. ECAD works out what is connected (the **nets**) from the
wires and gives every net a **wire number**. Grouping wires into cables is in [Cables](cables.md);
continuation arrows and references across pages are in [References](references.md).

## Draw a wire

Press **W** (or **Wire ▸ Wire** on the tool strip).

1. Move onto a connection point: a ring and the point's name show before you click ("joins A1 of
   -K01" in the intent line). Click.
2. Click where the wire bends (Ortho helps: the toggle at the foot of the strip).
3. Click another connection point: the wire is finished. Enter or a right-click finishes it where
   you are.

Typed: `wire {"points":[[40,10],[70,10],[70,0],[90,0]]}`; an end exactly on a connection point
attaches to it.

**A line and a wire.** What you draw is a line. When both ends land on connection points it
carries a wire. A line with a free end is drawn and can be slashed into a cable, continued or
given terminal blocks, but carries no wire yet: Checks lists **Line not connected at both ends: no
wire yet** with its page and place. Draw on from its free end to a connection point and the wire
forms.

**A T.** A line that ends on the middle of another line joins that net. A dot is drawn there, on
screen and in print. The dot is drawn from the lines, never stored: it moves and goes with its
line, and cannot be erased or moved on its own. Hovering the line says which wire its end joins
(`End B joins wire 3 (a T).`). Crossing lines never join.

**Across pages.** A continuation arrow (**Wire ▸ Continuation**) joins its net to every arrow with
the same name, on any page. A line from `-G01:-` to the `0V` arrow on page 1 and a line from the
`0V` arrow on page 2 to `-K01:A2` make one wire from `-G01:-` to `-K01:A2`. An arrow with no
partner carries no wire. See [References](references.md).

**How many wires a point takes.** A connection point accepts two wires unless its symbol says
more. A point with more is an error in Checks: **Connection point has more wires than it accepts**.

## Join, connect and disconnect

- **Draw on from an open end.** Starting a wire on another line's open end makes them one wire
  (`end A joins wire … (now one wire)`); the Wire tool shows `join` while you point at the end.
- **Join off.** Type `J` while drawing: ends that meet another wire stay open. Checks then shows
  **Looks connected** with **Join**.
- **Join Wires** (Ctrl+K **Join wires**, or the wire wheel's **Join**): click where two open ends
  meet. Typed: `joinwires {"at":[50,0]}`.
- **Different potentials are never joined:** `That wire carries 0V and this one 24V; joining them
  would short them.` Nothing is drawn.
- **Connect** (the wire end's wheel, or Ctrl+K **Connect**): click a wire near its end, then a
  connection point. **Disconnect** detaches an end.

## Nets and potentials

A net is everything electrically joined: connection points linked by wires, continuation arrows
with one name, jumper bars and bridges inside a device. The wire wheel's **Show net** selects every
wire of the net on this page.

A **potential** (`L1`, `N`, `PE`, `24V`, `0V`) is what a net carries. It sets the wire number's
prefix and the default colour and cross-section. Set it on a wire with the wheel's **Set
potential** (a short list of the project's potentials), or before drawing: type `P` at the Wire
tool's first prompt and give a potential; it stays for new wires in this session (`auto` clears
it). `C` at the same prompt picks a wire type for new wires ([Styles](styles.md)). Typed:
`potential {"conductor":"<id>","value":"24V"}` sets the whole net.

## Wire numbers

Every wire shows a label with its net's number, cross-section and colour, for example
`24V-04 0.5 BU`; vertical labels read bottom to top. Colours are IEC 60757 codes (`BN`, `BU`,
`BK`, `GY`, `GNYE`). Numbers are stored on the wires once issued, so adding a wire never
renumbers another net.

- **Fix a number:** the wire wheel's **Fix number** (or Ctrl+K **Fix wire number**), type
  `24V-50`, Enter. Every wire of the net shows it, fixed. Renumbering never changes a fixed number.
- **A number another net has** asks `24V-50 is used by another net. Swap, use another or merge?`:
  Enter takes the next free number, **S** swaps the two numbers, **M** joins the nets with a pair
  of continuation arrows when that is electrically sensible, and says why not otherwise.
- **Renumber wires:** Ctrl+K **Renumber wires**, or the Renumber dialog with **Wires**
  ([Devices](devices.md#numbering-and-renumbering)); fixed numbers stay.
- **Cross-section and colour** are set per wire in the inspector; empty returns to the potential's
  default, shown faint. A colour may also be free text.

Typed: `wirenumber {"number":"24V-04","value":"24V-10"}`, `renumberwires {"scope":"all"}`,
`nets {"number":"24V-04"}` (read only),
`setprop {"conductor":"<id>","key":"colour","value":"Violet stripe"}`. `setprop` refuses
`wire_number` and `potential` on a wire: use `wirenumber` and `potential`, which change the net.

**Wire colours on the page.** A wire with a colour is drawn in it; a two-colour code (`GNYE`,
`GN/YE`) draws the first colour with dashes of the second. An unknown code draws in ink and adds
the Checks row **Colour code not defined**. Prints are black by default, with the colour in the
label; Print Book can print in colour ([Publish](publish.md)).

## Wire settings

**Settings ▸ Pages and sheets ▸ Wires** (the wire wheel's **Wire settings** opens it):

| Setting | Choices |
| --- | --- |
| **Colours** | `BN BU GN/YE`, `BRN BLU GRN/YEL` or `Brown Blue Green/Yellow` |
| **Label height** | in mm (default 2.5) |
| **Labels** | **One per net per page** (default) or **Every wire** |
| **Shortest wire** | with Every wire: wires shorter than this get no label |
| **Wire numbers** | **One per net** (every wire shows the net's number) or **One per wire** (each its own, in the net's wire order; the first keeps the net's number) |

Switching wire numbers renumbers every net in the same undo step; fixed numbers stay.

Typed: `wiresettings` with `scheme` (`potential`: `24V-07`; `page`: `24V-105.03`; `page-column`:
one sequence per page and column), `digits`, `colour_style` (`code2`, `code3`, `name`),
`label_height`, `label_rule` (`net`, `each`), `label_min_length` and `number_by` (`net`,
`conductor`).

## Checks about wires

Among the rows Checks lists ([Checks](checks.md)): **Open end**, **Loose wire**, **Looks connected**
(with **Join**), **End drawn away from its connection point**, **Net with a single connection
point**, **Connection point not connected**, **Net joins two potentials**, **Two nets share a
number**, **Continuation without partner**, **Continuation name not a potential** and **Unnumbered
nets**.

## Limits

- A label sits on its wire's longest segment, clear of other labels and wires where it can be,
  but it does not avoid tags or reference texts, so it can cross one.
- The marker showing which way a T's wire order runs is not drawn yet.
- No built-in symbol accepts more than two wires on a point yet.
