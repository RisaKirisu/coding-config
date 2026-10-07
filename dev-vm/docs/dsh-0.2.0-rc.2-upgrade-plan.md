# DSH 0.2.0-rc.2 configuration upgrade

> Final implementation requirements supersede the historical plan below: changes are applied directly to `.dsh/` and `Dockerfile`; `setup-devvm.sh` retains its generic rebuild flow, and guest initialization installs only profiles, whose native `file:` package dependencies include custom plugin dependencies; DSH resolves runtime peers without fallback links; MCP uses native HTTP headers with no source patch or `bearerTokenRef`; the panel uses upstream 2.1.3 code; `.dsh/tests` is removed. The separate installation and MCP patch files are deleted. See [the final upgrade handoff](dsh-0.2.0-rc.2-upgrade-handoff.md).

## Outcome and fixed decisions

Implement the approved DSH `0.2.0-rc.2` configuration and plugin upgrade under `dev-vm/` for the user's later rebuild and restart. The user has authorized implementation and directed that verified changes land in `root/.dsh/` and the relevant DSH folders. Implementation supports rc.2 only: remove obsolete settings APIs and active configuration rather than adding compatibility adapters. Preserve the current values, credentials, style document, voice archive, and portable conversation/attachment/storage data.

The user subsequently authorized editing `/root/.dsh/` directly and stated that additional staging is unnecessary. This supersedes the staging-only and source-home read-only edit restrictions below. Configuration and plugin files are updated directly and checked against `dev-vm/root/.dsh/`; credentials, conversations, attachments, storage, and voice/style data remain preserved. Existing staged results are retained as verification evidence, not as an additional required workflow. Direct file authorization does not authorize stopping a process or rebuilding a VM. Process inspection during finalization found an existing Web process using the installed `0.1.5-rc.2` runtime; that process was left running.

Validate on separate copies, then land the verified DSH source changes in repository `root/.dsh/` and the DSH patch/documentation folders, as directed by the user's later implementation instruction. This source-file authorization does not permit operating the running runtime or any VM. Do not install into the running runtime, reinstall installed profiles, stop/start/restart an existing process or service, operate any VM, build images, or contact the production Sync Store. Only isolated test processes may be started and cleaned up. Do not change DevVM daemon/lifecycle code, wrapper/setup scripts, VM configuration, ingress, or networking. The user owns application of the prepared artifacts, rebuilding, restarting, and live verification. DSH-specific installation changes are delivered as a patch for the user; the existing Dockerfile is not edited.

The user has accepted the configuration ownership, dormant-preset nonregistration, shell budget, and native Browser replacement decisions below. API and authentication updates preserve required behavior and are implementation work. Plan acceptance does not itself authorize deployment; the implementer must follow these decisions rather than introduce alternatives.

- Web-editable settings belong in the Web profile patch. Headless gets its own initial copy of provider/default-model settings; subsequent profile edits are independent. Shared structural policy and local executors remain in the home patch. There is no cross-profile configuration synchronizer.
- Register the active custom `standard-bash` preset. Leave `prefab-anchored-standard`, `strict-minimal`, and `minimal-no-bash` directories unchanged but unregistered; do not port their experimental JavaScript or create aliases. Preserving execution of sessions bound to those three presets is outside this forward-only upgrade. Preserve `standard-bash`'s ID and conversations.
- Replace Better Sidebar's old embedded Browser with rc.2's native sidebar Browser by enabling `ui-sidebar-browser` in the Web profile. Keep Better Sidebar for its file tree, editor, changes, tasks, and side chat. Remove its retired browser configuration; use the native Browser's policies rather than invent an equivalent `browserAllowedLoopback` field. Preserve embedded access to guest applications through their existing DevVM Project URLs.
- Honor the saved shell budget: twenty-minute default and cap, 20,000-byte retained output. This deliberately replaces the currently effective sixty-second local-executor default.
- The custom Subagent Manager remains the Web authority for child provider/model/effort. Preserve its existing `isSubagent` predicate and request override behavior, including whichever children that predicate covers. Thinking-effort does not also supply subagent effort. Headless uses explicit native spawn-tool defaults and does not install the custom manager.
- Implement MCP bearer-reference authentication in one rc.2-native MCP transport patch. Keep tokens in the credential store, not Cordis YAML, browser responses, reports, or logs. No anonymous fallback for a configured bearer reference.

Paths below are relative to `dev-vm/`. `S` denotes the already-downloaded target source at `.agents/exploration/dsh-020-rc2/source/`; `T` denotes `.agents/exploration/dsh-020-rc2/third-party/scratch/unpacked/`. These are evidence pointers, not runtime dependencies.

## 1. Prepare isolated configuration artifacts

