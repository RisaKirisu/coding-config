# Run coding agents in the DevVM as the non-root user `agent`

Status: research complete, decisions locked, implementation not started (no repo file changed yet).

## Objective

DevVMs currently boot from an image where every guest process runs as root with `HOME=/root`;
the Project is mounted at `/root/workspace` and every coding-agent config lives under `/root`.
Change this so coding agents (DSH Runtime, OpenCode, `devvm shell`, `devvm exec`) run as the
unprivileged guest user `agent` (uid 1000, gid 1000, `HOME=/home/agent`), all coding-agent
configs live under `/home/agent`, `agent` can still use every global tool and `apt`, and
selected host-managed entries (e.g. `root/.ssh`) can be hidden from `agent`.

## Decisions (confirmed by the user)

1. **sudo scope**: passwordless sudo limited to `apt`, `apt-get`, `dpkg`. Not full sudo.
2. **Hidden entries**: `root/.ssh` is hidden from `agent` by default. The Session Sync key must
   still be usable by the sync plugin (which runs inside DSH, as `agent`), so the key is served
   through an **ssh-agent** owned by `agent` whose keys root loads from the hidden `.ssh`; the
   private key file is never readable by `agent`. The hidden list is configurable.
3. **Existing DevVMs**: must be recreated (`devvm rm`, then `devvm start`). No legacy fallback
   code for the old `/root/workspace` mount or for VMs lacking the `agent` user. VM-local
   Portable DSH State not yet pushed to a Sync Store is lost; document this.
4. Workspace path becomes `/home/agent/workspace` (non-root user cannot traverse `/root`).

## Current architecture (as read from the repo)

- `devvm` (bash CLI, host): `create` runs `smolvm machine create ... -e DEVVM_PROJECT_HOST=...
  -v "$PROJECT_DIR:/root/workspace" -v "$DEVVM_ROOT:/devvm-root"`. `start` = `ensure_ingress`
  (host frps), `smolvm machine start`, `start_ingress` (`smolvm machine exec -- /usr/local/bin/devvm-ingress`),
  then `link_root` (guest bash snippet as root, last arg literal `link-root`, used by test mock).
  `exec`/`shell` call `smolvm machine exec [--stream|-it] --name "$NAME" -- ...`.
- `link_root` today: symlinks `/devvm-root/.config/*` into `/root/.config/`; bind-mounts
  `/devvm-root/.dsh` onto `/root/.dsh` (bind, not symlink, because DSH saves via rename of a temp
  sibling); bind-mounts VM-local `/var/lib/devvm-dsh/{sessions,storages,attachments}` onto
  `/root/.dsh/{sessions,storages,attachments}` (migrating stray contents first); symlinks every
  other `/devvm-root/*` into `/root/*`; then runs
  `CI=true DSH_HOME=/root/.dsh dsh plugin --profile web install --frozen-lockfile` when
  `profiles/web/pnpm-lock.yaml` differs from `node_modules/.pnpm/lock.yaml`.
- `Dockerfile` (python:3.14-slim-trixie): ENV `CARGO_HOME=/root/.cargo NVM_DIR=/root/.nvm
  RUSTUP_HOME=/root/.rustup PATH=/root/.cargo/bin:/usr/local/bin:/root/.local/bin:$PATH`;
  installs node into `/usr/local` (tarball leaves `/usr/local/bin` and
  `/usr/local/lib/node_modules` owned by uid 1001), pnpm, nvm (`PROFILE=/root/.bashrc`), gh,
  rustup + components, cargo-binstall, frpc + caddy, copies `scripts/Caddyfile`, `scripts/frpc.toml`,
  `scripts/devvm-ingress`, `scripts/devvm-sync-startup`; pip upgrade; uv (`~/.local/bin`);
  `npm install -g @deepseek-ai/dsh@latest` (+ localhost-subdomains patch with sha256 pins);
  `npm install -g opencode-ai`; OPENCODE_* ENV; `git config --global --add safe.directory "/root/*"`;
  `WORKDIR /root`. No `sudo` installed.
- `scripts/devvm-ingress` (guest, run as root today): needs `DEVVM_PROJECT_HOST`; `install -d -m 0700 /run/devvm`;
  reads `/root/workspace/.devvm-id`; logs to `/devvm-root/.project-logs/<id>/ingress.log`;
  starts caddy (`/run/devvm/caddy.pid`) and frpc (`/run/devvm/frpc.pid`). Caddy listens `:10080`.
