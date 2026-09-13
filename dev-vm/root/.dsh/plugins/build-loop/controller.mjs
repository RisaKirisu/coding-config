/** In-memory process routing. The caller approves fixes; workers report evidence and disputes. */
import { randomUUID } from 'node:crypto'
import { assertAcceptable, initialAudits, missingAudits, missingChecks, recordVerdict, triageFindings, validateContract, validateDecision } from './loop.mjs'
import { runCheck } from './checks.mjs'
import { renderRun } from './assignments.mjs'
import { Workers } from './workers.mjs'
import { loadInstructions } from './prompts.mjs'

const CHECK_RETRIES = 2

export class Controller {
  constructor(ctx) {
    this.ctx = ctx
    this.workers = new Workers(ctx)
    this.runs = new Map()
    this.busy = new Set()
    this.checks = new Set()
  }

  async start(args, exec, config) {
    validateContract(args.contract)
    const run = {
      id: randomUUID(), revision: 1, owner: exec.agent.session.id,
      cwd: exec.agent.session.header.cwd, ticket: args.ticket,
      contract: args.contract, policy: structuredClone(config), instructions: loadInstructions(),
      phase: 'awaiting_design', task: { phase: 'approach', ids: [] },
      decisions: [], checks: [], audits: [], findings: [], attempt: 0,
      fixRounds: 0, maxFixRounds: config.maxFixRounds, failure: null,
    }
    this.runs.set(run.id, run)
    return this.perform(run, async () => this.advance(run, exec))
  }

  async decide(d, exec) {
    if (this.busy.has(d.run_id)) throw new Error('run already has work in flight')
    const run = this.runs.get(d.run_id)
    if (!run) throw new Error('unknown run; runs exist only for the lifetime of this plugin instance')
    if (run.owner !== exec.agent.session.id) throw new Error('run belongs to another calling session')
    if (d.kind === 'inspect') return this.result(run)
    validateDecision(run, d)
    if (d.kind === 'accept') assertAcceptable(run)
    const fixing = d.kind === 'triage' && triageFindings(run, d.dispositions, d.instructions)
    if (d.kind === 'approve_design') {
      run.approach = run.handoff
      run.task = { phase: 'implement', ids: [], instructions: d.instructions }
    } else if (d.kind === 'revise_design') {
      run.task = { phase: 'approach', ids: [], instructions: d.instructions }
    } else if (d.kind === 'continue') {
      run.task.ids = run.task.ids.filter((id) => run.findings.some((f) => f.id === id && f.status === 'open'))
      run.task.instructions = d.instructions
    }
    run.decisions.push(d)
    return this.perform(run, async () => {
      switch (d.kind) {
        case 'triage':
          if (!fixing) { run.phase = this.pausedPhase(run); return }
          break
        case 'ask': {
          const question = d.question + '\nQuestion-only turn: change no files and reply in plain text.'
          const assignment = { role: d.role, obligations: [], ids: [] }
          const answer = d.role === 'builder'
            ? await this.workers.builder(run, exec.agent, exec.signal, question, true)
            : await this.workers.auditor(run, exec.agent, assignment, exec.signal, question)
          run.phase = this.pausedPhase(run)
          return answer
        }
        case 'accept':
          run.phase = 'complete'
          run.confirmations = d.confirmations ?? []
          await this.workers.release(run.id)
          return
        case 'abandon':
          run.phase = 'abandoned'
          await this.workers.release(run.id)
          return
      }
      run.failure = null
      await this.advance(run, exec)
    })
  }

  /** The phase a run pauses in when no work is running, derived from its state. */
  pausedPhase(run) {
    if (run.task.phase === 'approach') return 'awaiting_design'
    if (run.failure || run.handoff?.outcome !== 'ready-for-audit' || missingChecks(run).length || missingAudits(run).length || run.findings.some((f) => f.status === 'open')) return 'awaiting_decision'
    return 'awaiting_acceptance'
  }

