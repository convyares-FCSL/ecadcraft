# 066: defaults as data (wire labels now 2.5 mm)

Lane 066 (brief 066, design 066) moves ECAD's built-in defaults out of the Rust code and into
data files built into the program (`crates/ecad-electrical/defaults/*.toml`, one per area). A
project that chose a value keeps it, even when it equals a built-in; a project that chose none
follows the built-in.

The first value changed as data is the **wire label cap height: 1.6 mm → 2.5 mm**.

Status: built in three parts. 066a: the data files. 066b: the saved-file rule for the wire
label settings (F2) and the 2.5 mm change. 066c: the same rule for the other project settings
(F3): tag number digits, wire number digits, wire colour codes, page number step, sheet and
reference style. Their built-ins are ecad-model's own data (`crates/ecad-model/defaults/
settings.toml`) and nowhere else; the registry reads them from ecad-model. Report tables'
row and header heights are data as well, and the rows per A3 sheet (44) are computed from them.

Checked automatically (`cargo test -p ecad-cadcraft lane_066`); the click steps below have not
been tried by the owner yet.

## Try it

Run the program from `ecad/` (`cargo run --release -p ecad-desktop`).

1. **An existing project shows 2.5 mm wire labels.** Open a project saved before this lane that
   never set a wire label height (for example `C:\dev\ecad-projects\demo.ecad`). Its wire labels
   are now larger: 2.5 mm instead of 1.6 mm. Projects saved before 066 that set 1.6 on purpose
   wrote nothing to the file, so they cannot be told apart and also show 2.5 (accepted
   pre-alpha).
2. **A chosen 1.6 is kept.** Open Project settings, set the wire label height to **1.6** and press
   OK. The labels shrink to 1.6 mm. Save, close and open the project again: the labels are still
   1.6 mm, and Project settings still shows 1.6.
3. **A chosen 2.5 is kept as chosen.** Set the height to 2.5 and save: the file now records 2.5,
   so the project keeps 2.5 even if the built-in changes later.
4. **Other settings keep a chosen value too (066c).** In Project settings change the sheet (for
   example to A2) or the page number step, press OK, save, close and reopen: the choice is kept.
   A value equal to the built-in is kept as chosen as well; the dialog sends only what changed,
   so record one with the command line, for example `setdefault {"key": "tags.padding",
   "level": "project", "value": "2"}`.

Nothing else on a page moves: every other built-in has the value it had before.

## What did not change

- No new command. `defaults` lists the same values (now read from the data) plus five new
  entries: `tags.padding`, `wiring.digits`, `wiring.colour_style`, `page.number_step` and
  `print.reference_style`, which `setdefault` can set. Their built-ins, the tag dash and case,
  and the new-project sheet live only in ecad-model's `settings.toml`; everything else lives only
  in ecad-electrical's files (owner: no value in two places). Clearing a chosen wire label height ("use default" in Standards, or
  `textstyle {"kind": "wire_label", "height": null}`) stores "not chosen".
- No company file is read from the machine: a project reads the same on any two machines.
- References, contact-table texts and the cable look keep their 2.5 mm text sizes (design 066
  §2.4; the reference text style says 2.0 mm, and which one wins is an open question for the
  owner).

## 066d: drawn text follows its style

Every text drawn on a page now reads its text style (Settings ▸ Standards ▸ Text styles): change
a style's height, weight, font or colour and the texts of that kind move with it. Visible
changes: continuation references and cable reference ends are 2.5 mm (was 2.0); contact
cross-references, contact tables and PLC card texts take their style's font and weight (a bold
tag style now draws the PLC card tag bold); strip page terminal labels and locations use the
connection point label and location label styles; the house title block's page number and
other fields take their styles' weight; the cable look's label boxes and flags are laid out for
the cable text height (1.6 mm, so they are smaller). To try: open Standards, set the
`reference` text style to 4 mm, and see the cross-references under contacts grow.

