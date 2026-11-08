# ECAD features

A **feature** is a whole user-facing capability, developed on one long-lived branch
`feature/<name>` (owner decision, 2026-11-08). Briefs are steps inside a feature: they land as
commits on the feature branch, not as PRs of their own. The feature reaches `dev` by one PR when
it works; `dev` reaches `main` through the QA gate (see `CLAUDE.md`, "Branches and testing").

Up to brief 016 the work was done one PR per brief. This table maps those briefs onto features so
that new briefs know which branch they belong to. The owner corrects it when a grouping is wrong.

| Feature | Branch (from now on) | Briefs so far | State |
| --- | --- | --- | --- |
| Platform: Windows and Linux, upstream sync | `feature/platform` | 000, 001, 002, 003, 007 | In `dev` |
| Cables: slash, views, look, cores, references | `feature/cables` | 004, 005, 009, 010 | 004–009 in `dev`; 010 in PR #28 |
| Usability: ECAD tools by mouse | `feature/usability` | 008 | In `dev` |
| Agent access (MCP) | `feature/agent-access` | 011 | PR #27 |
| Performance and the project file | `feature/performance` | 012, 013, 014, 015 | 012, 013, 015 in `dev`; 014 in PR #32 |
| Libraries and components | `feature/components` | 006 (contract only) | Stage 3 not started |
| Project manager: pages, then cables, components | `feature/project-manager` | 016 | Brief approved, design next |

A feature branch stays open while the feature is being shaped. When a later brief extends a feature
that already reached `dev`, branch `feature/<name>` again from `dev`.
