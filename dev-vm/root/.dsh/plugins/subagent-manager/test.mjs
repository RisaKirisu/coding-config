import test from 'node:test'
import assert from 'node:assert/strict'
import { isSubagent, normalizeConfig } from './helpers.mjs'
import { waitForIdle } from './index.mjs'

test('wait returns completed when the subagent becomes idle', async () => {
  let finish
  const idle = new Promise(resolve => { finish = resolve })
  const waiting = waitForIdle(idle, 1)
  finish()
  assert.equal(await waiting, 'completed')
})

test('timeout uses seconds and returns running without cancelling the idle promise', async (t) => {
  t.mock.timers.enable({ apis: ['setTimeout'] })
  let finish
  const idle = new Promise(resolve => { finish = resolve })
  let status
  const waiting = waitForIdle(idle, 2).then(value => { status = value; return value })
  t.mock.timers.tick(1999)
  await Promise.resolve()
  assert.equal(status, undefined)
  t.mock.timers.tick(1)
  assert.equal(await waiting, 'running')
  finish()
  assert.equal(await waitForIdle(idle, 2), 'completed')
})

test('omitting timeout waits 300 seconds', async (t) => {
  t.mock.timers.enable({ apis: ['setTimeout'] })
  let status
  const waiting = waitForIdle(new Promise(() => {})).then(value => { status = value; return value })
  t.mock.timers.tick(299999)
  await Promise.resolve()
  assert.equal(status, undefined)
  t.mock.timers.tick(1)
  assert.equal(await waiting, 'running')
})

test('invalid timeouts and real idle failures remain errors', async () => {
  for (const timeout of [0, -1, NaN, Infinity, null, '1', 2147484]) {
    await assert.rejects(waitForIdle(Promise.resolve(), timeout), /timeout must be a positive number of seconds/)
  }
  await assert.rejects(waitForIdle(Promise.reject(new Error('idle failed')), 1), /idle failed/)
})

test('normalizes persisted config', () => {
  assert.deepEqual(normalizeConfig({ provider: 'proxy-cli', model: 'gpt-5.6-terra', reasoningEffort: 'high' }), {
    provider: 'proxy-cli',
    model: 'gpt-5.6-terra',
    reasoningEffort: 'high',
  })
  assert.deepEqual(normalizeConfig(null), { provider: '', model: '', reasoningEffort: '' })
})

test('trims fields except exact provider model identifiers', () => {
  assert.deepEqual(
    normalizeConfig({
      provider: '  proxy-cli  ',
      model: ' gemini-3.8-flash-high',
      reasoningEffort: '  high  ',
    }),
    {
      provider: 'proxy-cli',
      model: ' gemini-3.8-flash-high',
      reasoningEffort: 'high',
    },
  )
  assert.equal(normalizeConfig({ model: '   ' }).model, '')
})

test('detects subagents without classifying ordinary forks as subagents', () => {
  assert.equal(isSubagent({ session: { header: { origin: 'subagent' } } }), true)
  assert.equal(isSubagent({ session: { header: { origin: 'user' } } }), false)
  assert.equal(isSubagent({
    session: {
      header: {
        parentSession: 'session-parent',
        seedLength: 14800,
        delegationDepth: 0,
      },
    },
  }), false)
})
