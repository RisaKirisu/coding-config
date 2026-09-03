# 08: Share DSH configuration at directory level

**What to build:** `/root/.dsh` inside every DevVM is the host-shared `root/.dsh/` directory itself, with `sessions/`, `storages/`, and `attachments/` covered by VM-local bind mounts. DSH's atomic saves then write through to the host, and Portable DSH State stays VM-local.

**Blocked by:** none

**Status:** resolved

**Why:** `link_root` in `devvm` builds `/root/.dsh` as a real directory with per-file symlinks into `/devvm-root/.dsh/`. DSH saves settings by writing a temp sibling and `rename()`ing it over the target; rename replaces a symlink instead of writing through it, so the first save after every VM start turns `settings.yaml` (and `.credentials.yaml`, `cordis.patch.yml`, `thinking-effort-loaded.json`) into a VM-local copy. Before the VM-local split of Portable DSH State, `/root/.dsh` was one directory symlink and renames stayed inside the shared directory. DSH behaviour has not changed (`dsh-settings-file` has used `writeFileAtomic` since 0.1.0-rc.7); the per-file links are the regression.

## Design

`link_root` in `devvm`, replacing the `.dsh` symlink loop:

1. `install -d -m 700 /var/lib/devvm-dsh/{sessions,storages,attachments}` (VM rootfs).
2. One-time migration: if `/root/.dsh` is not a mount point and any of `/root/.dsh/{sessions,storages,attachments}` is a non-empty real directory, move its contents into the matching `/var/lib/devvm-dsh/` directory. Then remove `/root/.dsh` if it is a symlink, and `install -d -m 700 /root/.dsh`.
3. `mountpoint -q /root/.dsh || mount --bind /devvm-root/.dsh /root/.dsh`.
4. For each of `sessions storages attachments`: `mountpoint -q /root/.dsh/$name || mount --bind /var/lib/devvm-dsh/$name /root/.dsh/$name`.
5. The `/devvm-root/.config/*` and `/devvm-root/*` loops stay as they are (their files are not rewritten by rename; revisit if that changes).

Host side: `root/.dsh/sessions/`, `root/.dsh/storages/`, `root/.dsh/attachments/` remain as empty mount points, each holding only `.gitkeep`. The stale pre-split data in `root/.dsh/sessions` and `root/.dsh/storages` (Aug 27) is deleted.

Sync status file moves from `/root/.dsh/.sync-status.json` to `/run/devvm/sync-status.json`: it is VM-local by nature, the plugin renames over it, and `/run/devvm` is already the guest's runtime directory. Change `STATUS_FILE_NAME`/path in `root/.dsh/plugins/remote-sync/index.mjs` (constructor option `statusFilePath` keeps tests in temp dirs), `read_status` in `src/sync.rs`, the plugin README, ADR 0005's file location sentence, and the `.sync-status.json` mention in `AGENTS.md` if any.

## Acceptance criteria

- [ ] `devvm start` on a fresh VM leaves four bind mounts: `/root/.dsh` and its three VM-local subdirectories (`mount | grep /root/.dsh`).
- [ ] A second `devvm start` is a no-op for the mounts (no duplicate mount entries).
- [ ] Changing a setting in DSH updates the host's `root/.dsh/settings.yaml` in place (same inode, new content).
- [ ] An existing VM keeps its sessions after upgrade: `/root/.dsh/sessions` inside the VM lists the pre-upgrade data, now stored under `/var/lib/devvm-dsh/sessions`.
- [ ] Host `root/.dsh/{sessions,storages,attachments}` contain only `.gitkeep` after any amount of VM use.
- [ ] Plugin writes `/run/devvm/sync-status.json`; daemon `read_status` reads that path; plugin tests and Rust tests updated and green.
- [ ] `bash -n devvm` passes; ADR 0005 and plugin README name the new status path.

## Verification requiring a real DevVM host

