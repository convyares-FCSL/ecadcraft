# Styles, wire types and project defaults

How ECAD decides the size, weight and colour of every text it draws, which texts are shown, how a
wire gets its colour and size, and where each default comes from. Words are as in the
[glossary](../reference/glossary.md).

## Where a value comes from

Every default (a tag prefix, a text height, whether a tag is shown, a wire's type…) is looked up
along one chain, and the nearest level that holds a value wins:

**Built-in → Standard → Project → Page → Object** (an object is a device, a symbol or a wire).

- **Built-in** values are ECAD's own data, the same on every machine. No company file is read from
  the computer, so a project reads the same everywhere.
- A project that chose a value keeps it, even when it equals today's built-in. A project that chose
  none follows the built-in.
- Not every default allows every level: text heights have no page level, for example. A level the
  default does not allow is refused, and the refusal names the levels it does allow.

To see the chain: open **Settings ▸ Standards** (project menu ▸ **Standards**, or click the
standard's name in the bottom bar). The **All defaults** list shows each setting, its value, a badge
for where the value comes from (Built-in, Standard, Project, Page, Object, Symbol) and its key.
Search, group, or show only **Overridden** rows. Select a row to see its chain with the level
**In use** marked. Type a **Project value**: before anything is written a line says what will
change. **Apply** writes it as one undoable step; **Reset** removes the project's value.

Typed: `defaults {}` lists every default, its value and where it comes from
(`defaults {"group":"Text"}` for one group); `setdefault {"key":"symbol.coil.prefix","value":"Q"}`
sets a project value; add `"level":"page"` or `"level":"object"` with the page, symbol or device;
`{"key":…,"reset":true}` removes it.

## Standards: a house style

A project uses at most one **standard**: a named copy of a house style that holds values for any
default at the Standard level, and its own styles. Without one it uses **IEC (built in)**.

In **Settings ▸ Standards** the header names the standard and offers **Make a standard…**,
**Rename**, **Copy from project…** (reads another `.ecad` file, which is not changed) and
**Back to IEC**. Each says what it will do before anything is written. The bottom bar shows
`Standard: <name>`.

Typed: `standard {}` (read), `standard {"new":"House"}`, `standard {"from":"<other .ecad>"}`,
`standard {"remove":true}` (back to IEC, one undoable step).

## Style catalogues

Settings ▸ Standards lists the catalogues on the left under **All defaults**: **Text styles**,
**Location boxes**, **Frames**, **Wire types**, **Cable types** and **Rating tables**. (Page types
are in **Settings ▸ Pages and sheets**.)

Select a style to see each field's chain (Built-in → Standard → Project), change it at **Project**
or **Standard** level, and **Reset** a level. **Used by** names what uses the style.
**New style from this** makes your own copy; your own styles can be renamed and deleted
(**Delete…** asks for a replacement when the style is in use); **Promote to standard** moves a
project style into the standard.

An object may choose its own style: a device a text style per kind, a location box its box style, a
page its frame style. The Properties panel's Layout tab shows the choice (see
[properties.md](properties.md)). A choice that names a style that no longer exists is drawn with
the next level's style and listed in Checks.

Typed: `styles {"catalogue":"text"}`,
`setstyle {"catalogue":"text","id":"tag","fields":{"colour":"RD","height":3}}`,
`setstyle {"catalogue":"text","new":true,"name":"Tag large","copy_of":"tag","fields":{"height":5}}`,
`deletestyle {"catalogue":"text","name":"Tag large","replace_with":"tag"}`,
`promote {"catalogue":"text","name":"Tag large"}`.

## Text styles

Every text ECAD draws (tags, connection point labels, references, wire labels, title block fields,
table text, PLC texts…) takes its **font**, **cap height**, **weight** and **colour** from the text
style of its kind. Change a style and every text of that kind follows.

| Text | Built-in cap height | Weight |
| --- | --- | --- |
| Tag | 2.5 mm | bold |
| Connection point label | 1.5 mm | regular |
| Description, part number | 1.6 mm | regular |
| Continuation name | 2.0 mm | regular |
| Reference (cross-references, contact tables, continuations, cable reference ends) | 2.5 mm | regular |
| Wire label | 2.5 mm | regular |
| Cable text (core labels, the cable's label box) | 1.6 mm | regular |
| Location label | 1.6 mm | regular |

The project font is **Barlow Semi Condensed** unless the project names another. A font the
computer does not have falls back to ECAD's stroke font, where bold is drawn as a heavier line; the
project's stored font name is kept.

To change the font and each kind's height, bold and shown state in one step: **Settings ▸ Pages and
sheets ▸ Text styles**. **OK** writes every change as one undoable step; Cancel or Esc writes
nothing. A height outside 0.5–10 mm says `A text height is 0.5 to 10 mm.` and is not written.

Typed: `textstyle {}` (the font, what draws it, every kind),
`textstyle {"kind":"tag","height":3,"weight":"regular"}`, `textstyle {"font":"Arial"}`,
`textstyle {"kind":"wire_label","height":2}` (the same value as the wire label height in Pages and
sheets), `textstyle {"kind":"wire_label","height":null}` (back to "not chosen").
`projectprops`, `pageprops` and `setprop` refuse `text.` keys: text goes through `textstyle`,
`showtext` and `setdefault`.

## Show and hide text

Tags, connection point labels, descriptions and part numbers can be shown or hidden on one symbol,
one device, one page or the whole project. Descriptions, part numbers and continuation names are
hidden by default. A terminal strip's tag is drawn once per run of adjacent terminal blocks, and a
PLC device's once per run of its channels.

- **Show or hide text** (type it in the search, or use the **Show or hide** slot of a selected
  text's wheel): click a tag or label to hide it; click a symbol to show its hidden texts. At the
  prompt choose **Device**, **Page** or **Project** first to act wider. Hiding keeps the data: only
  the drawing changes.
- **Show hidden text**: draws hidden texts dimmed. It records nothing and never prints.
- The Properties panel sets the same per kind: for a device, its Layout tab's **Text on the sheet**
  (each text's On box); for a page or another object, the Device tab's **Texts shown** group.

Typed: `showtext {"kind":"tag","show":"no"}` (the project; add `"page"`, `"component"`,
`"representation"` or `"conductor"` with an ID for narrower), `"show":"first"` (once per run, tags
only), `"show":null` (inherit); `hiddentext {}` toggles the dimmed view; `pagetexts {}` lists every
derived text on the page, hidden ones too.

## Where texts sit, and moving them

Tags (with any shown description and part number) are placed beside their symbol where they cross
no wire, symbol or other text. **Move text** (search, or the text wheel's **Move text**): pick a tag
or label, then where it goes (**Justify** first to give its alignment, such as `LT`). The text then
follows its symbol. Start Move text again and choose **Reset** to put it back to automatic. Free
text is moved with MOVE; texts ECAD derives cannot be edited on their own.

Typed: `movetext {"representation":"<id>","kind":"tag","to":[130,170],"justify":"LT"}`;
`"reset":true` instead of `to` puts it back.

## Wire types

A wire type gives a wire its **potential** (the type is that potential's default), **size**
(`2.5` mm² or `18 AWG`), **colours** (an ordered list: `GN/YE` is one two-colour wire),
**insulation** (`pvc`, `xlpe`, `epr`, `rubber`, `silicone`, `ptfe`, `lszh`), its drawn line (line
colour: `wire` for the insulation colour, `ink`, or a colour code; line type; line weight), its
**label rule** (`project`, `net`, `each`, `none`) and its BOM fields (part number, maker,
description, **allowance** in metres per wire).

Built-in types:

| Type | Potential | Colours | Size | Drawn |
| --- | --- | --- | --- | --- |
| Power L, L1, L2, L3 | L, L1, L2, L3 | BN, BN, BK, GY | 2.5 | ink, 0.6 mm |
| Neutral | N | BU | 2.5 | ink, 0.6 mm |
| Protective earth | PE | GN/YE | 2.5 | insulation colour |
| 230 V AC L, 230 V AC N | none | BN, BU | 1.5 | insulation colour |
| 24 V DC, 0 V DC | 24V, 0V | BU | 0.5 | insulation colour |
| Signal, Analogue | none | none | 0.5 | ink |

**A wire's type** is its own (the wire's **Category** in Project ▸ Wires, or `category`), else the type whose
potential is its net's, else the project's or standard's default type, else none. A wire's own
colour and size override its type's; with none of its own it shows the type's, named in grey.
Choosing a type on a net with no potential also sets the type's potential; a net with another
potential keeps it.

The Wire tool asks `[Potential/Category/Join]`; **Category** chooses the type for new wires for
that run of the tool.

Edit the types in **Settings ▸ Standards ▸ Wire types** like any style. **Cable types** work the
same way and can name a core colour scheme (`harmonised`, `numbered`, `din47100`, `icea_k2`); see
[cables.md](cables.md).

Typed: `styles {"catalogue":"wire"}`, `setstyle {"catalogue":"wire","id":"dc24","fields":{"colours":"RD"}}`,
`category {"conductor":"<id>","value":"Signal"}`, `category {"net":"<id of a wire on the net>","value":null}`,
`wire {"points":[[10,150],[60,150]],"category":"24 V DC"}`.

## Project defaults in Pages and sheets

**Settings ▸ Pages and sheets** (project menu ▸ **Project settings**) holds the project's own rows:
the sheet and its grid, the reference style, contact tables, tags (digits, case, **Leading - on
tags**, prefixes), revisions (pending changes marker and the revision table's columns, see
[releases.md](releases.md)), numbering (**Provisional tags until Finalise**) and wires (colour code
style `BN BU GN/YE`, `BRN BLU GRN/YEL` or `Brown Blue Green/Yellow`, and **Label height**). A second
tab holds **Page types**, a third **Text styles**.

Turning off **Leading - on tags** rewrites every device and cable tag in one undoable step. Case
**As typed** keeps the letters of tags typed from then on; existing tags stay as they are.

Typed: `projectprops {"tag_dash":false}`, `projectprops {"tag_case":"typed"}`,
`tagsettings {}` (tag padding and each symbol's prefix).

## Limits

- A device's own text style changes the height, weight and colour of its texts; the font stays the
  kind's project style's.
- An agent cannot run `standard {"from":…}`: it names another file.
- A change to a wire type that is a potential's default marks every page to redraw; only the page
  on screen redraws at once.
- A wire type chosen in the Wire tool lasts for that run of the tool.
- Named standards other than IEC (US, one from a library file) are not available yet.
