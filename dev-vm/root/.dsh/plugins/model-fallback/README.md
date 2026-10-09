# @devvm/dsh-model-fallback

Continue a session on the next configured model when the selected model keeps failing, and recover
the provider's own stream-disconnect wording so DSH's retry policy treats it as retriable. Both the
candidate list and the retriable-error rules are edited from a **Model Fallback** page in the Web
Settings surface.

## Configuration

| Field | Meaning |
|---|---|
| `fallbacks[]` | Candidate routes (`provider`, `model`, `reasoningEffort`) tried in order after the failed model. |
| `retriableErrors[]` | Rules (`code`, `messageContains`, `treatAs`) selecting failures to retry and then roll on. |

Both sections are schemastery `.volatile()` fields, so a saved page takes effect on the running
plugin without unloading it, and the values persist as a `config` override for the `model-fallback`
entry in the active profile's `cordis.patch.yml`.

`retriableErrors` defaults to one rule:

```yaml
- code: PI_AI_ERROR
  messageContains: stream disconnected before completion
  treatAs: TRANSPORT
```

A rule matches when `failure.code` equals `code` and, when `messageContains` is non-empty, the
failure message contains it. A matched failure is rewritten to `treatAs` at the `llm/stream`
boundary, so the provider's `retryPolicy` retries it; the rewrite is what makes DSH spend the
configured retry budget on a provider-specific failure code. `treatAs` defaults to `TRANSPORT`.

## What a roll does

1. The selected model fails. `@deepseek-ai/dsh-llm-retry` retries it under the provider's
   `retryPolicy` (normal mode: five retries by default).
2. With that budget spent, the retry plugin delegates on the `agent/request-error` waterfall and
   this plugin receives the failure.
3. The failure is rollable when the provider policy retries its code, when a configured rule selects
   it, or when the policy is unlimited. Otherwise the turn ends as before.
4. The next candidate after the failed route is selected and recorded for the current turn; the loop
   retries the step. Each request on the rolled route records its own `request/header`, so the
   session log and the client's model indicator show the model that actually served the turn.
5. Once the candidate list is exhausted, the failure is delegated and the turn ends with the
   provider's error. At most `fallbacks.length` rolls happen per turn, so no listener can drive an
   endless roll.

The roll is scoped to the turn that needed it. DSH continues a session on its last recorded request
route, so a session that rolled stays on the fallback until the user picks a model again.

Two consequences are worth knowing before writing a list:

- **Consecutive candidates that share a provider share one retry budget.** `dsh-llm-retry` counts
  retries per provider and policy, not per model, so a second candidate on the same provider is
  tried once rather than given a fresh budget. Candidates on distinct providers each get their own.
- **The roll is not a model-availability check.** A permanent failure such as `AUTH` or
  `INVALID_REQUEST` is not rollable unless a rule selects it, so it fails fast instead of moving the
  session to another model.

## Mounting

The bundle patch inserts one entry, whose `id` is the settings namespace the page writes to:

```yaml
- insert:
    - id: model-fallback
      name: '@devvm/dsh-model-fallback'
```

Mount it after `@deepseek-ai/dsh-llm-retry` (the base bundle) so its `agent/request-error` listener
sits inside the retry plugin's and is reached only when retries are spent. The Web profile loads it
through `dsh.profile.bundles`; the client page needs no build step because `dsh.client` serves
`client.js` to the browser as a module-loader factory.

## Tests

```bash
cd /root/.dsh/profiles/web/node_modules/@devvm/dsh-model-fallback
node --test test.mjs
```

The suite drives the real agent loop, retry plugin, LLM runtime, and Web server. The settings
service is a stand-in because DSH owns the config-editor and profile-patch write chain.
