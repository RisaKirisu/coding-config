/** Validate process handoffs. The caller judges source changes and evidence relevance. */
import { validateJsonSchemaValue } from '@deepseek-ai/dsh-tools'
import { CONTRACT_SCHEMA, HANDOFF_SCHEMA, VERDICT_SCHEMA, DECISION_SCHEMA } from './schemas.mjs'

export const ROLES = ['code', 'test']
export const textOf = (blocks) => typeof blocks === 'string' ? blocks : (blocks ?? []).filter((b) => b.type === 'text').map((b) => b.text).join('')

export function recordUpdate(run, update) {
  run.updates.push(structuredClone(update))
}

export function validate(schema, value) {
  if (value && typeof value === 'object' && typeof value.blocked === 'string' && !value.blocked.trim()) {
    delete value.blocked
  }
  const errors = validateJsonSchemaValue(schema, value, 'report')
  if (errors.length) throw new Error(errors.join('; '))
  const meaningful = (item) => {
    if (typeof item === 'string' && !item.trim()) throw new Error('report strings must be nonempty')
    if (item && typeof item === 'object') Object.values(item).forEach(meaningful)
  }
  meaningful(value)
}

export function unique(values, label) {
  if (new Set(values).size !== values.length) throw new Error('duplicate ' + label)
}

export function validateContract(contract) {
  validate(CONTRACT_SCHEMA, contract)
  if (!contract.behaviors.length || !contract.scope.length) throw new Error('contract needs behaviors and scope')
  if (contract.behaviors.some((b) => b.id === '@maintainability')) throw new Error('@maintainability is reserved')
  unique(contract.behaviors.map((b) => b.id), 'behavior id')
  unique(contract.checks.map((c) => c.id), 'check id')
  for (const b of contract.behaviors) {
    if (b.check && !contract.checks.some((c) => c.id === b.check)) throw new Error('unknown check for ' + b.id)
  }
}

export function parseReport(text, schema) {
  const blocks = [...text.matchAll(/```json\s*\n([\s\S]*?)```/g)]
  if (blocks.length !== 1 || text.slice(blocks[0].index + blocks[0][0].length).trim()) throw new Error('end with exactly one fenced json report')
  const value = JSON.parse(blocks[0][1])
  if (value && typeof value === 'object' && typeof value.blocked === 'string' && !value.blocked.trim()) {
    delete value.blocked
  }
  validate(schema, value)
  return value
}

export function validateHandoff(run, handoff) {
  validate(HANDOFF_SCHEMA, handoff)
  if (run.task.phase === 'approach' && !['approach', 'blocked', 'needs-decision'].includes(handoff.outcome)) throw new Error('approach cannot report implementation ready')
  if (run.task.phase !== 'approach' && handoff.outcome === 'approach') throw new Error('approved task cannot restart approach')
  if (['approach', 'ready-for-audit'].includes(handoff.outcome) && !Array.isArray(handoff.files)) throw new Error('handoff needs files')
  const notes = handoff.findings ?? []
  unique(notes.map((f) => f.id), 'builder finding')
  const ids = run.task.ids ?? []
  if (notes.some((n) => !ids.includes(n.id))) throw new Error('unknown assigned finding id')
  if (handoff.outcome === 'ready-for-audit' && notes.length !== ids.length) throw new Error('address exactly the assigned finding ids')
}

export function latestCheck(run, id) {
  return run.checks.findLast((c) => c.check === id && c.attempt === run.attempt)
}

export function missingChecks(run) {
  return run.contract.checks.filter((c) => {
    const record = latestCheck(run, c.id)
    return !record || record.failed || record.exitCode !== c.expectedExit
  })
}

export function obligations(run, role) {
  return [...run.contract.behaviors.map((b) => b.id), ...(role === 'code' ? ['@maintainability'] : [])]
}

export function initialAudits(run) {
  return ROLES.map((role) => ({ role, obligations: obligations(run, role), scope: run.contract.scope }))
}

export function missingAudits(run) {
  return ROLES.flatMap((role) => obligations(run, role).filter((id) => {
    const record = run.audits.findLast((a) => a.role === role && a.obligations.includes(id))
    return !record || record.failure || record.attempt !== run.attempt
  }).map((id) => role + ':' + id))
}

export function validateVerdict(run, assignment, verdict) {
  validate(VERDICT_SCHEMA, verdict)
  unique(verdict.prior.map((p) => p.id), 'prior finding id')
  if (verdict.blocked) return
  const ignored = assignment.ignored ?? []
  if (assignment.ids.some((id) => !verdict.prior.some((p) => p.id === id))) throw new Error('report every assigned active finding id')
  for (const p of verdict.prior) {
    if (!assignment.ids.includes(p.id) && !(ignored.includes(p.id) && p.status === 'open')) throw new Error('unknown or invalid prior finding id ' + p.id)
    if (!run.findings.some((f) => f.id === p.id && f.role === assignment.role)) throw new Error('foreign finding id ' + p.id)
  }
}

