import test from 'node:test'
import assert from 'node:assert/strict'
import fs from 'node:fs/promises'
import os from 'node:os'
import path from 'node:path'
import { randomUUID } from 'node:crypto'
import { setTimeout as delay } from 'node:timers/promises'
import { runtime } from './test-runtime.mjs'
import { Controller } from './controller.mjs'
import { runCheck } from './checks.mjs'
import { assertAcceptable, initialAudits, missingChecks, recordVerdict } from './loop.mjs'
import { VERDICT_SCHEMA } from './schemas.mjs'
import { DEFAULTS } from './config.mjs'
import { loadInstructions } from './prompts.mjs'
import { assignment, reminder, auditAssignment } from './assignments.mjs'
import { registerTools } from './index.mjs'

const directory = await fs.mkdtemp(path.join(os.tmpdir(), 'build-loop-process-test-'))
const host = await runtime()
test.after(async () => { await host.close(); await fs.rm(directory, { recursive: true, force: true }) })

async function fixture(t) {
  const cwd = await fs.mkdtemp(path.join(directory, 'workspace-'))
  const handle = await host.ctx.agents.create({ sessionId: randomUUID(), meta: { cwd } })
  t.after(() => handle.dispose())
  const contract = { summary: 'Small process pilot', scope: ['sample.mjs'], behaviors: [{ id: 'B1', observation: 'expected output', check: 'check' }], checks: [{ id: 'check', command: 'printf verified', scope: ['sample.mjs'], expectedExit: 0 }] }
  const run = { id: randomUUID(), revision: 1, owner: handle.agent.session.id, cwd, ticket: 'ticket.md', contract, policy: structuredClone(DEFAULTS), instructions: loadInstructions(), phase: 'awaiting_acceptance', task: { phase: 'implement', ids: [] }, decisions: [], checks: [], attempt: 1, audits: [], findings: [], fixRounds: 0, maxFixRounds: 3, failure: null, handoff: { outcome: 'ready-for-audit', summary: 'implemented', files: ['sample.mjs'], observations: ['specific observation'], findings: [] } }
  return { run, exec: { agent: handle.agent, signal: new AbortController().signal, callId: randomUUID() } }
}

function cleanAudits(run) {
  for (const a of initialAudits(run)) recordVerdict(run, { ...a, ids: [] }, { findings: [], prior: [], report: 'required behavior inspected' })
}

test('acceptance requires checks and audits; source changes are caller judgment', async (t) => {
  const { run, exec } = await fixture(t)
  assert.throws(() => assertAcceptable(run), /check results/)
  await runCheck(host.ctx, exec, run, run.contract.checks[0])
  assert.throws(() => assertAcceptable(run), /audit results/)
  cleanAudits(run)
  await fs.writeFile(path.join(run.cwd, 'unrelated.txt'), 'not inspected by controller')
  assert.doesNotThrow(() => assertAcceptable(run))
  assert.deepEqual(await fs.readdir(run.cwd), ['unrelated.txt'])
  const controller = new Controller(host.ctx)
  controller.runs.set(run.id, run)
  await assert.rejects(controller.decide({ run_id: run.id, revision: 99, kind: 'accept' }, exec), /stale/)
  const result = await controller.decide({ run_id: run.id, revision: 1, kind: 'accept' }, exec)
  assert.equal(result.phase, 'complete')
})

test('check results count only for the current builder attempt', async (t) => {
  const { run, exec } = await fixture(t)
  await runCheck(host.ctx, exec, run, run.contract.checks[0])
  assert.equal(missingChecks(run).length, 0)
  run.attempt += 1
  assert.equal(missingChecks(run).length, 1)
  await runCheck(host.ctx, exec, run, run.contract.checks[0])
  assert.equal(missingChecks(run).length, 0)
  assert.equal(run.checks.length, 2)
})

