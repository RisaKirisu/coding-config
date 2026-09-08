import test from 'node:test'
import assert from 'node:assert/strict'
import {
  auditPrompt,
  evaluateAuditOutcome,
  fixPrompt,
  formatAuditFailure,
  isClean,
  renderOutcome,
  runWithRetries,
  toVerdict,
} from './loop.mjs'
import { DEFAULTS, filterDeniedTools, validateConfig } from './config.mjs'

test('toVerdict accepts only valid structured verdicts and never returns clean without one', () => {
  assert.equal(toVerdict(undefined), undefined)
  assert.equal(toVerdict({ clean: true }), undefined)
  assert.equal(toVerdict({ clean: true, findings: [] }), undefined)
  assert.equal(toVerdict({ clean: true, findings: [], report: 123 }), undefined)
  assert.equal(toVerdict({ clean: false, findings: [123], report: 'r' }), undefined)
  assert.equal(toVerdict({ clean: 'yes', findings: [], report: 'r' }), undefined)
  assert.equal(toVerdict({ clean: true, findings: 'none', report: 'r' }), undefined)
  assert.deepEqual(toVerdict({ clean: true, findings: ['x'], report: 'r' }), { clean: false, findings: ['x'], report: 'r' })
  assert.deepEqual(toVerdict({ clean: true, findings: [], report: 'r' }), { clean: true, findings: [], report: 'r' })
  assert.deepEqual(toVerdict({ clean: false, findings: ['x'], report: 'r' }), { clean: false, findings: ['x'], report: 'r' })
})

test('evaluateAuditOutcome accepts valid structured verdict', () => {
  const cleanStructured = { clean: true, findings: [], report: 'All clean' }
  assert.deepEqual(evaluateAuditOutcome({ structured: cleanStructured, stopReason: 'completed' }, 'completed'), {
    ok: true,
    verdict: { clean: true, findings: [], report: 'All clean' },
  })

  const findingStructured = { clean: false, findings: ['Issue 1'], report: 'Found bug' }
  assert.deepEqual(evaluateAuditOutcome({ structured: findingStructured, stopReason: 'completed' }, 'completed'), {
    ok: true,
    verdict: { clean: false, findings: ['Issue 1'], report: 'Found bug' },
  })
})

test('evaluateAuditOutcome falls back to normally finished non-empty plain text as clean false', () => {
  // Normal plain-text completion when DSH maps missing-output stopReason to 'error'
  const resFromDshMappedError = evaluateAuditOutcome(
    {
      output: [{ type: 'text', text: 'Audit finding: file.ts line 10 needs fix' }],
      stopReason: 'error',
    },
    'completed',
  )
  assert.deepEqual(resFromDshMappedError, {
    ok: true,
    verdict: {
      clean: false,
      findings: ['Audit finding: file.ts line 10 needs fix'],
      report: 'Audit finding: file.ts line 10 needs fix',
    },
  })

  // Plain string output with completed stopReason
  const resPlainString = evaluateAuditOutcome(
    {
      output: 'All tests pass but coverage is thin',
      stopReason: 'completed',
    },
    undefined,
  )
  assert.deepEqual(resPlainString, {
    ok: true,
    verdict: {
      clean: false,
      findings: ['All tests pass but coverage is thin'],
      report: 'All tests pass but coverage is thin',
    },
  })
})

test('evaluateAuditOutcome rejects partial output from failed execution', () => {
  // Execution ended with error turnReason
  const failed = evaluateAuditOutcome(
    {
      output: [{ type: 'text', text: 'Partial report before exception...' }],
      stopReason: 'error',
    },
    'error',
  )
  assert.equal(failed.ok, false)
  assert.match(failed.cause, /error/)

  // Execution ended with error carrying diagnostic
  const withDiagnostic = evaluateAuditOutcome(
    {
      stopReason: 'error',
      diagnostic: 'timeout',
    },
    'error',
  )
  assert.equal(withDiagnostic.ok, false)
  assert.match(withDiagnostic.cause, /timeout/)

  // Execution aborted
  const aborted = evaluateAuditOutcome(
    {
      output: [{ type: 'text', text: 'Partial text...' }],
      stopReason: 'aborted',
    },
    'aborted',
  )
  assert.equal(aborted.ok, false)
})

test('evaluateAuditOutcome rejects empty execution even when completed', () => {
  assert.deepEqual(evaluateAuditOutcome({ output: '', stopReason: 'completed' }, 'completed'), {
    ok: false,
    cause: 'audit produced empty output',
  })
  assert.deepEqual(evaluateAuditOutcome({ output: [{ type: 'text', text: '   ' }], stopReason: 'completed' }, 'completed'), {
    ok: false,
    cause: 'audit produced empty output',
  })
  assert.deepEqual(evaluateAuditOutcome({ output: [], stopReason: 'completed' }, 'completed'), {
    ok: false,
    cause: 'audit produced empty output',
  })
})