  result(run, answer) {
    return { kind: 'foreground', run_id: run.id, revision: run.revision, phase: run.phase, text: renderRun(run, answer) }
  }

  async perform(run, work) {
    this.busy.add(run.id)
    run.phase = 'running'
    run.revision += 1
    let answer
    try {
      answer = await work()
      if (run.phase === 'running') run.phase = this.pausedPhase(run)
    } catch (error) {
      run.failure = error?.message ?? String(error)
      run.phase = 'interrupted'
    } finally {
      run.revision += 1
      this.busy.delete(run.id)
    }
    return this.result(run, answer)
  }

  /** One approved builder cycle, bounded check retries, then audits returned to the caller. */
  async advance(run, exec) {
    let checkRetries = 0
    while (true) {
      run.attempt += 1
      const handoff = await this.workers.builder(run, exec.agent, exec.signal, run.task.instructions)
      run.handoff = handoff
      for (const note of handoff.findings ?? []) run.findings.find((f) => f.id === note.id).builder = note
      if (run.task.phase === 'approach') { run.phase = 'awaiting_design'; return }
      if (handoff.outcome !== 'ready-for-audit' || handoff.findings?.some((f) => f.status === 'disputed')) {
        run.phase = 'awaiting_decision'
        return
      }
      for (const check of missingChecks(run)) {
        // Reuse a result captured by the builder during this attempt, even a failed one.
        if (!run.checks.some((c) => c.check === check.id && c.attempt === run.attempt)) await runCheck(this.ctx, exec, run, check)
        exec.signal.throwIfAborted()
      }
      const failed = missingChecks(run)
      if (!failed.length) break
      if (++checkRetries > CHECK_RETRIES) {
        run.failure = 'Required checks still failed after ' + CHECK_RETRIES + ' builder retries. Continue with instructions or abandon.'
        run.phase = 'awaiting_decision'
        return
      }
      run.task.instructions = 'Approved checks failed: ' + failed.map((c) => c.id).join(', ') + '. Read the captured results, correct the cause, rerun with build_ticket_check, and hand off again.'
    }
    await this.audit(run, exec)
    run.phase = this.pausedPhase(run)
  }

  /** Both auditors review every obligation; on a fix round each also rechecks its own open findings. */
  async audit(run, exec) {
    const abort = new AbortController()
    const signal = AbortSignal.any([exec.signal, abort.signal])
    const assignments = initialAudits(run).map((a) => ({
      ...a,
      ids: run.findings.filter((f) => f.role === a.role && f.status === 'open').map((f) => f.id),
      ignored: run.findings.filter((f) => f.role === a.role && f.status === 'ignored').map((f) => f.id),
    }))
    const results = await Promise.allSettled(assignments.map(async (a) => {
      try { return await this.workers.auditor(run, exec.agent, a, signal) }
      catch (error) { abort.abort('audit sibling failed'); throw error }
    }))
    exec.signal.throwIfAborted()
    for (let i = 0; i < results.length; i++) {
      const result = results[i]
      if (result.status === 'fulfilled') recordVerdict(run, assignments[i], result.value)
      else run.audits.push({ ...assignments[i], attempt: run.attempt, failure: result.reason?.message ?? String(result.reason) })
    }
    const missing = missingAudits(run)
    run.failure = missing.length ? 'Missing or failed audit results: ' + missing.join(', ') : null
  }

  async check(checkId, exec) {
    const worker = this.workers.active.get(exec.agent?.session.id)
    if (!worker || worker.role !== 'builder' || worker.run.task.phase === 'approach') throw new Error('only an implementing builder may request checks')
    const run = worker.run
    const check = run.contract.checks.find((c) => c.id === checkId)
    if (!check) throw new Error('unknown approved check')
    if (this.checks.has(run.id)) throw new Error('another verification check is running')
    this.checks.add(run.id)
    try { return await runCheck(this.ctx, exec, run, check) }
    finally { this.checks.delete(run.id) }
  }
}