test('all-ignore triage permits acceptance without another worker round; invalid triage is atomic', async (t) => {
  const { run, exec } = await fixture(t)
  await runCheck(host.ctx, exec, run, run.contract.checks[0])
  cleanAudits(run)
  const finding = { impact: 'high', location: 'sample.mjs:1', rule: 'B1', evidence: 'missing case', correction: 'add it' }
  recordVerdict(run, { role: 'code', ids: [], obligations: ['B1', '@maintainability'] }, { findings: [finding], prior: [], report: 'one issue' })
  run.phase = 'awaiting_decision'
  const controller = new Controller(host.ctx)
  controller.runs.set(run.id, run)
  await assert.rejects(controller.decide({ run_id: run.id, revision: 1, kind: 'continue', instructions: 'fix everything' }, exec), /triage/)
  for (const dispositions of [[], [{ id: 'other', action: 'ignore', reason: 'wrong id' }], [{ id: 'C1', action: 'ignore', reason: 'one' }, { id: 'C1', action: 'ignore', reason: 'duplicate' }]]) {
    await assert.rejects(controller.decide({ run_id: run.id, revision: 1, kind: 'triage', dispositions }, exec))
    assert.equal(run.findings[0].status, 'open')
    assert.equal(run.revision, 1)
  }
  const result = await controller.decide({ run_id: run.id, revision: 1, kind: 'triage', dispositions: [{ id: 'C1', action: 'ignore', reason: 'outside the required behavior' }] }, exec)
  assert.equal(result.phase, 'awaiting_acceptance')
  assert.equal(run.attempt, 1)
  assert.equal(run.fixRounds, 0)
  assert.equal(run.findings[0].caller.reason, 'outside the required behavior')
  assert.equal((await controller.decide({ run_id: run.id, revision: result.revision, kind: 'accept' }, exec)).phase, 'complete')
})

test('wrong auditor and blocked reports cannot close findings', async (t) => {
  const { run } = await fixture(t)
  run.findings = [{ id: 'T1', role: 'test', status: 'open' }]
  const report = { findings: [], prior: [{ id: 'T1', status: 'resolved', evidence: 'checked' }], report: 'audit' }
  assert.throws(() => recordVerdict(run, { role: 'code', ids: ['T1'], obligations: ['B1'] }, report), /foreign/)
  recordVerdict(run, { role: 'test', ids: ['T1'], obligations: ['B1'] }, { ...report, blocked: 'unable to inspect' })
  assert.equal(run.findings[0].status, 'open')
  recordVerdict(run, { role: 'test', ids: ['T1'], obligations: ['B1'] }, { ...report, blocked: '' })
  assert.equal(run.findings[0].status, 'closed')
})

