# @devvm/dsh-subagent-manager

Persistent DSH bundle plugin providing:

- A **Subagent Model** settings page for provider, model, and reasoning effort.
- Host-side `agent/request` routing only for sessions whose durable header has `origin: subagent`; ordinary user forks are never overridden.
- A `subagent_wait` tool that waits for a background subagent or job and returns status only.
- Durable configuration in the standard DSH `settings.yaml` document under `subagent-model`.

## Bundle layout

- `index.mjs` — Host plugin.
- `client.js` — Web client settings page.
- `cordis.patch.yml` — Bundle composition patch.

## Installation

Add this package to a DSH profile as a local dependency and include `@devvm/dsh-subagent-manager` in `dsh.profile.bundles`.
