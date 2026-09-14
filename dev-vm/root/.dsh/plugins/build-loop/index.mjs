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
      'Implement simple changes directly. Create tickets and use build_ticket for ticket implementation when user requests. A run that failed or was interrupted is not complete. build_ticket and build_ticket_decide can block for a long time: run them with run_in_background and end your turn; you will be notified when the call finishes.' : '',
  })
  ctx.tools.register(defineTool({
    name: 'build_ticket',
    description: 'Start a supervised build and pause for caller design approval. Scope is an instruction to workers, not a filesystem restriction. Each behavior names an observation and optional approved check ID. The controller runs captured checks after each ready handoff. Runs live in memory only; no Git, source hashing, sandboxing, or filesystem checkpoints.',
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
}
