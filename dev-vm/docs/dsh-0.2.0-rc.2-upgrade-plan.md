# DSH 0.2.0-rc.2 integration requirements

This record describes the current rc.2 configuration/plugin integration. [Runtime maintenance](dsh-runtime-maintenance.md) is the active deployment and release-adaptation procedure; [the handoff](dsh-0.2.0-rc.2-upgrade-handoff.md) summarizes its current verification boundaries. The [source-change manifest](dsh-0.2.0-rc.2-source-changes.tsv) is a migration receipt, not the current release edit inventory.

## Configuration ownership

The shared [home patch](../root/.dsh/cordis.patch.yml) owns execution policy and local executors. Web-editable fields belong to the [Web profile](../root/.dsh/profiles/web/cordis.patch.yml). [Headless](../root/.dsh/profiles/headless/cordis.patch.yml) owns its independent provider/default-model configuration and native spawn defaults. There is no cross-profile synchronizer. Preserve current configured provider routes, exact model IDs, compatibility dictionaries, field nulls, credential references, and private environment values; the files themselves are authoritative.

Legacy fields were migrated to profile entry configurations and [the original document](../root/.dsh/settings.yaml.pre-020-rc2) is archived. No active legacy settings document is needed. Relevant destinations are:

| Owned value | Native destination |
| --- | --- |
| Welcome notice | `ui-settings-general.config.welcomeNoticeVersion` |
| Default provider/model/effort | `agent-default-model.config` |
| Provider routes / DeepSeek models | `llm-pi-ai.config.providers` / `llm-deepseek.config.models` |
| Active Web preset | `agent-preset-registry.config.default` and `selectedDefault` |
| Busy-send preference | `ui-conversation.config.busyEnter: steer` |
| Locale | `locale.config.preference: zh` |
| Theme | `ui-theme.config.preference: system` |
| Web child provider/model/effort | `subagent-manager.config` |
| Build Loop policy | `build-loop.config`, layered over the plugin's exported defaults |
| Native Browser | `ui-sidebar-browser`, enabled in Web |

Cordis patch rows own complete configurations; successive partial rows can erase fields. Override existing entry IDs once and preserve complete owned values. Home-owned fields cannot be saved over by a lower-precedence profile form.

The home keeps `sandbox` available for native PTC, with `sandbox-policy.mode: danger-full-access`, and disables the sandboxed executor/permission/approval entries. Local Bash has a 1,200,000ms default and cap and 20,000 retained output bytes; call/session cwd remains authoritative. These are deployment-owned executor settings.

Web settings storage and remote UI persistence are distinct. The owner patch makes native forms use Host mode on every authenticated page, so Steer/Chinese profile overrides apply to remote browsers; see [settings policy admission](dsh-runtime-maintenance.md#settings-policy-admission).

## Native package composition

[Dockerfile](../Dockerfile) pins DSH `0.2.0-rc.2` and applies the localhost and Project-app owner patches. [Guest initialization](../devvm) performs native frozen profile installation with the existing guest pnpm store. Local Web bundles are relative `file:../../plugins/<directory>` dependencies. Installed packages are snapshots; runtime peers are supplied by native DSH resolution. Ordinary package dependencies belong in manifests/locks. Source-only changes need a forced install and a fresh target Runtime process.

The declarative [Agent preset bundle](../root/.dsh/plugins/agent-presets/cordis.patch.yml) registers the historical `standard-bash` ID once. It preserves terminal-only model-facing filesystem access, native PTC workflow, Ralph, plan/goals, continuable spawn/fork, and configured compaction/pruning budgets. Dormant experimental presets remain unregistered. Headless uses direct native composition instead of the Web preset registry.

The Web [manifest](../root/.dsh/profiles/web/package.json) pins Better Sidebar and Thinking Effort compatibility. Native Browser owns browsing; Web Subagent Manager owns child provider/model/effort. Headless uses its own spawn defaults. Update exact local runtime-peer/admission metadata against candidate public APIs, not compatibility guesses.

## Local plugin boundaries

- [Build Loop](../root/.dsh/plugins/build-loop/README.md) exports volatile native configuration and captures run policy at start. Configuration endpoints retain their shapes and write the active profile through native settings APIs. Jobs use Session ownership, `result` output, and producer-owned message sources. Its controller owns design approval, triage, worker retention, audit retry, and cancellation.
- [Subagent Manager](../root/.dsh/plugins/subagent-manager/README.md) exports provider/model/effort configuration and preserves its next-first `agent/request` predicate/override. Web has one child-model authority; headless is independent.
- [Voice Input](../root/.dsh/plugins/voice-input/package.json) preserves archive tools/data paths. [Style Control](../root/.dsh/plugins/style-control/package.json) preserves JSON selection format, per-session/global behavior, and prompt injection.
- [Remote Sync](../root/.dsh/plugins/remote-sync/README.md) remains the sole Sync engine. It flushes committed portable state, retains head sequencing/retries, separates session union from projection/storage replacement, includes image and generic-file objects/metadata, and excludes locks and migration staging files. V3/V4 predecessor/successor handling must be rechecked on a release change.
- The [Skill/MCP panel](../root/.dsh/plugins/dsh-skill-mcp-panel/package.json) uses the published panel implementation with local dependency packaging. Native HTTP MCP authentication uses its declared `headers` configuration supplied from private environment-backed Cordis expressions. Secrets are not committed or emitted in list responses. Environment-backed updates follow native reload/restart behavior.
- [Project app](project-app.md) uses native admission, input, Session, transport, slots, and layout services. Its owner patch is versioned independently of profile installation and must be present in the CLI.

## Verification and deployment boundaries

Use real published candidate artifacts and owned fixtures. [Frozen installation](../tests/dsh-plugin-install.test.mjs) checks packaging/import isolation; [native tools](../tests/dsh-rc2-tools.test.mjs) checks native tool/MCP integration. Each local plugin retains its focused checks. Native owner rebases require browser delivery/recovery, controls/mobile, and complete patch-state validation, not only package installation.

Settings checks must separate profile-backed native forms from custom plugin endpoints. Provider requests, production Context7 access, real guest Browser flows, physical-device behavior, and remote network-path claims need their own actual evidence.

No release record is proof that an existing VM or hosting process was upgraded. Host setup may rebuild the image; existing VMs retain their global CLI. [Runtime maintenance](dsh-runtime-maintenance.md#deployment) documents explicit same-version old-to-new patches, exact receipts, snapshot refresh with the guest store, and separate existing-VM CLI release replacement. Preserve credentials, profiles, Sessions, attachments, storages, voice/style data, and production Sync state throughout.
