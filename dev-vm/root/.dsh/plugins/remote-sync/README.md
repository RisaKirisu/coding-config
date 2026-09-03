# Remote Sync (`@devvm/dsh-remote-sync`)

Remote Sync is the DSH plugin that carries one Project's Portable DSH State — its session
logs, storage units, and attachment objects — between the DevVM it runs in and the
Project's Sync Store. It is a DSH host plugin (`index.mjs`), a web client bundle
(`client.js`), and a standalone startup reconciliation script (`reconcile.mjs`).

The plugin is the only Session Sync engine: it owns the rsync filter sets, runs the
transfers, keeps the Sync Status, detects when the Sync Store has moved ahead, and
reconciles at launch. Nothing outside the DevVM transfers Portable DSH State.

## What is synchronized

Everything lives under DSH Home (`DSH_HOME`, default `~/.dsh`):

- `sessions/` — append-only session logs (`<root>/<project>/<session-id>/session.jsonl`).
- `attachments/v1/objects/**` — content-addressed, immutable attachment objects.
- `storages/*.json` — whole-document storage units, except `session_projcache.json`,
  which is a rebuildable cache.

Nothing else transfers. Credentials, settings, plugins, presets, profiles, derived
request images, and the Sync Store head marker are excluded, and no transfer ever
deletes: Session Sync propagates additions only.

## Status vocabulary

The plugin keeps one status, written to the VM-local `/run/devvm/sync-status.json` and
shown in the web UI:

| Status | Indicator | Meaning for the user |
| --- | --- | --- |
| `not_configured` | `○ Sync off` | No sync configuration exists; DSH runs purely locally. |
| `synchronizing` | `◌ Syncing…` | A transfer is in flight. |
| `synchronized` | `● Synced` | Local state and the Sync Store were exchanged successfully. |
| `remote_ahead` | `▲ Sync Store ahead — restart DSH` | Another workstation wrote to the Sync Store; restart the DSH Runtime to load its work. |
| `degraded` | `▲ Sync Store unreachable — click to retry` | The Sync Store could not be reached; local work continues. |
| `failed` | `✕ Sync failed — click to retry` | A transfer failed after five attempts; local work is intact but unpushed. |

## The head protocol

Session Sync tracks one position, `head_seq`: the Sync Store sequence this workstation
last exchanged. It is stored in `/run/devvm/sync-status.json` (override with the
`statusFilePath` option or `DEVVM_SYNC_STATUS_PATH`):

```json
{ "status": "synchronized", "head_seq": 7, "last_error": null, "updated_at": "2025-01-01T00:00:00.000Z" }
```

The Sync Store side keeps `<remote_sync_root>/<project-id>/.sync-head.json`:

```json
{ "seq": 7, "writer_id": "<per-workstation uuid>", "updated_at": "2025-01-01T00:00:00.000Z" }
```

A missing marker means `seq = 0`. The marker is excluded from every transfer and the status
file lives outside DSH Home entirely, and the status file is always written by temporary
file and rename, so a torn write cannot misreport the position.

A push compares and advances the marker in one remote shell invocation: it reads `seq`,
requires it to equal the local `head_seq`, writes `seq + 1` with this workstation's
`writer_id`, and only then transfers. If the sequences differ, the Sync Store is ahead:
the push becomes session-and-attachment only and the status becomes `remote_ahead`. That
is not a failure and is not retried. A `head_seq` of `null` — a workstation that has never
reconciled — is treated the same way, so a fresh registry can never overwrite the store's.

Any other error is retried up to five attempts one second apart before the status becomes
`failed` with `last_error`. A trigger arriving during an active transfer queues exactly one
follow-up, which runs whether the active transfer succeeded or failed.

## Two-pass transfer rules

Every direction runs two rsync passes with separate filter lists and no `--delete`:

1. **Union pass** — `sessions/***` and `attachments/v1/objects/***`, with
   `-az --update --append-verify`. `--update` skips files newer on the receiver, and
   `--append-verify` skips files that are the same size or longer on the receiver and
   otherwise appends. Session logs are append-only, so this bounds clock skew and
   interrupted writes and never accepts a shorter log.
2. **Storages pass** — `storages/*.json` minus `session_projcache.json`. Pushes and the
   equal-sequence pull use `-az --update` (newest wins). The pull taken while the Sync
   Store is ahead uses `-az` without `--update`, so the store wins regardless of
   modification time.

Storage units are never pushed while the Sync Store is ahead: a whole-document push of a
stale unit would drop the other workstation's session references from the store.

## Transports

Remote Sync speaks two transports through one seam:

- **SSH** when `ssh_host` is set. Remote shell commands run as
  `ssh -p <port> -i <key> -o StrictHostKeyChecking=accept-new -o BatchMode=yes -o ConnectTimeout=10 user@host '<script>'`,
  and rsync uses the same options through `-e` with a `user@host:<path>` endpoint.
