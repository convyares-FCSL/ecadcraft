# 066: defaults as data (wire labels now 2.5 mm)

Lane 066 (brief 066, design 066) moves ECAD's built-in defaults out of the Rust code and into
data files built into the program (`crates/ecad-electrical/defaults/*.toml`, one per area). A
project that chose a value keeps it, even when it equals a built-in; a project that chose none
follows the built-in.

The first value changed as data is the **wire label cap height: 1.6 mm → 2.5 mm**.

Status: 066a (the data files) and 066b (the saved-file rule, F2, and the 2.5 mm change) are
built. 066c (the other project settings: tag padding, wire number digits, page number step, sheet,
reference style and report rows) is not; see the lane report.

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

Nothing else on a page moves: every other built-in has the value it had before.

## What did not change

- No new command. `defaults` lists the same values (now read from the data), and `setdefault`
  behaves as before. Clearing a chosen wire label height ("use default" in Standards, or
  `textstyle {"kind": "wire_label", "height": null}`) stores "not chosen".
- No company file is read from the machine: a project reads the same on any two machines.
- References, contact-table texts and the cable look keep their 2.5 mm text sizes (design 066
  §2.4; the reference text style says 2.0 mm, and which one wins is an open question for the
  owner).
