/**
 * Pure pieces of the build loop: prompt composition and report rendering.
 * No runtime dependencies so they are testable with plain `node --test`.
 */

/** Structured verdict every review/test child must return. */
export const VERDICT_SCHEMA = {
  type: 'object',
  properties: {
    clean: {
      type: 'boolean',
      description: 'true only when there are no findings that require the build agent to change anything',
    },
    findings: {
      type: 'array',
      items: { type: 'string' },
      description: 'actionable findings, each self-contained (file, what is wrong, minimal fix); empty when clean',
    },
    report: { type: 'string', description: 'the full review report as the final message would read' },
  },
  required: ['clean', 'findings', 'report'],
  additionalProperties: false,
}

/** Prompt for the build child's first turn. */
export function buildPrompt({ ticket, constraints }) {
  return [
    `Implement the ticket at \`${ticket}\`.`,
    constraints ? `Caller constraints:\n${constraints}` : '',
    'When finished, return the report described in your instructions as your final message.',
  ].filter(Boolean).join('\n\n')
}

/** Prompt for one review or test child. */
export function auditPrompt({ ticket, constraints, buildReport, round }) {
  return [
    `Ticket: \`${ticket}\`.`,
    constraints ? `Caller constraints given to the build agent:\n${constraints}` : '',
    `This is fix round ${round}. The build agent's description of what it changed follows; it is orientation, not evidence.`,
    '--- CHANGE DESCRIPTION ---',
    buildReport,
    '--- END CHANGE DESCRIPTION ---',
    'Audit per your instructions, then call structured_output with clean, findings, and report. Set clean=true only when findings is empty.',
  ].filter(Boolean).join('\n\n')
}

/** Follow-up prompt to the build child carrying both verdicts. */
export function fixPrompt({ review, test, round, maxRounds }) {
  const list = (label, verdict) => verdict.findings.length === 0
    ? `${label}: clean.`
    : `${label} findings:\n${verdict.findings.map((finding) => `- ${finding}`).join('\n')}`
  return [
    `Fix round ${round} of ${maxRounds}. Independent review and test audits returned findings.`,
    list('Code review', review),
    list('Test audit', test),
    'Fix what is in scope of the ticket; push back with reasons on anything out of scope or wrong. Re-run full verification and mutation checks, update the ticket `## Answer`, and return the updated report.',
  ].join('\n\n')
}

/** Both verdicts clean means the loop stops. */
export function isClean(review, test) {
  return review.clean && test.clean
}

/**
 * Coerce a child's structured value into a verdict. A missing or malformed
 * value is a non-clean verdict carrying the child's text, so a broken audit
 * blocks acceptance instead of silently passing.
 */
export function toVerdict(structured, fallbackText) {
  if (structured && typeof structured === 'object'
    && typeof structured.clean === 'boolean'
    && Array.isArray(structured.findings)
    && structured.findings.every((finding) => typeof finding === 'string')
    && typeof structured.report === 'string') {
    return {
      clean: structured.clean && structured.findings.length === 0,
      findings: structured.findings,
      report: structured.report,
    }
  }
  return {
    clean: false,
    findings: ['audit returned no valid structured verdict; treat as unresolved'],
    report: fallbackText || '(no output)',
  }
}

/** Final text returned to the orchestrator. Faithful: no summarizing of child reports. */
export function renderOutcome({ ticket, status, rounds, maxRounds, build, review, test, failure }) {
  const head = {
    clean: `Ticket \`${ticket}\`: build accepted — review and test audits clean after ${rounds} fix round(s).`,
    unresolved: `Ticket \`${ticket}\`: NOT clean after ${maxRounds} fix round(s). Findings below remain open; do not treat the build as done.`,
    failed: `Ticket \`${ticket}\`: loop stopped early — ${failure}. Reports below are the latest available state.`,
  }[status]
  const section = (title, body) => `## ${title}\n\n${body ?? '(none)'}`
  return [
    head,
    section('Build report', build),
    section(`Code review (${review ? (review.clean ? 'clean' : `${review.findings.length} finding(s)`) : 'not run'})`, review?.report),
    section(`Test audit (${test ? (test.clean ? 'clean' : `${test.findings.length} finding(s)`) : 'not run'})`, test?.report),
  ].join('\n\n')
}
