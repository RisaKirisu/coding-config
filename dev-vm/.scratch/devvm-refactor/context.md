# Context for continuing the DevVM refactor

Read with [spec.md](spec.md), [plan.md](plan.md), and the tickets in [issues/](issues/). This file records planning context that is not in those documents. The earlier upgrade-only plan, [../smolvm-upgrade-windows/plan.md](../smolvm-upgrade-windows/plan.md), is superseded and kept for reference.

## Where agents can and cannot work

- The current DevVM has no `/dev/kvm` and hides the CPU virtualization extensions, so Smolvm cannot run inside it. Agents can build, run the mocked test suites, run the Project app browser checks against the fixture daemon, and edit workflows. They cannot boot a DevVM: the opt-in live acceptance test and every acceptance step that starts a real DevVM run on a host.
- Agents get GitHub access only to `RisaKirisu/devvm`, through a fine-grained personal access token (repository access: only `RisaKirisu/devvm`; Contents read and write, Workflows read and write, Pull requests read and write, Actions read). With it an agent pushes branches, opens pull requests, and reads CI results. The token can also merge pull requests and push to `main`; GitHub cannot prevent that on a private Free-plan repository with a single account, so agents work on branches and open pull requests and the maintainer merges.
- The token is logged in per DevVM with `gh auth login` and `gh auth setup-git`, which writes `/root/.config/gh` inside that DevVM. It must never be placed under `/devvm-root/.config`, which every DevVM shares and which `.gitignore` does not exclude for `gh`.
- Windows code is tested on GitHub's `windows-2025` runner; no Windows host is available to agents.

## Maintainer steps and their timing

1. Before ticket 01: create the empty private repository `RisaKirisu/devvm` on GitHub, create the token described above, and run `gh auth login` and `gh auth setup-git` in the `coding-config` Project's DevVM. Keep `coding-config/dev-vm` until step 3 is done.
2. After ticket 01: on the host, clone `RisaKirisu/devvm` into its own directory (for example `~/coding/devvm`), register it as a Project in the Control Daemon, start its DevVM, and run `gh auth login` and `gh auth setup-git` there. Tickets 02–07 run in that DevVM. Merge each ticket's pull request after CI passes.
3. After ticket 03, on the Linux workstation and then the Mac: confirm every Project's Sync Status is `synchronized`; with the old tools from `coding-config/dev-vm`, run `devvm rm` in each Project, uninstall the old service, and remove old Smolvm state; install the new `devvm`, run `devvm setup --root-dir <clone>/root --service` and `devvm sync setup`; re-register Projects; run ticket 03's acceptance checks. Then remove `dev-vm/` from `coding-config`.
4. After ticket 04's first release: set the `devvm` package to public in GitHub's package settings; create one new DevVM per host from the default image.
5. After ticket 07, on Windows 11: enable the Hypervisor Platform feature (admin, reboot) and Developer Mode; install Git with `core.symlinks=true` (Docker only for local image builds); clone the repository; run `devvm setup`; for remote access install Tailscale and Caddy as a Windows service; run ticket 07's acceptance checks.

## Repository and working directories

- Ticket 01 runs in the current `coding-config` Project's DevVM, because the history to extract exists only in its workspace and the agent's token cannot read `coding-config` on GitHub. The planning files in `dev-vm/.scratch/devvm-refactor/` are committed to `coding-config` first so the extracted history contains them. The agent makes a fresh local clone with `git clone --no-local /root/workspace <gitignored directory inside the workspace>` (`git filter-repo` refuses to rewrite a repository that is not a fresh clone), runs `git filter-repo --subdirectory-filter dev-vm`, adds the CI workflow, and pushes to `RisaKirisu/devvm`.
- Tickets 02–07 run in a separate Project whose directory is a clone of `RisaKirisu/devvm`, so its `/root/workspace` is the repository root. A DevVM can only write its own `/root/workspace` and `/devvm-root`, so the maintainer creates that directory on the host.
- `.devvm-id` is tracked, so the new Project gets the Project ID `7973ae3e-8e0b-47f8-a144-0490c5cff468` and any Session Sync history stored under it. `coding-config/dev-vm` must not be registered as a Project at the same time, or two Projects share one ID in the Sync Store.
- Until the clean reinstall, the running Control Daemon and every DevVM keep using `coding-config/dev-vm`; work in the new clone does not affect them. After the reinstall, `--root-dir` points at a clone's `root/`. A clone that agents edit exposes their uncommitted plugin and profile edits to DSH immediately; a separate clone that only receives `main` avoids that.

