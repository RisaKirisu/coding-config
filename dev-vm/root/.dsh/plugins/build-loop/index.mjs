/**
 * `build_ticket`: implement one ticket through a build child, then audit it with
 * a review child and a test child in parallel, feeding their findings back to
 * the same build child until both audits are clean or the fix budget is spent.
 * The orchestrator receives all three latest reports verbatim.
 *
 * Personas, fix budget, provider, and the child tool denylist are settings
 * (`build-loop` namespace in settings.yaml) editable from the web settings page.
 */
import { defineTool } from '@deepseek-ai/dsh-tools'
import { finalAssistantOutput } from '@deepseek-ai/dsh-subagent'
import { foldConsumedWork } from '@deepseek-ai/dsh-agent'
import { createUserMessage } from '@deepseek-ai/dsh-llm'
import z from '@deepseek-ai/schemastery'
import { DEFAULTS, validateConfig } from './config.mjs'
import {
  VERDICT_SCHEMA,
  auditPrompt,
  buildPrompt,
  fixPrompt,
  isClean,
  renderOutcome,
  toVerdict,
} from './loop.mjs'

export const name = 'build-loop'
export const inject = ['tools', 'subagents', 'settings', 'webServer', 'systemPrompt']

const SETTINGS_NS = 'build-loop'

const ConfigSchema = z.object({
  provider: z.string().default(DEFAULTS.provider),
  maxFixRounds: z.natural().default(DEFAULTS.maxFixRounds),
  buildPersona: z.string().default(DEFAULTS.buildPersona),
  reviewPersona: z.string().default(DEFAULTS.reviewPersona),
  testPersona: z.string().default(DEFAULTS.testPersona),
  deniedTools: z.array(z.string()).default(DEFAULTS.deniedTools),
})

