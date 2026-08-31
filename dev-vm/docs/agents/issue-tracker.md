# Issue tracker: Local Markdown

Issues and specs for this repo live as Markdown files in `.scratch/`.

## Conventions

- One feature per directory: `.scratch/<feature-slug>/`
- The spec is `.scratch/<feature-slug>/spec.md`
- Implementation issues are one file per ticket at `.scratch/<feature-slug>/issues/<NN>-<slug>.md`, numbered from `01`, never a single combined tickets file
- Comments and conversation history append to the bottom of the file under a `## Comments` heading

## When a skill says "publish to the issue tracker"

Create a new file under `.scratch/<feature-slug>/`, creating the directory if needed.

## When a skill says "fetch the relevant ticket"

Read the file at the referenced path. The user will normally pass the path or issue number directly.

## Wayfinding operations

Used by `/wayfinder`. The map is a file with one child file per ticket.

- **Map:** `.scratch/<effort>/map.md`
- **Child ticket:** `.scratch/<effort>/issues/NN-<slug>.md`, numbered from `01`
- **Type:** `research`, `prototype`, `grilling`, or `task`
- **Status:** `claimed` or `resolved`
- **Blocking:** a `Blocked by: NN, NN` line near the top
- **Frontier:** first numbered open, unblocked, unclaimed ticket
- **Claim:** set `Status: claimed` before starting work
- **Resolve:** append the result under `## Answer`, set `Status: resolved`, then add a context pointer to the map
