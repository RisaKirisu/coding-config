import { defineTool } from '@deepseek-ai/dsh-tools'
import z from '@deepseek-ai/schemastery'
import { isSubagent, normalizeConfig } from './helpers.mjs'

export { isSubagent, normalizeConfig }

export const name = 'subagent-manager'
export const inject = ['settings', 'tools', 'webServer', 'llm', 'systemPrompt']

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

export async function waitForIdle(idle, timeout = 300) {
  if (!Number.isFinite(timeout) || timeout <= 0 || timeout * 1000 > 2147483647) {
    throw new RangeError('timeout must be a positive number of seconds no greater than 2147483.647')
  }
  let timer
  try {
    return await Promise.race([
      idle.then(() => 'completed'),
      new Promise((resolve) => {
        timer = setTimeout(() => resolve('running'), timeout * 1000)
      }),
    ])
  } finally {
    clearTimeout(timer)
  }
}

function registerWaitTool(ctx) {
  ctx.systemPrompt.section({
    name: 'tool:subagent_wait',
    order: ctx.systemPrompt.getSectionOrder('TOOL_JOBS') + 0.01,
    text: (context) => ctx.tools.get('subagent_wait', context.scope) === undefined
      ? ''
      : 'Waiting or repeated polling with `subagent_wait` is permitted when no further foreground work can proceed until the subagent finishes.',
  })
  ctx.tools.register(defineTool({
    name: 'subagent_wait',
    description: 'Wait for a subagent to finish.',
    parameters: {
      subagent_id: {
        type: 'string',
        required: true,
        description: 'Subagent ID.',
      },
      timeout: {
        type: 'number',
        description: 'Optional maximum wait in seconds. Defaults to 300.',
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
      const agent = ctx.get('agents')?.get(id)
      if (agent && isSubagent(agent)) {
        const status = await waitForIdle(
          agent.status === 'running' ? agent.whenIdle() : Promise.resolve(),
          args.timeout,
        )
        return {
          status,
          subagent_id: id,
          message: status === 'running'
            ? `Subagent ${id} is still running.`
            : `Subagent ${id} finished execution.`,
        }
      }

      const session = ctx.get('sessions')?.get(id)
      if (session && isSubagent({ session })) {
        return {
          status: 'completed',
          subagent_id: id,
          message: `Subagent ${id} finished execution.`,
        }
      }

      throw new Error(`Subagent "${id}" was not found.`)
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