- `scripts/devvm-sync-startup` (guest): runs `node /root/.dsh/plugins/remote-sync/reconcile.mjs`, always exits 0.
- `src/runtime.rs`: `DSH_START_COMMAND` (pid file `/tmp/devvm-daemon-dsh.pid`, `log_dir=/devvm-root/.project-logs/{project_id}`,
  `cd /root/workspace && devvm-sync-startup`, `exec dsh web`), `DSH_STATUS_COMMAND`, `DSH_STOP_COMMAND`;
  all run via `devvm exec /bin/bash -c <snippet>` with cwd = project path. Unit tests assert
  substrings (`devvm-sync-startup` before `exec dsh web`, `echo $$ > /tmp/devvm-daemon-dsh.pid`,
  `log_dir=/devvm-root/.project-logs/abc-123`, exact `DSH_STATUS_COMMAND`).
- `src/sync.rs`: `SyncConfig { ssh_user, ssh_host, ssh_port, ssh_key_path: PathBuf, remote_sync_root, writer_id, daemon_url }`.
  `resolve_host_ssh_key_path` maps a guest path `/root/<sub>` to `DEVVM_ROOT/<sub>` or `$HOME/<sub>`.
  `provision_sync_setup` copies the host key into `DEVVM_ROOT/.ssh/<name>` (0700/0600) and writes
  guest config `DEVVM_ROOT/.config/devvm/sync.json` with `ssh_key_path=/root/.ssh/<name>`, preserving
  `writer_id`. `SystemSyncRunner::ssh_args` uses `-i resolve_host_ssh_key_path(...)` on the host.
  `read_status` runs `devvm exec /bin/sh -c "cat /run/devvm/sync-status.json"`.
  Other users of `ssh_key_path`: `src/main.rs` (`sync setup --ssh-key`, test at ~line 653/667),
  `src/api.rs:493` (`SyncSetupRequest.ssh_key_path`), `src/models.rs:122`, `src/ui.rs:554,936,953,957,974`.
- `root/.dsh/plugins/remote-sync/index.mjs`: `resolveProjectId` reads `${DEVVM_WORKSPACE:-/root/workspace}/.devvm-id`;
  `SyncStore.keyPath = config.ssh_key_path || join(HOME||'/root','.ssh/id_ed25519')`; `sshArgs()` passes
  `-p port -i keyPath -o StrictHostKeyChecking=accept-new -o BatchMode=yes -o ConnectTimeout=10`;
  `rshCommand()` builds the rsync `-e` string; `exec()` runs `ssh` for remote or `sh -c` locally.
  Status file `/run/devvm/sync-status.json`. README lines ~107, ~112, ~153 mention `ssh_key_path`, `/root/workspace`, `/root/.dsh/plugins/...`.
- `root/.dsh/plugins/voice-input`: default path `/root/voice-dictation-cleanup/data/archive_voice_input.jsonl`
  in `index.mjs:8`, `README.md:26`, `cordis.patch.yml:6`; `remote-sync/test.mjs:724` asserts the web
  `--dump-config` contains that path.
- `root/.dsh/profiles/web/package.json`: deps `link:/root/.dsh/plugins/build-loop`,
  `file:/root/.dsh/plugins/{remote-sync,subagent-manager,voice-input,dsh-skill-mcp-panel}`.
  `pnpm-lock.yaml` embeds `link:/root/.dsh/plugins/...` specifiers and
  `file:../../../../root/.dsh/plugins/...` versions/resolutions (lines ~15-34, 158-169, 642-643, 1263-1269, 1846).
  `node_modules/.modules.yaml` has `storeDir: /root/workspace/.pnpm-store/v11`, `nodeLinker: hoisted`.
  `tests/install_test.rs::test_web_profile_links_first_party_plugins_to_their_sources` asserts
  `link:/root/.dsh/plugins/<source>` for remote-sync, subagent-manager, voice-input, dsh-skill-mcp-panel
  (currently failing-by-design vs `file:`? — check when running tests).
- `tests/common/mod.rs` (mock `devvm`, lines ~140-150): rewrites guest paths in `-c` snippets:
  `/tmp/devvm-daemon-dsh.pid`→`$PWD/.mock_dsh.pid`, `/devvm-root/.project-logs`→log dir,
  `/run/devvm`→`$PWD/.mock_run`, `/root/workspace`→`$PWD`, `/root/.dsh`→`$PWD/.mock_dsh`.
  AGENTS.md "Runtime verification" documents this list; update it.
