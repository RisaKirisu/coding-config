# DSH runtime maintenance

This is the active procedure for installing, deploying, and adapting DevVM to a DSH release. [The app contracts](project-app.md), [the specification](../.scratch/devvm-project-app/spec.md), and [the rc.2 integration record](dsh-0.2.0-rc.2-upgrade-plan.md) describe behavior and open requirements. Current image admission pins DSH to `0.2.0-rc.2`.

## Build and installation inputs

| Input | Responsibility |
| --- | --- |
| [Dockerfile](../Dockerfile) | Exact DSH/Node/tool versions; installs the global CLI; applies localhost and Project-app patches with zero-fuzz checks |
| [build.sh](../build.sh) | Docker/Podman build and architecture-specific OCI archive export |
| [setup](../setup-devvm.sh) | Conditional image build, daemon build/install, and configured host services |
| [guest wrapper](../devvm) | Creates new machines from the archive, mounts shared configuration and VM-local state, installs native profiles when locks differ |
| [Web manifest](../root/.dsh/profiles/web/package.json) and [lock](../root/.dsh/profiles/web/pnpm-lock.yaml) | Local plugin snapshots, ordinary dependencies, and exact runtime-peer admission |
| [headless manifest](../root/.dsh/profiles/headless/package.json) and [lock](../root/.dsh/profiles/headless/pnpm-lock.yaml) | Independent headless dependency graph |
| [Web profile patch](../root/.dsh/profiles/web/cordis.patch.yml) | Web Runtime configuration and saved form overrides |
| [home patch](../root/.dsh/cordis.patch.yml) | Shared execution policy and host plugins |

The shared home is mounted at `/root/.dsh`; Sessions, storages, and attachments are VM-local mounts. Guest initialization does not copy/reset profile configuration. It installs Web/headless only when the source lock differs from the installed pnpm lock or installation is absent. An already-running VM returns from startup before that initialization. Local `file:` dependencies are installed snapshots, so source-only edits require a forced native install.

Profile settings belong to the active Runtime profile's `cordis.patch.yml`. Legacy settings were migrated/archived; do not create a new `settings.yaml` to configure rc.2. A process using headless has its own profile configuration; do not assume Web fields configure every process.

## Native contract inventory

The [Project-app owner patch](../patches/deepseek-harness-project-app.patch) and [distributed copy](../root/.dsh/devvm-project-app.patch) must be byte-identical. Docker consumes the first; same-version existing-VM updates consume the second. The owner patch covers Session admission/display and embedded UI, and leaves the native settings persistence policy unchanged.

The patch targets eight generated files under `node_modules/@deepseek-ai/` in the global CLI:

| Native owner | Published targets | Required behavior |
| --- | --- | --- |
| `dsh-api-session-controller` | `lib/client.js`, `lib/index.js`, `lib/types/client/contract/session.d.ts` | Retained displayed projections, caller request identity, concurrent same-ID admission, accepted-ID recovery |
| `dsh-client-ui-conversation` | `lib/client.js`, `lib/types/client/contract/input.d.ts`, `lib/types/client/service.d.ts` | Embedded draft provider, awaitable admission/dispatch, captured identities, native prompt/bubble continuity, mobile styling |
| `dsh-client-ui-layout` | `lib/client.js`, `lib/types/client/service.d.ts` | `setSidebarOpen`, native slot/grid integration, mobile drawer geometry and transitions |

The [localhost patch](../patches/deepseek-harness-localhost-subdomains.patch) targets published `dsh-client-connection` client/host artifacts and admits existing `.localhost` facades. Both patches require revalidation for a new DSH release. Zero fuzz validates matching contexts; it does not establish API semantics or visual correctness.

The [host bridge](../root/.dsh/plugins/project-app/index.mjs) depends on `webServer` and `webserver/index-inject`. The [client bridge](../root/.dsh/plugins/project-app/client.js) depends on `window.__ModuleLoader__.load`, `react/jsx-runtime`, native `Tooltip`/`IconPanelLeftOutlineRegular`, `conversation.header.leading`, and the native services listed in the app contract. The bridge additionally injects `remote`/`remote.session` and calls public `session.projections({sessionId})` for authoritative selection checks. Its Remote result must distinguish lookup failure from a successful null absence and a successful baseline; the read must remain non-activating. Native `sessions.refresh()` has a void result and can fold read failures, so its cached list cannot establish absence. Peer admission alone does not prove the owner patch exists.