1. Read the current working-tree values rather than reverting to Git. Treat repository `root/.dsh/` as read-only until verified source changes are ready to land. Credentials, voice archive, style data, session/storage/attachment state, and the installed runtime remain read-only throughout. Do not edit the user's skill files.
2. Use `.agents/exploration/upgrade-rc2-stage/{home,runtime,workspace,logs}`. Copy configuration and actual plugin files into `home`, resolving source links when copying so edits cannot write through to the live tree. Exclude live `node_modules`, sockets, locks, and runtime temporary files. All `root/.dsh/...` edit paths in this plan name final destinations; make those edits only to their corresponding staged `home/...` copies.
3. Install exact target npm packages only under the staged `runtime` prefix if needed for isolated validation; do not use global installation or build a VM/container image. Set `DSH_HOME` to the absolute staged `home`, `TMPDIR` to staged `home/test-data/tmp`, and `DSH_PACKAGE_ENTRY` to staged `runtime/node_modules/@deepseek-ai/dsh/package.json`. Use staged `workspace` as cwd. Run only the staged CLI and staged plugin tests.
4. For validation, rewrite `link:/root/.dsh/plugins/<name>` to the absolute staged plugin copy. Use a staged-only Web overlay with entry `tool-voice-input.config.file` pointing to staged `home/test-data/archive_voice_input.jsonl`; entry `remote-sync.config` has `dshHome` equal to staged home, `statusFilePath` under staged `home/test-data`, `projectId: 00000000-0000-4000-8000-000000000001`, `retryDelayMs: 0`, and `syncConfig: {remote_sync_root: <absolute staged home>/test-data/sync-store, writer_id: rc2-candidate}`. Omit `ssh_host`. Set `style-control.config.filePath` to staged `home/style-presets.json`. Create temp/archive/store paths before tests. Pass this overlay explicitly to any isolated Web test subprocess; headless receives no Web-only overlay.
5. Copy the complete original settings document to staged `home/settings.yaml.pre-020-rc2`, failing instead of overwriting an existing archive, then remove `settings.yaml` only from the staged home after migration. At source landing, archive the current repository `root/.dsh/settings.yaml` under that name and remove the source legacy file after migrating its values. Never rename the hosting runtime's separate `/root/.dsh/settings.yaml`. Do not use the target's automatic importer as the migration procedure.
6. Export the finished artifact home separately from the validation home, restoring canonical production `link:/root/.dsh/plugins/...` specifications and lockfile link references. Remove test overlays, test stores, copied private credentials, installed dependency directories, and runtime state from the distributable artifact. Keep source credentials and state in place; the artifact supplies configuration/plugins and an explicit source-file change list, not a replacement state directory.

Completion: validation operates only on staged copies; verified source files land in repository `.dsh/` and the relevant DSH folders. The installed runtime, VMs, credentials, preserved data, and production Sync Store remain unchanged.

## 2. Pin packages and prepare DSH installation changes

### Manifest edits

In `root/.dsh/profiles/web/package.json`, preserve bundle order and every existing local `link:` dependency. Replace only the two registry specifications:

```json
"@hytime/dsh-thinking-effort": "0.3.6",
"dsh-better-sidebar": "0.24.1"
```

Keep `dsh-skill-mcp-panel` linked to its local fork. Replace that fork's vendored published artifacts with upstream `2.1.3` as specified in section 6, and set its local version to `2.1.3-devvm.1`. Add the new declarative-preset bundle from section 4 once, immediately after `@deepseek-ai/dsh-web-app`.

Retain `root/.dsh/profiles/headless/package.json` with base/headless bundles only, empty third-party dependencies, and `patchReload: startup`.

Use these local package versions: Build Loop `3.0.0`, Subagent Manager `2.0.0`, Voice Input `2.0.0`, Remote Sync `0.2.0`, Style Control `1.0.1`, and the new preset bundle `1.0.0`. In each local manifest, set every existing `@deepseek-ai/dsh*` peer to exact `0.2.0-rc.2`, and add `@deepseek-ai/dsh: 0.2.0-rc.2` as an admission constraint. Imported DSH packages must be declared as target peers, not old private runtime dependencies. Use target vendor peers `@deepseek-ai/cordis: ~4.0.4` and, where imported, `@deepseek-ai/schemastery: ~3.18.4`. Do not change unrelated dependencies or use `allow-version`.

For the panel fork, move `@deepseek-ai/dsh-home-paths`, `@deepseek-ai/dsh-typert-protocol`, and `@deepseek-ai/dsh-subprocess` from its old DSH dependencies to exact target peers; declare the credential service dependency used by its host integration. Preserve the new upstream non-DSH dependencies, exports, assets, client injection metadata, and CLI entry. Preserve local tests and credential-specific files during the rebase.

Keep Web workspace settings `nodeLinker: hoisted`, `autoInstallPeers: false`, `node-pty: 1.2.0-beta.15`, and the existing build permissions. The linked panel has its own isolated pnpm workspace and lock; install its ordinary SDK/probe dependencies there before installing Web. Retain the existing `plugins/node_modules -> ../profiles/node_modules` fallback for native DSH peers. A Web `link:` dependency alone does not install dependencies beside the plugin source. Regenerate Web/headless locks only in staged copies using the staged target runtime. In the exported manifests and locks, restore canonical `link:/root/.dsh/plugins/...` specifications and references. Do not export staging paths, installed node_modules, or live-profile edits.

