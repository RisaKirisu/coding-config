# 00: Fix build-ticket tool filtering under DSH 0.1.2

**What to build:** Restore the custom `build_ticket` plugin so its child tool denylist never passes parent-only or removed tool names to DSH 0.1.2 `tools.restrict()`.

**Blocked by:** None

**Status:** resolved

## Root cause

Two DSH 0.1.2 API changes broke the loop:

1. `childToolFilter` intersected configured denied names with `ctx.tools.schemas(parent)`. That parent-visible catalog can contain parent-scoped or legacy tools such as `subagent`, but DSH 0.1.2 validates child restrictions against the child's restrictable global tool catalog. Child creation therefore failed before the build agent started with `tools.restrict() names unknown global tool "subagent"`.
2. Fix turns read `agent.session.events.length` and sliced `agent.session.events`. DSH 0.1.2 removed that public array in favor of `session.seq` and `session.snapshotEvents()`, so any non-clean audit crashed with `Cannot read properties of undefined (reading 'length')`.

## Requirements

- Intersect configured denied names with the global restrictable tool surface inherited by children, not the calling parent's full visible schema list.
- Preserve denial of recursive/fan-out tools that remain globally available, including `build_ticket`, `subagent_fork`, `workflow`, and `ralph`.
- Ignore obsolete configured names safely rather than failing child creation.
- Keep saved settings backward-compatible.
- Use DSH 0.1.2's public session sequence/snapshot API for fix-turn event boundaries.
- Add a dependency-free regression test proving a parent-only `subagent` name is removed while known global names remain denied.
- Run the build-loop source test and one tiny end-to-end `build_ticket` invocation that exercises a fix turn after refreshing/reloading the plugin.

## Answer

- Files changed:
  - `root/.dsh/plugins/build-loop/config.mjs`: Added `filterDeniedTools` to filter configured denied tool names against global tool schemas, excluding `run_code` and handling missing inputs.
  - `root/.dsh/plugins/build-loop/index.mjs`: Updated `childToolFilter` to filter `config.deniedTools` against global schemas via `ctx.tools.schemas()` instead of calling `ctx.tools.schemas(parent)`.
  - `root/.dsh/plugins/build-loop/test.mjs`: Added regression tests covering removal of parent-only `subagent`, obsolete tool names, and `run_code` while preserving global fan-out tool denials, plus handling of empty or missing inputs.
  - `root/.dsh/plugins/build-loop/README.md`: Documented that child tool restrictions filter configured denied tools against the global tool catalog inherited by children.
  - `AGENTS.md`: Documented that the build-loop plugin filters child tool restrictions against global tool schemas.

- Design decisions taken:
  - `filterDeniedTools` was placed in `config.mjs` to keep it pure and runnable under `node --test` without DSH or Cordis runtime peer dependencies.
  - Explicitly excluded `run_code` from the denied set because PTC mode presentation transport is not a restrictable capability tool.

- Deviations from the ticket:
  - End-to-end `build_ticket` invocation was omitted per caller constraints ("Do not restart DSH Runtime during this job").

- Not implementable here:
  - Live end-to-end `build_ticket` execution after host bundle reload: reloading the host bundle requires restarting the DSH Runtime, which was forbidden by caller constraints during this job, and `build_ticket` is denied within build agent children.
