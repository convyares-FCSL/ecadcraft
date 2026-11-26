# Company standard file and more defaults as data

Status: Draft, not started; waits for libraries (`.ecadlib`, see the [roadmap](../roadmap.md))

## Company standard file

Loading a house standard from a TOML file into the project's standard, as one undoable edit with the
differences shown first. It waits for libraries, where a standard also comes from a `.ecadlib`, and reuses the
defaults loader ([design/defaults.md](../design/defaults.md)).

## More defaults as data

Each its own piece of work: the house title block's columns and rows, cover page sizes, the terminal strip
drawing look, drawing line weights (with plot styles).
