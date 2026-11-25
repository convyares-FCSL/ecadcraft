# References

A reference is text on a page that points at a place on another page (or elsewhere on the same page):
where a contact's coil is, which contacts a coil has, where a cable carries on, and where a continuation
arrow's other ends are. ECAD works every reference out from the drawing each time the page is drawn.
Nothing is typed and nothing is stored for it, so moving a symbol, renumbering a page or reordering the
book never leaves a reference wrong.

Words used here are in the [glossary](../reference/glossary.md).

## How a reference reads

A reference names a page number and a zone of that page's frame. The project chooses the style in
**Project menu ▸ Project settings ▸ References ▸ Style**:

| Style | Example | Means |
| --- | --- | --- |
| page and zone (new projects) | `105/B3` | page 105, zone B3 (the letter first, whichever axis is lettered) |
| page and zone, dashed | `105-B-3` | page 105, then the zone's letter and number |
| page and column | `/105.3` | page 105, column 3 |

The built-in A3 sheet numbers its columns `1`–`8` and letters its rows `A`–`F`; see
[Pages](pages.md#sheets-frames-and-zones). An end that cannot be found shows `?` (`/?` in the page and
column style). Changing the sheet's grid changes reference texts on every page; Project settings says so.

The same setting from the command window: `projectprops {"reference_style": "page-column-row"}`
(`page-zone`, `page-column-row` or `page-column`).

## Contacts and coils

A contact shows where its device's head is (its coil, actuator or breaker): the place sits under the
contact, beside its lower connection point. The head shows a **contact table** under it: one row per
pole, power poles first, then the auxiliary contacts, each with the place where it is drawn.

- **Contacts offered** (inspector, Device tab) is what the part has: `3P+1NO+1NC`, or contacts with
  their own labels, `13/14 NO, 21/22 NC`. Poles the part offers that nobody has drawn yet show as grey
  rows with `—`. A contact drawn beyond the offer shows as a red row, and Checks lists **More contacts
  than the part offers**. Text that does not read is refused under the field:
  `"3X" is not a contact list. Write it like 3P+1NO+1NC, or 13/14 NO, 21/22 NC.`
- **Contact table** (inspector) places the table: Project default, Auto, Below, Right or Off. Off hides
  the table; the places under the contacts stay. The project default is **Project settings ▸ References
  ▸ Contact tables**.
- To put the table somewhere else, use **Move contact table** (search with Ctrl+K), pick the head, then
  click the new place. The table moves with its head after that.
- Selecting the head lists its **Contacts** in the inspector, each with a **Go to** button; selecting a
  contact shows its **Coil** with **Go to coil**.
- Hovering a contact adds the coil's place to the hover text; hovering a head adds its contacts.

Neither the places nor the table can be edited or erased on their own: ECAD answers
`The contact table is derived; move it with Move Mirror Table, or set it in Properties.`

Command window:

```
setprop {"component": "<device id>", "key": "contacts", "value": "3P+1NO+1NC"}
setprop {"component": "<device id>", "key": "mirror", "value": "below"}
mirrormove {"component": "<device id>", "at": [x, y]}
contacts {"component": "<device id>"}
references {"component": "<device id>"}
projectprops {"contact_mirror": "off"}
```

`mirror` takes `auto`, `below`, `right`, `off` or `at dx,dy` (millimetres from the head; `null` follows
the project). `contacts` and `references` only read.

## Cables across pages

A cable can be drawn on two pages. A **cable reference** marks where it leaves one page, and the
reference at the other end shows where it arrives.

1. Select the cable's slash and choose **Add reference** on its wheel (or search **Cable reference**).
   Click where the cable leaves the page. Until the other end exists the reference shows `?`.
2. Go to the other page and choose **Continue cable** (Cable group in the tool strip). Press Enter to
   take the cable offered (`L` lists the references still waiting), click where the cable arrives, then
   point the way the wires should run. ECAD draws short wires that are the same wires as on the first
   page, and a slash across them.
3. Attach each short wire to its connection point with **Connect**. There is no automatic routing.

Each end now shows the other end's place. To jump between them, select a reference and choose **Go to
reference** on the cable's wheel. **Preview reference** on a reference's wheel shows the other page
read-only. Hovering a reference lists both ends and, for each wire, the connection point it reaches.
Erasing one end turns the other into an orphan (`?`) and Checks lists **Cable reference with no other
end**; undo brings it back.

Command window:

```
cablereference {"cable": "<id>", "at": [x, y]}
cablecontinue {"cable": "<id>", "at": [x, y], "toward": [x, y]}
cablegoto {"reference": "<id>"}
cablereferences {}
```

`cablecontinue` takes `"members": [...]` to continue only some wires. `cablereferences` lists every
reference with its text, its partner and whether it is an orphan; it only reads.

More about cables themselves: [Cables](cables.md).

## Continuation arrows

A continuation arrow joins its net to every arrow with the same name, on any page (see
[Wires](wires.md)). Each arrow shows where its other ends are, past its tip and reading along the wire:
`102/C2 103/B5` (in the project's reference style), in book order. At most three places are shown, then `+n`.

- Place an arrow from the Wire group: **Continuation**. Command window:
  `place {"symbol": "continuation", "at": [x, y], "name": "24V"}`.
- An arrow is a drawing aid, not a device: it has no tag and is not in the device list, the BOM or
  numbering.
- Selecting an arrow shows the **Continuation** section in the inspector: **Name**, **Caption** and
  **Other ends**, each other end with a **Go to** button.
- **Go to target** on the reference's wheel (or `refgoto`) opens the first other end; again, the next.
- A **caption** (`TO CONTROL SUPPLY`) says where the arrows go. Arrows on one page with the same caption
  and direction, close together, share one bracket with the caption under it. The caption joins
  nothing. Set it in the inspector or with **Caption Continuations** (`contcaption`): select the arrows,
  press Enter, type the caption (Enter keeps the last one, `R` removes it).
- Deleting the reference text is refused:
  `The continuation reference is derived; move or rename the continuation instead.`

Settings ▸ Standards ▸ **Continuations** holds how many places show before `+n` (3), the gaps, and the
most space between arrows under one caption (20 mm). The caption's text style is **continuation
caption**.

Command window:

```
contcaption {"continuations": ["<aid>", ...], "caption": "TO CONTROL SUPPLY"}
setprop {"aids": ["<aid>"], "key": "name", "value": "0V"}
references {"aid": "<aid>"}
refgoto {"source": "<representation id>"}
```

## Limits

- A cable has at most two references (one continuation). A third is refused; a chain over more pages
  is not possible yet.
- A cable cannot be continued on a page it is already drawn on.
- A wire attached at both ends is not continued; the reply names it.
- The contact table lists NO and NC poles only; the offer grammar has no change-over entry.
- A contact's place sits at a fixed spot under it; only the whole table of a head can be moved.
- The hover text of a cable reference is text, not a picture of the other page.