`devvm start`; `devvm exec mount | grep /root/.dsh`; open DSH, change a model setting; on host `stat -c '%i %y' root/.dsh/settings.yaml` before and after shows same inode, newer mtime, and `grep` finds the change; `devvm exec ls /root/.dsh/sessions` shows the VM-local tree.

## Answer

What changed:

- `devvm` `link_root`: the `/root/.dsh` per-file symlink loop is replaced by `install -d -m 700 /var/lib/devvm-dsh/{sessions,storages,attachments}`, a one-time `mv` migration of non-empty, non-mounted real subdirectories into that VM-local tree (only while `/root/.dsh` is not a mount point), removal of a `/root/.dsh` symlink, `install -d -m 700 /root/.dsh`, `mountpoint -q /root/.dsh || mount --bind /devvm-root/.dsh /root/.dsh`, and one guarded `mount --bind` per subdirectory. The `/root/.config`, `/devvm-root/*`, and pnpm profile blocks are unchanged.
- Host mount points: stale Aug 27 data under `root/.dsh/sessions` and `root/.dsh/storages` deleted, `root/.dsh/attachments` created; all three are mode 700 and hold only `.gitkeep`.
- `root/.dsh/plugins/remote-sync/index.mjs`: `STATUS_FILE_NAME` replaced by `DEFAULT_STATUS_FILE_PATH = '/run/devvm/sync-status.json'`, a `statusFilePath` constructor option (also accepted by `apply`, with a `DEVVM_SYNC_STATUS_PATH` env fallback so the `reconcile.mjs` subprocess test stays in a temp dir), and `mkdirSync(dirname(...))` before the atomic write. The rsync filter lists never named the status file (a catch-all `--exclude=*` covers it), so nothing was removed there.
- `root/.dsh/plugins/remote-sync/test.mjs`: fixtures and ad-hoc managers pass `statusFilePath` inside their temp DSH Home; the boot test passes `DEVVM_SYNC_STATUS_PATH`; `STATUS_FILE_NAME` is now a test-local `'sync-status.json'`. The Sync Store assertion that no status file transfers is kept under the new name.
- `root/.dsh/plugins/remote-sync/README.md`, `docs/adr/0005-...md`: name `/run/devvm/sync-status.json`. `reconcile.mjs` keeps the default and needed no change. No other doc mentioned the old path.
- `src/sync.rs` `read_status`: reads `/run/devvm/sync-status.json`. `tests/common/mod.rs` maps guest `/run/devvm` to `$PWD/.mock_run`, and the three status-file writes in `tests/sync_test.rs` plus the one in `tests/acceptance_workflow_test.rs` use that directory.

How verified here: `bash -n devvm` passes and the mode is still `-rwxr-xr-x`; `node --test root/.dsh/plugins/remote-sync/test.mjs` 19/19 pass with `/run/devvm` untouched; `cargo test --test sync_test` 4/4 pass; `cargo test --test acceptance_workflow_test` 1/1 passes; the `sync-status` grep shows no `/root/.dsh/.sync-status.json` or `<DSH_HOME>/.sync-status.json` mention; `find root/.dsh/{sessions,storages,attachments} -mindepth 1` lists only the three `.gitkeep` files; `DSH_HOME=/root/workspace/root/.dsh dsh plugin --profile web install --force` refreshed the pnpm copy, which now contains `DEFAULT_STATUS_FILE_PATH`.

Not verified here — needs a real DevVM host (`smolvm` is not installed on this machine):

- `devvm start` leaves four bind mounts (`/root/.dsh` plus its three VM-local subdirectories).
- A second `devvm start` adds no duplicate mount entries.
- A DSH setting change updates host `root/.dsh/settings.yaml` in place (same inode, new content).
- An existing VM keeps its sessions after upgrade, now stored under `/var/lib/devvm-dsh/sessions`.
- Host `root/.dsh/{sessions,storages,attachments}` stay `.gitkeep`-only after real VM use.
- The plugin writing `/run/devvm/sync-status.json` inside a running DevVM and the daemon reading it over `devvm exec`.