Native CSS integration pins generated class families `wSkVaW`, `uV2eYG`, `pI_x6G`, `Sixlwa`, `hWmORq`, and `IW6AQa`. The header button itself uses `IW6AQa_iconButton`. Review selectors in the owner patch against the candidate's actual styles, plus its `--dsh-chat-content-width`, `--dsh-composer-text-max-height`, `--dsh-content-font-size`, `--dsh-composer-side-clearance`, and `--dsw-*` font variables. Generated hashes can change while a patch still applies. Verify computed fonts, header/composer dimensions, touch targets, overflow, drawer widths, and intermediate animation positions in Chromium.

## Settings policy admission

The desired Web fields are `ui-conversation.config.busyEnter: steer` and `locale.config.preference: zh`. They are valid profile overrides.

Busy-Enter is a preference for supported busy submissions. Native idle/non-steering submissions use Queue, and the accelerated chord selects the complement while busy. Preserve that policy and the outbox's captured mode when adapting versions; a delivery-mode label alone does not establish a preference reset. Published rc.2's native `ui-settings` provider selects Host or memory mode from the served page's loopback classification. Its README expressly documents that authenticated non-loopback pages have inert form writes. This causes local-looking language/send selections to disappear when the client state is recreated: locale uses browser-language selection with English fallback, while busy-send uses Queue. The settings probe declares an English browser to make this signal deterministic.

The native `ui-settings` configuration exposes only the developer-tools `enabled` field. Its client runtime exports `apply`/`inject`; the form/mirror constructors are type-visible but not public runtime exports. The `configForms` consumer API has no persistence setter. Cordis interception supplies configuration only to services that consume it; this provider does not consume an intercepted persistence setting. The native Locale/composer consumers remain bound to those forms.

Remote native preference persistence is therefore unmet under current public configuration/extension APIs. Keep loopback facts and the worker-only `ownsHost` transport fact truthful. Do not treat direct profile edits or process restart as a remote persistence fix. For a new release, inspect its published policy and public configuration before admission. If it offers a supported policy, configure that policy and require [the real settings probe](../tests/project_app_settings.cjs) to pass across non-loopback read/save, reload, fresh contexts, reconnect, and Runtime restart. If no supported policy exists, retain/report the limitation explicitly.

## Deployment

All deployment commands are for the authorized host and target Project VM. Never use them on the Runtime hosting the agent session. Preserve current remote-domain/IP arguments and private environment files.

### Host image and shell

After transferring the checkout, keep the patch copies synchronized and use the normal host setup:

```sh
cmp patches/deepseek-harness-project-app.patch root/.dsh/devvm-project-app.patch
./setup-devvm.sh --service --remote
```

Setup checks whether the architecture-specific archive is absent or whether `Dockerfile`, `scripts`, or `patches` have newer inputs. It can skip an unchanged image; `--skip-image` always skips. Release verification can build/export explicitly with `./build.sh`. Docker's content cache remains available. Shared `root/` configuration is mounted into guests, not copied into the image.

The daemon's shell is compiled into the executable, so redeploy its rebuilt binary. A waiting service worker asks users to close/reopen DevVM windows; it does not force active conversations to refresh. A rebuilt image is used for newly created VMs. Existing machine creation is idempotent and retains the old machine/global CLI; restart does not replace that installation.

### Existing VM: same DSH release, new owner patch

Stop only the target Project's DSH through lifecycle controls, keep its VM running, and preserve its exact deployed patch receipt before updating shared sources. New images carry the receipt at `/opt/devvm-patches/project-app.patch`. A VM updated manually must retain the patch actually applied; an unrelated older receipt cannot validate an upgrade.

First test the complete transition on an owned copy of the target package: current-patch reverse dry run, pristine forward dry run, or exact old reverse followed by current forward, all with zero fuzz. Do not validate only conversation/layout: all eight targets participate. Unknown states must stop before deployment.

From the target Project's registered host directory, the same-version guest update is:

```sh
devvm exec -- bash -lc '
set -euo pipefail
package=/usr/local/lib/node_modules/@deepseek-ai/dsh
new=/devvm-root/.dsh/devvm-project-app.patch
old=/opt/devvm-patches/project-app.patch
test -f "$new"
if patch --batch --reverse --fuzz=0 --dry-run -p1 -d "$package" < "$new"; then
  :
elif patch --batch --forward --fuzz=0 --dry-run -p1 -d "$package" < "$new"; then
  patch --batch --forward --fuzz=0 -p1 -d "$package" < "$new"
else
  test -f "$old"
  patch --batch --reverse --fuzz=0 --dry-run -p1 -d "$package" < "$old"
  patch --batch --reverse --fuzz=0 -p1 -d "$package" < "$old"
  patch --batch --forward --fuzz=0 --dry-run -p1 -d "$package" < "$new"
  patch --batch --forward --fuzz=0 -p1 -d "$package" < "$new"
fi
patch --batch --reverse --fuzz=0 --dry-run -p1 -d "$package" < "$new"
install -m 0644 "$new" "$old"
CI=true DSH_HOME=/root/.dsh dsh plugin --profile web install --force --frozen-lockfile --store-dir /root/workspace/.pnpm-store/v11
'
```