test('runWithRetries respects three-attempt limit and reports exhausted attempts', async () => {
  let attempts = 0
  const res = await runWithRetries(async () => {
    attempts++
    return { ok: false, cause: 'flaky failure' }
  }, { maxAttempts: 3 })

  assert.equal(attempts, 3)
  assert.deepEqual(res, { ok: false, attempts: 3, lastCause: 'flaky failure' })
})

test('runWithRetries succeeds after retry without executing further attempts', async () => {
  let attempts = 0
  const res = await runWithRetries(async (n) => {
    attempts = n
    if (n === 1) return { ok: false, cause: 'temp issue' }
    return { ok: true, verdict: { clean: true, findings: [], report: 'clean report' } }
  }, { maxAttempts: 3 })

  assert.equal(attempts, 2)
  assert.equal(res.ok, true)
  assert.equal(res.attempts, 2)
  assert.equal(res.verdict.clean, true)
})

test('runWithRetries retries on thrown exceptions in attempt function', async () => {
  let attempts = 0
  const res = await runWithRetries(async (n) => {
    attempts = n
    if (n === 1) throw new Error('connection dropped')
    return { ok: true, verdict: { clean: false, findings: ['f'], report: 'r' } }
  }, { maxAttempts: 3 })

  assert.equal(attempts, 2)
  assert.equal(res.ok, true)
  assert.equal(res.attempts, 2)
})

test('runWithRetries halts immediately on caller cancellation without retrying', async () => {
  const preAborted = new AbortController()
  preAborted.abort()
  let preAttempts = 0
  await assert.rejects(
    async () => {
      await runWithRetries(async () => {
        preAttempts++
        return { ok: true, verdict: { clean: true, findings: [], report: 'r' } }
      }, { maxAttempts: 3, signal: preAborted.signal })
    },
    /cancelled/,
  )
  assert.equal(preAttempts, 0)

  const controller = new AbortController()
  let attempts = 0
  await assert.rejects(
    async () => {
      await runWithRetries(async () => {
        attempts++
        controller.abort()
        throw new Error('child interrupted by caller abort')
      }, { maxAttempts: 3, signal: controller.signal })
    },
    /child interrupted by caller abort/,
  )
  assert.equal(attempts, 1)

  const resolveController = new AbortController()
  let resolveAttempts = 0
  await assert.rejects(
    async () => {
      await runWithRetries(async () => {
        resolveAttempts++
        resolveController.abort()
        return { ok: true, verdict: { clean: true, findings: [], report: 'clean' } }
      }, { maxAttempts: 3, signal: resolveController.signal })
    },
    /build_ticket was cancelled/,
  )
  assert.equal(resolveAttempts, 1)
})

test('runWithRetries ensures cleanup runs across all attempts', async () => {
  const disposed = []
  let attempts = 0
  const res = await runWithRetries(async (n) => {
    attempts = n
    try {
      if (n < 3) return { ok: false, cause: 'retry' }
      return { ok: true, verdict: { clean: true, findings: [], report: 'done' } }
    } finally {
      disposed.push(`child-${n}`)
    }
  }, { maxAttempts: 3 })

  assert.equal(attempts, 3)
  assert.deepEqual(disposed, ['child-1', 'child-2', 'child-3'])
  assert.equal(res.ok, true)
})

test('formatAuditFailure includes failed phases, attempt counts, last causes, and exact user instruction', () => {
  const single = formatAuditFailure([{ phase: 'review', attempts: 3, lastCause: 'stream ended abnormally' }])
  assert.match(single, /review failed after 3 attempt\(s\) \(last cause: stream ended abnormally\)/)
  assert.match(single, /Notify the user immediately\. Do not retry build_ticket\./)

  const sibling = formatAuditFailure([
    { phase: 'review', attempts: 3, lastCause: 'crash' },
    { phase: 'test-audit', attempts: 3, lastCause: 'empty output' },
  ])
  assert.match(sibling, /review failed after 3 attempt\(s\) \(last cause: crash\)/)
  assert.match(sibling, /test-audit failed after 3 attempt\(s\) \(last cause: empty output\)/)
  assert.match(sibling, /Notify the user immediately\. Do not retry build_ticket\./)
})

test('loop stops only when both audits are clean', () => {
  const clean = { clean: true, findings: [], report: '' }
  const dirty = { clean: false, findings: ['f'], report: '' }
  assert.equal(isClean(clean, clean), true)
  assert.equal(isClean(clean, dirty), false)
  assert.equal(isClean(dirty, clean), false)
})

