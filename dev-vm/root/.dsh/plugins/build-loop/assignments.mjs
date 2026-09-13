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
    'Use build_ticket_check with an approved check_id for required verification. It records the actual result; do not repeat the same command just to make another copy. Rerun after relevant changes. Stop or collect your background jobs before handoff.',
    'Use ready-for-audit only after every behavior has an observation and required checks have passed. Return needs-decision or blocked for a real conflict or environmental failure.',
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

export function renderRun(run, answer) {
  return [
    'Run ' + run.id + ' — ' + run.phase + ', revision ' + run.revision,
    'To resume use build_ticket_decide with this exact run_id and revision. Triage every open finding with fix or ignore and a reason. Only approved fixes reach the builder; disputes return to you. Ignored findings can be reopened by auditors with stronger evidence.',
    json(currentAssignment(run)), '# Audit reports', json(run.audits),
    answer ? '# Answer\n' + answer : '',
  ].filter(Boolean).join('\n\n')
}
