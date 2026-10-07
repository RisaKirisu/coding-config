# Remote Sync

A push must check the remote head before transferring storage units and publish a new head only after all three transfer passes succeed. Transfer failures must preserve both heads and remain retryable. The final marker comparison must refuse a stale commit; transfers are not transactional or locked.

Keep the union, projection, and storage filters separate. Projection documents are whole-file checkpoints, not append-only logs.

Run tests against an isolated DSH Home, status file, and local Sync Store. Never use the production SSH configuration or `/run/devvm/sync-status.json` for tests. The integration suite requires `DSH_PACKAGE_ENTRY` pointing to the rc.2 runtime manifest and `DSH_HOME` pointing to the staged fixture home.