export function recordVerdict(run, assignment, verdict) {
  validateVerdict(run, assignment, verdict)
  const firstFinding = run.findings.length
  run.audits.push({ ...assignment, attempt: run.attempt, report: verdict.report, failure: verdict.blocked ?? null })
  if (!verdict.blocked) {
    for (const prior of verdict.prior) {
      const f = run.findings.find((f) => f.id === prior.id)
      f.verified = prior
      f.status = prior.status === 'resolved' ? 'closed' : 'open'
    }
    for (const f of verdict.findings) {
      const id = (assignment.role === 'code' ? 'C' : 'T') + (run.findings.filter((f) => f.role === assignment.role).length + 1)
      run.findings.push({ ...f, id, role: assignment.role, status: 'open' })
    }
  }
  recordUpdate(run, {
    type: 'audit', role: assignment.role, attempt: run.attempt,
    report: verdict.report, blocked: verdict.blocked ?? null,
    prior: verdict.prior, findings: run.findings.slice(firstFinding),
  })
}

/** Apply only caller-approved dispositions, after validating the entire batch. */
export function triageFindings(run, dispositions, instructions) {
  if (!dispositions) throw new Error('triage requires dispositions')
  unique(dispositions.map((d) => d.id), 'disposition id')
  const open = run.findings.filter((f) => f.status === 'open')
  if (!open.length || open.length !== dispositions.length || dispositions.some((d) => !open.some((f) => f.id === d.id))) throw new Error('triage every open finding exactly once')
  const ids = dispositions.filter((d) => d.action === 'fix').map((d) => d.id)
  const resume = run.task.phase === 'fix' && missingAudits(run).length > 0
    && ids.every((id) => run.task.ids.includes(id) && open.find((f) => f.id === id).caller?.action === 'fix')
  if (ids.length && !resume && run.fixRounds >= run.maxFixRounds) throw new Error('fix budget exhausted; ignore with a reason or abandon')
  for (const d of dispositions) {
    const f = open.find((f) => f.id === d.id)
    f.caller = { action: d.action, reason: d.reason }
    f.status = d.action === 'ignore' ? 'ignored' : 'open'
  }
  if (ids.length) {
    if (!resume) run.fixRounds += 1
    run.task = { phase: 'fix', ids, instructions }
  }
  return ids.length > 0
}

const ALLOWED = {
  awaiting_design: ['approve_design', 'revise_design', 'ask', 'abandon'],
  awaiting_decision: ['triage', 'continue', 'ask', 'abandon'],
  awaiting_acceptance: ['accept', 'continue', 'ask', 'abandon'],
  interrupted: ['triage', 'continue', 'ask', 'abandon'],
}

export function validateDecision(run, decision) {
  validate(DECISION_SCHEMA, decision)
  if (!Number.isSafeInteger(decision.revision) || decision.revision < 1) throw new Error('revision must be a positive integer')
  if (decision.revision !== run.revision) throw new Error('stale decision; inspect the current run')
  if (!ALLOWED[run.phase]?.includes(decision.kind)) throw new Error('decision not allowed in ' + run.phase)
  if (decision.kind === 'approve_design' && run.handoff?.outcome !== 'approach') throw new Error('approve_design needs an approach')
  if (['revise_design', 'continue'].includes(decision.kind) && !decision.instructions) throw new Error('decision needs instructions')
  if (decision.kind === 'ask' && (!decision.role || !decision.question)) throw new Error('ask needs role and question')
  if (decision.kind === 'continue' && run.findings.some((f) => f.status === 'open')) {
    if (run.task.phase !== 'fix' || !missingAudits(run).length || run.findings.some((f) => f.status === 'open' && (!run.task.ids.includes(f.id) || f.caller?.action !== 'fix'))) throw new Error('triage open findings before continuing')
  }
}

export function assertAcceptable(run) {
  if (run.failure) throw new Error('resolve the recorded failure before acceptance')
  if (run.handoff?.outcome !== 'ready-for-audit') throw new Error('no ready handoff')
  validateHandoff(run, run.handoff)
  if (missingChecks(run).length) throw new Error('required check results are missing or failed')
  if (missingAudits(run).length) throw new Error('required audit results are missing or failed')
  if (run.findings.some((f) => f.status === 'open')) throw new Error('open findings remain; the caller must triage them')
}