- `tests/install_test.rs::test_devvm_start_is_idempotent_and_exec_reuses_running_machine` (line ~431):
  fake `smolvm` logs `machine-create|machine-start|ingress-start|profile-prepare`; the `link-root`
  branch requires `CI=true DSH_HOME=/root/.dsh` in the argv else fails; asserts exec payload line
  ends with `echo running_guest_payload`; asserts one `ingress-start` and one `profile-prepare`.
- `tests/sync_test.rs` (139,169,186,226), `tests/acceptance_workflow_test.rs:689`: host-side
  `ssh_key_path` strings `/root/.ssh/id_*` (host path, fine to keep if field stays).
- `src/sync.rs` unit test `test_provision_sync_setup_host_and_guest_paths` asserts guest
  `ssh_key_path == /root/.ssh/host_id_ed25519`, `.ssh` 0700, key 0600 — must change.
- `readme.md`: lines 8, 10, 11, 91 mention `/root/workspace`, `/root/.dsh`. `smolvm.toml.example:4` `workdir = "/root"`.
  User's local `smolvm.toml` has `workdir = "/workspace"`.
- Host-side `root/` tree (DEVVM_ROOT) ownership: host user 502:20 (macOS `staff`) for most; uid 0 for
  `.ingress-logs`, `.pnpm-store`, `.project-logs`, `.dsh/.credentials.yaml`, `.dsh/.pnpm-store`,
  `.dsh/attachments`, `.dsh/sessions`, `.dsh/settings.yaml`, `.dsh/storages`, `profiles/web/node_modules`
  (created by guest root). `root/.ssh` exists, empty, 0700, 502:20. `root/.config/devvm` empty.
- `.agents/lessons.md` relevant: never rewrite `devvm`/`scripts/*` with the `write` tool (mode loss);
  re-check `ls -l` after edits. Lifecycle fakes must isolate guest paths.

## Experimental findings inside a running DevVM (kernel 6.12.95, Debian 13, util-linux 2.41.5)

VERIFIED:
- `smolvm machine exec` enters as root with image ENV applied (PATH includes `/root/.cargo/bin`,
  `DEVVM_PROJECT_HOST` from `-e`, `OPENCODE_*` present). No `--user` option in smolvm CLI/docs.
- virtiofs mounts expose raw host uids (`/devvm-root` and `/root/workspace` owned 502:20 in guest).
  A process with uid 65534 creating a file in `/devvm-root` produced a 65534-owned file → no host-side
  uid squashing.
- Idmapped bind mount works on virtiofs:
  `mount --bind -o X-mount.idmap="u:502:1000:1 g:20:1000:1" /devvm-root /tmp/idm` → files appear
  1000:1000; uid 1000 can create files; on the underlying mount they are 502:20. Root (uid 0, unmapped)
  writing through the idmapped mount fails: `Value too large for defined data type` (EOVERFLOW).
  A plain `mount --bind` of a subdirectory of an idmapped mount inherits the mapping.
  `X-mount.idmap` on a plain tmpfs subdirectory (non-mountpoint) failed with EINVAL — irrelevant for virtiofs.
- `chown 502:20` by root on virtiofs files works (guest view updates). `chown -Rh --from=0 502:20 DIR`
  changes only uid-0 entries, does not follow symlinks, and does not touch symlink targets.
- Hiding: `mount -t tmpfs -o ro,mode=0700 tmpfs /devvm-root/.ssh` → uid 1000 gets
  `ls: cannot open directory: Permission denied`.
- `useradd -m -u 1000 -s /bin/bash agent` works in the image (uid 1000 free; uid 1001 is used by node
  tarball files only, no passwd entry).
- `setpriv --reuid agent --regid agent --init-groups env HOME=/home/agent USER=agent LOGNAME=agent bash -c ...`
  yields `HOME=/home/agent`, preserves other ENV (OPENCODE_* count 5), PATH still the image PATH.
- ssh-agent flow: root runs `setpriv ... env HOME=/home/agent ssh-agent -a /run/devvm-test/ssh-agent.sock`
  (socket owned 1000:1000, 0600); root `SSH_AUTH_SOCK=... ssh-add /tmp/testkey` succeeds; `agent` with
  the same `SSH_AUTH_SOCK` can `ssh-add -l` (sees key) and `ssh-add -d`. Test user/socket were removed afterwards.