- **Local directory** when `ssh_host` is absent or empty. `remote_sync_root` is then a
  local path, remote shell commands run through `sh -c`, and rsync endpoints are plain
  paths. This is a first-class transport, used for a Sync Store on an attached volume and
  by the test suite.

## Configuration

The workstation-wide config lives at `DEVVM_SYNC_CONFIG_PATH` or
`~/.config/devvm/sync.json` and is shared by every Project:

| Key | Meaning |
| --- | --- |
| `remote_sync_root` | Sync Store root; a Project uses `<remote_sync_root>/<project-id>/`. Required. |
| `ssh_user`, `ssh_host`, `ssh_port`, `ssh_key_path` | SSH endpoint; omit `ssh_host` for the local transport. |
| `writer_id` | Per-workstation UUID recorded in the Sync Store head marker. |
| `daemon_url` | Base URL of the Control Daemon, used for the client banner link. |

The Project ID comes from `DEVVM_PROJECT_ID`, otherwise from the first line of
`${DEVVM_WORKSPACE:-/root/workspace}/.devvm-id`. There is no fallback: if sync is
configured and no Project ID can be found, the status becomes `failed` with
`Project ID not found (.devvm-id missing)` and nothing is transferred.

## Triggers and routes

The plugin pushes after DSH reports a saved change: a completed turn (`turn/end`, after
the session log is flushed) and a saved `workspace` or `message_feedback` domain change.

The web server exposes:

- `GET /api/sync/status` — `{ status, head_seq, last_error, updated_at, project_id, daemon_url }`.
- `POST /api/sync/retry` — pushes when the status is `failed`, `degraded`, or
  `remote_ahead`; otherwise answers with the current status and transfers nothing.
- `POST /api/sync/check` — compares the Sync Store head with `head_seq` without
  transferring anything. An unreachable store leaves the status untouched.

## Web client

The client bundle registers two components and needs React, which the web client runtime
provides; without React it registers nothing.

- A status indicator in `conversation.session.header.actions`. It renders nothing until
  the first `/api/sync/status` answer arrives, then polls every three seconds. In the
  `failed`, `degraded`, and `remote_ahead` states it is clickable and posts
  `/api/sync/retry`; in every other state it is inert.
- A banner in the `shell.overlay` slot, shown only while the status is `remote_ahead`. It
  explains that another workstation has written to the Sync Store and links to the
  Project's card on the Control Daemon page when `daemon_url` is configured. It is fixed
  to the top of the window and takes pointer events only on itself, so the rest of the UI
  stays usable.

On window focus and on becoming visible again, the client posts `/api/sync/check`, at most
once every thirty seconds.

## Startup reconciliation

Pulling is safe only while no DSH Runtime holds the Project's state open, so it happens in
`reconcile.mjs`, which the DSH Runtime launch command runs before `dsh web`:

```sh
node /root/.dsh/plugins/remote-sync/reconcile.mjs
```

It reads the Sync Store head first and branches on it. When the store is unreachable it
records `degraded` and changes nothing. When the sequence equals `head_seq` this
workstation wrote last: it pushes both passes, then pulls both with `--update`. When the
sequence is higher — including a workstation with no `head_seq` at all — the store wrote
last: sessions and attachments still push and pull as a union, storage units are pulled
store-wins and never pushed. Either branch sets `head_seq` to the store's sequence and the
status to `synchronized`. The first failing step records `failed` and stops.

The script logs one line per step to stdout, errors to stderr, and **always exits 0**: a
Session Sync problem must never block the DSH Runtime launch. The status it leaves behind
is what the UI shows, and the plugin retries after the next saved change.

## Package layout

```
remote-sync/
├── package.json        # dsh.bundle (cordis.patch.yml) and dsh.client (web)
├── cordis.patch.yml    # bundle patch inserting host plugin '@devvm/dsh-remote-sync'
├── index.mjs           # host plugin: RemoteSyncManager, SyncStore, routes
├── reconcile.mjs       # standalone startup reconciliation, always exits 0
├── client.js           # web indicator and Sync Store banner
├── test.mjs            # integration tests over real rsync and real DSH services
└── README.md
```

Install it into a DSH profile from the workspace root:

```sh
dsh plugin --profile web add link:./root/.dsh/plugins/remote-sync
```

## Tests

The suite uses real rsync, real temporary directories, real child processes, and real DSH
persistence services; nothing about ssh, rsync, or DSH is mocked. Sync Store scenarios run
over the local transport with an injected zero retry delay.

```sh
node --test root/.dsh/plugins/remote-sync/test.mjs
```

One test boots `dsh --profile web` on port 3599 to check the routes and the client bundle,
so it needs the plugin installed in that profile.
