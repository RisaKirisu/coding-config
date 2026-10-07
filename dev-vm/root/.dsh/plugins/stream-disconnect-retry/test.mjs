import assert from 'node:assert/strict'
import { createRequire } from 'node:module'
import test from 'node:test'
import * as plugin from './index.mjs'

const require = createRequire('/usr/local/lib/node_modules/@deepseek-ai/dsh/package.json')
const { Context } = await import(require.resolve('@deepseek-ai/cordis'))
const { LlmRuntime, LlmAdapter, resolveRetryPolicy, createUserMessage } = await import(require.resolve('@deepseek-ai/dsh-llm'))
const { AgentRegistry } = await import(require.resolve('@deepseek-ai/dsh-agent'))
const { SessionStore } = await import(require.resolve('@deepseek-ai/dsh-session'))
const { SessionProjectionRegistry } = await import(require.resolve('@deepseek-ai/dsh-session-projection'))
const { SystemPrompt } = await import(require.resolve('@deepseek-ai/dsh-system-prompt'))
const { ToolRuntime } = await import(require.resolve('@deepseek-ai/dsh-tools'))
const { AgentLoop } = await import(require.resolve('@deepseek-ai/dsh-agent-loop'))
const retryPlugin = await import(require.resolve('@deepseek-ai/dsh-llm-retry'))

const disconnectedMessage = 'stream error: stream disconnected before completion: stream closed before response.completed'

function errorChunk(message, code = 'PI_AI_ERROR', kind = 'error') {
  return { type: 'finish', reason: { kind, failure: { message, code } } }
}

test('only disconnected PI_AI_ERROR finishes become TRANSPORT; chunks remain immutable', async (t) => {
  const ctx = new Context()
  t.after(() => ctx.fiber.dispose())
  await ctx.plugin(LlmRuntime)
  const fork = ctx.plugin(plugin)
  await fork

  const chunks = [
    { type: 'text-delta', index: 0, text: 'partial output' },
    errorChunk(disconnectedMessage),
    errorChunk(`provider prefix: ${disconnectedMessage}; extra details`),
    errorChunk('Response incomplete: content_filter'),
    errorChunk('No response body'),
    errorChunk(disconnectedMessage, 'AUTH'),
    errorChunk(disconnectedMessage, 'ABORTED', 'aborted'),
    { type: 'finish', reason: { kind: 'stop' } },
  ]
  t.mock.method(ctx.llm, 'adapterStream', async function* () {
    yield* chunks
  })
  const options = { provider: 'test', model: 'test', messages: [] }
  const actual = await Array.fromAsync(ctx.llm.stream(options))
  for (const [index, chunk] of chunks.entries()) {
    if (index === 1 || index === 2) {
      assert.deepEqual(actual[index], {
        ...chunk,
        reason: { ...chunk.reason, failure: { ...chunk.reason.failure, code: 'TRANSPORT' } },
      })
      assert.equal(chunk.reason.failure.code, 'PI_AI_ERROR')
    } else {
      assert.equal(actual[index], chunk)
    }
  }
  await fork.dispose()
  assert.deepEqual(await Array.fromAsync(ctx.llm.stream(options)), chunks)
})

test('the real agent loop retries disconnects up to its provider budget, not other PI_AI_ERRORs', async (t) => {
  const ctx = new Context()
  t.after(() => ctx.fiber.dispose())
  for (const service of [LlmRuntime, AgentRegistry, SessionStore, SessionProjectionRegistry, SystemPrompt, ToolRuntime]) {
    await ctx.plugin(service)
  }
  await ctx.plugin(AgentLoop)
  await ctx.plugin(retryPlugin)
  await ctx.plugin(plugin)

  const adapter = new LlmAdapter()
  t.mock.method(adapter, 'providerRetryPolicy', () => resolveRetryPolicy({
    mode: 'normal',
    maxRetries: 2,
    backoff: { initialDelayMs: 1, maxDelayMs: 1, jitterRatio: 0 },
  }, 'test'))
  let failureMessage = disconnectedMessage
  const stream = adapter.stream = t.mock.fn(async function* () {
    yield errorChunk(failureMessage)
  })
  ctx.llm.registerAdapter(['test'], adapter)

  const agent = await ctx.agentLoop.create('disconnect-retry-test', { provider: 'test', model: 'test' })
  agent.followup(createUserMessage({ content: [{ type: 'text', text: 'test disconnect recovery' }], source: { kind: 'user' } }))
  await agent.whenIdle()
  assert.equal(stream.mock.callCount(), 3, JSON.stringify(agent.session.snapshotEvents().findLast((event) => event.type === 'turn/end').data))
  const retries = agent.session.snapshotEvents().filter((event) => event.type === 'llm/retry')
  assert.equal(retries.length, 2)
  assert.deepEqual(retries.map((event) => event.data.failure.code), ['TRANSPORT', 'TRANSPORT'])
  assert.equal(agent.session.snapshotEvents().findLast((event) => event.type === 'turn/end').data.reason.kind, 'error')

  failureMessage = 'Response incomplete: content_filter'
  stream.mock.resetCalls()
  agent.followup(createUserMessage({ content: [{ type: 'text', text: 'test permanent failure' }], source: { kind: 'user' } }))
  await agent.whenIdle()
  assert.equal(stream.mock.callCount(), 1)
  assert.equal(agent.session.snapshotEvents().filter((event) => event.type === 'llm/retry').length, 2)
})
