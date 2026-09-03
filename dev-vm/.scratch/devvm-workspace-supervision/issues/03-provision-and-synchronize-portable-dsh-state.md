# 03: Provision and synchronize Portable DSH State

**What to build:** Deliver optional Project-specific Session Sync from VM-local persistence to a VPS Sync Store, including setup, automatic sync, status, retries, startup reconciliation, and separated remote deletion. The DSH plugin is the single Session Sync engine (ADR 0003); the Control Daemon provisions, sequences the launch, deletes, and shows status.

**Blocked by:** 01: Build the Control Daemon and manage Project runtimes

**Status:** resolved

- [x] Local DevVM and DSH use works when synchronization is not configured.
- [x] A one-time VPS setup (central UI or `sync setup` CLI) configures one shared SSH/rsync credential, the remote Project-ID directory root, and verifies round-trip access from the host. Both entry points provision the guest configuration and copy the key into the DevVM.
- [x] Sync Store data is grouped by Project ID and does not depend on workstation paths.
- [x] Rsync runs inside the DevVM and reads VM-local Portable DSH State.
- [x] Synchronization includes session logs, authoritative attachment objects, workspace registry data, and message-feedback data when present.
- [x] Synchronization excludes projection cache data and workstation-wide DSH configuration.
- [x] The Control Daemon transfers nothing itself and has no manual sync trigger. Its central UI shows Sync Status read from the DevVM and offers a DSH Runtime restart.
- [x] The DSH indicator is green when synchronized, gray when not configured, animated while synchronizing, yellow when failed, degraded, or remote ahead, and clickable for manual retry in the yellow states. Its initial state comes from the plugin, never a hard-coded value.
- [x] Completed DSH turns start Session Sync only after DSH finishes saving the session.
- [x] Saved workspace and message-feedback changes start Session Sync, while projection-cache changes do not.
- [x] Only one synchronization runs at a time; triggers arriving during a transfer cause one follow-up transfer, whether the running transfer succeeds or fails.
- [x] A failed transfer retries after one second for five total attempts, then remains failed until the next completed turn, relevant saved change, or manual retry in DSH.
- [x] Session Sync tracks its position with one head sequence (ADR 0005): a push confirms the Sync Store sequence matches before advancing it, and a mismatch marks the Project remote ahead.
- [x] While remote ahead, DSH shows a persistent banner directing the user to restart the DSH Runtime from the Control Daemon page, keeps pushing session logs and attachment objects, and holds `storages/` pushes.
- [x] DSH startup reconciliation runs as a guest startup script ahead of `dsh web`: it pushes then pulls with `--update` when this workstation last exchanged with the Sync Store, pulls `storages/` remote-wins when the Sync Store is ahead, records Degraded Sync when the Sync Store is unreachable, records failed when its push fails, and never blocks the launch.
- [x] Portable DSH State is pulled only while no DSH Runtime is running (ADR 0004); the Control Daemon refuses to launch a second DSH Runtime for a running Project.
- [x] Version one relies on the Single Writer Rule without leases, conflict detection, or merge behavior.
- [x] Sync Store deletion is separate from Unregister and local DevVM deletion and requires confirmation.
- [x] Synchronization actions and failures are written to Project Logs, including the startup script's output.
- [x] Plugin tests compose real Cordis, SessionStore, JSONL persistence, and storage-domain services in temporary DSH state, and use real local rsync against a temporary Sync Store to verify saved-change events, save ordering, synchronized categories, the head protocol, remote-ahead holds, startup reconciliation branches, profile isolation, and Web route activation without hand-built mocks.

## Comments

Reopened after a whole-code review found two Session Sync engines (daemon and plugin) sharing a dirty marker, which lost changes saved mid-transfer. The design was consolidated into the plugin; see ADRs 0003, 0004, and 0005 and the amendment to ADR 0001. Acceptance criteria that named the dirty marker, the daemon-side manual trigger, or blocking startup were replaced above.

Resolved: the daemon engine was deleted (`src/sync.rs` keeps provisioning, setup-time verify, Sync Store deletion, and status reading), the plugin gained the head protocol, `reconcile.mjs`, the `remote_ahead` banner, and manual retry; `scripts/devvm-sync-startup` runs ahead of `dsh web`. Not exercised by the automated suites: the ssh transport against a real Sync Store, the rendered client UI, and the Docker image build. The `#[ignore]`d live acceptance test covers the first when a VPS is available.
