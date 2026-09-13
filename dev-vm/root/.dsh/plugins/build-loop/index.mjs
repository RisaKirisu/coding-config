/** Tool registration and settings; build policy and execution live with the controller. */
import { defineTool } from '@deepseek-ai/dsh-tools'
import { Controller } from './controller.mjs'
import { registerSettings } from './config.mjs'
import { CONTRACT_SCHEMA, DECISION_SCHEMA, parameters } from './schemas.mjs'

export const name = 'build-loop'
export const inject = ['tools', 'subagents', 'settings', 'webServer', 'systemPrompt', 'tokenMeter']

const OUTPUT = {
  schema: { type: 'object', additionalProperties: true },
  render: (_args, value) => [{ type: 'text', text: value.kind === 'background' ? 'Build job ' + value.jobId : value.text ?? JSON.stringify(value) }],
}

/** Background completion preserves the caller decision point; cancellation reaches owned work. */
function dispatch(ctx, exec, background, label, work) {
  if (!background) return work(exec)
  const jobs = ctx.get('jobs')
  if (!jobs) throw new Error('background jobs unavailable')
  const abort = new AbortController()
  return {
    kind: 'background',
    jobId: jobs.start({
      kind: 'subagent', label, owner: exec.agent,
      run: () => ({
        cancel: (reason) => abort.abort(reason),
        done: work({ ...exec, signal: abort.signal })
          .then((value) => ({ status: abort.signal.aborted ? 'killed' : 'completed', output: value.text }))
          .catch((error) => ({ status: abort.signal.aborted ? 'killed' : 'failed', detail: String(error) })),
      }),
    }),
  }
}

export async function apply(ctx) {
  const scope = await registerSettings(ctx)
  const controller = new Controller(ctx)
  ctx.effect(() => () => controller.workers.close(), 'build-loop workers')
  registerTools(ctx, scope, controller)
}

/** Register tool interfaces independently of the HTTP carrier. */
export function registerTools(ctx, scope, controller) {
  ctx.systemPrompt.section({
    name: 'tool:build_ticket', order: 116.7,
    text: (context) => ctx.tools.get('build_ticket', context.scope) ?
      'Use build_ticket for ticket implementation. Supply an explicit contract: observable behaviors, approved check commands, and authorized scope. Approve the proposed approach through build_ticket_decide. Triage every open audit finding with fix or ignore and a reason; only approved fixes go to the builder. Resolve builder disputes as the caller. Auditors can reopen ignored findings with stronger evidence. Resume the same run and revision; failed or interrupted work is not complete.' : '',
  })
  ctx.tools.register(defineTool({
    name: 'build_ticket',
    description: 'Start a supervised build and pause for caller design approval. Scope is an instruction to workers, not a filesystem restriction. Each behavior names an observation and optional approved check ID. The builder requests captured checks. Runs live in memory only; no Git, source hashing, sandboxing, or filesystem checkpoints.',
    parameters: {
      ticket: { type: 'string', required: true, description: 'Ticket path in the calling workspace.' },
      contract: { type: 'object', properties: parameters(CONTRACT_SCHEMA), additionalProperties: false, required: true },
      run_in_background: { type: 'boolean' },
    },
    output: OUTPUT,
    async execute(args, exec) {
      if (!exec.agent) throw new Error('build_ticket requires a calling agent')
      if (!args.ticket.trim()) throw new Error('ticket must be nonempty')
      return dispatch(ctx, exec, args.run_in_background, 'build ' + args.ticket, (execution) => controller.start(args, execution, scope.get()))
    },
  }))
  ctx.tools.register(defineTool({
    name: 'build_ticket_decide',
    description: 'Resume an in-memory run with its exact decision revision. approve_design/revise_design answer an approach; triage requires dispositions [{id, action: fix|ignore, reason}] for every open finding and sends only approved fixes to the builder; continue supplies guidance or resumes an already approved task, not new untriaged fixes; ask questions a worker; accept requires checks and audits for the current attempt and no open findings; abandon stops; inspect reads state. Builder disputes remain open until caller decision; ignored findings may reopen with stronger auditor evidence. Host restart loses runs.',
    parameters: parameters(DECISION_SCHEMA), output: OUTPUT,
    async execute(args, exec) {
      if (!exec.agent) throw new Error('build_ticket_decide requires a calling agent')
      return dispatch(ctx, exec, args.run_in_background, 'build decision ' + args.run_id, (execution) => controller.decide(args, execution))
    },
  }))
  ctx.tools.register(defineTool({
    name: 'build_ticket_check',
    description: 'Implementing build worker only: execute an approved check by ID through the controller. Returns captured exit, duration, stdout and stderr from the native bash tool. Use this instead of repeating the same command through bash.',
    parameters: { check_id: { type: 'string', required: true } }, output: OUTPUT,
    execute: (args, exec) => controller.check(args.check_id, exec),
  }))
}
