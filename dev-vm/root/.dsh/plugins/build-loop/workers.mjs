/** Worker turns and their lifetime: retain one builder, drain every child before returning. */
import { finalAssistantOutput } from '@deepseek-ai/dsh-subagent'
import { foldConsumedWork } from '@deepseek-ai/dsh-agent'
import { createUserMessage } from '@deepseek-ai/dsh-llm'
import { scopeOf, scopeParentOf } from '@deepseek-ai/dsh-scope'
import { DEFAULTS, filterDeniedTools } from './config.mjs'
import { HANDOFF_SCHEMA, VERDICT_SCHEMA } from './schemas.mjs'
import { parseReport, textOf, validateHandoff, validateVerdict } from './loop.mjs'
import { assignment, FORMAT_REPAIR, auditAssignment, reminder } from './assignments.mjs'

const message = (text) => createUserMessage({ content: [{ type: 'text', text }], source: { kind: 'plugin', plugin: 'build-loop', form: 'instructions' } })
const turnEnd = (agent, boundary = 0) => foldConsumedWork(agent.session.snapshotEvents(boundary)).end?.data.reason

export class Workers {
  constructor(ctx) {
    this.ctx = ctx
    this.builders = new Map()
    this.active = new Map()
    // Web installs `subagent` on every agent's own layer, where toolFilter cannot reach; deny execution instead.
    ctx.tools.guard((exec) => {
      const worker = this.active.get(exec.agent?.session.id)
      return worker?.denied.has(exec.name) ? 'build-loop workers may not use ' + exec.name : undefined
    })
    // The only place the system reminder enters a builder: after compaction, or once context pressure
    // (ctx.tokenMeter, anchored on provider-reported usage) has grown by reminderTokens since the last one.
    ctx.on('agent/pre-step', async (payload, next) => {
      const decision = await next()
      const worker = this.active.get(payload.agent.session.id)
      if (!worker || worker.role !== 'builder' || decision.kind !== 'enter') return decision
      const session = payload.agent.session
      const compacted = session.snapshotEvents(worker.seq).some((e) => e.type === 'compaction/end' && !e.data.error)
      worker.seq = session.seq
      const pressure = ctx.tokenMeter.measure(session).totalTokens
      if (!compacted && pressure - worker.remindedAt < worker.run.policy.reminderTokens) return decision
      worker.remindedAt = pressure
      return { ...decision, messages: [...decision.messages, message(reminder(worker.run))] }
    }, { global: true })
  }

  /** Collect recorded job outcomes; unexpected live jobs are stopped before a handoff. */
  async collect(worker) {
    const jobs = this.ctx.get('jobs')
    if (!jobs) return
    const owner = worker.child.localAgent
    for (const item of jobs.list(owner).filter((j) => j.ownerSession === owner.session.id)) {
      if (['running', 'stopping'].includes(item.status)) jobs.kill(item.id, owner, 'Build phase ended; source must be quiescent')
      let settled = jobs.get(item.id, owner)
      while (['running', 'stopping'].includes(settled.status)) settled = await jobs.wait(item.id, 60000, owner)
      jobs.read(item.id, owner)
    }
  }

  async start(run, parent, role, text, signal) {
    signal.throwIfAborted()
    const denied = new Set([...DEFAULTS.deniedTools, ...run.policy.deniedTools])
    // The child inherits the parent's enclosing scope (preset mount or global layer), never the parent's own layer.
    const restrictable = filterDeniedTools([...denied], this.ctx.tools.schemas(scopeParentOf(scopeOf(parent.ctx))))
    const abort = new AbortController()
    const rolePrefix = role === 'builder' ? 'builder: ' : role === 'code' ? 'code-audit: ' : role === 'test' ? 'test-audit: ' : `${role}: `
    const child = await this.ctx.subagents.start(run.policy.provider, {
      parent, signal: AbortSignal.any([abort.signal, signal]), label: rolePrefix + run.ticket,
      persona: [run.instructions.common, run.instructions[role === 'builder' ? 'builder' : role === 'code' ? 'reviewer' : 'tester'], ...(role === 'builder' ? [] : [run.instructions.auditorFindings])].join('\n\n'),
      prompt: [{ type: 'text', text }], toolFilter: { deny: restrictable },
    })
    if (!child.localAgent) { await child.dispose(); throw new Error('Build loop requires a local child provider') }
    const worker = { child, abort, role, run, denied, seq: 0, remindedAt: 0 }
    this.active.set(child.localAgent.session.id, worker)
    if (role === 'builder') this.builders.set(run.id, worker)
    return worker
  }

