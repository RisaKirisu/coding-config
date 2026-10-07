import test from 'node:test'
import assert from 'node:assert/strict'
import { createRequire } from 'node:module'
import { mkdtemp, rm } from 'node:fs/promises'
import { tmpdir } from 'node:os'
import { join } from 'node:path'
import { randomUUID } from 'node:crypto'
import Jobs from '@deepseek-ai/dsh-jobs-local'
import { runtime } from './test-runtime.mjs'
import { Controller } from './controller.mjs'
import { registerTools } from './index.mjs'
import { DEFAULTS } from './config.mjs'
import { loadInstructions } from './prompts.mjs'

assert.equal(createRequire(process.env.DSH_PACKAGE_ENTRY)('@deepseek-ai/dsh/package.json').version, '0.2.0-rc.2')

test('Build Loop background report uses rc.2 job result and owner access', async () => {
  const host = await runtime()
  const jobsFiber = host.ctx.plugin(Jobs)
  await jobsFiber.await()
  const detach = host.ctx.jobs.attachController('upgrade-test')
  const dir = await mkdtemp(join(tmpdir(), 'dsh-build-jobs-'))
  const parent = await host.ctx.agents.create({ sessionId: randomUUID(), meta: { cwd: dir } })
  const controller = new Controller(host.ctx)
  const toolFiber = host.ctx.plugin({ inject: ['tools', 'systemPrompt'], apply: ctx => registerTools(ctx, { get: () => structuredClone(DEFAULTS) }, controller) })
  await toolFiber.await()
  try {
    const response = await host.ctx.tools.execute({ agent: parent.agent, callId: randomUUID(), signal: AbortSignal.timeout(5000), name: 'build_ticket', arguments: {
      ticket: 'ticket.md', run_in_background: true,
      contract: { instruction: 'Return a build report', scope: ['sample.mjs'], behaviors: [{ id: 'B1', observation: 'sample prints verified', check: 'check' }], checks: [{ id: 'check', command: 'printf verified', scope: ['sample.mjs'], expectedExit: 0 }] },
    } })
    assert.equal(response.isError, false, JSON.stringify(response))
    const id = response.value.jobId
    const owner = parent.agent.session.id
    const settled = await host.ctx.jobs.wait(id, 5000, owner)
    assert.equal(settled.owner, owner)
    assert.equal(settled.status, 'completed')
    assert.throws(() => host.ctx.jobs.get(id, randomUUID()))
    assert.match(host.ctx.jobs.read(id, owner).result, /builder ended with error/)
    assert.equal(host.ctx.jobs.read(id, owner).result, undefined)
  } finally {
    await controller.workers.close()
    await toolFiber.dispose()
    await parent.dispose()
    detach()
    await jobsFiber.dispose()
    await host.close()
    await rm(dir, { recursive: true, force: true })
  }
})

test('worker collection cancels only child-owned shell work and consumes results once', async () => {
  const host = await runtime()
  const jobsFiber = host.ctx.plugin(Jobs)
  await jobsFiber.await()
  const detach = host.ctx.jobs.attachController('upgrade-test')
  const dir = await mkdtemp(join(tmpdir(), 'dsh-worker-jobs-'))
  const parent = await host.ctx.agents.create({ sessionId: randomUUID(), meta: { cwd: dir } })
  const controller = new Controller(host.ctx)
  try {
    const run = { id: randomUUID(), ticket: 'ticket.md', policy: structuredClone(DEFAULTS), instructions: loadInstructions() }
    const worker = await controller.workers.start(run, parent.agent, 'builder', 'Inspect the ticket', AbortSignal.timeout(5000))
    await worker.child.result
    const owner = worker.child.localAgent.session.id
    const completed = host.ctx.jobs.start({ kind: 'subagent', label: 'child report', owner,
      run: () => ({ cancel() {}, done: Promise.resolve({ status: 'completed', result: 'child report' }) }) })
    await host.ctx.jobs.wait(completed, 5000, owner)
    const shell = await host.ctx.shell.execute(host.ctx.shell.resolve({ command: 'sleep 30', workdir: dir }))
    const active = host.ctx.jobs.start({ kind: 'shell', label: 'child shell', owner,
      run: () => ({ cancel: reason => shell.kill(reason), done: shell.result().then(() => ({ status: 'killed' })) }) })
    const foreign = host.ctx.jobs.start({ kind: 'subagent', label: 'parent report', owner: parent.agent.session.id,
      run: () => ({ cancel() {}, done: Promise.resolve({ status: 'completed', result: 'parent report' }) }) })
    await host.ctx.jobs.wait(foreign, 5000, parent.agent.session.id)
    await controller.workers.collect(worker)
    assert.equal(host.ctx.jobs.get(active, owner).status, 'killed')
    assert.equal(host.ctx.jobs.read(completed, owner).result, undefined, 'collection already consumed the child report')
    assert.equal(host.ctx.jobs.read(foreign, parent.agent.session.id).result, 'parent report')
    await controller.workers.collect(worker)
  } finally {
    await controller.workers.close()
    await parent.dispose()
    detach()
    await jobsFiber.dispose()
    await host.close()
    await rm(dir, { recursive: true, force: true })
  }
})
