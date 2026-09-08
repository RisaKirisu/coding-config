# @devvm/dsh-subagent-manager

Persistent DSH bundle plugin providing:

- A **Subagent Model** settings page for provider, model, and reasoning effort.
- Host-side `agent/request` routing only for sessions whose durable header has `origin: subagent`; ordinary user forks are never overridden.
- A `subagent_wait` tool for subagent IDs only. Its optional `timeout` is in seconds and defaults to 300. It returns `completed` when the subagent becomes idle, or `running` when the timeout expires; a timeout does not cancel the subagent. Background jobs use `job_output(wait: true)` instead. When the tool is available, it injects guidance immediately after the jobs prompt permitting waits and repeated waits while foreground progress depends on a subagent.
- Durable configuration in the standard DSH `settings.yaml` document under `subagent-model`. Provider and reasoning fields are trimmed, but model IDs are preserved exactly because provider catalogs may contain leading whitespace.

## Bundle layout

- `index.mjs` — Host plugin.
- `client.js` — Web client settings page.
- `cordis.patch.yml` — Bundle composition patch.

## Installation

Add this package to a DSH profile as a local dependency and include `@devvm/dsh-subagent-manager` in `dsh.profile.bundles`.
