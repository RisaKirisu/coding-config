# DevVM rc.2 agent preset

This bundle declares the existing custom `standard-bash` ID through rc.2's native preset registry. It is loaded immediately after the stock Web bundle; the Web profile selects it as the default. Dormant legacy preset directories are retained but are not registered.

The declaration preserves the terminal-only tool composition, plan-mode groups, prompt, and realm. Compaction uses threshold 0.5, maxTokens 10000, and five retries/overflow retries; pruning uses 8192/2048/1024 characters. Fetch is enabled and search is disabled. Ralph retains 64 rounds, and workflow uses the native PTC backend. The sandbox service remains enabled because PTC requires it, with deployment policy `danger-full-access`; shell/filesystem sandbox executors remain disabled.

Web-editable providers, default model, and local plugin values live in the Web profile patch. Headless owns independent copies and native spawn defaults. Shared shell budgets remain deployment-owned in the home patch: 1200000 ms default/cap and 20000 bytes retained output.
