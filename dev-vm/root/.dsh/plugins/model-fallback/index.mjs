import z from '@deepseek-ai/schemastery'
import { STREAM_DISCONNECT_RULE, isRollable, nextFallback, reclassifyFailure, validateConfig } from './rules.mjs'

export const name = 'model-fallback'

/** Durable settings for the page, HTTP routes for it, LLM streams, and Agents. */
export const inject = ['settings', 'webServer', 'llm', 'agents']

/**
 * Both sections are volatile: the settings page rewrites them on the running
 * plugin, so a saved list takes effect without unloading it.
 */
export const Config = z.object({
  fallbacks: z.array(z.object({
    provider: z.string().default(''),
    model: z.string().default(''),
    reasoningEffort: z.string().default(''),
  })).default([]).volatile(),
  retriableErrors: z.array(z.object({
    code: z.string().default(''),
    messageContains: z.string().default(''),
    treatAs: z.string().default('TRANSPORT'),
  })).default([STREAM_DISCONNECT_RULE]).volatile(),
})

/**
 * Read the live configuration through the Loader's volatile references.
 * @param config - resolved plugin configuration.
 * @returns the detached `{ fallbacks, retriableErrors }` sections.
 */
function currentConfig(config) {
  return {
    fallbacks: config.fallbacks.get(),
    retriableErrors: config.retriableErrors.get(),
  }
}

function json(res, status, value) {
  res.writeHead(status, { 'Content-Type': 'application/json; charset=utf-8' })
  res.end(JSON.stringify(value))
}

async function readJson(req) {
  const chunks = []
  for await (const chunk of req) chunks.push(chunk)
  if (chunks.length === 0) return {}
  return JSON.parse(Buffer.concat(chunks).toString('utf8'))
}

/**
 * Every registered provider, its models, and each model's reasoning efforts,
 * so the page offers pickers instead of free-text routes.
 * @param llm - the LLM runtime service.
 * @returns the directory the page renders.
 */
async function modelDirectory(llm) {
  const providers = llm.listProviders()
  const modelsByProvider = {}
  const reasoningByModel = {}

  for (const provider of providers) {
    try {
      const models = await llm.listModels(provider.id)
      modelsByProvider[provider.id] = models.map((model) => ({
        id: model.id,
        name: model.name || model.id,
      }))

      for (const model of models) {
        const key = `${provider.id}/${model.id}`
        try {
          const info = await llm.resolveModelInfo(provider.id, model.id)
          reasoningByModel[key] = info?.reasoning?.efforts?.map((effort) => effort.id)
            || ['off', 'low', 'medium', 'high', 'xhigh', 'max']
        } catch {
          reasoningByModel[key] = ['off', 'low', 'medium', 'high', 'xhigh', 'max']
        }
      }
    } catch {
      modelsByProvider[provider.id] = []
    }
  }

  return { providers, modelsByProvider, reasoningByModel }
}

/**
 * Serve the settings page: read the running list, and persist a replacement
 * through the profile's configuration editor.
 * @param ctx - plugin context carrying the web server and settings services.
 * @param config - resolved plugin configuration.
 * @returns disposer for both routes.
 */
function registerRoutes(ctx, config) {
  const routes = [
    ctx.webServer.register({
      kind: 'exact',
      path: '/api/model-fallback/config',
      handler: async (req, res) => {
        if (req.method === 'GET' || req.method === 'HEAD') {
          json(res, 200, currentConfig(config))
          return
        }
        if (req.method !== 'POST') {
          json(res, 405, { error: 'Method Not Allowed' })
          return
        }
        try {
          await ctx.settings.update('model-fallback', validateConfig(await readJson(req)))
          json(res, 200, currentConfig(config))
        } catch (error) {
          json(res, 400, { error: error?.message || String(error) })
        }
      },
    }),
    ctx.webServer.register({
      kind: 'exact',
      path: '/api/model-fallback/models',
      handler: async (req, res) => {
        if (req.method !== 'GET' && req.method !== 'HEAD') {
          json(res, 405, { error: 'Method Not Allowed' })
          return
        }
        try {
          json(res, 200, await modelDirectory(ctx.llm))
        } catch (error) {
          json(res, 500, { error: error?.message || String(error) })
        }
      },
    }),
  ]

  return () => routes.forEach((dispose) => dispose())
}

export function apply(ctx, config) {
  ctx.effect(() => ctx.settings.configure({ auto: false }, ctx.fiber))

  /**
   * Fallback route per Agent for the turn that rolled, plus how many rolls that
   * turn already spent. The count bounds the roll even when another listener
   * rewrites the route back to the failed one.
   */
  const routes = new WeakMap()

  ctx.on('llm/stream', async function* (_options, next) {
    for await (const chunk of next()) {
      if (chunk.type !== 'finish' || chunk.reason.kind !== 'error') {
        yield chunk
        continue
      }
      const failure = reclassifyFailure(config.retriableErrors.get(), chunk.reason.failure)
      yield failure === undefined ? chunk : { ...chunk, reason: { ...chunk.reason, failure } }
    }
  })

  ctx.on('agent/request', async (payload, next) => {
    const request = await next()
    const route = routes.get(payload.agent)
    if (route === undefined || route.turn !== payload.turn) return request
    return {
      ...request,
      provider: route.provider,
      model: route.model,
      ...(route.reasoningEffort ? { reasoningEffort: route.reasoningEffort } : {}),
    }
  }, { prepend: true })

  ctx.on('agent/request-error', async (payload, next) => {
    const downstream = await next()
    if (downstream?.kind === 'retry' || payload.signal.aborted) return downstream

    const { fallbacks, retriableErrors } = currentConfig(config)
    if (fallbacks.length === 0 || !isRollable(payload.retryPolicy, retriableErrors, payload.failure)) return downstream

    const previous = routes.get(payload.agent)
    const rolls = previous?.turn === payload.turn ? previous.rolls : 0
    if (rolls >= fallbacks.length) return downstream

    const logged = payload.agent.session.requestHeader()?.config
    const route = nextFallback(fallbacks, {
      provider: logged?.provider ?? payload.provider,
      model: logged?.model,
    })
    if (route === undefined) {
      ctx.logger.warn(`model-fallback: no fallback follows ${logged?.provider ?? payload.provider}/${logged?.model} for session "${payload.agent.session.id}"`)
      return downstream
    }

    routes.set(payload.agent, {
      turn: payload.turn,
      provider: route.provider,
      model: route.model,
      reasoningEffort: route.reasoningEffort,
      rolls: rolls + 1,
    })
    ctx.logger.warn(`model-fallback: session "${payload.agent.session.id}" continues on ${route.provider}/${route.model} after ${payload.failure.code}`)
    return { kind: 'retry' }
  })

  ctx.effect(() => registerRoutes(ctx, config), 'model-fallback: settings routes')
}
