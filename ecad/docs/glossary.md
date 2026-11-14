# ECAD glossary

The words ECAD uses in everything a person sees: button labels, prompts, replies, the hover lens,
the Project panel, the quick start and design documents (owner decision, 2026-11-08, option A).
Code, file and command names may differ, as noted.

## Where conductors land

| Word | Means | Example |
| --- | --- | --- |
| **connection point** | A place on a device where a conductor's end can be attached: a coil's `A1`, a contact's `13`, a motor's `U1`. "Pin" is acceptable where space is tight. | `-K01:A1`; "end attached to connection point A1"; **Add Connection Point** |
| **terminal** | Only the DIN-rail terminal device: a device of its own (prefix `X`), with one connection point per terminal level. The starter symbol **Terminal** is this device. | `-X01:1` |
| **terminal strip** | A group of terminals with one tag, side by side on a rail. | `-X01` with terminals 1, 2, 3 |

**In code and files** the connection point is still `Terminal` / `TerminalId`, the op is
`AddTerminal`, the command `addterminal`, and JSON parameters say `terminal`. They stay until the
next major format version: op names are written in `.ecad` files. MCP command names stay as they
are.

## Devices and what draws them

| Word | Means |
| --- | --- |
| **device** | A real part with a tag (`-K01`), its properties (description, maker, part number, location `+`, function `=`) and its connection points. In code: a component with a `tag` property. |
| **component** | The model object a device is. A component without a tag (an outline converted with **Convert to Component**) is listed under "No tag". |
| **symbol** | The drawing of a device, or part of one, on a page: a coil, a contact, a terminal. One device can be drawn by several symbols on several pages (`-K01`'s coil on page 2, its contacts on pages 2 and 3). |
| **representation** | The model's link between a subject (a device, a conductor, a cable) and what draws it on one page: a symbol, a drawn outline, a wire's route, a cable's slash or reference. |
| **definition** | A symbol's shape and its connection points, embedded in the project (81 symbols come built in). |
| **change-over contact** | A contact with a common, a normally-closed and a normally-open point: `-K01:11`/`12`/`14`. |
| **fuse bank** | One device holding several fuses fed from one supply point, with a connection point and a rating per fuse: `-F01:L+`, `-F01:3`. |
| **fuse terminal** | A terminal-sized fuse holder; for now a device of its own, not a strip position: `-F02`. |
| **connector half** | The plug or the socket side of a connector, drawn as a body with one connection point per pin. Mating two halves is not built yet: `-X03:1`. |
| **bulkhead** | A connector through a panel wall; each pin is one connection point that wires reach from both sides: `-X04:1`. |
| **card body** | The outline of a PLC card or CPU, with its supply points; its channels join it as PLC input and output symbols: `-A01:L+`. |

## Connections

| Word | Means |
| --- | --- |
| **conductor** | One electrical connection with two ends, A and B, each attached to a connection point or loose. The model object behind a wire. |
| **wire** | A conductor as drawn: its route on a page. Continued on another page, the same conductor has a route there too. |
| **net** | Everything electrically joined: connection points linked by conductors, continuations with the same name, and bridges inside a device. Derived from the attachments, never stored; crossings never join, and a T joins only at a junction. |
| **junction** | The dot where three or more wires meet away from a device: a connection point of its own. A T without a junction is not connected. |
| **continuation** | An arrow with a name (`24V`) where a net leaves a page. All continuations with one name are one net. |
| **potential** | What a wire carries for numbering and defaults: `L`, `L1`…`L3`, `N`, `PE`, `24V`, `0V`. It sets the number prefix, colour and cross-section. Several nets share one potential (`24V-01`, `24V-02`). |
| **wire number** | The number a net carries and every one of its conductors shows (`24V-07`). Automatic, or **fixed** by hand. Stored on the conductors once issued, so adding a wire never renumbers others. |
| **cable** | A group of conductors that run together (`-W01`), made with a slash across their wires. Its members are its **cores**. Cable and device tags share one namespace (brief 026). |
| **core** | One conductor of a cable, with its core number or colour (`1`, `BK`). |
| **drawing order** | Page in book order, then column, then row of the sheet grid, then top to bottom and left to right inside a cell (brief 026). Renumber and Finalise number in it. |
| **fixed** | A tag or wire number chosen by hand that Renumber and Finalise never change (`F`). Typing a tag fixes it. |
| **provisional tag** | A placeholder tag (`-K?3`, `-W?1`, wire `24V-?`) that **Finalise numbering** replaces. Not issued. |
| **cable reference** | Where a cable leaves one page and continues on another, shown as the other end's page, column and row on its sheet (`105-C-4`, brief 019). An end with no other end is an **orphan** (`?`). |

## Pages

| Word | Means |
| --- | --- |
| **page** | One drawing sheet of the project. Exactly one is the **active page**, shown and editable in the page window. |
| **page number** | The number a page is shown with, chosen by the designer (`100`, `105a`) and kept when the page moves; a page without one (files from before brief 019) shows its position. |
| **folder** | A named group of pages in the Pages tree; folders nest (at most 8 deep). Within a folder, pages come before sub-folders. |
| **sheet** | The paper a page is drawn on (A3 by default, A2, A1, A0 or a custom size), with its **frame** and grid. The paper's bottom-left corner is the drawing's origin. |
| **frame** | The border inside the paper's margins, divided into zones: **columns** lettered `A`, `B`… from the left and **rows** numbered `1`, `2`… from the top, over the whole frame. |
| **column**, **row** | The frame's zones, used in reference texts: `105-C-4` is page 105, column C, row 4. |
| **title block** | The box along the bottom of the frame showing the project's and the page's facts and the revisions. Derived from the properties, never drawn by hand. |
| **revision** | One step in the project's release history (`Rev A`, `B`…), with its date, description and who drew, checked and approved it. Users only ever see "revision", never "version". |
| **issue** | Releasing a revision: its fields are fixed and the project as it is then can be reopened read-only (**Reopen**). Undoing an issue asks first. |
| **release** | Issuing a revision and writing its outputs to one folder (brief 027). Also the folder itself. |
| **release folder** | `release/Rev B/` beside the project file (by default): the PDF book, the CSV lists, the check rows accepted at release and `release.json`, all stamped `Rev B`. Outside the project file. |
| **pre-release check** | The Checks of the state about to be issued, grouped for a release and shown before anything is written. |
| **release gap** | A warning a release lists but does not stop for: open ends, orphan reference ends, provisional tags and wire numbers, `?` parts. |
| **preview** | A read-only view of another page, for checking a reference's other end. |
| **generated page** | A page whose content ECAD draws from the project (cover, contents, revision list, BOM by location). It has a number, title and folder like any page; nothing on it is stored (brief 024). |
| **sheet** (of a page) | One printed A3 (or other) sheet. A generated page whose table does not fit continues on more sheets of the same page; Total counts sheets. |

## Locations and outputs (brief 024)

| Word | Means |
| --- | --- |
| **location** | Where a device physically is (`+CP1`, `+FIELD`), the IEC 81346 `+` aspect. A device's own value, else inherited: the location box its main symbol is in, then its page's, then the project's. |
| **location box** | A dashed rectangle or closed polyline labelled `+…` on a page; every device symbol inside it takes its location, live. Innermost wins. |
| **report** | A table derived from the project (BOM, wire report, cable report), shown and exported as CSV, never stored. Wires are never on the BOM. |
| **Print Book** | One PDF of every page (or a folder, or a page list) in book order, generated pages included. |

## Terminal strips and cables (brief 028)

| Word | Means |
| --- | --- |
| **bridge** | A wire whose two ends are on two terminals of one strip (`-X01:3–4`). Derived: nothing says "bridge" in the file. The wire report shows it as `Bridge -X01`. |
| **inside / outside** | The two sides of a terminal: inside faces the panel's own devices, outside faces the field. Derived: a cable core is outside; else the far device's location against the strip's; else the wire's direction (up or left is inside). |
| **spare terminal** | A position the strip offers (**Terminals offered**, `12` or `12+2PE`) that no terminal symbol draws yet. |
| **core offer** | The cores a cable has: its **Cores** (`7`, `7G`, `5X`, `BN,BK,GY,BU,GNYE`), else read from its type (`7G1.5` → `7G`: cores 1–6 and green-yellow). |
| **used / spare / over core** | A core a wire of the cable takes / offered and not taken / a wire beyond the offer. A new wire added to a cable takes the next spare core. |
| **terminal plan** | Per strip, one row per position (drawn and spare): inside and outside target, wire numbers, cable and core, bridges and place. Derived, shown in Reports ▸ Terminals and on a generated **Terminal plan** page. |
| **cable schedule** | The cable report completed with Used, Spare and the devices at each end, and per cable its core list. Also a generated **Cable schedule** page. |

## Terminal accessories (brief 030)

| Word | Means |
| --- | --- |
| **accessory** | A part on the rail that is not a terminal: end stop, end plate, partition plate, marker, strip label, jumper. Placed by you; nothing is added by a rule. |
| **end stop** | The clamp that holds a strip on the rail (an "end clamp"). |
| **end plate** | The cover on a terminal's open side. |
| **partition** | A partition plate between two positions: separates groups and stops a jumper. |
| **marker** | The label clipped onto one terminal, bought as cards of several. |
| **strip label** | The marker that carries the strip's tag, usually on an end stop. |
| **gap** | Where a plate or stop sits: start, after a position, or end. |
| **jumper** | A plug-in or screw bridge between terminals of one strip: a part, not a wire. |
| **link** | How ECAD stores a jumper: positions of one strip joined into one net (`1-4`, `7, 9, 11`), owned by the strip, drawn as a dashed line through its terminals on every page. |
| **bar** | One jumper bar as bought (10-way unless the strip or project says otherwise); a link needs span ÷ bar size, rounded up. |

## Multi-level terminals (brief 036)

| Word | Means |
| --- | --- |
| **position** | One terminal block on the rail: the strip terminals that share one position label (`1`, `2.1`). |
| **level** | One deck of a position: one connection point, or two for a fuse or disconnect level (in and out). Numbered 1… from the top deck. |
| **terminal type** | What a position is: feed-through, PE, two-level, two-level bridged, three-level, L/N/PE, sensor, disconnect, fuse. |
| **level name** | A level's short name in its label: `L`, `N`, `PE`; `S`, `+`, `-`; `1`, `2`, `3`. A default a project or strip can change. |
| **terminal label** | What the page and the plan show for one level: the position, the separator and the level name (`1.L`), or the level's own label. |
| **internal bridge** | Levels of one position joined inside the block by its type (a bridged two-level terminal). |
| **free level** | A level of a drawn position that no symbol draws yet. |

## PLC racks and cards (brief 045)

| Word | Means |
| --- | --- |
| **rack** | A CPU or a head (interface) module and the cards in its slots. The rack **is** the head device: no new object. |
| **slot** | A numbered place in a rack. A card is in at most one slot. |
| **card** | A device in a slot (or in none yet): an input, output, analogue, RTD, IO-Link, power or communication module. Its type comes from its card body. |
| **card type** | DI, DQ, AI, AO, RTD, IOL, PS, PM, CM. |
| **channel** | One input or output of a card: one or more **channel points**, one per **role**. |
| **role** | What a channel point is: `in`, `out`, `+`, `-`, `I+`, `S+`, `S-`, `I-`, `L+`, `C/Q`, `L-`. |
| **pin** | The label printed at a channel point and typed after the tag (`-A02:4`): the point's connection point label. |
| **channel number** | The channel's index shown from the card's first number (0 or 1): `channel 3`. |
| **address** | What the PLC program calls the channel: automatic by slot and channel (`I2.3`, `%IX2.3`, `I:3/03`), or typed. |
| **spare** | A channel with no wire and no description. **Reserved**: no wire, but a description. |
| **I/O list** | One row per channel of every card: rack, slot, card, channel, address, pins, description, what it is wired to, wire number, place. |

## Connectors (brief 044)

| Word | Means |
| --- | --- |
| **connector** | One device (`-X10`) with up to two halves that mate. |
| **half** | The socket side or the plug side of a connector, shown and typed as `-X10.S` / `-X10.P`. A half is a BOM row and has its own location, not a tag of its own. |
| **side** | Which half a pin belongs to: socket (female contacts) or plug (male contacts). |
| **pin** | One contact position of a connector (`1`, `A`, `PE`); one connection point on each half that draws it. |
| **mate** | The pin of the same name on the other half; mated pins are one net. |
| **family** | Plain, D-sub, M12, RJ45 bulkhead, USB-A or USB-B bulkhead; both halves of a connector have one. |
| **pin pair** | A symbol showing one pin on both halves at once (socket cup over plug pin), as drawn in wire runs. |
| **bulkhead** | A one-piece through-panel connector: one point per pin that wires reach from both sides; it has no halves. |

## Symbols (brief 048)

| Word | Means |
| --- | --- |
| **symbol** | An embedded definition people can place: built-in (a starter), project (made here) or, later, from a library. |
| **project symbol** | A symbol made or edited in this project. Its source reads "Project". |
| **version** | One saved content of a symbol. Versions of one symbol share a **lineage**; the newest is what Place uses. |
| **placed copy** | One symbol representation showing a version of a symbol. |
| **text slot** | Where a device text (tag, description, rating…) of a placed copy goes, in the symbol's frame. |
| **origin** (tool) | The symbol's base point, (0, 0) of its frame: the point the cursor holds when placing. Where provenance is shown it is called **source**, never origin. |
| **update available** | A placed copy whose symbol has a newer version in the project. |
| **clash** | A change an update cannot make to a placed copy without losing a connection. |

## Continuation references (brief 049)

| Word | Means |
| --- | --- |
| **continuation arrow** | The tagless arrow (`kind = continuation`) that joins its net to every arrow with the same name, on any page. Not a *continuation sheet* (a generated report's next page). |
| **other end** | Another arrow with the same name. An arrow shows its other ends' places past its tip (`102-C-2 103-B-5`), derived, never typed. |
| **caption** | A short text on an arrow (`TO CONTROL SUPPLY`) saying where it goes. Display only: it joins nothing. |
| **caption run** | The arrows on one page with the same caption and direction, next to each other, drawn under one bracket with the caption. |
