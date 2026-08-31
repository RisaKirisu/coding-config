# 03: Provision and synchronize Portable DSH State

**What to build:** Deliver optional Project-specific Session Sync from VM-local persistence to a VPS Sync Store, including setup, automatic and manual sync, status controls, retries, reconciliation, and separated remote deletion.

**Blocked by:** 01: Build the Control Daemon and manage Project runtimes

**Status:** resolved

- [x] Local DevVM and DSH use works when synchronization is not configured.
- [x] A one-time VPS setup wizard configures one shared SSH/rsync credential, the remote Project-ID directory root, and verifies round-trip access.
- [x] Sync Store data is grouped by Project ID and does not depend on workstation paths.
- [x] Rsync runs inside the DevVM and reads VM-local Portable DSH State.
- [x] Synchronization includes session logs, authoritative attachment objects, workspace registry data, and message-feedback data when present.
- [x] Synchronization excludes projection cache data and workstation-wide DSH configuration.
- [x] A shared DSH host/client plugin provides manual synchronization and Sync Status in both the central UI and DSH.
- [x] The DSH indicator is green when synchronized, animated while synchronizing, yellow after failure, and clickable for manual retry.
- [x] Completed DSH turns start Session Sync only after DSH finishes saving the session.
- [x] Saved workspace and message-feedback changes start Session Sync, while projection-cache changes do not.
- [x] Only one synchronization runs at a time; triggers arriving during a transfer cause one follow-up transfer.
- [x] A failed transfer retries after one second for five total attempts, then remains failed until the next completed turn, relevant saved change, or manual retry.
- [x] Local state is marked dirty before background synchronization begins.
- [x] DSH startup pulls clean or fresh local state from the Sync Store and pushes Dirty Local State before pulling.
- [x] Existing local state may open with Degraded Sync when the VPS is unavailable.
- [x] DSH startup is blocked when synchronization is configured, the VPS is unreachable, and no local Portable DSH State exists.
- [x] Version one relies on the Single Writer Rule without leases, conflict detection, or merge behavior.
- [x] Sync Store deletion is separate from Unregister and local DevVM deletion and requires confirmation.
- [x] Synchronization actions and failures are written to Project Logs.
- [x] Plugin tests compose real Cordis, SessionStore, JSONL persistence, and storage-domain services in temporary DSH state, and use real local rsync to verify saved-change events, save ordering, synchronized categories, profile isolation, and Web route activation without hand-built mocks.