- `sudo` is not installed in the image (`which sudo` empty, dpkg status has no sudo).
- No docker/podman inside this DevVM → image rebuild and fresh-VM boot cannot be verified from here.

UNVERIFIED / ASSUMPTIONS:
- `smolvm machine create -v HOST:/mnt/devvm/workspace` mounts at an arbitrary guest path (only
  `/workspace` is documented as special; `/root/workspace` and `/devvm-root` already work).
- Whether smolvm `workdir` from the Smolfile affects `machine exec` cwd (daemon snippets `cd` explicitly anyway).
- Behaviour of `git config --global` when `HOME=/home/agent` and `/home/agent/.gitconfig` may later be
  replaced by a symlink from `/devvm-root/.gitconfig` (link_root symlinks all top-level host entries).
- rsync respecting `SSH_AUTH_SOCK` from the DSH process environment (standard OpenSSH behaviour; not run here).
- Docker `USER agent` steps: rustup/nvm/uv installers writing under `/home/agent` (standard, not run here).
- Whether the profile `pnpm-lock.yaml` regenerates cleanly after rewriting paths (may need
  `dsh plugin --profile web install` without `--frozen-lockfile` once, as agent, inside a new VM).

## Implementation plan (file by file)

1. `Dockerfile`
   - `apt-get install sudo`; `useradd -m -u 1000 -s /bin/bash agent`; `/etc/sudoers.d/agent`:
     `agent ALL=(root) NOPASSWD: /usr/bin/apt, /usr/bin/apt-get, /usr/bin/dpkg` (`chmod 0440`).
   - ENV: `CARGO_HOME=/home/agent/.cargo NVM_DIR=/home/agent/.nvm RUSTUP_HOME=/home/agent/.rustup
     PATH=/home/agent/.cargo/bin:/home/agent/.local/bin:/usr/local/bin:$PATH`.
   - Run nvm, rustup (+components), cargo-binstall, uv installs as `USER agent` (`PROFILE=/home/agent/.bashrc`).
   - `npm config set prefix /home/agent/.local` as agent so `npm i -g` works without sudo.
   - `git config --global --add safe.directory "*"` as agent (or `/home/agent/*` + `/devvm-root/*`).
   - `COPY --chmod=0755 scripts/devvm-as-agent /usr/local/bin/devvm-as-agent`.
   - `WORKDIR /home/agent`; keep image `USER root` at the end (smolvm exec is root anyway; link_root needs root).
   - Copy pnpm global / fix `/usr/local/lib/node_modules` ownership: leave root-owned (global tools read-only for agent; user-level globals go to `~/.local`).
2. `scripts/devvm-as-agent` (new, 0755):
   `exec setpriv --reuid agent --regid agent --init-groups env HOME=/home/agent USER=agent LOGNAME=agent SHELL=/bin/bash "$@"`.
3. `devvm`
   - `create`: `-v "$PROJECT_DIR:/mnt/devvm/workspace" -v "$DEVVM_ROOT:/mnt/devvm/root"`.
   - `start`: `smolvm machine start` → `link_root` (root prep) → `start_ingress` (as agent via wrapper).
   - `link_root` (root snippet, still tagged `link-root`):
     - `HU=$(stat -c %u /mnt/devvm/root)`, `HG=$(stat -c %g ...)`; same per workspace mount.
     - once per VM (marker `/var/lib/devvm/owner-migrated`): `chown -Rh --from=0 $HU:$HG /mnt/devvm/root /mnt/devvm/workspace`.
     - `install -d /devvm-root /home/agent/workspace`; idmapped binds:
       `mount --bind -o X-mount.idmap="u:$HU:1000:1 g:$HG:1000:1" /mnt/devvm/root /devvm-root` and
       `.../workspace /home/agent/workspace` (skip if already mountpoint).
     - hidden entries: read `/mnt/devvm/root/.config/devvm/agent-hidden` (one relative path per line; default `.ssh` when file absent);
       for dirs `mount -t tmpfs -o ro,mode=0700 tmpfs /devvm-root/<p>`; for files bind `/run/devvm/hidden` (root 0600) over them.
     - `install -d -m 0700 -o agent -g agent /run/devvm`; VM-local `/var/lib/devvm-dsh/{sessions,storages,attachments}` owned agent 0700;
       bind `/devvm-root/.dsh` → `/home/agent/.dsh` (agent-owned 0700 dir), then VM-local binds on top.
     - symlink `/devvm-root/.config/*` → `/home/agent/.config/*` and other `/devvm-root/*` → `/home/agent/*` (skip `.config`, `.dsh`, hidden names).
     - ssh-agent: if `/mnt/devvm/root/.ssh` has private keys: start `devvm-as-agent ssh-agent -a /run/devvm/ssh-agent.sock` (if not running),
       then as root `SSH_AUTH_SOCK=/run/devvm/ssh-agent.sock ssh-add <each key>`. Export `SSH_AUTH_SOCK` for agent via wrapper
       (set in `devvm-as-agent`: `SSH_AUTH_SOCK=/run/devvm/ssh-agent.sock` when socket exists).
   - profile prepare: separate `smolvm machine exec -- devvm-as-agent bash -c '... CI=true DSH_HOME=/home/agent/.dsh dsh plugin --profile web install --frozen-lockfile'` (keep `CI=true DSH_HOME=` substring for the test mock, update path in test).
   - `exec`: `smolvm machine exec --stream --name "$NAME" -- devvm-as-agent "$@"`; `shell`: `... -it -- devvm-as-agent /bin/bash`.
   - Update usage text.
