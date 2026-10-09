# DSH 0.2.0-rc.2 integration handoff

The repository prepares exact DSH `0.2.0-rc.2` integration. [Runtime maintenance](dsh-runtime-maintenance.md) owns active deployment/adaptation commands; [the release requirements](dsh-0.2.0-rc.2-upgrade-plan.md) own configuration/plugin boundaries. Source preparation does not establish live host/VM deployment.

## Installation and version ownership

The image installs the pinned CLI and applies both [localhost](../patches/deepseek-harness-localhost-subdomains.patch) and [Project-app](../patches/deepseek-harness-project-app.patch) native patches. Build applicability checks are zero-fuzz. Setup conditionally builds/exports the image and rebuilds the daemon. New images affect new VMs; existing VMs keep their global installation until explicitly migrated.

Guest initialization mounts shared configuration and VM-local portable state, then installs Web/headless profiles when locks differ or installation is absent. The native Web manifest uses relative `file:` plugin packages, ordinary dependencies, and runtime peers. Installed file packages are snapshots; unchanged locks do not trigger source-only refresh.

Use the forced native install with `/root/workspace/.pnpm-store/v11`, then a fresh target Runtime process. Existing-VM owner updates require the exact deployed receipt and a complete old-to-new transition; a CLI release update also requires replacing that VM's CLI and validating candidate data/tool requirements. See [deployment](dsh-runtime-maintenance.md#deployment). Never restart/probe the Runtime hosting the agent session.

## Configuration and settings

Web fields live in its profile patch; headless is independent. Shared execution policy/local Bash live in the home patch. The active `standard-bash` preset is declarative and retains its historical ID; dormant experiments are unregistered. Native Browser supplies browsing. Web Subagent Manager owns child provider/model/effort. HTTP MCP uses native environment-backed headers with private values preserved.

Web defaults remain Steer and Simplified Chinese. The owner patch makes the native form provider Host-backed on every authenticated page, so remote browsers load and save profile fields. [Settings policy admission](dsh-runtime-maintenance.md#settings-policy-admission) records the exact boundary and conformance probe.

## Current verification scope

The [app verification guide](project-app.md#verification) describes owned real-native carriers and targeted tests. Current controls/mobile evidence covers neutral focus, real refresh, icons/action rows, compact header, native navigation, drawer width and animations, both swipe directions, reduced motion, viewport/draft retention, and pinch zoom. Full delivery/recovery checks remain required when their native owners change. Complete patch-state validation covers pristine/current and exact older-to-current transitions across eight generated targets.

Native package/source/defaults review does not prove provider requests, real production MCP access, actual guest Browser use, remote FRP/Tailscale stability, or physical Samsung installation/IME/hinge/permission behavior. Those remain explicit acceptance checks. Image deployment, existing-VM migration, and fresh target Runtime activation must be reported only after performed.
