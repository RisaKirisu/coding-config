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

export const Config = z.object({
  provider: z.string().pattern(/\S/).default(DEFAULTS.provider).volatile(),
  maxFixRounds: z.natural().max(Number.MAX_SAFE_INTEGER).default(DEFAULTS.maxFixRounds).volatile(),
  reminderTokens: z.natural().min(1).max(Number.MAX_SAFE_INTEGER).default(DEFAULTS.reminderTokens).volatile(),
  deniedTools: z.array(z.string()).default(DEFAULTS.deniedTools).volatile(),
})

/** Read one detached policy from Loader's live configuration references. */
export function currentConfig(config) {
  return {
    provider: config.provider.get(),
    maxFixRounds: config.maxFixRounds.get(),
    reminderTokens: config.reminderTokens.get(),
    deniedTools: [...config.deniedTools.get()],
  }
}

function json(res, status, value) {
  res.writeHead(status, { 'Content-Type': 'application/json; charset=utf-8' })
  res.end(JSON.stringify(value))
}

async function readJson(req) {
  const chunks = []
  for await (const chunk of req) chunks.push(chunk)
  return chunks.length === 0 ? {} : JSON.parse(Buffer.concat(chunks).toString('utf8'))
}

/** Serve the live entry configuration through the existing Build Loop settings page. */
export function registerSettings(ctx, config) {
  ctx.effect(() => ctx.settings.configure({ auto: false }, ctx.fiber))
  ctx.effect(() => ctx.webServer.register({
    kind: 'exact',
    path: '/api/build-loop/config',
    handler: async (req, res) => {
      if (req.method === 'GET' || req.method === 'HEAD') {
        json(res, 200, { config: currentConfig(config), defaults: DEFAULTS })
        return
      }
      if (req.method === 'DELETE') {
        await ctx.settings.replace('build-loop', {})
        json(res, 200, { config: currentConfig(config), defaults: DEFAULTS })
        return
      }
      if (req.method !== 'POST') {
        json(res, 405, { error: 'Method Not Allowed' })
        return
      }
      try {
        const next = await readJson(req)
        validateConfig(next)
        await ctx.settings.replace('build-loop', next)
        json(res, 200, { config: currentConfig(config), defaults: DEFAULTS })
      } catch (error) {
        json(res, 400, { error: error?.message || String(error) })
      }
    },
  }), 'build-loop: web routes')
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