4. `scripts/devvm-ingress`: `/home/agent/workspace/.devvm-id`; drop `install -d /run/devvm` root assumption (dir pre-created by link_root; keep `install -d` since agent owns it).
5. `scripts/devvm-sync-startup`: `/home/agent/.dsh/plugins/remote-sync/reconcile.mjs`.
6. `src/runtime.rs`: `cd /home/agent/workspace`. Comments mention agent user.
7. `src/sync.rs`: `SyncConfig.ssh_key_path` → guest config no longer needs it (plugin uses ssh-agent). Minimal: keep host field;
   `provision_sync_setup` still copies key into `DEVVM_ROOT/.ssh/` (hidden from agent, loaded by root into ssh-agent) and writes guest
   `ssh_key_path` as `None`/omitted (make field `Option<PathBuf>` with `skip_serializing_if`) — update `resolve_host_ssh_key_path` callers,
   `main.rs`, `api.rs`, `ui.rs` accordingly; update unit test expectations.
8. `root/.dsh/plugins/remote-sync/index.mjs`: `resolveProjectId` default `/home/agent/workspace`; `sshArgs()` omits `-i` when no
   `ssh_key_path` (agent auth via `SSH_AUTH_SOCK`); default `keyPath` removed. README updates. `test.mjs` check any `-i` assertions.
9. `root/.dsh/plugins/voice-input/{index.mjs,README.md,cordis.patch.yml}` and `remote-sync/test.mjs:724`: `/home/agent/voice-dictation-cleanup/...`.
10. `root/.dsh/profiles/web/package.json` + `pnpm-lock.yaml`: `sed 's#/root/.dsh/plugins#/home/agent/.dsh/plugins#g'` (relative `../../../../root/.dsh` → `../../../../home/agent/.dsh` — lock uses paths relative to profile dir `/home/agent/.dsh/profiles/web`; four `..` from there is `/`; so `../../../../home/agent/.dsh/plugins/x`).
11. Tests: `tests/common/mod.rs` rewrites `/home/agent/workspace`, `/home/agent/.dsh`; `tests/install_test.rs` expectations
    (`link:/home/agent/.dsh/plugins/...`, `CI=true DSH_HOME=/home/agent/.dsh`, ingress after link-root ordering if asserted); sync tests for `Option` field.
12. Docs: `readme.md` (paths, `agent` user, sudo scope, hidden list file, recreation note, `devvm exec` runs as agent; root via `smolvm machine exec --name $(devvm name)`),
    `AGENTS.md` (mock path list, plugin refresh commands with `/home/agent/.dsh`, `DSH_HOME`), plugin READMEs, `smolvm.toml.example` `workdir = "/home/agent/workspace"`,
    new `docs/adr/0006-run-coding-agents-as-the-agent-user.md` (idmapped mounts, ssh-agent for the sync key, root never writes through idmapped mounts).
13. Verify: `cargo test` (guard: hosting DSH pid alive), `node --test root/.dsh/plugins/remote-sync/test.mjs`, `bash -n devvm scripts/*`, `ls -l devvm scripts/*` modes.
    Host-only follow-up for the user: `./setup-devvm.sh` (rebuild image), `devvm rm && devvm start` per Project, check `id` via `devvm exec id`.

## Comments
