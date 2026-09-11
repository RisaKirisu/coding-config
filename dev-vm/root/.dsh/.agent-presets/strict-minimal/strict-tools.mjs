/**
 * Strict tool whitelist for the `strict-minimal` agent preset.
 *
 * The shipped `minimal` composition supplies its two tools through the agent
 * plane, but a tool registered by a HOST-plane plugin lands in the
 * process-global layer, which every preset still sees. `minimal` therefore is
 * minimal only on a deployment with no extra global tools. This row closes
 * that hole: the model-facing catalog is the minimal pair and nothing else,
 * whatever else the deployment composes.
 *
 * Two mechanisms, because they cover different surfaces:
 *
 *  1. `system-prompt/assemble`, registered with `prepend`, filters the
 *     assembled catalog — which `dsh-agent-loop` passes to the request as its
 *     `tools` array. Outermost placement is why the filter is last: a listener
 *     registered later is dispatched INSIDE this one, so nothing can re-add a
 *     schema after the filter returns.
 *
 *  2. `ctx.tools.guard` denies any dispatch whose name is not whitelisted. The
 *     catalog filter alone would still EXECUTE a global tool named from
 *     context the model can read — a resumed transcript produced under another
 *     preset, or a hallucinated name. Guards are monotonic: a later guard can
 *     deny more, never re-allow.
 *
 * Both registrations go through the preset's standing scope, whose chain every
 * agent joined to this preset inherits, so one mount covers every session on
 * it.
 *
 * Deliberately NOT `ctx.tools.restrict()`: a restriction is exempt only for the
 * registering scope's OWN layer, and the viewing agent's own layer holds
 * nothing here — `minimal`'s two tools are an ANCESTOR (standing) contribution.
 * A restriction registered from this preset would therefore strip `bash` and
 * `str_replace_editor` along with the globals, and naming them in `allow` is
 * rejected as unknown whenever the deployment registers no global tool of that
 * name.
 */

/** Cordis plugin name used by loader diagnostics. */
export const name = 'strict-tools'

/** The tool runtime must exist before its guard can be registered. */
export const inject = ['tools']

/**
 * Narrow the model-facing catalog and the dispatch boundary to `config.allow`.
 * @param ctx - the preset row's context (the preset's standing scope).
 * @param config - `{ allow: string[] }`, the only tool names this preset exposes.
 */
export function apply(ctx, config) {
  const allow = config === undefined ? undefined : config.allow
  if (!Array.isArray(allow) || allow.length === 0 || allow.some((entry) => typeof entry !== 'string' || entry.length === 0)) {
    throw new TypeError(`${name}: config.allow must be a non-empty array of tool names`)
  }
  const allowed = new Set(allow)
  const list = [...allowed].join(', ')

  let warned = false
  const warnOnce = (message) => {
    if (warned) return
    warned = true
    try {
      ctx.logger.warn(message)
    } catch {
      // Logger unavailable — the guard exists only to avoid spamming.
    }
  }

  ctx.on('system-prompt/assemble', async (_assembly, _context, next) => {
    const assembly = await next()
    const present = new Set(assembly.tools.map((tool) => tool.name))
    const missing = allow.filter((entry) => !present.has(entry))
    if (missing.length > 0) {
      // Composition drift: better a visible warning than a silent empty or
      // silently unfiltered catalog.
      warnOnce(`${name}: whitelisted tool(s) not registered: ${missing.join(', ')} — the composition drifted from the whitelist`)
    }
    const kept = assembly.tools.filter((tool) => allowed.has(tool.name))
    if (kept.length === assembly.tools.length) return assembly
    return { ...assembly, tools: kept }
  }, { prepend: true })

  ctx.tools.guard((exec) => allowed.has(exec.name)
    ? undefined
    : `tool "${exec.name}" is unavailable: this preset exposes only ${list}`)
}