### DSH-only installation patch

Prepare `patches/dsh-0.2.0-rc.2-installation.patch` for the user to apply during their rebuild. Limit its Dockerfile diff to `DSH_VERSION: 0.2.0-rc.2`, removal of the existing DSH hash guards, and application of the MCP package patch. Leave the actual Dockerfile and every other DevVM file unchanged. Keep `patches/deepseek-harness-localhost-subdomains.patch`; it already dry-runs against the exact target.

The user's implementation update explicitly prohibits `sha256sum`; do not add package hash guards. Pin exact rc.2 and validate patch applicability with `patch --dry-run`; patch failures remain fatal. Verify `.localhost` handling and bearer-reference support after applying patches to the isolated runtime.

Completion: the user receives a DSH-only installation diff and both package patches; staged local packages admit rc.2 and reject the old runtime, and no dependency pulls a private old DSH service implementation. No image has been built and no running runtime has been changed.

## 3. Replace legacy settings with exact configuration entries

### Shared home patch

In `root/.dsh/cordis.patch.yml`, preserve:

- `sandbox-policy.config.mode: danger-full-access` and `workspaceRoot: !!js process.cwd()`.
- `disabled: true` for `bash-sandbox`, `pwsh-sandbox`, `fs-sandbox`, `permission`, and `approval`. Keep `sandbox` enabled: rc.2's native PTC provider injects that service. The `danger-full-access` policy preserves unrestricted execution; enabling the service adds no confinement.
- The existing `bash-local` and `fs-local` insertions, with the following complete Bash config:

```yaml
- insert:
    - id: bash-local
      name: '@deepseek-ai/dsh-bash-local'
      config:
        timeoutMs: 1200000
        maxTimeoutMs: 1200000
        maxOutputBytes: 20000
    - id: fs-local
      name: '@deepseek-ai/dsh-fs-local'
```

Do not add a fixed `cwd`; session/call cwd remains authoritative. Remove the home-level `compaction-basic`, `tool-result-pruner`, and `tool-web` overrides after transferring their values below. Do not add providers, model defaults, UI settings, or editable local-plugin values to the home patch: it outranks profile patches, and the Web editor cannot save over it. The local executor is intentionally deployment-owned; changing its budgets requires editing this home patch, not its Web form.

### Complete settings mapping

Write one complete config row per destination in `root/.dsh/profiles/web/cordis.patch.yml`. Existing bundle entries are overridden by ID; do not insert second instances. Retain the existing `ui-permission.disabled: true` row and the single marker-managed Context7 block. Enable the existing native Browser entry with this exact override; do not add a second Browser package or instance:

```yaml
- id: ui-sidebar-browser
  disabled: false
```

| Source in the archived settings document | Destination entry ID and field | Exact action/value |
|---|---|---|
| `ui-onboarding.welcomeNoticeVersion` | `ui-settings-general.config.welcomeNoticeVersion` | Preserve string `2026-08-13.1`. |
| `agent-default-model.provider/model/reasoningEffort` | `agent-default-model.config.provider/model/reasoningEffort` | Preserve the current source selection: `krill-codex`, `gpt-6.1-sol`, `high`; restate all three. |
| `llm-pi-ai.providers` | `llm-pi-ai.config.providers` | Copy the complete dict unchanged; field coverage follows. |
| `llm-pi-ai.subagentEffort` | No additional active setting | Remove redundant `high` from core LLM config. Subagent Manager retains `high`; explicitly set `thinking-effort.config.subagentEffort: ''`. |
| `agent-presets.default` | `agent-preset-registry.config` | Set the complete config to `{default: standard-bash, selectedDefault: standard-bash}`. The target entry is **agent-preset-registry**, not `agent-presets`. |
| `ui-conversation.busyEnter` | `ui-conversation.config.busyEnter` | Preserve `steer`. |
| `permission.defaultPreset` | No active destination | Archive only. Permission remains disabled; do not re-enable it to migrate an inert value. |
| `locale.preference` | `locale.config.preference` | Preserve `zh`. |
| `shell.timeoutMs/maxOutputBytes` | Home `bash-local.config` | Apply the budgets above; remove the old `shell` namespace. |
| `ui-theme.preference` | `ui-theme.config.preference` | Preserve `system`. |
| `subagent-model.provider/model/reasoningEffort` | `subagent-manager.config.provider/model/reasoningEffort` | Preserve the current source selection: `krill-codex`, `gpt-6.1-sol`, `high`; its schema is created in section 5. |
| `build-loop: {}` | `build-loop.config` | Use `{}` so the exported target schema supplies the unchanged four defaults. No prompt values are stored in this source section. |
| `llm-deepseek.models` | `llm-deepseek.config.models` | Copy all three complete model objects unchanged. Target bundle owns this ID through `@deepseek-ai/dsh-llm-deepseek-api-key`; do not mount a second adapter. |
| `dsh-better-sidebar.workspaceFence` | No active destination | Remove retired `false` field. Do not fabricate a replacement flag. |
| `dsh-better-sidebar.agentOpenTools` | `better-sidebar.config.agentOpenTools` | Preserve `false`. |
| `dsh-better-sidebar.browserAllowedLoopback` | No equivalent field; native entry `ui-sidebar-browser` is enabled | Remove the retired Better Sidebar allowlist and retain it in the archive. Use the accepted native Browser replacement and its own policies; do not copy the key into native config. |
| `dsh-thinking-effort.opencodeSession` | `thinking-effort.config.opencodeSession` | Preserve `{providers: {proxy-cli: {models: {}}}}`. Combine with `subagentEffort: ''` in this same row. |

