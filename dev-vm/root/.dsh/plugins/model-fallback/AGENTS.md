# Model fallback plugin

`@devvm/dsh-model-fallback` rolls a failed session onto the next configured model and classifies
provider-specific failures as retriable so DSH's retry policy spends its budget on them.

- `rules.mjs` holds every decision as a pure function. Keep it free of Cordis types so the rule and
  route logic stays testable without mounting a context.
- `index.mjs` is the host plugin: the `llm/stream` reclassification, the `agent/request` route
  override, the `agent/request-error` roll, and the two settings routes.
- `client.js` is a hand-written browser bundle. It is a classic script registering a
  `window.__ModuleLoader__.load({ id, factory })` factory that returns `{ name, inject, apply }`.
  `require` resolves the shell's seed words, so only `react` is used. React is a browser seed word and
  is not resolvable from Node, which is why the bundle test drives the real component through a
  stand-in `createElement`/`useState`.
- `cordis.patch.yml` declares the entry `id`, which is also the settings namespace. Renaming the id
  requires renaming it in `index.mjs`'s `ctx.settings.update` call and in the client's fetch paths.

## Environment constraints that shape the tests

The plugin is loaded by Cordis' loader, which resolves bare specifiers from the DSH installation.
A test file run straight from `/root/.dsh/plugins/model-fallback/` therefore cannot resolve
`@deepseek-ai/schemastery`, so `test.mjs` is listed in the package `files` and is run from the
installed profile copy:

```bash
cd /root/.dsh/profiles/web && pnpm install --ignore-scripts    # after editing any source file
cd node_modules/@devvm/dsh-model-fallback && node --test test.mjs
```

`pnpm install` hard-links `file:` dependencies, so an in-place write to a source file is visible in
the installed copy; a future install re-creates the link.

## Verifying a change

1. `cd /root/.dsh/profiles/web && pnpm install --ignore-scripts`
2. `cd node_modules/@devvm/dsh-model-fallback && node --test test.mjs`
3. `dsh --profile web --dump-config | grep -A2 model-fallback` to confirm composition and that
   `@deepseek-ai/dsh-llm-retry` still precedes it.

A new bundle and client page only reach the running Web server on the next start; the Settings page
does not appear in an already-running server.
