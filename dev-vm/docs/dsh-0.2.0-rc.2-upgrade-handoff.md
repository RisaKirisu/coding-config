# DSH 0.2.0-rc.2 upgrade

The upgrade is applied directly to `/root/.dsh/`, the shared source under `dev-vm/root/.dsh/`, and `Dockerfile`. Run `./setup-devvm.sh` through the existing host setup process. No manual patch application or separate profile dependency installation is required. Setup retains its original generic image-build flow.

## Installation

The Dockerfile pins DSH to `0.2.0-rc.2`. Setup rebuilds the image when its inputs change. Guest initialization in `devvm` only runs native frozen-lockfile installs for the Web/headless profiles using the existing workspace pnpm store. The Web profile declares local bundles as relative `file:../../plugins/<dir>` packages, so pnpm installs their ordinary dependencies in the same operation. DSH resolves runtime peers natively; there is no plugin-specific install or fallback-link creation. Installation runs only when the installed lock differs or is absent. `--skip-image` retains its existing meaning.

Installed local packages are snapshots. After source-only changes, run `dsh plugin --profile web install --force --frozen-lockfile` with the configured store, then restart the runtime through the existing lifecycle. After dependency changes, regenerate and retain the Web profile lockfile. Startup reconciliation uses the installed Remote Sync package rather than its source tree. See `AGENTS.md` for the plugin update procedure.

Only `patches/deepseek-harness-localhost-subdomains.patch` remains required for DSH's existing DevVM browser origins. The obsolete DSH hash guards are removed; patch applicability still fails the build on error. There is no MCP source patch and no separate installation patch file.

The running guest runtime observed during preparation still used `0.1.5-rc.2`. Source preparation does not replace an already running guest process. Existing VM recreation/restart remains governed by the existing DevVM lifecycle; setup does not delete existing VM state.

## Configuration and plugins

Web-editable settings live in `profiles/web/cordis.patch.yml`. Headless owns its independent provider/default-model copy and native spawn defaults. Shared execution policy and the twenty-minute Bash default/cap with 20,000 retained output bytes live in the home patch. The complete legacy settings document is archived as `settings.yaml.pre-020-rc2`.

`plugins/agent-presets/cordis.patch.yml` is a declarative Cordis bundle using rc.2's native `@deepseek-ai/dsh-agent-preset` entry. It registers the historical `standard-bash` ID once, preserving terminal-only filesystem tools, native PTC workflow, Ralph, and configured compaction/pruner budgets. It contains no custom preset implementation. The duplicated legacy preset directory is removed; dormant experimental directories stay unregistered.

Better Sidebar is pinned to `0.24.1`; Browser uses rc.2's native sidebar entry. Thinking Effort is pinned to `0.3.6`; Web Subagent Manager remains the child model/effort authority. Local packages declare exact rc.2 admission/peers. The panel is upstream published `2.1.3` with local dependency packaging; custom bearer-reference code is removed.

## Native MCP authentication

HTTP MCP uses stock rc.2 `headers` configuration. The existing Context7 token is preserved in the private, gitignored DSH-home `.env`; a native Cordis `!!js` expression supplies its `Authorization` header at startup. No token literal is committed, and no MCP package code is changed. The upstream panel's ordinary secret-header editor remains available and redacts values in its server list responses. Environment-backed authentication updates take effect on reload/restart under native configuration behavior.

There is no `.dsh/tests` directory. The native tool integration check now lives at `tests/dsh-rc2-tools.test.mjs`; local plugin checks remain with their plugins.

## Verification

- Before the native-MCP change, completed isolated runs covered settings save/restart, job ownership/cancellation, voice/style, and V3-to-V4 persistence/rsync, with 62 distinct passing tests. Authentication tests for the removed custom extension are superseded.
- Final native MCP test passes against exact unmodified rc.2 artifacts and a real local HTTP MCP server: headers arrive, tools discover/call successfully, and the upstream panel redacts and preserves secret headers.
- `bash -n setup-devvm.sh`, `bash -n devvm`, and `bash -n build.sh` pass. Setup is byte-for-byte unchanged from Git; setup and the guest wrapper retain executable mode.
- Cargo installation checks were attempted but blocked on the package-cache lock; they are not reported as passed.
- No image build or setup invocation was performed inside the current guest. Real provider requests, production Context7 calls, and native Browser interaction through actual local/remote Project URLs remain live acceptance checks after the host rebuild.
