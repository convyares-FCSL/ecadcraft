# Brief 011: agent access mcp

Part of the [quick start](../quick-start.md). A section number refers to a section of this file
when one is headed with it here, otherwise to the quick start's (1–10, 5A…).

## What this brief changed

**Branch `ecad/agent-mcp` (brief 011)**, cut from `ecad/cable-views` (PR #24) with `dev`
merged, adds `ecad-mcp`: an AI agent such as Claude Code can read and change a project through
MCP, headless (section 7A). Nothing outside `ecad/` changes, and the file format is unchanged. For
this branch there are automated checks only, written with the branch and run by the tester and
CI; nobody has connected Claude Code to it yet.
