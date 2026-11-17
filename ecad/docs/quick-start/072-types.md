# 072 Typed object model: lane A1 (foundation types)

What changed for someone using the app:

- **A project holding a word this version does not know opens read-only.** A setting stored as
  text but meant to be one of a fixed set of words (for example `text.tag.show`, which is `on`,
  `off` or `first`) can hold something else if the file was edited by hand or made by another
  program. Before, ECAD quietly read such a value as the default. Now the project opens
  read-only, and the open message names the word, the setting and where it is (the project, a
  page, a device, a wire, a cable, the settings or the standard). The file is kept as it is.
  Save is refused; **Save As** still saves a copy.
- A yes/no property in a file (`tag_fixed`, `numbering.provisional`, `wire_number_fixed`) must hold
  exactly `yes` or `no`. Commands still accept `true`, `1` and `on` (and `false`, `0`, `off`) as
  you type them, and always write `yes` or `no`.
- Nothing else changes in how the app looks or behaves.

## Try it

There is no click-only way to make such a file: it needs a file edited outside ECAD. The headless
test `lanes::lane_072a::unknown_words_open_read_only` (ecad-cadcraft) makes one and checks the
message, the refused save and the unchanged file. The inspector still shows such a file in its
Raw and Readable views (`lanes::lane_072a::a_project_with_an_unknown_word_is_still_shown`,
ecad-inspector).

Run them from `ecad/`:

```sh
cargo test -p ecad-cadcraft --test all lane_072a
cargo test -p ecad-inspector --test all lane_072a
```

These are automated checks only; nobody has tried this by hand yet.
