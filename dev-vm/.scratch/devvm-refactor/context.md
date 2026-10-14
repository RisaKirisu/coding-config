# Context for continuing the DevVM refactor

Read with [spec.md](spec.md), [plan.md](plan.md), and the tickets in [issues/](issues/). This file records planning context that is not in those documents. The earlier upgrade-only plan, [../smolvm-upgrade-windows/plan.md](../smolvm-upgrade-windows/plan.md), is superseded and kept for reference.

## Where agents can and cannot work

- The current DevVM has no `/dev/kvm` and hides the CPU virtualization extensions, so Smolvm cannot run inside it. Agents can build, run the mocked test suites, run the Project app browser checks against the fixture daemon, and edit workflows. They cannot boot a DevVM: the opt-in live acceptance test and every acceptance step that starts a real DevVM run on a host.
- The current DevVM has no GitHub access: `gh` is not logged in and the only SSH key is the Sync Store key. Before ticket 01 the maintainer runs `gh auth login` in the DevVM (or adds a GitHub deploy key to the shared root's SSH directory).
- Windows code is tested on GitHub's `windows-2025` runner; no Windows host is available to agents.

## Maintainer steps and their timing

1. Before ticket 01: create the empty private repository `RisaKirisu/devvm` on GitHub and give the DevVM GitHub access. Keep `coding-config/dev-vm` until step 2 is done.
2. After ticket 03, on the Linux workstation and then the Mac: confirm every Project's Sync Status is `synchronized`; with the old tools from `coding-config/dev-vm`, run `devvm rm` in each Project, uninstall the old service, and remove old Smolvm state; install the new `devvm`, run `devvm setup --root-dir <clone>/root --service` and `devvm sync setup`; re-register Projects; run ticket 03's acceptance checks. Then remove `dev-vm/` from `coding-config`.
3. After ticket 04's first release: set the `devvm` package to public in GitHub's package settings; create one new DevVM per host from the default image.
4. After ticket 07, on Windows 11: enable the Hypervisor Platform feature (admin, reboot) and Developer Mode; install Git with `core.symlinks=true` (Docker only for local image builds); clone the repository; run `devvm setup`; for remote access install Tailscale and Caddy as a Windows service; run ticket 07's acceptance checks.

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