The `llm-pi-ai` dict has exactly six routes: `krill-codex`, `krill-china`, `amazon-bedrock`, `proxy-cli`, `openrouter`, and `krill-claude`. Retain every present provider-level field at the same nested path: `displayName`, `apiKeyEnv`, `api`, `baseURL`, `models`, `modelOverrides`, `compat`, `defaultContextWindow`, `defaultMaxTokens`, `defaultInput`, `headers`, `thinkingBudgets`, `streamIdleTimeoutMs`, `maxRequestImageBytes`, `requestImagePixelBudget`, and `requestImageMaxBytes`. Retain model `id`, `name`, `contextWindow`, `maxTokens`, `input`, `reasoningEfforts`, and `compat`. Preserve all effort map values—including nulls—and the existing empty `chatTemplateKwargs`/`chatTemplateArgs` dictionaries. Do not rename explicit model IDs, inference-profile ARNs, API-key reference names, or route names. Keep `api` absent for Bedrock/OpenRouter; do not force them through an invented generic protocol.

For DeepSeek models, preserve every listed `id`, `name`, `description`, `contextWindow`, `inputModalities`, `imagePixelBudget`, and `imageMaxBytes` field that is present. Do not introduce removed `protocol`/`imageDetail` fields. The current document does not contain those fields, or removed pi-ai `provider`/`maxRetries`/`maxRetryDelayMs`; no transformation for absent fields is needed.

### Headless patch

Replace `root/.dsh/profiles/headless/cordis.patch.yml`'s empty array with these exact base-entry overrides:

- `llm-pi-ai.config.providers`: the same complete six-route dict initially copied into Web.
- `llm-deepseek.config.models`: the same complete three-model list.
- `agent-default-model.config`: `{provider: krill-codex, model: gpt-6.1-sol, reasoningEffort: high}`.
- `compaction-basic.config`: `{thresholdRatio: 0.5, maxTokens: 10000, compactionRetries: 5, maxOverflowRetries: 5}`.
- `tool-result-pruner.config`: `{thresholdChars: 8192, headChars: 2048, tailChars: 1024}`.
- `tool-web`: `disabled: false`, config `{search: false, fetch: true, searchTimeoutMs: 60000}`.
- `tool-subagent.config`: `{provider: spawn, toolName: subagent, backgroundMode: continuable, agentOptions: {provider: krill-codex, model: gpt-6.1-sol, reasoningEffort: high}}`.

Do not put Web-only IDs, a Web preset registry, third-party preferences, or overrides on `tool-subagent-fork` in headless. It uses the direct base composition, not Web declaration presets.

Cordis config patches replace a row's whole config. Emit complete owned configs; do not write successive partial rows that erase earlier fields. Completion: every source settings field is either mapped above or archived as explicitly retired; neither profile depends on the legacy importer; editable Web settings can save and survive restart.

## 4. Declare the active preset

Create `root/.dsh/plugins/agent-presets/package.json` with name `@devvm/dsh-agent-presets`, version `1.0.0`, `private: true`, `type: module`, `files: [cordis.patch.yml]`, export `./cordis.patch.yml`, and `dsh.bundle.patch: ./cordis.patch.yml`. Declare exact rc.2 peers for the CLI, `@deepseek-ai/dsh-agent-preset`, and `@deepseek-ai/dsh-agent-preset-registry`. Add `@devvm/dsh-agent-presets: link:/root/.dsh/plugins/agent-presets` to Web and include it once in the bundle list after web-app. Do not add it to headless.

Its `cordis.patch.yml` inserts exactly one declaration:

```yaml
- insert:
    - id: preset-standard-bash
      name: '@deepseek-ai/dsh-agent-preset'
      config:
        id: standard-bash
        name: 标准模式-终端
        description: 标准编码 Agent，仅通过终端处理文件；保留网页检索、Skills、计划、目标、子代理和工作流。
        order: 2
        plugins: # Full list copied and edited as directed below, not an include/path.
```

