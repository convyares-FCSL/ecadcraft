# Brief 057: wire and cable rating checks

Part of the [quick start](../quick-start.md).

## What this brief changed

**Branch `feature/ratings` (brief 057, design 057).** Every panel wire and field cable that has
rating data is checked against the device that protects it, **Ib ≤ In ≤ Iz**, and a cable's volt
drop against its limit. Results are rows in the Checks list (the bottom counter, go-to and fix);
there is no calculator screen. Project ▸ Wires and Project ▸ Cables show a **Rating** section in
the right panel (boards 2.05 and 2.06).

**ECAD ships no IEC or BS 7671 table values.** The Table basis ("IEC table" on cables) reads a
**rating table** that you enter or import from a CSV you are licensed to use (Settings ▸
Standards ▸ Rating tables). Every number below is **made up for this guide**; none is a value
from a standard.

## Try it (made-up values)

Run `cargo run --release -p ecad-trial`, open a project with a breaker, a contactor and a cable
(the demo of the main guide works), and type at the command line:

1. **A panel wire on the Datasheet basis.** Give the Power L category a made-up datasheet rating:
   `setstyle {"catalogue":"wire","id":"l","fields":{"rated_current":"27","rated_at":"30"}}`.
   Set the panel temperature in Settings ▸ Standards ▸ Defaults ▸ Rating (40 °C is the default),
   and the breaker's rating in its properties (`4A C`). Open Project ▸ Wires and pick an L wire
   leaving the breaker: Rating shows `Rated 27 A at 30 °C`, `Panel × 0.87 40 °C`, `Bundled × 1.00
   not set` (warning) and `Carries 23.5 A  -Q01 4 A ✓`. Type `0.8` in Bundled and press Enter:
   `Carries 18.8 A`.
2. **The error and its fix.** Set the breaker's rating to `20 A`. Checks shows the error
   `… carries 18.8 A; -Q01 is 20 A`, the counter rises by one, and the wire's Status reads
   `underrated`. **Show rating** selects the wire. `u` takes it back.
3. **A rating table.** In Settings ▸ Standards ▸ Rating tables, make a table and enter made-up
   rows (`kind,…` header optional):
   ```
   capacity,E,pvc,3,1.5,20
   grouping,E,3,0.90
   resistance,5,1.5,10.0
   ```
   **Apply rows**, then choose it in Defaults ▸ Rating ▸ Rating table. Import CSV… and Export CSV
   take a file path.
4. **A field cable.** Pick -W01 in Project ▸ Cables (type `4G1.5`, length 12 m). Rating: basis
   **IEC table**, method `E · perforated tray`, ambient 35 °C (computed × 0.94 when the table has
   no ambient rows), grouping 3 (× 0.90): `Table 20 A`, `Carries 16.9 A`, `Volt drop 0.2 %  ≤ 5 % ✓`
   (at the breaker's 4 A when no load is set).
5. **Volt drop over.** Make a 24 V DC cable 60 m long with a 0.75 mm² class 5 cable type and
   `load 0.5`, and give the table `resistance,5,0.75,20.0` and `resistance,5,1.0,15.0`: the
   warning `… drops 6.0 % at 0.5 A; limit 5 % (1 mm² would drop 4.5 %)`. Raise its limit with
   `rating {"cable":"…","vd_limit":7}` and the row goes.

The same through commands and MCP: `rating {wire|cable, …}` (one transaction), `ratingtable
{name, source, csv}` (MCP passes `csv`; `import` and `export` are command line only) and
`ratings {wire|cable|{}}` (reads; allowed with writes off).

## What is checked automatically

`cargo test -p ecad-cadcraft lane_057` (the design's worked examples on synthetic values, the
parsers' hostile inputs, the CSV import refusals, no shipped values, the update map),
`cargo test -p ecad-mcp lane_057` (one agent transaction per write, refusals with writes off) and
`cargo test -p ecad-trial lane_057` (the Rating rows of boards 2.05 and 2.06; with
`ECAD_TESTER_SHOTS` set, screenshots).

## Not built yet

Fault protection (Zs, k²S²), volt drop from the origin and loads from Power (0.2.3), the parts
catalogue (2.3), the cable manager (2.7), Change cable type… filtered by size, and the `rating`
command's prompt sequence at the command line (its JSON form works).
