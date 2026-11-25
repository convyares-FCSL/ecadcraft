# ECAD docs

Every current doc, one line each. They describe ECAD as it is now. The history (briefs, design specs, quick
starts, discussions, reviews and plans, as written) is in the [archive](archive/README.md). Where a new doc
goes is in "Where docs go" in [`../CLAUDE.md`](../CLAUDE.md).

- [roadmap.md](roadmap.md): the plan, release by release, and the deferred work.

## User guides

- [Quick start](user/quick-start.md): install, open a project, draw a first circuit, save.
- [Workspace](user/workspace.md): the bars, panels and workspaces of the window.
- [Drawing](user/drawing.md): the tool strip, the wheel, picking, the keys, the canvas and the command line.
- [Projects](user/projects.md): open, save, locks, read-only files and project settings.
- [Pages](user/pages.md): page types, folders, numbering, front pages, frames and title blocks.
- [Devices](user/devices.md): devices, tags, numbering and renumbering.
- [Library](user/library.md): the built-in symbols, search and the symbol builder.
- [Wires](user/wires.md): wires, nets, wire numbers and joining.
- [Cables](user/cables.md): cables, cores, the slash, views and the cable manager.
- [References](user/references.md): device and cable references, continuation arrows.
- [Connectors](user/connectors.md): connector halves and pins.
- [Terminal strips](user/terminal-strips.md): terminal blocks and levels, accessories, jumper bars, strip pages.
- [PLC](user/plc.md): racks, cards, channels, signals and the I/O list.
- [Circuits](user/circuits.md): ratings, wire and cable sizing, volt drop and protection.
- [Locations and functions](user/locations.md): locations, functions and their boxes.
- [Styles](user/styles.md): text styles, wire types, catalogues and project defaults.
- [Properties](user/properties.md): the Properties panel.
- [Checks](user/checks.md): the Checks tab and what each check means.
- [Reports](user/reports.md): the BOM, lists and report pages.
- [History](user/history.md): history, versions, branches and merge.
- [Revisions and release](user/releases.md): revisions, issue, release and the pre-release check.
- [Publish](user/publish.md): Print Book and PDF.
- [Agent access](user/agent-access.md): what an agent may do, the banner, undoing an agent's session.

## Developer guides

- [Architecture](developer/architecture.md): crates and layers, the document model, the contracts.
- [Commands](developer/commands.md): generic and session commands, the runner, how to add one.
- [Adding a feature](developer/features.md): the folders a feature has in each crate, and its tests.
- [UI components](developer/ui-components.md): the components for values in panels and dialogs, and their gallery.
- [Testing and benches](developer/testing.md): the test layout, the slow set, `test-changed`, benches.
- [Building ECAD](developer/build.md): machine setup, build folders and profiles, Windows, Linux and WSL.
- [Guards](developer/guards.md): what each `cargo xtask` check prevents.
- [The debugger and the inspector](developer/debugger.md): the live debugger and the inspector page.

## For agents

- [ecad-mcp](agents/mcp.md): the MCP tools, the agent gate and authorship.
- [Control protocol](agents/control-protocol.md): ECAD's control port.
- [Command reference](agents/commands.md): how to get each command's parameters.

## Reference

- [Glossary](reference/glossary.md): the words used in everything a person sees, with their code names.
- [File format](reference/file-format.md): the `.ecad` file as built.
- [Design tokens](reference/tokens.md): every colour, size and line weight (read by the doc-sync tests).

## Design: decisions and why

- [Decisions](design/decisions.md): the binding decisions, by topic.
- [Source layout](design/layout.md): the binding source layout.
- Why each part works the way it does: [agent access](design/agent.md), [cables](design/cables.md),
  [defaults](design/defaults.md), [devices](design/devices.md), [history](design/history.md),
  [interaction](design/interaction.md), [nets](design/nets.md), [numbering](design/numbering.md),
  [object model](design/object-model.md), [references](design/references.md), [styles](design/styles.md),
  [terminal strips](design/terminal-strips.md), [UI](design/ui.md), [views](design/views.md).
