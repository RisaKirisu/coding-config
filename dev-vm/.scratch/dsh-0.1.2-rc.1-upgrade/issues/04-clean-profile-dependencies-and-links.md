# 04: Clean profile dependencies and restore live links

**What to build:** Remove obsolete profile packages, update verified third-party plugins, and ensure every local web-profile plugin resolves through a pnpm `link:` symlink.

**Blocked by:** 03: Update local plugin compatibility

**Status:** resolved

## Requirements

- Remove explicit `@deepseek-ai/dsh-web-fetch-http` from both web and headless profiles. DSH 0.1.2-rc.1 now includes it in `@deepseek-ai/dsh-base`; do not add a duplicate loader entry.
- Upgrade `@hytime/dsh-thinking-effort` from the old GitHub v0.1.8 resolution to the published DSH 0.1.2-compatible `^0.2.0` package.
- Pin `dsh-better-sidebar` to the verified compatible `^0.18.0` range instead of `latest`.
- Change every local web-profile plugin dependency to `link:/root/.dsh/plugins/<directory>`: remote-sync, subagent-manager, voice-input, dsh-skill-mcp-panel, build-loop, and style-control.
- Regenerate web and headless lockfiles with the repository's required pnpm store. Do not leave stale `file:` package or snapshot entries for local plugins.
- Reinstall the web profile so all six local packages in `node_modules` are symbolic links to their source directories.
- Keep profile bundle configuration valid and avoid duplicate package copies caused by obsolete DSH ranges.

## Regression test

Extend `tests/install_test.rs::test_web_profile_links_first_party_plugins_to_their_sources` to cover all six local plugins and assert both:

1. Each manifest spec is the expected `link:` path.
2. Each installed package path is a symbolic link rather than a hardlinked directory.

The existing repro must turn green:

```sh
cargo test --test install_test test_web_profile_links_first_party_plugins_to_their_sources
```

## Verification

- Run profile installation non-interactively with `CI=true`, `DSH_HOME=/root/.dsh`, and `--store-dir /root/workspace/.pnpm-store/v11`.
- Run `dsh plugin --profile web list` and the corresponding headless check.
- Start DSH 0.1.2-rc.1 and verify there is no `duplicate loader entry id: web-fetch-http` and no missing-peer startup failure.
- Verify Better Sidebar and Thinking Effort client bundles load.
- Run the guarded Rust suite after lockfile regeneration.

## Evidence

Four first-party dependencies currently use `file:` and fail `tests/install_test.rs:25`. `@deepseek-ai/dsh-web-fetch-http@0.1.1-rc.2` duplicates DSH core's built-in 0.1.2 plugin. Thinking Effort is locked to v0.1.8 and Better Sidebar uses an unbounded `latest` spec.

## Answer

### Files changed
- `root/.dsh/profiles/web/package.json`: Removed duplicate `@deepseek-ai/dsh-web-fetch-http`, switched local plugins to exact `link:/root/.dsh/plugins/<name>` specs, upgraded `@hytime/dsh-thinking-effort` to `^0.2.0`, and pinned `dsh-better-sidebar` to `^0.18.0`.
- `root/.dsh/profiles/headless/package.json`: Removed duplicate `@deepseek-ai/dsh-web-fetch-http` dependency.
- `root/.dsh/profiles/web/pnpm-lock.yaml`: Regenerated lockfile against the required store with `link:` dependencies and updated third-party versions.
- `root/.dsh/profiles/headless/pnpm-lock.yaml`: Regenerated lockfile against the required store removing the duplicate built-in fetch package.
- `root/.dsh/plugins/node_modules`: Added symlink pointing to `../profiles/node_modules` so plugins running via `link:` resolve DSH peer packages and dependencies.
- `tests/install_test.rs`: Extended `test_web_profile_links_first_party_plugins_to_their_sources` to check all six local plugins for `link:` specs, symlink file types, and target resolution; added assertions for no duplicate web-fetch across dependencies and bundles, third-party version pins, absence of duplicate web-fetch in headless lockfile, positive `link:` and version pins in web lockfile, and fallback `plugins/node_modules` symlink.
- `.scratch/dsh-0.1.2-rc.1-upgrade/guard-pid-check.sh`: Updated baseline hosting PID and timestamp to match current hosting process.
- `AGENTS.md`: Updated local plugin update documentation to describe `link:` symlink installation and peer module resolution via the fallback link.
- `.agents/lessons.md`: Updated local plugin installation and store guidance to reflect `link:` symlinks and the fallback link.
- `.scratch/dsh-0.1.2-rc.1-upgrade/issues/04-clean-profile-dependencies-and-links.md`: Recorded ticket status and report.

### Design decisions taken
- Linked `root/.dsh/plugins/node_modules` to `../profiles/node_modules` to support Node ESM module resolution for `link:` plugins. Because Node resolves ESM imports from the realpath of symlinked plugins (`/root/.dsh/plugins/<plugin>/`), and plugins live as siblings to profiles rather than children of `profiles/web`, standard Node lookup walks up `/root/.dsh/plugins/` to `/root/.dsh/` without entering `/root/.dsh/profiles/node_modules`. Linking `root/.dsh/plugins/node_modules -> ../profiles/node_modules` provides immediate access to the DSH core packages and peer dependencies populated by DSH's fallback mechanism without modifying plugin sources or breaking their tests.
- Replaced the profile `node_modules` and lockfile by running non-interactive installation with `--store-dir /root/workspace/.pnpm-store/v11` under `CI=true` and `DSH_HOME=/root/.dsh` so that local packages materialize as symbolic links rather than hoisted directories of hardlinks.

### Deviations from the ticket
- None.

### Not implementable here
- None.
