# Remote Sync (`@devvm/dsh-remote-sync`)

DSH plugin and installable bundle for Session Sync between a DevVM and its remote Sync Store.

## Overview

Remote Sync preserves project history and session state across workstations by synchronizing Portable DSH State over `rsync` via SSH. It implements:
- **Automatic Session Sync**: Starts after DSH finishes saving a completed turn, workspace registry change, or message-feedback change. Non-portable caches and projections are ignored.
- **Trigger Coalescing**: Concurrent sync triggers while a transfer is active are coalesced into a single follow-up sync pass.
- **Retry**: Retries after one second for five total attempts, then marks status as `failed` while retaining Dirty Local State.
- **Manual Trigger & Status API**: WebServer routes `/api/sync/status` and `/api/sync/trigger` for status polling and manual sync recovery.
- **Web Client UI Overlay**: Status indicator action registered in the conversation header actions slot (`conversation.session.header.actions`) showing live sync state.

## Package & Bundle Structure

```
remote-sync/
├── package.json        # Declares dsh.bundle (cordis.patch.yml) and dsh.client (web)
├── cordis.patch.yml    # Bundle patch layer inserting host plugin '@devvm/dsh-remote-sync'
├── index.mjs           # Host plugin entry point (RemoteSyncManager)
├── client.js           # Web client UI overlay (status indicator)
├── test.mjs            # Real integration and package-contract checks
└── README.md           # Documentation
```

### Manifest Declarations (`package.json`)

- **Bundle Manifest**:
  ```json
  "dsh": {
    "bundle": {
      "patch": "./cordis.patch.yml"
    },
    "client": {
      "platform": "web"
    }
  }
  ```
- **Module Exports**:
  ```json
  "exports": {
    ".": "./index.mjs",
    "./client": "./client.js",
    "./cordis.patch.yml": "./cordis.patch.yml",
    "./package.json": "./package.json"
  }
  ```

### Bundle Patch (`cordis.patch.yml`)

The bundle patch contributes the host plugin layer when installed into a DSH profile:
```yaml
- insert:
    - id: remote-sync
      name: '@devvm/dsh-remote-sync'
```

## Installation

### Add to a Profile

From the workspace root, install the bundle into a target DSH profile (e.g. `web`):

```sh
dsh plugin --profile web add ./root/.dsh/plugins/remote-sync
```

This updates the profile's `package.json` to include `@devvm/dsh-remote-sync` in `dependencies` and appends it to `dsh.profile.bundles`.

### Direct Profile Manifest Declaration

Add `@devvm/dsh-remote-sync` to `dsh.profile.bundles` in the profile's `package.json`:

```json
{
  "dependencies": {
    "@devvm/dsh-remote-sync": "link:../../plugins/remote-sync"
  },
  "dsh": {
    "profile": {
      "bundles": [
        "@deepseek-ai/dsh-base",
        "@deepseek-ai/dsh-web-app",
        "@devvm/dsh-remote-sync"
      ]
    }
  }
}
```

## Configuration & Environment

- `DSH_HOME`: Root path for DSH state (defaults to `~/.dsh`).
- `DEVVM_SYNC_CONFIG_PATH`: Path to sync JSON configuration (defaults to `~/.config/devvm/sync.json`).
- `DEVVM_PROJECT_ID`: Current workspace Project ID (or reads from `.devvm-id`).
- `DEVVM_WORKSPACE`: Current workspace path.

## Status Indicators

| Indicator | Status | Description |
|-----------|--------|-------------|
| `● Synced` | `synchronized` | Portable DSH State is clean and synchronized with Sync Store. |
| `◌ Syncing...` | `synchronizing` | Rsync transfer is actively in flight. |
| `▲ Degraded` | `degraded` | Local state has uncommitted changes not yet confirmed in Sync Store. |
| `✕ Sync Failed (Click to retry)` | `failed` | Transfer failed after retries; local dirty state is preserved. |

## Running Tests

```sh
node --test root/.dsh/plugins/remote-sync/test.mjs
```
