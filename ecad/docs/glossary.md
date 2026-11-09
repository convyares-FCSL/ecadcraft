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
| **definition** | A symbol's shape and its connection points, embedded in the project (the 18 starter symbols come built in). |

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
| **cable** | A group of conductors that run together (`-W01`), made with a slash across their wires. Its members are its **cores**. |
| **core** | One conductor of a cable, with its core number or colour (`1`, `BK`). |
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
| **preview** | A read-only view of another page, for checking a reference's other end. |
