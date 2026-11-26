# Small code follow-ups

Status: Draft, not started; release not yet placed

- Consider moving the definition content hash (`definition_hash`, canonical JSON, today in ecad-file's
  `record.rs`) into ecad-model, so the electrical rules don't depend on the file format.
- Pass the branch seed in explicitly: `create_branch` (ecad-file's `project/branches.rs`) takes its entropy
  from `ecad_store::random_u128` inside it.

Deleting `apply_ops` and the `test-ops` feature is in [0.2.4](release-0.2.4.md) (#171).
