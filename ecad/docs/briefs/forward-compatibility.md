# Forward compatibility of stored words

Status: Draft, not started; decided in [the format release](release-0.2.7.md), an entry criterion of the
format freeze at 0.3.0

## Today

ECAD does not read files from newer builds or other tools. A word this build doesn't know (a style catalogue,
a role, a device kind, a settings word) is a read error: the project opens read-only, the message names the
word, the setting and where it is, and the file is never changed. Enums are strict; there is no "keep the
unknown word" variant.

## To decide for v1

- Whether a v1 build keeps and writes back words from a newer build, and how a person sees them.
- The **explicit import step** for files from other tools, which is where foreign words are mapped or
  refused.
