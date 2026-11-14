# 053: the performance batch (v0.2.1)

Nothing to click: the batch makes existing commands faster and changes no behaviour. The
large-tier rows it fixes are listed in its pull request.

## Timing one group of rows

A full tier run takes minutes. To time one group of rows on the large tier while working on it,
write the project once and measure the group on it (release build):

```sh
cd ecad
cargo build --release -p ecad-bench
target/release/ecad-bench --write large lean <folder>/large-lean.ecad
target/release/ecad-bench --part outputs <folder>/large-lean.ecad
```

- `--write <tier> <lean|churned> <file>` writes one generated project, as a tier run does.
- `--part <measure|outputs|strips|text|wires|plc> <file>` measures one group on a copy of it and
  prints the group's rows (the BOM rows are under `outputs`). A group takes seconds; `measure`,
  the main table, takes longest.

The numbers vary with whatever else the machine is doing: compare runs made side by side, and
take the smallest of a few.