## The clean reinstall removes the agent's own environment

The agent runs inside a DevVM managed by the current Control Daemon. The clean reinstall deletes that DevVM and replaces the daemon and the Smolvm installation hosting it. An agent can build and merge ticket 03, but the maintainer performs the reinstall from the host. Later tickets run in the recreated DevVMs; DSH sessions return from the Sync Store.

## Open decision: nested virtualization for DevVMs

Smolvm 1.27.3 has `machine create --nested`, which exposes the host's virtualization extensions to the guest so the guest can run KVM and therefore Smolvm. If DevVMs were created with it, later agents could run the live acceptance test inside a DevVM. Smolvm documents that nested guests run much slower, and support on macOS hosts is unverified. Not in the plan; the maintainer has not decided.

## Why the shared root stays in the repository

`root/` (mounted at `/devvm-root`) stays in the devvm repository clone, selected with `devvm setup --root-dir <clone>/root`, and not in each host's configuration directory:

- DevVM's own DSH plugins live in it (`remote-sync` reads the sync file the daemon writes; `project-app` is the DSH side of the Project app), so daemon and plugin contract changes land in one commit.
- The profiles reference the plugins by relative `file:` paths, so profiles, plugins, and lockfiles must stay together.
- Git shares skills, profiles, presets, and saved DSH settings across workstations.

Host-local material in the clone (credentials, the copied Sync Store key, `.config/devvm/sync.json`, Project Logs, installed `node_modules`) is excluded by `.gitignore`. A `git clean -fdx` in the clone would delete it.

## Guest connections to host services through the gateway

Guest services reach host loopback services through the gateway address `100.96.0.1`: frpc to host frps on port 17007, and both DSH profiles' model provider to the host model proxy at `http://100.96.0.1:8317` (`root/.dsh/profiles/*/cordis.patch.yml`). In Smolvm 1.27.3 with virtio-net:

- The egress policy checks the destination the guest dials. The default local floor denies only link-local (`169.254.0.0/16`) and loopback (`127.0.0.0/8`, `::1`), not `100.96.0.1` (Smolvm `crates/smolvm-network/src/egress.rs`, `floor_mode` and `is_floored`).
- The relay then rewrites a gateway destination to host loopback with the same port (`crates/smolvm-network/src/tcp_relay.rs`, `host_connect_addr`). The Windows build uses the same networking crate.
- Conditions: the host service listens on `127.0.0.1` or all interfaces, and the host does not set `SMOLVM_EGRESS_FLOOR=strict` or `SMOLVM_PUBLISH_ADDR` (multi-tenant mode blocks the gateway range). `--allow-host-loopback` is not needed.

Tickets 03 and 07 include both connections in their acceptance checks.

## Research evidence

Research and review notes are in `.agents/exploration/microvm-backends/` and `.agents/exploration/smolvm-upgrade/`, which `.gitignore` excludes. They exist only on the workstation where the planning ran and do not travel with the repository. They include the unpacked Smolvm v1.19.3 (`old/`) and v1.27.3 (`new/`) sources that the plan's Smolvm source citations refer to. To check a citation elsewhere, read the Smolvm source at tag `v1.27.3` from https://github.com/smol-machines/smolvm. The conclusions are recorded in `plan.md` and `spec.md`.
