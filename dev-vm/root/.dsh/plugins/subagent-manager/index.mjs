import { defineTool } from '@deepseek-ai/dsh-tools'
import z from '@deepseek-ai/schemastery'

export const name = 'subagent-manager'
export const inject = ['settings', 'tools', 'webServer', 'llm']

const SETTINGS_NS = 'subagent-model'
const DEFAULT_CONFIG = Object.freeze({
  provider: '',
  model: '',
  reasoningEffort: '',
})

const ConfigSchema = z.object({
  provider: z.string().default(''),
  model: z.string().default(''),
  reasoningEffort: z.string().default(''),
})

export function normalizeConfig(value) {
  return {
    provider: typeof value?.provider === 'string' ? value.provider : '',
    model: typeof value?.model === 'string' ? value.model : '',
    reasoningEffort: typeof value?.reasoningEffort === 'string' ? value.reasoningEffort : '',
  }
}

export function isSubagent(agent) {
  return agent?.session?.header?.origin === 'subagent'
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

function registerRoutes(ctx, scope) {
  const routes = [
    ctx.webServer.register({
      kind: 'exact',
      path: '/api/subagent-manager/config',
      handler: async (req, res) => {
        if (req.method === 'GET' || req.method === 'HEAD') {
          json(res, 200, normalizeConfig(scope.get()))
          return
        }
        if (req.method !== 'POST') {
          json(res, 405, { error: 'Method Not Allowed' })
          return
        }
        try {
          const next = normalizeConfig(await readJson(req))
          await scope.update(next)
          json(res, 200, normalizeConfig(scope.get()))
        } catch (error) {
          json(res, 400, { error: error?.message || String(error) })
        }
      },
    }),
    ctx.webServer.register({
      kind: 'exact',
      path: '/api/subagent-manager/models',
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

function waitTimeout(ms, id) {
  let timer
  const promise = new Promise((_, reject) => {
    timer = setTimeout(() => reject(new Error(`Timeout waiting for ${id} after ${ms}ms`)), ms)
    timer.unref?.()
  })
  return { promise, clear: () => clearTimeout(timer) }
}

function registerWaitTool(ctx) {
  ctx.tools.register(defineTool({
    name: 'subagent_wait',
    description: 'Wait for a running background subagent or background job to finish, then return status only. Native DSH subagent-result context injection remains unchanged.',
    parameters: {
      subagent_id: {
        type: 'string',
        required: true,
        description: 'Running background subagent ID or background job ID.',
      },
      timeout_ms: {
        type: 'number',
        description: 'Maximum wait in milliseconds. Defaults to 300000.',
      },
    },
    output: {
      schema: {
        type: 'object',
        additionalProperties: false,
        properties: {
          status: { type: 'string', required: true },
          subagent_id: { type: 'string', required: true },
          message: { type: 'string', required: true },
        },
      },
      render: (_args, value) => [{ type: 'text', text: value.message }],
    },
    async execute(args) {
      const id = args.subagent_id
      const timeoutMs = Number.isFinite(args.timeout_ms) && args.timeout_ms > 0
        ? args.timeout_ms
        : 300000

      const jobs = ctx.get('jobs')
      if (jobs) {
        try {
          const job = jobs.get(id)
          if (job) {
            const result = await jobs.wait(id, timeoutMs)
            return {
              status: result.status,
              subagent_id: id,
              message: `Background job ${id} finished with status "${result.status}".`,
            }
          }
        } catch {
          // Not a job ID; continue with subagent lookup.
        }
      }

      const agent = ctx.get('agents')?.get(id)
      if (agent) {
        if (agent.status === 'running') {
          const timeout = waitTimeout(timeoutMs, `subagent ${id}`)
          try {
            await Promise.race([agent.whenIdle(), timeout.promise])
          } finally {
            timeout.clear()
          }
        }
        return {
          status: 'completed',
          subagent_id: id,
          message: `Subagent ${id} finished execution.`,
        }
      }

      const session = ctx.get('sessions')?.get(id)
      if (session) {
        return {
          status: 'completed',
          subagent_id: id,
          message: `Subagent ${id} finished execution.`,
        }
      }

      throw new Error(`Running subagent or background job "${id}" was not found.`)
    },
    presentCall: (args) => ({
      card: 'generic',
      title: `Wait for ${args.subagent_id}`,
      kind: 'other',
      rawInput: args,
    }),
  }))
}

export function apply(ctx) {
  const scope = ctx.settings.register(SETTINGS_NS, ConfigSchema, { base: DEFAULT_CONFIG })

  ctx.on('agent/request', async (payload, next) => {
    const request = await next()
    if (!isSubagent(payload?.agent)) return request

    const configured = normalizeConfig(scope.get())
    if (!configured.provider || !configured.model) return request

    return {
      ...request,
      provider: configured.provider,
      model: configured.model,
      ...(configured.reasoningEffort
        ? { reasoningEffort: configured.reasoningEffort }
        : {}),
    }
  }, { global: true })

  ctx.effect(() => registerRoutes(ctx, scope), 'subagent-manager: web routes')
  registerWaitTool(ctx)
}
