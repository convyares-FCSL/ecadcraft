# Versioned built-in defaults

Status: Draft, not started; in [the format release](release-0.2.7.md) (B6), an entry criterion of the format
freeze at 0.3.0

## Today

While ECAD is pre-alpha there is one built-in defaults set, edited freely, and every project reads the
current built-ins: a changed built-in moves every project that did not choose its own value. A stored value
that does not parse silently falls back to its built-in. How the defaults work now is in
[design/defaults.md](../design/defaults.md).

## What

- Numbered built-in sets that never change once released; a project names its set in its settings, recorded
  in the "New project" transaction.
- A lock file of released values, and a test that fails when one changes.
- A `usedefaults` command and a Standards line "Built-in defaults: set *n* (newest: *m*)", to move a project
  to newer built-ins, with a **Keep how it looks** option.
- A build that does not know a project's set refuses to open it.
- Unparsable stored values are shown as Checks rows instead of falling back silently.

## Open questions

- Does a set freeze when it reaches `dev`, or at a release?
- Is **Keep how it looks** on by default?