Copy the entire list from `root/.dsh/.agent-presets/standard-bash/agent.cordis.yml` into `plugins`, preserving every row/group and isolation map, then apply only these edits:

1. Replace `workflow-worker-thread` with ID `workflow-ptc`, package `@deepseek-ai/dsh-workflow-ptc`, and unchanged `{provider: spawn}` config.
2. Set the compaction group's `compaction-basic.config` to `{thresholdRatio: 0.5, maxTokens: 10000, compactionRetries: 5, maxOverflowRetries: 5}`.
3. Set that group's `tool-result-pruner.config` to `{thresholdChars: 8192, headChars: 2048, tailChars: 1024}`.
4. Set preset `tool-web` to `disabled: false`, config `{search: false, fetch: true, searchTimeoutMs: 60000}`. Leave the Web host's same-named tool disabled.
5. Preserve persona prefix/suffix, `agent-instructions.maxBytes: 65536`, `tool-jobs.maxConsecutiveWakes: 100`, the current plan-mode instruction text, continuable spawn/fork tool rows, `tool-todo.allowParallelInProgress: true`, and active Ralph with `{subagentProvider: spawn, maxRounds: 64}`. Keep Codex/Claude tools disabled. Do not add model-facing `tool-fs` or `tool-fs-search` to this terminal-only preset.

After the declaration is verified, remove only the now-duplicated legacy `standard-bash` directory from the runtime configuration tree; the new declaration is its single source. Do not register the three dormant experimental directories. Leave stock target presets supplied by web-app alone. Do not add local PowerShell executors: DevVM is Linux and the new custom preset retains its disabled Windows-only tool row.

Completion: the roster contains one `standard-bash`, it is selected for new Web sessions, historical `standard-bash` sessions reopen under that same ID, and its scoped tool/compaction behavior matches the explicit configuration above.

## 5. Port local plugin APIs

### Build Loop

Edit `root/.dsh/plugins/build-loop/config.mjs` and `index.mjs`:

- Export a namespace `Config` from `index.mjs` so Loader sees it. All four fields are volatile: `provider` (non-whitespace string, default `spawn`), `maxFixRounds` (safe nonnegative integer, default `3`), `reminderTokens` (safe positive integer, default `100000`), and `deniedTools` (string array, default the existing exact `DEFAULTS.deniedTools`). Keep the current validation constraints and instruction-file loading; do not manufacture prompt settings.
- Change `apply(ctx)` to `apply(ctx, config)`. Remove `settings.register` and the returned old namespace scope. Read each volatile field with `.get()` and pass a plain, detached four-field snapshot to `Controller.start` so each run pins its policy.
- Keep `/api/build-loop/config` response shapes unchanged. GET/HEAD return `{config, defaults}` from live Config/defaults. POST validates the full submitted config and calls `ctx.settings.replace('build-loop', next)`. DELETE calls `ctx.settings.replace('build-loop', {})`, then returns current values/defaults. Keep HTTP 400 for invalid values, including `maxFixRounds: -1`. If suppressing generic auto-generated forms, register `settings.configure({auto: false}, ctx.fiber)` effectfully.
- In background dispatch, pass `owner: exec.agent.session.id`, and settle with `{status, result: value.text}` rather than `output`. Preserve cancellation wiring and failure detail.

Edit `workers.mjs`:

- In `collect`, use `ownerId = worker.child.localAgent.session.id` for `jobs.list`, `kill`, `get`, `wait`, and `read`. Filter `JobView.owner === ownerId`, not `ownerSession`. Preserve quiescence before handoff and drain/consume results once.
- Replace injected message source `{kind: 'plugin', plugin: 'build-loop', form: 'instructions'}` with producer-owned `{kind: 'build-loop', form: 'instructions'}`. Rc.2 has no shared catch-all plugin source.
- Keep existing `snapshotEvents` calls for this upgrade; target still supports them. Do not broaden into a projection/event-reader refactor. Preserve caller approval, triage, builder retention, audit retry, and cancellation behavior.

### Subagent Manager

Edit `root/.dsh/plugins/subagent-manager/index.mjs`:

- Export `Config` with volatile strings `provider`, `model`, and `reasoningEffort`, each default `''`; retain the current normalization/predicate helpers. Read live references directly; remove the `subagent-model` namespace and `settings.register`.
- Keep the `agent/request` waterfall next-first, then apply the existing predicate and configured provider/model/effort rewrite. No new hook, alternate precedence scheme, or additional fork override is introduced.
- Preserve endpoint shapes: GET `/api/subagent-manager/config` returns the normalized three-field snapshot; POST saves through `ctx.settings.update('subagent-manager', next)`; `/models` continues to use the target LLM directory. Keep the wait tool and its cancellation/timeout behavior.
- In Web configuration, leave core `subagent-model-selection-settings` disabled by configuration (`enabled: false`, `allowedModels: []`) rather than enabling a second model-selection authority. Keep thinking-effort's own `subagentEffort: ''`.

