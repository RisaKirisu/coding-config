# AGENTS.md

## Agent skills

### Issue tracker

Issues and specs are tracked as local Markdown under `.scratch/`. See `docs/agents/issue-tracker.md`.

### Triage labels

The five canonical triage roles use their default label strings. See `docs/agents/triage-labels.md`.

### Domain docs

This repository uses a single-context domain-doc layout. See `docs/agents/domain.md`.

### Runtime verification

Changes to DevVM or DSH Runtime lifecycle code must cover non-interactive execution, stop-and-relaunch behavior, DSH Status read back from the DevVM after a simulated daemon restart, and Project Log updates.

The DSH Runtime is started, stopped, and probed with three `devvm exec` snippets; the daemon holds no child process. Each Project has one host log directory, `<DEVVM_ROOT>/.project-logs/<project-id>/`, with `daemon.log` (daemon), `dsh.log` and `ingress.log` (guest), every line prefixed with an ISO-8601 UTC millisecond timestamp.

Lifecycle test fakes must isolate guest PID files and never evaluate commands against `/tmp/devvm-daemon-dsh.pid` on the host: the mock `devvm` rewrites the guest paths (`/tmp/devvm-daemon-dsh.pid`, `/devvm-root/.project-logs`, `/run/devvm`, `/root/workspace`, `/root/.dsh`) into the test's own directories before running the snippet with real `bash`. When changing that isolation, run the suite with a guard that verifies the hosting DSH PID remains alive and unchanged.

### Session Sync

The DSH plugin at `root/.dsh/plugins/remote-sync/` is the only Session Sync engine (ADR 0003); the daemon never runs rsync. Plugin tests (`node --test root/.dsh/plugins/remote-sync/test.mjs`) use real rsync over the local transport (no `ssh_host`). First-party plugins in the web profile use pnpm `link:` dependencies, so `node_modules` resolves directly to `root/.dsh/plugins/`; never replace them with `file:` dependencies, whose hard-linked files can become stale after atomic source-file replacement.

### Build Loop

The DSH plugin at `root/.dsh/plugins/build-loop/` provides the `build_ticket` tool (build agent, then parallel review and test-audit agents, findings fed back to the same build agent for at most `maxFixRounds` rounds, all reports returned verbatim) and its **Settings → Build Loop** page. Prompts and flow live in the `build-loop` settings namespace; defaults in `prompts.mjs`. Tests: `node --test root/.dsh/plugins/build-loop/test.mjs` (pure helpers only; no mocks). Orchestrators should dispatch ticket implementation through `build_ticket`, not a plain subagent.

### Updating a local DSH plugin

Local plugins (`root/.dsh/plugins/*`, mounted in the DevVM at `/root/.dsh/plugins/*`) are installed into the web profile as hardlinked copies: pnpm rewrites `link:` to `file:` when the package declares `peerDependencies`, and `nodeLinker: hoisted` materializes copies, so an atomic file replacement leaves `~/.dsh/profiles/web/node_modules/<pkg>/` on the old inode. After editing plugin source, inside the DevVM:

1. Run the plugin's own tests (`node --test /root/.dsh/plugins/<dir>/test.mjs`); they must not need peer dependencies.
2. If you added a module, list it in the plugin's `package.json` `files` and `exports`.
3. Refresh the profile copy: `cd ~/.dsh/profiles/web && rm -rf node_modules/<pkg> && CI=true DSH_HOME=/root/.dsh dsh plugin --profile web install --store-dir /root/workspace/.pnpm-store/v11`. The `rm -rf` is required (a plain `install` sees the package present and does nothing); `--store-dir` is required because the profile was linked from that store and pnpm refuses to mix stores.
4. Verify with `cmp` between each source file and its `node_modules` copy.
5. Restart the DSH Runtime (`devvm` stop/start). Host bundles are imported once; there is no HMR for them, and `client.js` is served from the profile copy, so a browser refresh alone is not enough.

Plugin defaults that are also exposed as settings (for example `build-loop` personas) are shadowed by any value saved in `~/.dsh/settings.yaml`; reset the field from its settings page to pick up a new default.
