# AGENTS.md

## Agent skills

### Issue tracker

Issues and specs are tracked as local Markdown under `.scratch/`. See `docs/agents/issue-tracker.md`.

### Domain docs

This repository uses a single-context domain-doc layout. See `docs/agents/domain.md`.

### Runtime verification

Changes to DevVM or DSH Runtime lifecycle code must cover non-interactive execution, bounded startup failure, stop-and-relaunch behavior, and Project Log updates.

Lifecycle test fakes must isolate guest PID files and never evaluate commands against `/tmp/devvm-daemon-dsh.pid` on the host. When changing that isolation, run the suite with a guard that verifies the hosting DSH PID remains alive and unchanged.