Run this only after checking the target CLI's exact supported release and retaining the old patch. Every command must succeed before launching a fresh target DSH process. If current-forward dry run fails after old removal, keep the Runtime stopped and resolve the package/patch mismatch. Do not force/fuzz a mismatch. Source-only local-package changes require the forced install even when locks match. Headless install is required as well when headless dependencies change.

### Existing VM: different DSH release

An image rebuild does not perform this migration. Back up VM-local Sessions/storages and preserve shared profiles/private environment. Stop the target Runtime; verify the candidate's Node/tool requirements in the existing VM; replace its global CLI with the exact tested candidate; apply the candidate's rebased localhost/owner patches to pristine candidate files; retain its owner receipt; and install matching Web/headless manifests/locks with the guest store. Then launch a fresh target process through lifecycle controls and verify configuration, Sessions/history, Sync, native tools, and browser recovery. Document any candidate-specific data migration before executing it. Never delete/recreate an existing VM as an implicit upgrade step.

## Adapting to a new DSH version

1. Select an exact candidate. Read its published API declarations, schemas, package exports, generated owner files, CSS, migration/Node requirements, and settings policy. Do not infer compatibility from names or a passing patch dry run.
2. Rebase owner behavior at the native seams in the inventory; keep native transport, rendering, and Session authority. Revalidate localhost ingress. Synchronize the distributed owner copy. Preserve deployed receipts for existing-VM transitions.
3. Update admission inputs below against actual candidate APIs. Regenerate affected native profile locks in an owned stage; do not edit lock YAML by hand. Confirm ordinary package files/exports and runtime-peer resolution without source links.
4. Run exact-candidate frozen profile installation, pristine/current/older-patch transition checks, and affected daemon/native tools/plugin tests. Run native controls/mobile/selection and the broad delivery/recovery carrier for native owner rebases. Run the settings admission probe whenever policy changes. Preserve passing and failing evidence separately.
5. Build/export the image and rebuild the daemon on the authorized host. Distinguish new-VM deployment, same-release patch updates, and an explicit existing-VM CLI migration. Refresh installed file-package snapshots and launch a fresh target Runtime.
6. Recheck physical-device, install/update, permission, and real remote-path acceptance. Update active docs/specs with actual results and remaining limits; never substitute emulated/local evidence for physical or remote-path proof.

### Release edit inventory

- [Dockerfile](../Dockerfile): `DSH_VERSION`, and any candidate Node/tool changes.
- [Browser preparation](../scripts/prepare-project-app-browser.mjs): exact CLI assertion, matching replay package, overlay entry IDs/configuration, published replay entry point. [Carrier](../tests/project_app_fixture.rs): CLI entry path. [Recording provenance](../tests/fixtures/README.md): supported source/format.
- Local admission/peers in [Project app](../root/.dsh/plugins/project-app/package.json), [Agent presets](../root/.dsh/plugins/agent-presets/package.json), [Build Loop](../root/.dsh/plugins/build-loop/package.json), [Subagent Manager](../root/.dsh/plugins/subagent-manager/package.json), [Voice Input](../root/.dsh/plugins/voice-input/package.json), [Remote Sync](../root/.dsh/plugins/remote-sync/package.json), [Style Control](../root/.dsh/plugins/style-control/package.json), and [Skill/MCP panel](../root/.dsh/plugins/dsh-skill-mcp-panel/package.json). Their Native API/type admissions must match the actual candidate.
- Web/headless manifests and locks; the Web compatibility pins for third-party sidebar/thinking-effort/context packages; local ordinary-dependency locks when those dependencies change.
- Exact-candidate checks in [native tools](../tests/dsh-rc2-tools.test.mjs), [Build Loop upgrade](../root/.dsh/plugins/build-loop/upgrade.test.mjs), [Remote Sync](../root/.dsh/plugins/remote-sync/test.mjs), and [Context7 registration](../root/.dsh/plugins/dsh-skill-mcp-panel/test-context7-registration.mjs). [Installation](../tests/dsh-plugin-install.test.mjs) verifies packaging, not owner/CSS compatibility.
- Active app/spec/plugin/agent docs and the release integration record. Keep current settings limitations and proof boundaries explicit.