test('fix prompt carries every finding from both audits', () => {
  const text = fixPrompt({
    review: { clean: false, findings: ['review-1', 'review-2'], report: '' },
    test: { clean: true, findings: [], report: '' },
    round: 2,
    maxRounds: 3,
  })
  assert.match(text, /round 2 of 3/)
  assert.match(text, /- review-1\n- review-2/)
  assert.match(text, /Test audit: clean\./)
})

test('audit prompt embeds the build report verbatim', () => {
  const text = auditPrompt({ ticket: 't.md', buildReport: 'LINE A\nLINE B', round: 1 })
  assert.match(text, /--- CHANGE DESCRIPTION ---\n\nLINE A\nLINE B\n\n--- END CHANGE DESCRIPTION ---/)
})

test('outcome renders all three reports verbatim and states the status', () => {
  const out = renderOutcome({
    ticket: 't.md', status: 'unresolved', rounds: 3, maxRounds: 3,
    build: 'BUILD-REPORT', review: { clean: false, findings: ['a', 'b'], report: 'REVIEW-REPORT' },
    test: { clean: true, findings: [], report: 'TEST-REPORT' },
  })
  assert.match(out, /NOT clean after 3 fix round/)
  assert.match(out, /## Build report\n\nBUILD-REPORT/)
  assert.match(out, /## Code review \(2 finding\(s\)\)\n\nREVIEW-REPORT/)
  assert.match(out, /## Test audit \(clean\)\n\nTEST-REPORT/)
  assert.match(renderOutcome({ ticket: 't', status: 'failed', rounds: 0, maxRounds: 3, build: 'B', failure: 'boom' }), /stopped early — boom/)
})

test('renderOutcome preserves latest reports and formats failure on exhausted audit', () => {
  const failure = formatAuditFailure([{ phase: 'test-audit', attempts: 3, lastCause: 'process crashed' }])
  const out = renderOutcome({
    ticket: 't.md',
    status: 'failed',
    rounds: 1,
    maxRounds: 3,
    build: 'BUILD-REPORT-FIXED',
    review: { clean: true, findings: [], report: 'REVIEW-REPORT-CLEAN' },
    test: { clean: false, findings: ['prior'], report: 'TEST-REPORT-PRIOR' },
    failure,
  })
  assert.match(out, /stopped early — test-audit failed after 3 attempt\(s\) \(last cause: process crashed\)\. Notify the user immediately\. Do not retry build_ticket\./)
  assert.match(out, /## Build report\n\nBUILD-REPORT-FIXED/)
  assert.match(out, /## Code review \(clean\)\n\nREVIEW-REPORT-CLEAN/)
  assert.match(out, /## Test audit \(1 finding\(s\)\)\n\nTEST-REPORT-PRIOR/)
})

test('config validation rejects empty prompts and negative budgets, accepts defaults', () => {
  validateConfig(DEFAULTS)
  assert.throws(() => validateConfig({ ...DEFAULTS, buildPersona: '  ' }), /buildPersona/)
  assert.throws(() => validateConfig({ ...DEFAULTS, maxFixRounds: -1 }), /maxFixRounds/)
  assert.throws(() => validateConfig({ ...DEFAULTS, deniedTools: 'x' }), /deniedTools/)
})

test('child denylist ignores parent-only and removed tool names while preserving global fan-out tools', () => {
  const denied = filterDeniedTools(
    ['build_ticket', 'subagent', 'subagent_fork', 'subagent_codex', 'workflow', 'ralph', 'run_code'],
    [
      { name: 'build_ticket' },
      { name: 'subagent_fork' },
      { name: 'workflow' },
      { name: 'ralph' },
      { name: 'bash' },
      { name: 'run_code' },
    ],
  )
  assert.deepEqual(denied, ['build_ticket', 'subagent_fork', 'workflow', 'ralph'])
})

test('child denylist safely handles empty and missing inputs', () => {
  assert.deepEqual(filterDeniedTools([], [{ name: 'bash' }]), [])
  assert.deepEqual(filterDeniedTools(['bash'], []), [])
  assert.deepEqual(filterDeniedTools(undefined, [{ name: 'bash' }]), [])
  assert.deepEqual(filterDeniedTools(['bash'], undefined), [])
})

test('build persona forbids verification claims in the report so auditors are not anchored', () => {
  const report = DEFAULTS.buildPersona.slice(DEFAULTS.buildPersona.indexOf('## Report'), DEFAULTS.buildPersona.indexOf('## Fix rounds'))
  assert.doesNotMatch(report, /Mutants:|Test counts/)
  assert.match(report, /Do not include test counts.*mutation results/)
  assert.match(DEFAULTS.testPersona, /Choose every mutation yourself/)
})
