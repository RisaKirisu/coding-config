import test from 'node:test'
import assert from 'node:assert/strict'
import { auditPrompt, fixPrompt, isClean, renderOutcome, toVerdict } from './loop.mjs'
import { DEFAULTS, validateConfig } from './config.mjs'

test('toVerdict never returns clean without a valid structured verdict', () => {
  assert.equal(toVerdict(undefined, 'partial text').clean, false)
  assert.equal(toVerdict({ clean: true }, '').clean, false)
  assert.equal(toVerdict({ clean: true, findings: ['x'], report: 'r' }, '').clean, false)
  assert.equal(toVerdict({ clean: true, findings: [], report: 'r' }, '').clean, true)
  assert.equal(toVerdict(undefined, 'partial text').report, 'partial text')
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

test('config validation rejects empty prompts and negative budgets, accepts defaults', () => {
  validateConfig(DEFAULTS)
  assert.throws(() => validateConfig({ ...DEFAULTS, buildPersona: '  ' }), /buildPersona/)
  assert.throws(() => validateConfig({ ...DEFAULTS, maxFixRounds: -1 }), /maxFixRounds/)
  assert.throws(() => validateConfig({ ...DEFAULTS, deniedTools: 'x' }), /deniedTools/)
})

test('build persona forbids verification claims in the report so auditors are not anchored', () => {
  const report = DEFAULTS.buildPersona.slice(DEFAULTS.buildPersona.indexOf('## Report'), DEFAULTS.buildPersona.indexOf('## Fix rounds'))
  assert.doesNotMatch(report, /Mutants:|Test counts/)
  assert.match(report, /Do not include test counts.*mutation results/)
  assert.match(DEFAULTS.testPersona, /Choose every mutation yourself/)
})