  /** Link only this turn to caller cancellation; idle builders survive caller checkpoints. */
  async turn(worker, signal, text) {
    const agent = worker.child.localAgent
    const cancel = () => agent.cancel({ kind: 'parent' })
    signal.addEventListener('abort', cancel, { once: true })
    const boundary = agent.session.seq
    try {
      if (signal.aborted) { cancel(); await agent.whenIdle(); signal.throwIfAborted() }
      let result
      let end
      if (text !== undefined) {
        agent.followup(message(text))
        await agent.whenIdle()
        const events = agent.session.snapshotEvents(boundary)
        end = turnEnd(agent, boundary)
        result = { output: finalAssistantOutput(events), stopReason: end?.kind }
      } else {
        result = await worker.child.result
        end = turnEnd(agent)
      }
      if (end?.kind === 'error' && result.diagnostic === undefined) result = { ...result, diagnostic: end.error.message }
      await this.collect(worker)
      signal.throwIfAborted()
      return result
    } finally {
      signal.removeEventListener('abort', cancel)
    }
  }

  async dispose(worker) {
    worker.abort.abort('worker released')
    worker.child.localAgent.cancel({ kind: 'parent' })
    await worker.child.localAgent.whenIdle()
    await this.collect(worker)
    await worker.child.dispose()
    this.active.delete(worker.child.localAgent.session.id)
    if (worker.role === 'builder') this.builders.delete(worker.run.id)
  }

  /** One builder per run, started on first use and kept through failures until release. */
  async builder(run, parent, signal, instruction, plain = false) {
    let worker = this.builders.get(run.id)
    const text = assignment(run, instruction) + (plain ? '\n\nQuestion-only override: answer the supplied question in plain text, change no files, and do not produce a handoff.' : '')
    let result
    if (worker) result = await this.turn(worker, signal, text)
    else { worker = await this.start(run, parent, 'builder', text, signal); result = await this.turn(worker, signal) }
    if (result.stopReason !== 'completed') throw new Error('builder ended with ' + result.stopReason + ': ' + (result.diagnostic ?? ''))
    if (plain) return textOf(result.output)
    try {
      const handoff = parseReport(textOf(result.output), HANDOFF_SCHEMA)
      validateHandoff(run, handoff)
      return handoff
    } catch (error) {
      result = await this.turn(worker, signal, assignment(run, FORMAT_REPAIR + '\nValidation: ' + error.message))
      if (result.stopReason !== 'completed') throw new Error('builder format repair failed: ' + result.stopReason)
      const handoff = parseReport(textOf(result.output), HANDOFF_SCHEMA)
      validateHandoff(run, handoff)
      return handoff
    }
  }

  async auditor(run, parent, assignment, signal, question) {
    const worker = await this.start(run, parent, assignment.role, auditAssignment(run, assignment, question), signal)
    try {
      let result = await this.turn(worker, signal)
      if (question) {
        if (result.stopReason !== 'completed') throw new Error('audit question failed: ' + result.stopReason)
        return textOf(result.output)
      }
      if (result.stopReason !== 'completed') throw new Error('audit failed: ' + result.stopReason + ': ' + (result.diagnostic ?? ''))
      try {
        const verdict = parseReport(textOf(result.output), VERDICT_SCHEMA)
        validateVerdict(run, assignment, verdict)
        return verdict
      } catch (error) {
        result = await this.turn(worker, signal, auditAssignment(run, assignment, undefined, error.message))
        if (result.stopReason !== 'completed') throw new Error('audit format repair failed: ' + result.stopReason)
        const verdict = parseReport(textOf(result.output), VERDICT_SCHEMA)
        validateVerdict(run, assignment, verdict)
        return verdict
      }
    } finally {
      await this.dispose(worker)
    }
  }

  async release(runId) {
    const worker = this.builders.get(runId)
    if (worker) await this.dispose(worker)
  }

  async close() {
    await Promise.allSettled([...this.active.values()].map((worker) => this.dispose(worker)))
  }
}
