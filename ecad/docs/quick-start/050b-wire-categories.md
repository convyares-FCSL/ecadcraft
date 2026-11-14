# Brief 050b: wire categories

Part of the [quick start](../quick-start.md). Builds on [050a-style-model.md](050a-style-model.md).

## What this brief changed

**Branch `feature/style-model` (brief 050b)**, after 050a.

| Where | What is there now |
| --- | --- |
| Wire categories | A fourth catalogue on Settings ▸ Standards: **Wire categories**. Built in: Power L, L1, L2, L3, Neutral, Protective earth, 24 V DC, 0 V DC, Signal, Analogue. Each has a potential it is the default for, a colour and cross-section (label, reports, BOM), a line colour (`wire` = the insulation colour, `ink`, or a code), line type and weight, a label rule (`project`, `net`, `each`, `none`) and BOM fields (part number, maker, description, allowance in metres per wire) |
| A wire's category | Its own (`category` command, or the inspector's **Wire category** row), else the category of its net's potential, else the project's or standard's `wire.category`, else none |
| Colour and cross-section | A wire's own `colour` and `cross_section` stay and override its category; with none of its own it shows the category's, named in grey (`0.5 (24 V DC)`) |
| Potentials table | Name, prefix and numbered only. `wiresettings` refuses colours and cross-sections ("set on wire categories now"); a project from before 050b keeps them in the file, ignored, with one Checks row until the table is written again |
| Wire tool | `Specify first point of wire or [Potential/Category/Join]`; **C** asks `Category for new wires [auto/…]` for this run of the tool. `wire {category}` in the JSON form |
| Choosing a category | On a net with no potential it also sets the category's potential; a net with another potential keeps it ("Category set; the net keeps potential L") |
| Reports | The wire report has a **Category** column; its colour and cross-section are the resolved ones. The BOM gains one metres row per part number, colour and cross-section of categories with a part number: wires × allowance, rounded up to 0.5 m, marked "estimate" |
| Checks | `category-unknown`, `category-potential-twice`, `wire-allowance` (a category on the BOM with no allowance), and the potential kind of `ignored-old-setting` |

**Differences on the page from `dev`** (asked for by the UI spec): power wires (L, L1–L3, N) are
drawn in ink at 0.6 mm instead of in their insulation colour; their label still says the colour.
24 V DC, 0 V DC and PE look as before.

Known limits: a change of a category that is a potential's default marks every page to redraw
(only the page on screen redraws at once); the wire report shows no marker for an overridden
value (the inspector does); the BOM wire rows are worked out when the BOM is read; a category
chosen in the Wire tool lasts for that run of the tool.

## Try it

1. Open the demo, **Settings ▸ Standards ▸ Wire categories**. Expected: ten rows; click **24 V
   DC**: its fields with their chains.
2. Set **Colour** to `RD`: the line says how many wires use 24 V DC and `Colour BU → RD`.
   **Apply**. Expected: 24 V wires turn red and their labels say `RD`.
3. Select one 24 V wire, type a colour `WH` in its Colour field. Expected: it stays white when the
   category changes again; clearing the field brings back `RD (24 V DC)`.
4. The drawing pack style: **New style from this** on Power L1, name `48 V AC`, set **Potential**
   `48V` and **Line colour** `RD`, Apply; then set Power L's **Line colour** to `BN`.
5. Give 24 V DC a **Part number** `W-05-BU` and **Allowance** `1.5`; **Lists ▸ Reports ▸ BOM**:
   one row in metres, "estimate".
6. Type `wire` at the command line, then `C`, `Signal`, and draw: the new wire is a Signal wire.

## Commands

```text
styles {"catalogue": "wire"}
setstyle {"catalogue": "wire", "id": "dc24", "fields": {"colour": "RD"}}
category {"conductor": "<id>", "value": "Signal"}
category {"net": "<id of a wire on the net>", "value": null}
wire {"points": [[10,150],[60,150]], "category": "24 V DC"}
```

## Checked here

Automated: `cargo test -p ecad-cadcraft style_contracts`, `-p ecad-mcp style_contracts`. Not owner-tried yet.
