# Brief 031: text

Part of the [quick start](../quick-start.md). A section number refers to a section of this file
when one is headed with it here, otherwise to the quick start's (1–10, 5A…).

## What this brief changed

**Brief 031 (design `ecad/docs/design/031.md`), on `feature/text-styles`, not yet in `dev`,
steps Q1 (text styles), Q3 (show and hide) and Q2 (placement):** every derived text takes its size and weight from one style table per
project: tags 2.5 mm bold, connection point labels 1.5 mm, location labels 1.6 mm, continuation
names and references 2.0 mm, wire labels 1.6 mm (was 1.8). One project font (`BIZ UDPGothic` by
default) draws them when the build embeds craft-fonts or the machine has it; otherwise the stroke
font draws, and bold is a heavier line. **Project settings ▸ Text** sets the font and each kind's
height, bold and shown state in one transaction; `textstyle` does the same typed, and the
read-only `defaults` lists every registered default and where its value comes from. Section 5-031.
Tags, connection point labels, descriptions and part numbers (new, hidden by default) can be
hidden or shown per symbol, device, page or project (**Show/Hide Text**, Properties ▸ Texts,
`showtext`); a strip's tag prints once per run of adjacent terminals and a PLC device's once per
run of its channels; continuation names are hidden by default. **Show Hidden Text** draws hidden
texts dimmed, never printed. Tags (with any shown description and part number under them) are
placed beside their symbol where they cross no wire, symbol or other text; **Move Text** and
`movetext` store a place that follows the symbol; `text` with `justify` is recorded. The cable look's texts (core labels, label box) are drawn at the cable text height (1.6 mm) and
reference ends at the reference height (2.0 mm); a copied symbol keeps its own text settings.
- *Automated checks:* `ecad/crates/ecad-cadcraft/src/defaults/mod.rs` and `text_style.rs` (the
  registry, hostile values, levels), `ecad/crates/ecad-cadcraft/tests/all/text_style_contracts.rs`
  (drawn sizes and styles, redraw, a font change then an edit),
  `ecad/apps/ecad-mcp/tests/all/text_contracts.rs` (`textstyle`, `defaults`, the `text.` key
  refusals, writes off) and `ecad/apps/ecad-trial/tests/all/text_ui.rs` (the Text section).
- *Agent-observed:* the worker looked at the headless screenshot `031-text-section.png`; no agent
  has clicked through the live window.
- *Owner-tried:* not yet.

## 5-031. Text styles (by clicking)

Brief 031, design `ecad/docs/design/031.md`. Sizes and the font are one table per project.

**The font.** The default font, BIZ UDPGothic, comes from the craft-fonts repository and is built
in only when the build is told where it is. To see it, clone `storytold/craft-fonts` next to this
checkout and build with `CRAFT_FONTS_DIR` set to that folder (PowerShell:
`$env:CRAFT_FONTS_DIR = "C:\dev\craft-fonts"`; bash: `export CRAFT_FONTS_DIR=~/craft-fonts`).
Without it, texts are drawn in CADCraft's stroke font and bold tags are drawn with a heavier line;
nothing else changes, and the project's stored font is kept.

1. Place a `coil` (5F). Expected: its tag `-K01` is small and bold (2.5 mm) and its `A1`/`A2`
   are smaller still (1.5 mm).
2. **Project panel ▸ Project settings**, scroll to **Text**. The line under **Font** says `drawn
   with BIZ UDPGothic`, or, in yellow, why another font draws.
3. Set **Tag** to `3`, untick its **Bold**, OK. Expected: the tag redraws at once, larger and
   regular; one `u` puts both back. A height of `11` shows `A text height is 0.5 to 10 mm.` and
   OK is disabled. **Defaults** puts every row back (written only on OK).
4. **Once per run.** Place a `terminal`, then nine more of the same strip in a row 10 mm apart
   (**Place**, answer the device prompt with the strip). Expected: `-X01` is drawn once, over the
   leftmost terminal. A group of the same strip further along the line (more than 15 mm between
   boxes) prints it once more.
5. **Show/Hide Text** (ECAD tab ▸ **Text**): click a coil's tag. Expected: it disappears and the
   command line says `Tag -K01 hidden on this symbol.`; type `D` (Device), `P` (Page) or `PR`
   (Project) first to hide it wider. Click the symbol to show its hidden tag again. Hover the
   symbol: the lens lists `Tag -K01 (hidden here)`. Esc or Enter ends the tool.
6. **Show Hidden Text** (ECAD tab ▸ **Text**): hidden tags and labels are drawn dimmed (grey);
   click it again to stop. It records nothing and is never printed. Properties of a device, a
   symbol or a page has a **Texts** group (`Default (shown)`, Shown, Hidden, Once per run).
7. **Placement.** Place two `no` contacts 10 mm apart and draw a wire through where a tag sits:
   the tags move to the next free place (left top, left middle, right, above, below) and never
   overlap each other while a free place exists.
8. **Move Text** (ECAD tab ▸ **Text**): click a tag, then click where it goes (`J` first to type
   its alignment, e.g. `LT`). Expected: `Tag -K01 moved. Reset puts it back.` Move the symbol: the
   tag follows. Start Move Text again, type `R` (Reset) and click the tag: placed automatically
   again. `text {"at":[100,100],"text":"X","justify":"MC"}` now draws X centred (older builds
   cannot open a project with such a text).

Typed instead: `textstyle {}` (reads: the font, what draws it, every kind),
`textstyle {"kind":"tag","height":3,"weight":"regular"}`, `textstyle {"font":"Arial"}`,
`textstyle {"kind":"wire_label","height":2}` (the same value as **Wires ▸ Label height**),
`defaults {"group":"Text"}` (every registered default, its value and where it comes from).
`showtext {"kind":"tag","show":"off"}` (the project; add `"page"`, `"component"` or
`"representation"` for narrower), `showtext {"kind":"description","show":"on","component":"<id>"}`,
`hiddentext {}` (toggles), `pagetexts {}` (every derived text on the page, hidden ones too, with
`moved` and `overlaps`), `movetext {"representation":"<id>","kind":"tag","to":[130,170],"justify":"LT"}`
(`"reset":true` instead of `to` puts it back).
`projectprops`, `pageprops` and `setprop` refuse `text.` keys.