test('real child failure keeps the one builder in memory and writes no checkpoint', async (t) => {
  const { run, exec } = await fixture(t)
  run.phase = 'awaiting_design'
  run.task = { phase: 'approach', ids: [] }
  run.handoff = { outcome: 'approach', summary: 'use a direct loop', files: ['sample.mjs'] }
  const controller = new Controller(host.ctx)
  controller.runs.set(run.id, run)
  run.policy.reminderTokens = 1
  // Web installs `subagent` on the parent's own layer: the child cannot restrict it, so the denylist must skip it.
  const tool = (name) => ({ name, description: name, parameters: {}, output: { schema: { type: 'object' }, render: () => [] }, execute: async () => ({}) })
  exec.agent.ctx.tools.register(tool('subagent'))
  const global = host.ctx.tools.register(tool('workflow'))
  t.after(global)
  const started = t.mock.method(host.ctx.subagents, 'start')
  const result = await controller.decide({ run_id: run.id, revision: 1, kind: 'approve_design' }, exec)
  const request = started.mock.calls[0].arguments[1]
  // Persona is the instruction files; the turn message carries only the assignment.
  assert.ok(request.persona.startsWith(run.instructions.common))
  assert.ok(request.persona.endsWith(run.instructions.builder))
  assert.doesNotMatch(request.persona, /Auditor finding policy/)
  assert.doesNotMatch(request.prompt[0].text, /system_reminder|# Builder instructions/)
  assert.deepEqual(request.toolFilter.deny, ['workflow'])
  assert.equal(result.phase, 'interrupted')
  assert.match(run.failure, /builder ended with error/)
  assert.equal(run.task.phase, 'implement')
  assert.equal(run.fixRounds, 0)
  assert.equal(controller.workers.active.size, 1)
  assert.deepEqual(await fs.readdir(run.cwd), [])
  await assert.rejects(new Controller(host.ctx).decide({ run_id: run.id, revision: run.revision, kind: 'inspect' }, exec), /unknown run/)
  await controller.decide({ run_id: run.id, revision: run.revision, kind: 'continue', instructions: 'retry' }, exec)
  assert.equal(started.mock.calls.length, 1)
  // The retry is a follow-up turn on the same builder; with a 1-token threshold its pre-step injects one reminder from ctx.tokenMeter pressure.
  const builder = controller.workers.builders.get(run.id)
  const texts = builder.child.localAgent.session.deriveMessages().map((m) => m.content.map((c) => c.text ?? '').join(''))
  assert.equal(texts.filter((text) => text.includes('<system_reminder>')).length, 1)
  assert.ok(builder.remindedAt > 0)
  assert.equal((await controller.decide({ run_id: run.id, revision: run.revision, kind: 'abandon' }, exec)).phase, 'abandoned')
  assert.equal(controller.workers.active.size, 0)
})

test('native cancellation stops descendants without custom execution machinery', async (t) => {
  const { run, exec } = await fixture(t)
  const abort = new AbortController()
  const script = "require('fs').writeFileSync('started', 'yes'); setTimeout(() => require('fs').writeFileSync('late', 'bad'), 1000)"
  const pending = runCheck(host.ctx, { ...exec, signal: abort.signal }, run, { ...run.contract.checks[0], command: 'node -e ' + JSON.stringify(script) + '; :' })
  const start = Date.now()
  while (!(await fs.stat(path.join(run.cwd, 'started')).catch(() => null))) {
    assert.ok(Date.now() - start < 5000)
    await delay(10)
  }
  abort.abort()
  assert.equal((await pending).failed, true)
  await delay(1100)
  await assert.rejects(fs.stat(path.join(run.cwd, 'late')), { code: 'ENOENT' })
})

test('prompts come from instructions/*.md; turn messages carry state, not personas', async (t) => {
  const { run } = await fixture(t)
  assert.equal(run.instructions.auditorFindings, await fs.readFile(new URL('./instructions/auditor-findings.md', import.meta.url), 'utf8'))
  assert.match(run.instructions.auditorFindings, /strictly greater than 75; 75 is excluded/)
  assert.doesNotMatch(run.instructions.common + run.instructions.builder + run.instructions.reviewer + run.instructions.tester, /revision|source identity|hash|checkpoint/)
  const text = reminder(run)
  assert.ok(text.startsWith('<system_reminder>\n\n' + run.instructions.common + '\n\n' + run.instructions.builder))
  assert.ok(text.endsWith('</system_reminder>'))
  assert.doesNotMatch(assignment(run), /# Builder instructions|# Shared worker instructions/)
  assert.equal(VERDICT_SCHEMA.properties.findings.items.properties.confidence, undefined)
  for (const role of ['code', 'test']) {
    const packet = auditAssignment(run, { role, ids: [], ignored: [], obligations: ['B1'] })
    assert.match(packet, /# Audit assignment/)
    assert.doesNotMatch(packet, /Auditor finding policy|# Shared worker instructions/)
  }
})

test('registered tool reaches the real builder in a plain directory without Git', async (t) => {
  const { run, exec } = await fixture(t)
  const controller = new Controller(host.ctx)
  const fiber = host.ctx.plugin({ inject: ['tools', 'systemPrompt'], apply: (ctx) => registerTools(ctx, { get: () => DEFAULTS }, controller) })
  await fiber.await()
  try {
    const result = await host.ctx.tools.execute({ ...exec, name: 'build_ticket', arguments: { ticket: 'ticket.md', contract: run.contract } })
    assert.equal(result.isError, false, JSON.stringify(result))
    // No model provider is installed: reaching the child error proves startup passed routing.
    assert.match(result.value.text, /builder ended with error/)
    assert.deepEqual(await fs.readdir(run.cwd), [])
    assert.equal(controller.workers.active.size, 1)
    controller.workers.active.set(exec.agent.session.id, { role: 'builder', run, denied: new Set(['build_ticket']) })
    const checked = await host.ctx.tools.execute({ ...exec, callId: randomUUID(), name: 'build_ticket_check', arguments: { check_id: 'check' } })
    assert.equal(checked.isError, false, JSON.stringify(checked))
    assert.equal(checked.value.exitCode, 0)
    const guarded = await host.ctx.tools.execute({ ...exec, callId: randomUUID(), name: 'build_ticket', arguments: { ticket: 'ticket.md', contract: run.contract } })
    assert.equal(guarded.isError, true)
    assert.match(guarded.error.message, /workers may not use build_ticket/)
    controller.workers.active.delete(exec.agent.session.id)
  } finally { await controller.workers.close(); await fiber.dispose() }
})

// Mock only the plugin's worker-call boundary with node:test; bash/check dispatch remains real.
test('audit findings pause for caller; only approved fixes run and ignored IDs can reopen', async (t) => {
  const { run, exec } = await fixture(t)
  const controller = new Controller(host.ctx)
  controller.runs.set(run.id, run)
  let cycle = 0
  t.mock.method(controller.workers, 'builder', async () => {
    cycle += 1
    if (cycle > 1) assert.deepEqual(run.task.ids, ['C1'])
    return { ...run.handoff, findings: run.task.ids.map((id) => ({ id, status: 'fixed', evidence: 'corrected' })) }
  })
  t.mock.method(controller.workers, 'auditor', async (_run, _parent, a) => {
    if (cycle === 1) return { findings: [{ impact: 'medium', location: 'sample.mjs:1', rule: 'B1', evidence: 'observed gap', correction: 'fix gap' }], prior: [], report: 'initial audit' }
    assert.equal(cycle, 2)
    if (a.role === 'test') {
      assert.deepEqual(a.ignored, ['T1'])
      assert.match(auditAssignment(run, a), /existing test covers this/)
      return { findings: [], prior: [{ id: 'T1', status: 'open', evidence: 'the existing assertion checks a different output; traced required result is unobserved' }], report: 'stronger evidence' }
    }
    return { findings: [], prior: [{ id: 'C1', status: 'resolved', evidence: 'corrected' }], report: 'fixed' }
  })
  const first = await controller.decide({ run_id: run.id, revision: 1, kind: 'continue', instructions: 'implement' }, exec)
  assert.equal(first.phase, 'awaiting_decision')
  assert.equal(cycle, 1)
  const second = await controller.decide({ run_id: run.id, revision: first.revision, kind: 'triage', dispositions: [
    { id: 'C1', action: 'fix', reason: 'required correctness' },
    { id: 'T1', action: 'ignore', reason: 'existing test covers this' },
  ] }, exec)
  assert.equal(second.phase, 'awaiting_decision')
  assert.equal(cycle, 2)
  assert.equal(run.fixRounds, 1)
  assert.equal(run.findings.find((f) => f.id === 'C1').status, 'closed')
  const reopened = run.findings.find((f) => f.id === 'T1')
  assert.equal(reopened.status, 'open')
  assert.equal(reopened.caller.reason, 'existing test covers this')
  assert.match(reopened.verified.evidence, /different output/)
  await assert.rejects(controller.decide({ run_id: run.id, revision: second.revision, kind: 'continue', instructions: 'skip triage' }, exec), /triage/)
})

test('builder dispute pauses without closing findings; ignoring cannot skip current audits', async (t) => {
  const { run, exec } = await fixture(t)
  await runCheck(host.ctx, exec, run, run.contract.checks[0])
  cleanAudits(run)
  run.findings = [{ id: 'C1', role: 'code', status: 'open' }]
  run.phase = 'awaiting_decision'
  run.maxFixRounds = 1
  const controller = new Controller(host.ctx)
  controller.runs.set(run.id, run)
  let calls = 0
  t.mock.method(controller.workers, 'builder', async () => {
    calls += 1
    if (calls === 1) return { outcome: 'needs-decision', summary: 'scope conflict', findings: [{ id: 'C1', status: 'disputed', evidence: 'requires changing public contract' }] }
    return { outcome: 'ready-for-audit', summary: 'completed remaining work', files: [], observations: ['verified'], findings: [] }
  })
  t.mock.method(controller.workers, 'auditor', async () => ({ findings: [], prior: [], report: 'no remaining issue' }))
  const disputed = await controller.decide({ run_id: run.id, revision: 1, kind: 'triage', dispositions: [{ id: 'C1', action: 'fix', reason: 'required' }] }, exec)
  assert.equal(disputed.phase, 'awaiting_decision')
  assert.equal(run.findings[0].status, 'open')
  assert.equal(run.findings[0].builder.status, 'disputed')
  const ignored = await controller.decide({ run_id: run.id, revision: disputed.revision, kind: 'triage', dispositions: [{ id: 'C1', action: 'ignore', reason: 'preserve current contract' }] }, exec)
  assert.equal(ignored.phase, 'awaiting_decision')
  assert.throws(() => assertAcceptable(run), /ready handoff/)
  const resumed = await controller.decide({ run_id: run.id, revision: ignored.revision, kind: 'continue', instructions: 'finish verification without the ignored change' }, exec)
  assert.equal(resumed.phase, 'awaiting_acceptance')
  assert.equal(run.fixRounds, 1)
  assert.equal(calls, 2)
})

test('reopening validates supplied IDs, preserves omitted ignores, and rejects confidence fields', async (t) => {
  const { run } = await fixture(t)
  run.findings = [{ id: 'C1', role: 'code', status: 'ignored', caller: { action: 'ignore', reason: 'out of scope' } }]
  const a = { role: 'code', ids: [], ignored: ['C1'], obligations: ['B1'] }
  const report = { findings: [], prior: [], report: 'reviewed' }
  recordVerdict(run, a, report)
  assert.equal(run.findings[0].status, 'ignored')
  for (const prior of [[{ id: 'unknown', status: 'open', evidence: 'x' }], [{ id: 'C1', status: 'resolved', evidence: 'not assigned' }], [{ id: 'C1', status: 'open', evidence: 'x' }, { id: 'C1', status: 'open', evidence: 'duplicate' }]]) {
    assert.throws(() => recordVerdict(run, a, { ...report, prior }))
    assert.equal(run.findings[0].status, 'ignored')
  }
  assert.throws(() => recordVerdict(run, a, { ...report, findings: [{ impact: 'high', confidence: 90, location: 'x', rule: 'B1', evidence: 'x', correction: 'x' }] }), /confidence/)
})

test('guidance resumes the same fix batch without charging again; new batches still obey the cap', async (t) => {
  const { run, exec } = await fixture(t)
  await runCheck(host.ctx, exec, run, run.contract.checks[0])
  cleanAudits(run)
  run.findings = [{ id: 'C1', role: 'code', status: 'open' }]
  run.phase = 'awaiting_decision'
  run.maxFixRounds = 1
  const controller = new Controller(host.ctx)
  controller.runs.set(run.id, run)
  let calls = 0
  t.mock.method(controller.workers, 'builder', async () => {
    calls += 1
    if (calls === 1) return { outcome: 'needs-decision', summary: 'need guidance', findings: [{ id: 'C1', status: 'disputed', evidence: 'scope question' }] }
    return { outcome: 'ready-for-audit', summary: 'implemented guidance', files: [], observations: ['checked'], findings: [{ id: 'C1', status: 'fixed', evidence: 'implemented' }] }
  })
  t.mock.method(controller.workers, 'auditor', async (_run, _parent, a) => ({ findings: [], prior: a.ids.map((id) => ({ id, status: 'open', evidence: 'required case is still missing' })), report: 'audited' }))
  const decision = { run_id: run.id, revision: 1, kind: 'triage', dispositions: [{ id: 'C1', action: 'fix', reason: 'required case' }] }
  const paused = await controller.decide(decision, exec)
  assert.equal(run.fixRounds, 1)
  const resumed = await controller.decide({ run_id: run.id, revision: paused.revision, kind: 'continue', instructions: 'keep the public API and correct only the implementation' }, exec)
  assert.equal(resumed.phase, 'awaiting_decision')
  assert.equal(run.fixRounds, 1)
  assert.equal(calls, 2)
  await assert.rejects(controller.decide({ ...decision, revision: resumed.revision }, exec), /budget exhausted/)
  await assert.rejects(controller.decide({ run_id: run.id, revision: resumed.revision, kind: 'continue', instructions: 'one more fix' }, exec), /triage/)
  assert.equal(run.fixRounds, 1)
  assert.equal(run.revision, resumed.revision)
})
