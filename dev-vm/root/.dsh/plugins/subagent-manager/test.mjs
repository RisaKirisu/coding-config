import test from 'node:test'
import assert from 'node:assert/strict'
import { isSubagent, normalizeConfig } from './index.mjs'

test('normalizes persisted config', () => {
  assert.deepEqual(normalizeConfig({ provider: 'proxy-cli', model: 'gpt-5.6-terra', reasoningEffort: 'high' }), {
    provider: 'proxy-cli',
    model: 'gpt-5.6-terra',
    reasoningEffort: 'high',
  })
  assert.deepEqual(normalizeConfig(null), { provider: '', model: '', reasoningEffort: '' })
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
