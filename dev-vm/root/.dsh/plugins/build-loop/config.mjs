/** The `build-loop` settings namespace: flow defaults, validation, registration, and its web config route. */
import z from '@deepseek-ai/schemastery'

export const DEFAULTS = Object.freeze({
  provider: 'spawn',
  maxFixRounds: 3,
  reminderTokens: 100_000,
  deniedTools: [
    'build_ticket', 'build_ticket_decide', 'subagent', 'subagent_fork',
    'subagent_wait', 'list_agents', 'ralph', 'workflow', 'send_message',
    'interrupt_agent', 'create_goal', 'update_goal', 'get_goal',
    'ask_user_question', 'exit_plan_mode', 'archive_voice_input',
    'remove_voice_input_record',
  ],
})

export const ConfigSchema = z.object({
  provider: z.string().default(DEFAULTS.provider),
  maxFixRounds: z.natural().default(DEFAULTS.maxFixRounds),
  reminderTokens: z.natural().default(DEFAULTS.reminderTokens),
  deniedTools: z.array(z.string()).default(DEFAULTS.deniedTools),
})

function json(res, status, value) {
  res.writeHead(status, { 'Content-Type': 'application/json; charset=utf-8' })
  res.end(JSON.stringify(value))
}

async function readJson(req) {
  const chunks = []
  for await (const chunk of req) chunks.push(chunk)
  return chunks.length === 0 ? {} : JSON.parse(Buffer.concat(chunks).toString('utf8'))
}

/** Register the settings namespace and `/api/build-loop/config` (GET, POST replace, DELETE reset); returns the settings scope. */
export async function registerSettings(ctx) {
  const scope = ctx.settings.register('build-loop', ConfigSchema, { base: {}, validate: validateConfig })
  ctx.effect(() => ctx.webServer.register({
    kind: 'exact',
    path: '/api/build-loop/config',
    handler: async (req, res) => {
      if (req.method === 'GET' || req.method === 'HEAD') {
        json(res, 200, { config: scope.get(), defaults: DEFAULTS })
        return
      }
      if (req.method === 'DELETE') {
        await scope.replace({})
        json(res, 200, { config: scope.get(), defaults: DEFAULTS })
        return
      }
      if (req.method !== 'POST') {
        json(res, 405, { error: 'Method Not Allowed' })
        return
      }
      try {
        const next = await readJson(req)
        validateConfig(next)
        await scope.replace(next)
        json(res, 200, { config: scope.get(), defaults: DEFAULTS })
      } catch (error) {
        json(res, 400, { error: error?.message || String(error) })
      }
    },
  }), 'build-loop: web routes')
  return scope
}

/** Keep only denylist names the parent sees; `tools.restrict` rejects unknown names and the reserved `run_code`. */
export function filterDeniedTools(deniedTools, parentSchemas) {
  const names = new Set((parentSchemas ?? []).map((schema) => schema?.name).filter(Boolean))
  names.delete('run_code')
  return (deniedTools ?? []).filter((tool) => names.has(tool))
}

/** Reject a saved section the loop could not run with. */
export function validateConfig(value) {
  if (!Number.isSafeInteger(value.maxFixRounds) || value.maxFixRounds < 0) {
    throw new Error('maxFixRounds must be a non-negative integer')
  }
  if (!Number.isSafeInteger(value.reminderTokens) || value.reminderTokens <= 0) {
    throw new Error('reminderTokens must be a positive integer')
  }
  if (typeof value.provider !== 'string' || value.provider.trim().length === 0) {
    throw new Error('provider must be a non-empty provider name')
  }
  if (!Array.isArray(value.deniedTools) || !value.deniedTools.every((tool) => typeof tool === 'string')) {
    throw new Error('deniedTools must be a list of tool names')
  }
}
