/** Worker turn messages and fresh process state, separate from routing code. Personas live in instructions/*.md. */
import { HANDOFF_SCHEMA, VERDICT_SCHEMA } from './schemas.mjs'
import { latestCheck } from './loop.mjs'

const json = (value) => JSON.stringify(value, null, 2)

export function currentAssignment(run) {
  return {
    run_id: run.id, revision: run.revision, phase: run.phase, task: run.task,
    workspace: run.cwd, ticket: run.ticket, contract: run.contract,
    approved_approach: run.approach ?? 'Pending caller approval',
    decisions: run.decisions, handoff: run.handoff,
    checks: run.contract.checks.map((c) => latestCheck(run, c.id)).filter(Boolean),
    findings: run.findings, budget: { used: run.fixRounds, limit: run.maxFixRounds },
    failure: run.failure,
  }
}

/** The builder's turn message: current state plus the instruction. The persona is in the system prompt. */
export function assignment(run, instruction) {
  return [
    '# Current assignment', json(currentAssignment(run)),
    instruction ?? 'Continue the assigned task without restarting completed work.',
    'Run focused verification directly through ordinary tools while implementing. The controller runs the approved final checks after a ready-for-audit handoff. Do not duplicate the complete check set solely to create another result. Stop or collect background jobs before handoff.',
    'Use ready-for-audit only after every behavior has an observation and your focused verification supports the handoff. Return needs-decision or blocked for a real conflict or environmental failure.',
    'End with exactly one fenced json report matching this schema, with nothing after it:', json(HANDOFF_SCHEMA),
  ].join('\n\n')
}

/** Injected by the pre-step hook only after compaction or every reminderTokens of new context, never per turn. */
export function reminder(run) {
  return ['<system_reminder>', run.instructions.common, run.instructions.builder, assignment(run), '</system_reminder>'].join('\n\n')
}

export function auditAssignment(run, assignment, question, repair) {
  const packet = currentAssignment(run)
  packet.findings = packet.findings.filter((f) => f.role === assignment.role)
  if (packet.handoff?.findings) packet.handoff = { ...packet.handoff, findings: packet.handoff.findings.filter((f) => packet.findings.some((own) => own.id === f.id)) }
  return [
    '# Current assignment', json(packet), '# Audit assignment', json(assignment),
    'Inspect the named obligations and assigned findings. Private reports from other auditors are not supplied.',
    question ? 'Answer only this caller question in plain text without edits: ' + question
      : 'Return findings to the caller for triage. A blocked inspection is not a clean audit.',
    repair ? 'Format-only repair: ' + repair + '. Use existing evidence; do not repeat the audit.' : '',
    question ? '' : 'End with exactly one fenced json report matching this schema, with nothing after it:',
    question ? '' : json(VERDICT_SCHEMA),
  ].filter(Boolean).join('\n\n')
}

export const FORMAT_REPAIR = 'Format-only repair. Use the completed work, change no source, and return the corrected report. Do not rerun implementation or verification.'

function nextStep(run) {
  const open = run.findings.filter((finding) => finding.status === 'open').map((finding) => finding.id)
  if (open.length) return 'Next: triage open findings ' + open.join(', ') + '.'
  if (run.phase === 'awaiting_design') return 'Next: approve or revise the builder approach.'
  if (run.phase === 'awaiting_acceptance') return 'Next: accept, continue, ask, or abandon.'
  if (run.phase === 'interrupted') return 'Next: continue, triage, ask, or abandon.'
  return 'Next: run is ' + run.phase + '.'
}

function inspectionState(run) {
  return {
    task: run.task,
    handoff: run.handoff?.outcome ?? null,
    checks: run.contract.checks.map((check) => {
      const record = latestCheck(run, check.id)
      return record === undefined ? { check: check.id, status: 'missing' } : {
        check: check.id, exitCode: record.exitCode, failed: record.failed,
      }
    }),
    openFindings: run.findings.filter((finding) => finding.status === 'open'),
    failure: run.failure,
  }
}

export function renderRun(run, updates, inspection = false) {
  return [
    'Run ' + run.id + ' — ' + run.phase + ', revision ' + run.revision,
    nextStep(run),
    inspection ? '# Current state\n' + json(inspectionState(run)) : '',
    updates.length ? '# New updates\n' + json(updates) : '',
  ].filter(Boolean).join('\n\n')
}