### Voice Input and Style Control

Keep Voice Input's tool names and production archive path unchanged; update its manifest as above. Keep Style Control's existing JSON format, per-session/global selection, HTTP endpoint shapes, prompt injection, and client loader. Do not migrate these data files into unrelated settings namespaces or rewrite the clients solely because `dsh.client.inject` is absent. Update only target incompatibilities that fail the named acceptance checks, preserving these specified interfaces.

### Remote Sync

In `root/.dsh/plugins/remote-sync/index.mjs`, retain `sessions.flush`, event hooks, three Web endpoints, head-marker sequencing, retries, and union/projection/storage transfer separation. Add both authoritative generic-file paths to `UNION_FILTER_ARGS` before `--exclude=*`:

```text
--include=attachments/v1/file-objects/
--include=attachments/v1/file-objects/***
--include=attachments/v1/files/
--include=attachments/v1/files/***
```

Retain image object inclusion and `session.lock` exclusion. Before `--include=sessions/***`, add these exact session staging exclusions: `--exclude=session.migration.*.tmp`, `--exclude=session.v*.jsonl.*.tmp`, and `--exclude=session.v*.jsonl.zstd.*.tmp`. They cover rc.2's `session.migration.<token><suffix>.tmp` and `<finalPath>.<12-hex>.tmp` writers (`S/packages/session/session-persistence-jsonl/src/generation.ts:728`, `src/index.ts:1273`); do not exclude committed `.jsonl` or `.jsonl.zstd` files. Keep committed predecessor/successor generations eligible for the union. Do not synchronize request-image caches or turn projection checkpoints into append-only files.

Completion: all local tools load on rc.2; configuration uses target APIs only; background jobs return their reports and cancel correctly; portable committed V4 state and both image/file attachment types synchronize without transferring locks or migration staging.

## 6. Rebase the panel and implement native MCP authentication

Replace the local panel's upstream-owned `lib/`, `assets/`, `cordis.patch.yml`, locale metadata, and documentation with the published `2.1.3` artifacts. Reapply only the local bearer-credential changes from the existing `lib/mcp/{credentials,model,gateway,probe,wire}.js`, `lib/cli-mcp.js`, and `lib/client.js`; retain test files. This repository vendors built JS, not a complete upstream TypeScript checkout: do not claim a local `tsc` rebuild without adding that source. Use upstream 2.1.3 RPC codec `.create` and session lookup implementations unchanged.

Preserve these local behaviors exactly: `MCP_<UTF8-server-name-as-uppercase-hex>_TOKEN` reference naming; bearer token input on Web/CLI save; an omitted token on edit keeps the stored token; renaming carries its token to the new reference then removes the old reference; switching authentication to none or deleting a server removes the owned reference. Redacted views expose configured state, never the token. Probes use stored tokens and the same transport/security behavior as live connections. Add `credentials` to the local panel's exported `inject` list in `lib/index.js`. Host gateway code uses `ctx.credentials.resolve/set/unset` with `credentialRef(...)`, reading `.value` only inside the host transport/probe path; preserve non-secret configured-state responses. CLI-only credential helpers may keep the same file format and must not clobber other refs/records. Pass a credential resolver into the probe from its host or CLI caller so both use per-request resolution, the same no-fallback rule, endpoint-origin restriction, and redirect rejection; do not cache the token in probe configuration.

Create `patches/deepseek-harness-mcp-credential-ref.patch` against the exact published rc.2 `@deepseek-ai/dsh-mcp-client` artifacts, not against the downloaded TypeScript tree:

1. Patch `package.json` to declare peer `@deepseek-ai/dsh-credentials: 0.2.0-rc.2`.
2. Patch `lib/index.js` imports/injection to use the target credential service and constructor. Add optional HTTP `bearerTokenRef` to the actual exported Config; validate it using `credentialRef(...)` before connecting. Stdio and unauthenticated HTTP remain native behavior.
3. Pass `ctx` into the internal transport factory. For bearer HTTP, provide `StreamableHTTPClientTransport`'s supported `fetch` option. Before each network request, resolve the reference through `ctx.credentials.resolve`; if missing/empty, throw a reference-specific configuration error without issuing the request. Overlay `Authorization: Bearer <resolved.value>` on a copy of the SDK's request headers, preserving its other headers, body, method, and cancellation signal.
4. Allow credential injection only for the configured endpoint origin and reject redirects (`redirect: 'error'`); never forward a credential through a redirect or to an origin substituted by the SDK. Do not write the resolved token back to Loader config or retain it between operations. Credential changes reach the next request, including reconnect requests.
5. Update the published `lib/types/index.d.ts` HTTP config declaration with the optional reference field so runtime and public declaration agree. No duplicate MCP provider, new service, or global environment bridge is introduced.

