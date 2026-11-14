# UI rev 3, board 1.01: the frame

What the UI rev 3 pilot changed, and how to try it by clicking. The build spec is
`docs/discussions/ui-2026-11/rev3-build-spec.md`. Folded into `docs/quick-start.md` at the
phase-end tidy-up.

## What changed

The window has one bar at each edge, and the drawing comes first. The default view is board 1.01,
**Draw with both sides closed**.

- **Top bar, 40 px.**
  - At the far left: the project name pill. Its menu holds New, Open, Save, Save As, Close, the
    project popouts, Settings and Standards.
  - Beside it: the workspace switch, **Draw / Project / Library / Publish**. **Project** is
    the old Lists, renamed; it works as before.
  - Centred: search (Ctrl K).
  - At the right: the agent icon, **Share** (coming: it does nothing yet), the light/dark switch
    (only dark works for now), help and the account avatar.
  - No branding and no revision pill.
- **Tool strip, 56 px, at the left edge.**
  - At the top: the **Book**, **Devices** and **History** panel buttons. Each opens its panel
    beside the strip, and pressing it again closes the panel. The panel's own close button does
    the same.
  - Then the tool groups: Select, Draw, Text | Device, Terminal, PLC | Wire, Cable | Location.
    PLC has no tools yet, so it is greyed.
  - At the foot: the **Snap**, **Grid** and **Ortho** toggles.
  - Hovering a group still lists its tools. Rev 3's own flyouts come later.
- **Inspector: the left side's mirror.** A permanent 56 px rail at the right edge has one labelled
  button each for **Device**, **Layout** and **Checks**; Checks carries a red dot when there are
  problems. Pressing one opens the inspector beside the rail on that tab; pressing it again
  closes it, as Book does on the strip. The rail's buttons are the tabs: the panel has no tab
  selector. `]` or the panel's close button closes it too. For now Device and Layout show today's
  properties, and Checks shows the check list that was the drawer's.
- **Bottom bar, 32 px.**
  - Status at the left:
    - the save state and time;
    - **Rev C D\***, which opens History;
    - the checks counter, which opens the Checks tab.
  - The page well in the centre:
    - the page shown, number, "/ total" and title, at its centre between `« ‹` and `› »`, with a
      divider either side;
    - left of it, the groups before (folded) and the open group's page buttons;
    - right of it, the groups after (folded to a folder, a count and a ›) and **+**, which adds a
      page;
    - right-click a page button for the same wheel a page in Book gets.
  - The view at the right:
    - **IEC**, which opens Settings ▸ Standards;
    - the sheet size;
    - the sheet colour menu (Paper or Dark; Follow waits for the light theme);
    - **Fit**, zoom out, zoom % and zoom in.
- **A page first opens with its sheet fitted**: the frame and title block are visible, with a
  24 px margin. Brief 040 opened it at a readable zoom instead.
- **Keys:** V D T W C start the Select, Draw, Text, Wire and Cable groups. P and B are gone: the
  Device and Location groups have no key. `]` collapses or expands the inspector.
- The intent line now sits at the top of the canvas and shows only while a tool prompts or a
  refusal waits. It no longer shows a hint at rest.

## Try it

1. Run the trial at 1920 × 1080 (or full screen) and open a project with a few pages.
2. **Owner check.** The window should look like board 1.01: the strip at the left, the rail at
   the right, the sheet fitted between them, one bar top and bottom.
3. Click **Book** on the strip. The page list opens beside the strip and the drawing moves over.
   Click **Book** again to close it. **Devices** and **History** behave the same.
4. Click a page number in the bottom bar's well, then the ‹ and › arrows. Each shows its page.
5. Click **Checks** on the rail, or the checks counter in the bottom bar. The inspector opens on
   the check list. Press `]` with the pointer over the page to fold it.
6. Click **Fit**, then **−** and **+**. The zoom % follows.
7. Click **Rev** in the bottom bar. History opens beside the strip.
8. Switch to **Project**. There is no strip or panel, and the bottom bar keeps only its status.

## Limitations

- Pinning and floating panels, the inspector's own tab contents, rev 3's flyouts, the toggles'
  popouts, the page well's hover preview and the picker are later rev 3 sections.
- The saved time is the time the project became saved in this session. A project opened already
  saved shows "Saved" with no time until it is saved again.
- The avatar shows the first letters of the computer's login name. There are no accounts yet.
