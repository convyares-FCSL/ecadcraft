# Interaction: how a command is driven

Every ECAD action is a command. A command has one JSON form, and a command that a person drives
with the mouse also has an interactive form. This note says how the two relate and why. The user's
side is in [../user/drawing.md](../user/drawing.md); how to add a command is in
[../developer/commands.md](../developer/commands.md).

## Two forms, one result

- **The JSON form is the command.** It takes resolved values (IDs, coordinates, words), builds one
  transaction and commits it. `engine.execute`, the control channel, MCP, scripts and the app's own
  calls all run it, and it never prompts and never opens a dialog.
- **The interactive form only gathers values.** It is a prompt machine that resolves clicks, keys
  and typed points into the same values, then commits the command's JSON form with them. It adds
  no rule and writes nothing of its own.
- **Only a person's action starts it:** a menu item, a tool, the wheel, or the command's name typed
  at the command line. A program never gets a prompt.

Why: there is one code path for every caller. What a person does by clicking, an agent can do
with one JSON call, and a test that drives the JSON form tests the rule the person uses. Undo,
the agent gate and the history see the same transaction whoever made it.

## Where the forms live

- A generic command's registry entry can carry an interactive factory
  (`Entry::interactive` in `ecad-engine/src/generic.rs`); a session command's form lives in its
  feature's `interact` module in `ecad-engine` (`devices/`, `wires/`, `references/`,
  `terminal_strips/`, `styles/` and others).
- The forms that place and connect (add connection point, wire, connect, convert, the cable slash,
  core, fan and view forms) are in `ecad-engine/src/properties/forms/`.
- Each form uses CADCraft's prompt machine as it is: prompts, keywords, typed coordinates
  (`40,10`, `@30,0`), object snaps, ortho and polar, Enter, Space as Enter, Esc and right-click
  behave as in CADCraft's own drawing commands. ECAD adds no second prompt system.

## The principles every form follows

1. **One commit is one JSON transaction.** Each time a form commits, it commits exactly the
   transaction the JSON form would record for the values it resolved. A command that repeats
   until Enter commits once per object it places.
2. **Esc never changes what was not already committed.** Esc drops the pending input; the
   document, the head and the selection stay as they were before it. Objects committed earlier
   in the same repeating command stay, as segments drawn before Esc stay in a CAD line command.
   Undo then removes the last committed object in one step.
3. **Click first.** Every prompt can be answered with one click; typing is optional. No prompt
   asks for an ID or a handle.
4. **Capture, not guess.** Where a click means "this connection point" or "this wire end", the
   pointer is captured only within CADCraft's capture aperture of it (a setting in screen pixels),
   and the capture is drawn before the click. Outside the aperture nothing is captured and nothing
   is drawn to suggest it.
5. **Feedback in words.** After each commit the command line says what was made, in words, not
   JSON. IDs, where shown, are the full hex the debugger shows, so they can be pasted into a JSON
   line.
6. **Hostile input is refused at the prompt.** Typed values go through CADCraft's grammar; a value
   that is not finite or is out of range is refused with a message and the same prompt is asked
   again ("Point out of range.").

## When something goes wrong inside a form

ECAD's code is written never to panic. If a form's step still did, the engine's own guard would put
back the drawing from before the command, behind objects this run has already committed. ECAD's
forms guard themselves instead (`safely` and `Safe` in `ecad-engine/src/properties/forms/mod.rs`):

- a panic in a step redraws the page from the project and ends the command; the transactions
  already committed stay (roll forward) and the command line says so;
- a panic in a prompt or a preview gives a plain fallback prompt or no preview.

The project, not the drawing, is the truth; the page is always redrawn from it.

## Bounds on what a drawing may ask for

Values that come from a file or from typing are hostile until checked, and what they make the
engine draw is bounded too. One example: a two-colour wire draws its second colour as dashes, at
most 4,000 per wire and 20,000 per page (`cables/words.rs`, `MAX_DASHES`, `MAX_PAGE_DASHES`). A wire
past the budget keeps its first colour, and the Checks list shows one warning per page
(`dashes-left-out`). A file that asks for millions of dashes draws in one colour; it does not stop
the program.

## What a form does not do

- It does not decide anything the rules decide: refusals come from the command's rules in
  `ecad-electrical`, in the same words for every caller.
- It does not open a dialog for a value the JSON form takes as a default.
- It does not read the screen for meaning: a click resolves to an object through the project's
  handle map and connection points, never through a drawn shape's look.