Keep the production marker-managed Context7 row in `root/.dsh/profiles/web/cordis.patch.yml` exactly once with package `@deepseek-ai/dsh-mcp-client`, ID `panel-mcp-context7`, and config:

```yaml
serverName: context7
toolCallTimeoutMs: 60000
failOnStartupError: true
reconnect:
  enabled: true
  initialDelayMs: 500
  maxDelayMs: 30000
  maxAttempts: 10
transport: streamable-http
url: https://mcp.context7.com/mcp
bearerTokenRef: MCP_636F6E7465787437_TOKEN
headers: {}
```

`bearerTokenRef` remains a deliberate local extension implemented by this patch, not an unsupported key copied into stock rc.2. Do not replace it with literal `headers.Authorization`.

Completion: the panel uses 2.1.3's target RPC/session APIs, Context7 receives the referenced token, token rotation affects the next request, and neither secrets nor an anonymous fallback appear in persisted config or responses.

## 7. Tests and acceptance gates

First repair test isolation, not production behavior:

- Remove hardcoded imports of `/usr/local/lib/node_modules/...` from `remote-sync/test.mjs`, `style-control/test.mjs`, and any Voice Input tests that use them. Resolve packages through the candidate profile/runtime. Candidate assertions must verify runtime version `0.2.0-rc.2`.
- Replace Style Control's hardcoded live-home fixture directory with a temp directory under candidate `TMPDIR`.
- Replace Remote Sync's live-profile Web launch with the actual `dsh web --profile web --no-open --host 127.0.0.1 --port 0` application command, an isolated `DSH_HOME`, workspace, status path, local Sync Store, and archive path. Parse the ready URL/assigned port rather than fixing port 3599. No production profile or store may be consulted by a fixture.
- Replace obsolete persistence filenames/assertions with rc.2-created session generation files and real target projections. Use the target Loader composition for persistence integration instead of assembling the old incomplete service list. Use real dependencies; do not add hand-rolled service or LLM mocks.
- Keep pure helper tests. Add focused real-composition cases for the settings/jobs changes and real local HTTP MCP servers for header/redirect/token-rotation checks. Do not run the upstream monorepo's full suite.

Run validation only against the staged copies and target runtime. From the `dev-vm` checkout, set the following environment. Every test subprocess inherits it; Web fixture arguments explicitly include the staged overlay from section 1. Record full logs with `tee` and `set -o pipefail`. Never substitute the current global `dsh` command.

```sh
DSH_STAGE="$PWD/.agents/exploration/upgrade-rc2-stage"
export DSH_HOME="$DSH_STAGE/home"
export TMPDIR="$DSH_STAGE/home/test-data/tmp"
export DSH_PACKAGE_ENTRY="$DSH_STAGE/runtime/node_modules/@deepseek-ai/dsh/package.json"
mkdir -p "$TMPDIR" "$DSH_STAGE/logs"
set -o pipefail
node --test "$DSH_HOME/plugins/build-loop/test.mjs" \
  "$DSH_HOME/plugins/subagent-manager/test.mjs" \
  "$DSH_HOME/plugins/voice-input/index.test.mjs" \
  "$DSH_HOME/plugins/style-control/test.mjs" \
  "$DSH_HOME/plugins/remote-sync/test.mjs"
npm --prefix "$DSH_HOME/plugins/dsh-skill-mcp-panel" test
"$DSH_STAGE/runtime/node_modules/.bin/dsh" --profile web --dump-config
"$DSH_STAGE/runtime/node_modules/.bin/dsh" --profile headless --dump-config
```

Run newly added Node tests explicitly as well. Passing old helper tests alone is insufficient. The table specifies expected target behavior. Configuration, API, local transport, and persistence checks run on isolated fixtures; real provider calls, guest-application browsing, and deployment checks belong to the user after rebuilding/restarting. Report deferred checks as unverified rather than operating a VM to complete them.

