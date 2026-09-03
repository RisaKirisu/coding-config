# @devvm/dsh-build-loop

DSH bundle providing the `build_ticket` tool and a web settings page for it.

## Flow

1. A **build** child (persona: autonomous-building-protocol without the self-review step) implements the ticket and returns a report.
2. A **review** child (spec + repository standards + ponytail simplicity + correctness) and a **test** child (behavior-over-implementation, spec-over-pass-rate, no hand-rolled mocks, mutation testing) audit the result in parallel. Each returns a structured `{clean, findings, report}` verdict via `structured_output`.
3. Both clean → the tool returns all three reports verbatim with status `clean`. Otherwise the findings are sent to the **same** build child as a fix turn, and step 2 repeats.
4. After `maxFixRounds` (default 3) fix rounds the latest state is returned as `unresolved`. A child that ends abnormally returns `failed` with whatever reports exist. Nothing is summarized or softened.

Children are one-shot spawn children of the calling agent; they inherit its workspace, sandbox policy, and preset tool catalog minus the configured denylist (delegation, goals, plan mode, user questions). The build child stays live between fix rounds so it keeps its own context.

## Settings

Namespace `build-loop` in the standard `settings.yaml`, edited from **Settings → Build Loop**: the three personas, `maxFixRounds`, `provider`, and `deniedTools`. Values are read at the start of each `build_ticket` call. `DELETE /api/build-loop/config` (the "Reset all" button) drops every override.

Personas must not contain `{{...}}` groups: the prompt assembler interpolates them strictly.

## Files

- `index.mjs` — host plugin: tool, settings namespace, `/api/build-loop/config` routes, system-prompt section.
- `loop.mjs` — pure prompt composition and report rendering (unit-tested).
- `prompts.mjs` — default personas and flow constants.
- `config.mjs` — settings defaults and validation.
- `client.js` — web settings page.

## Tests

```sh
node --test root/.dsh/plugins/build-loop/test.mjs
```

## Installation

Add this package to a DSH profile as a local dependency and include `@devvm/dsh-build-loop` in `dsh.profile.bundles`. A new bundle requires a Host restart.
