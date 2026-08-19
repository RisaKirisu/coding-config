# DeepSeek Minimal Bootstrap

OpenCode plugin that boots `deepseek-v4-pro` and `deepseek-v4-flash` sessions
with the DeepSeek Harness Minimal prompt and two-tool catalog, then unlocks the
full OpenCode tool catalog after two durable tool calls or once the assistant
has completed a reply. Activation is gated by the models listed in
`deepseek-minimal-bootstrap.json`; every other model is unchanged.

## Why

DeepSeek V4 Pro conditions strongly on the API-visible tool catalog. In the
Project2 evaluation, the Standard and PTC presets scored 91 and 92, while the
official Minimal preset scored 99 and 96. Staying on Minimal permanently,
however, gives up the Standard preset's broader tool set.

This plugin separates initial trajectory selection from later tool use:

1. Keep the exact Minimal system prompt on every request.
2. Expose only `bash` and `str_replace_editor` on request #1.
3. After two durable tool calls, or after a completed assistant reply (the
   reply that precedes the second user message), expose the full OpenCode tool
   catalog.
4. A single tool call or a completed first reply still triggers the persisted
   prompt-section injection at the next user message, exactly as before.
5. A text-only first reply still restores the catalog at request #2, so the
   bootstrap catalog can no longer trap the session.

## Behavior

For each eligible OpenCode session:

1. Request #1 sends the exact DeepSeek Harness Minimal system prompt:

   ```text
   You are a helpful software engineer assistant.
   ```

2. Request #1 exposes exactly the DeepSeek Harness Minimal tools, `bash` and
   `str_replace_editor`, with the same descriptions and JSON Schemas as Harness
   commit `47f943859bef60e4160492346772ded9b24f765a`.
3. Two durable tool calls, or a completed assistant message, restore the
   session catalog. A single durable tool call keeps the two-tool catalog for
   the rest of the turn.
4. Every request keeps the exact Minimal system prompt, and catalog-restored
   requests expose OpenCode's full tool catalog.
5. On the first user message after a promotion signal — one durable tool call
   or a completed assistant reply — the plugin persists one synthetic
   text part into that message containing the OpenCode context the Minimal
   transform strips — environment, project and global instructions, MCP server
   instructions, skill catalog, structured-output prompt, and that message's
   `user.system` — followed by the compression reminder. The part is written
   once to stored history; later requests rewrite only the system prompt and
   tool catalog, so the DeepSeek prompt-cache prefix stays stable.
6. The context is snapshotted from OpenCode's own
   `experimental.chat.system.transform` hook before promotion, so the sections
   are byte-for-byte what OpenCode 1.18.18 would have placed in the system
   prompt. It is snapshotted once and does not refresh when instructions,
   skills, MCP servers, or the environment change mid-session.

The plugin derives both signals from `client.session.messages()` immediately
before each API request. Catalog restoration needs two durable tool calls or a
completed assistant reply; prompt-section injection keeps its original first
tool-call / completed-reply trigger. Retries remain in bootstrap, resumed
sessions recover their durable phase, and only positive decisions are memoized.
Utility requests from OpenCode's `title`, `summary`, and `compaction` agents are
not modified, and utility assistant messages those agents persist inside a
session never count as signals, so compaction or auto-compaction cannot unlock
the full catalog during bootstrap. Other agent sessions, including subagents,
derive their own phase.
The plugin supplies an executable `str_replace_editor` adapter for its first-turn
tool calls. That adapter is hidden from non-target models and removed once the
session restores OpenCode's normal full catalog.

If history cannot be read, the persisted context injection fails closed for that
user message rather than annotating a bootstrap turn. In the rare case a session
was already promoted before this plugin process observed one of its system
prompts (for example, resumed and immediately reused), the plugin persists a
plugin-assembled environment and instruction fallback instead and logs a warning
that MCP instructions and the skill catalog are omitted.

If history cannot be read, required bootstrap tools are missing, or the wire
body is incompatible, the plugin fails open rather than blocking the request.
The pinned API-visible definitions are recorded in
[`fixtures/deepseek-harness-minimal-tools.json`](./fixtures/deepseek-harness-minimal-tools.json).

## Models

The plugin reads the sibling `deepseek-minimal-bootstrap.json` file next to
`dcp.jsonc`. It only needs a `models` array:

```json
{
  "models": ["deepseek-v4-pro", "deepseek-v4-flash"]
}
```

When the file is absent or invalid, both models above are active by default.
`DEEPSEEK_MINIMAL_BOOTSTRAP_CONFIG` can point at an alternative config file.

## Install

Add the local plugin and model to `opencode.json` or `opencode.jsonc`. The path
uses `{env:HOME}` so a synced `coding-config` directory works on any machine:

```json
{
  "$schema": "https://opencode.ai/config.json",
  "model": "deepseek/deepseek-v4-pro",
  "plugin": ["{env:HOME}/coding-config/deepseek-minimal-bootstrap/index.mjs"]
}
```

Connect the built-in `deepseek` provider through OpenCode's `/connect` flow.
The plugin adds only a runtime `options.fetch` wrapper; it preserves the
built-in provider catalog, endpoint, authentication, reasoning configuration,
and any existing custom fetch function.

Quit and restart OpenCode after changing plugin configuration, then start a
fresh session. Existing sessions with a completed assistant reply are already
catalog-restored by design.

## Other Providers

The official `deepseek` provider is wrapped by default. A configured provider
that explicitly declares one of the configured target models is detected
automatically. For a built-in or otherwise implicit provider, list its ID in
plugin options:

```json
{
  "plugin": [
    [
      "{env:HOME}/coding-config/deepseek-minimal-bootstrap/index.mjs",
      { "providerIDs": ["opencode-go"] }
    ]
  ]
}
```

The behavior still activates only when OpenCode's selected model ID is one of
the `models` entries in `deepseek-minimal-bootstrap.json`.

## Compatibility

Tested against OpenCode `1.18.18`.

The transform targets OpenAI-compatible Chat Completions request bodies. The
current built-in DeepSeek provider uses `@ai-sdk/openai-compatible`, so it is
the primary supported path. OpenAI Responses and Anthropic Messages use
different request bodies and are not rewritten by this plugin. When
`OPENCODE_EXPERIMENTAL_NATIVE_LLM` is enabled, the plugin disables itself and
logs a warning because that runtime bypasses provider fetch wrappers; use
OpenCode's default AI SDK runtime for this experiment.

Run the plugin tests from the `coding-config` directory:

```sh
node --test deepseek-minimal-bootstrap/index.test.mjs
```