| Area | Required observation |
|---|---|
| Configuration | Both profile dumps contain the exact mapped values. Web has every selected bundle once, no retired active keys, exactly one custom declaration with `config.id: standard-bash` (stock target declarations remain), and one Context7 row. Headless has no Web/local-Web bundles. No legacy import runs. |
| Editable settings | Models/default model, locale/theme/conversation, Build Loop, Subagent Manager, sidebar preference, and thinking-effort changes save through the Web UI, affect the running composition, and survive restart. Home-owned shell changes are documented as deployment edits, not falsely promised as editable. |
| Providers | Default `krill-codex/gpt-6.1-sol/high` completes a short request. Exercise one existing configured model per credentialed route and one configured DeepSeek model; do not replace explicit IDs to make tests pass. Record missing credentials as unverified, and do not claim an untested route works. |
| Preset/tools | New and reopened `standard-bash` sessions use the declaration; filesystem access remains terminal-only; fetch exists and search does not; Ralph/workflow are available; spawn/fork retain their specified tool modes; compaction/pruner budgets match the declaration. |
| Build/subagents | A tiny Build Loop ticket reaches design review, executes a real approved check, receives audits, and can be accepted; background output is collected; cancellation stops/reaps owned work. Child model/effort follows the specified manager/default precedence. Invalid settings return 400. |
| Voice/style/sidebar | Archive/remove tools act only on candidate data; existing style selections work per session without resetting the global choice; Better Sidebar files/editor load with `agentOpenTools:false`. The native Browser tab loads, navigates to a guest application through its existing DevVM Project URL, and renders/interacts successfully under the actual local and remote DSH browser origins. A failure here blocks cutover; do not silently substitute an external tab. |
| Panel/MCP | Skills list/enable/disable and session-bound lookup work; server add/edit/rename/delete and enable/disable work; real test server receives bearer auth, a missing reference produces no request, rotation changes the next request, and redirects do not receive credentials. Context7 tool discovery and one real call succeed with configured auth. |
| Persistence/sync | On copied standard-bash V3 data, rc.2 publishes its V4 successor without modifying the predecessor; restart/reopen/append work. Two isolated homes exchange committed sessions, image objects, generic file blobs/metadata, projection records, and storage data; lock/migration files stay local. Reopen on the receiving instance. |

Stop when staged configuration and plugin checks pass and the requested artifacts are ready. Clean up only processes started for isolated tests. Leave all running DSH/DevVM processes and services alone. Clearly list the user-owned live checks still unverified; do not treat those as permission to deploy or operate a VM. Do not add unrelated refactors, provider catalog rewrites, speculative guards, or a compatibility matrix.

## 8. Deliver DSH artifacts; user applies and rebuilds

Export clean files from the validation home, then land the verified profile manifests/locks/patches, local plugin sources/tests, and preset declaration in repository `root/.dsh/`. Land the MCP package patch and DSH-only installation diff in `patches/`. Archive repository `settings.yaml` as `settings.yaml.pre-020-rc2` and remove its active legacy file and duplicated legacy `standard-bash` directory. Include a source-relative manifest of additions, replacements, and removals. Exclude credentials, session/storage/attachment data, node_modules, test overlays/stores, and validation runtime state. Preserve existing source data and dormant presets.

Update the verified DSH/plugin READMEs and this DSH upgrade documentation with the new layout, profile ownership, native Browser replacement, MCP patch, and generic-file attachment paths, then land them with the source changes. Do not edit DevVM's `readme.md`, `AGENTS.md`, `docs/remote-access.md`, daemon, wrapper, setup, ingress, or VM configuration.

Hand off the landed source paths, exact tests run, and deferred live checks. The user applies the DSH-only installation patch, installs profile/plugin dependencies, rebuilds, and restarts using their existing process. Do not supply or execute an existing-VM migration procedure.

Done: verified DSH sources are landed, the installation patch is complete, isolated checks pass, and the handoff names checks requiring the user's later rebuild/restart. The installed runtime, VMs, DevVM code/configuration, credentials, and production state have not been modified.

## Evidence pointers

Implementation status: the configuration/plugin files are updated directly in `/root/.dsh/` and match `dev-vm/root/.dsh/`. Completed isolated validation established the behavior described in [the upgrade handoff](dsh-0.2.0-rc.2-upgrade-handoff.md). The final settings-fixture recheck after npm-prefix isolation was interrupted; its earlier browser/settings/panel run and the final Remote Sync isolation recheck passed. The source change list is [dsh-0.2.0-rc.2-source-changes.tsv](dsh-0.2.0-rc.2-source-changes.tsv). Installed runtime replacement, rebuild, restart, real provider calls, production Context7 access, and guest-application browsing remain pending.

- Exact configuration review: `.agents/exploration/upgrade-plan-review/config-review.md` (every source section, target schema, and ownership rule).
- Target configuration editing: `S/packages/boot/config-editor/src/index.ts:127-142`; settings importer: `S/packages/settings/settings/src/index.ts:238-257`.
- Preset declaration/registry: `S/packages/preset/agent-preset/src/index.ts:16-23`; `S/packages/preset/agent-preset-registry/src/index.ts:53-74`; target standard composition: `S/packages/bundle/web-app/presets/standard.patch.yml`.
- Target job results/ownership: `S/packages/jobs/jobs/src/types.ts:16-30,137`; producer-owned message sources: `S/packages/llm/llm/src/message.ts:103-115`.
- MCP native transport/config: `S/packages/mcp/mcp-client/src/transport.ts:40-44`, `src/index.ts:119-142`; target credential resolution: `S/packages/credentials/credentials/src/index.ts:175-209`.
- Exact published MCP artifact: https://registry.npmjs.org/@deepseek-ai/dsh-mcp-client/0.2.0-rc.2 ; SDK fetch option: `.agents/exploration/upgrade-plan-review/mcp-sdk/dist/index.d.mts:3050-3054`, published at https://registry.npmjs.org/@modelcontextprotocol/client/2.0.0 .
