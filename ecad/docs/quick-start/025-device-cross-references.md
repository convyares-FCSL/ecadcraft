# Brief 025: device cross references

Part of the [quick start](../quick-start.md). A section number refers to a section of this file
when one is headed with it here, otherwise to the quick start's (1–10, 5A…).

## What this brief changed

**Brief 025 (design `ecad/docs/design/025.md`), on `feature/cross-references`, not yet in `dev`:**
a contact shows where its coil is (`100-B-3`) and a coil shows a **contact table** under it: one
row per pole, power then auxiliary, with each pole's place, the poles the part offers that nobody
drew (grey, `—`) and those drawn beyond the offer (red, and a Checks warning). A device's
**Contacts offered** (`3P+1NO+1NC`) and **Contact table** placement are set in Properties; Move
Mirror Table moves the table; Go to Reference follows contacts and coils both ways. Cable
references now run on the same reference index with no visible change. Section 5L describes it.
- *Automated checks:* `ecad/crates/ecad-cadcraft/tests/all/xref_contracts.rs` (design 025 §10
  checks A–G, J, K, and the index against a rebuild after every commit in verify mode),
  `ecad/apps/ecad-mcp/tests/all/xref_contracts.rs` (the brief 011 rule), unit tests of the
  `contacts` grammar and `mirror` values in `ecad/crates/ecad-cadcraft/src/xref.rs`.
- *Agent-observed:* the worker looked at the headless screenshots `025-coil-table.png`,
  `025-contacts.png`, `025-props.png` and `025-checks.png`; no agent has clicked through the live
  window.
- *Owner-tried:* not yet.