function textOf(blocks) {
  return (blocks ?? []).filter((block) => block.type === 'text').map((block) => block.text).join('')
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

function registerRoutes(ctx, scope) {
  const dispose = ctx.webServer.register({
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
  })
  return dispose
}

/**
 * The child denylist may only name tools the child would otherwise see;
 * `tools.restrict()` throws on unknown names, and the catalog differs per preset.
 */
function childToolFilter(ctx, parent, denied) {
  const visible = new Set(ctx.tools.schemas(parent).map((schema) => schema.name))
  const deny = denied.filter((tool) => visible.has(tool))
  return deny.length === 0 ? undefined : { deny }
}

/** Run one one-shot audit child to completion and dispose it. */
async function runAudit(ctx, base, label, persona, prompt) {
  const run = await ctx.subagents.start(base.provider, {
    ...base.request,
    label,
    persona,
    prompt: [{ type: 'text', text: prompt }],
    outputSchema: VERDICT_SCHEMA,
  })
  try {
    const result = await run.result
    return toVerdict(result.structured, `${label} ended with stopReason=${result.stopReason}.\n${textOf(result.output)}`)
  } finally {
    await run.dispose()
  }
}

/** One more turn on the live build child; returns its new final text or throws on an abnormal end. */
async function fixTurn(agent, text) {
  const boundary = agent.session.events.length
  agent.followup(createUserMessage({ content: [{ type: 'text', text }], source: { kind: 'user' } }))
  await agent.whenIdle()
  const own = agent.session.events.slice(boundary)
  const reason = foldConsumedWork(own).end?.data.reason?.kind
  if (reason !== 'completed') throw new Error(`build agent fix turn ended with ${reason ?? 'no turn/end'}`)
  return textOf(finalAssistantOutput(own))
}

async function runLoop(ctx, config, { ticket, constraints, parent, signal }) {
  const toolFilter = childToolFilter(ctx, parent, config.deniedTools)
  const base = { provider: config.provider, request: { parent, signal, ...(toolFilter ? { toolFilter } : {}) } }
  const maxRounds = config.maxFixRounds
  const state = { ticket, maxRounds, rounds: 0, build: undefined, review: undefined, test: undefined }

  const buildRun = await ctx.subagents.start(config.provider, {
    ...base.request,
    label: `build ${ticket}`,
    persona: config.buildPersona,
    prompt: [{ type: 'text', text: buildPrompt({ ticket, constraints }) }],
  })
  const onAbort = () => buildRun.localAgent?.cancel({ kind: 'parent' })
  signal.addEventListener('abort', onAbort, { once: true })
  try {
    const built = await buildRun.result
    state.build = textOf(built.output)
    if (built.stopReason !== 'completed') {
      return renderOutcome({ ...state, status: 'failed', failure: `build agent ended with ${built.stopReason}${built.diagnostic ? ` (${built.diagnostic})` : ''}` })
    }
    if (buildRun.localAgent === undefined && maxRounds > 0) {
      return renderOutcome({ ...state, status: 'failed', failure: `provider "${config.provider}" exposes no local agent; fix rounds need one` })
    }

    for (;;) {
      if (signal.aborted) throw new Error('build_ticket was cancelled')
      const audit = { ticket, constraints, buildReport: state.build, round: state.rounds + 1 }
      const [review, test] = await Promise.all([
        runAudit(ctx, base, `review ${ticket}`, config.reviewPersona, auditPrompt(audit)),
        runAudit(ctx, base, `test-audit ${ticket}`, config.testPersona, auditPrompt(audit)),
      ])
      state.review = review
      state.test = test
      if (signal.aborted) throw new Error('build_ticket was cancelled')
      if (isClean(review, test)) return renderOutcome({ ...state, status: 'clean' })
      if (state.rounds >= maxRounds) return renderOutcome({ ...state, status: 'unresolved' })
      state.rounds += 1
      try {
        state.build = await fixTurn(buildRun.localAgent, fixPrompt({ review, test, round: state.rounds, maxRounds }))
      } catch (error) {
        return renderOutcome({ ...state, status: 'failed', failure: error.message })
      }
    }
  } finally {
    signal.removeEventListener('abort', onAbort)
    await buildRun.dispose()
  }
}

const DESCRIPTION = 'Implement one ticket end to end: a build agent implements it, then a code-review agent and a test-quality agent audit the result in parallel; their findings go back to the same build agent for fixing, up to the configured fix budget. Returns the build report, review report, and test report verbatim, with a clean/unresolved/failed status. Use this instead of a plain subagent whenever the task is "implement this ticket". This call waits for the whole loop by default; set run_in_background to get a job id.'

export function apply(ctx) {
  const scope = ctx.settings.register(SETTINGS_NS, ConfigSchema, { base: {}, validate: validateConfig })
  ctx.effect(() => registerRoutes(ctx, scope), 'build-loop: web routes')

  ctx.systemPrompt.section({
    name: 'tool:build_ticket',
    order: 116.7,
    text: (context) => ctx.tools.get('build_ticket', context.scope) === undefined
      ? ''
      : 'When dispatching implementation of a defined ticket, use build_ticket rather than a plain subagent: it runs the build, independent review, and test audit loop and returns all reports. Independent tickets may be dispatched together in parallel build_ticket calls. Read the returned status: an unresolved or failed loop is not a finished ticket.',
  })

  ctx.tools.register(defineTool({
    name: 'build_ticket',
    description: DESCRIPTION,
    parameters: {
      ticket: {
        type: 'string',
        required: true,
        description: 'Path to the ticket file to implement (the spec).',
      },
      constraints: {
        type: 'string',
        description: 'Extra caller constraints for the build agent beyond the ticket text.',
      },
      run_in_background: {
        type: 'boolean',
        description: 'Return a job id immediately instead of waiting; collect with job_output. Defaults to false.',
      },
    },
    output: {
      schema: {
        oneOf: [
          {
            type: 'object',
            additionalProperties: false,
            properties: {
              kind: { type: 'string', required: true, const: 'background' },
              jobId: { type: 'string', required: true },
            },
          },
          {
            type: 'object',
            additionalProperties: false,
            properties: {
              kind: { type: 'string', required: true, const: 'foreground' },
              text: { type: 'string', required: true },
            },
          },
        ],
      },
      render: (_args, value) => [{
        type: 'text',
        text: value.kind === 'background' ? `started background build loop job ${value.jobId}` : value.text,
      }],
    },
    isConcurrencySafe: () => true,
    async execute(args, exec) {
      const parent = exec.agent
      if (!parent) throw new Error('build_ticket requires a calling agent')
      const ticket = String(args.ticket ?? '').trim()
      if (ticket.length === 0) throw new Error('ticket must be a non-empty path')
      const config = scope.get()
      const input = { ticket, constraints: args.constraints?.trim() || undefined, parent }

      if (args.run_in_background === true) {
        const jobs = ctx.get('jobs')
        if (jobs === undefined) throw new Error('background jobs unavailable: load @deepseek-ai/dsh-jobs and @deepseek-ai/dsh-tool-jobs')
        return {
          kind: 'background',
          jobId: jobs.start({
            kind: 'subagent',
            label: `build_ticket ${ticket}`,
            owner: parent,
            run: () => {
              const controller = new AbortController()
              return {
                cancel: (reason) => controller.abort(reason ?? 'build loop killed'),
                done: runLoop(ctx, config, { ...input, signal: controller.signal })
                  .then((text) => ({ status: 'completed', output: text }))
                  .catch((error) => controller.signal.aborted ? { status: 'killed' } : { status: 'failed', detail: String(error) }),
              }
            },
          }),
        }
      }
      return { kind: 'foreground', text: await runLoop(ctx, config, { ...input, signal: exec.signal }) }
    },
    presentCall: (args) => ({ card: 'generic', title: `build_ticket ${args.ticket}`, kind: 'other', rawInput: args }),
  }))
}
