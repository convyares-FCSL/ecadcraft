# Small code follow-ups

Status: Draft, not started; release not yet placed

- Consider moving the definition content hash (`definition_hash`, canonical JSON, today in ecad-file's
  `record.rs`) into ecad-model, so the electrical rules don't depend on the file format.
- Pass the branch seed in explicitly: `create_branch` (ecad-file's `project/branches.rs`) takes its entropy
  from `ecad_store::random_u128` inside it.

- Build a cable's reference end on a second page with a command in the tests that pair two ends
  (`reference_end` in the engine, app and MCP test helpers), once a command draws a cable on two pages.
  The `test-ops` feature (`apply_ops`) stays for the hostile and stale edits tests commit on purpose.
