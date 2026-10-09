import assert from 'node:assert/strict'
import { createRequire } from 'node:module'
import test from 'node:test'
import * as plugin from './index.mjs'
import { isRollable, nextFallback, reclassifyFailure, validateConfig } from './rules.mjs'

const require = createRequire('/usr/local/lib/node_modules/@deepseek-ai/dsh/package.json')
const { Context, Service } = await import(require.resolve('@deepseek-ai/cordis'))
const { LlmRuntime, LlmAdapter, resolveRetryPolicy, createUserMessage } = await import(require.resolve('@deepseek-ai/dsh-llm'))
const { AgentRegistry } = await import(require.resolve('@deepseek-ai/dsh-agent'))
const { SessionStore } = await import(require.resolve('@deepseek-ai/dsh-session'))
const { SessionProjectionRegistry } = await import(require.resolve('@deepseek-ai/dsh-session-projection'))
const { SystemPrompt } = await import(require.resolve('@deepseek-ai/dsh-system-prompt'))
const { ToolRuntime } = await import(require.resolve('@deepseek-ai/dsh-tools'))
const { AgentLoop } = await import(require.resolve('@deepseek-ai/dsh-agent-loop'))
const retryPlugin = await import(require.resolve('@deepseek-ai/dsh-llm-retry'))

const disconnectMessage = 'stream error: stream disconnected before completion: stream closed before response.completed'
const retryPolicy = { mode: 'normal', maxRetries: 2, backoff: { initialDelayMs: 1, maxDelayMs: 1, jitterRatio: 0 } }

function errorChunk(message, code = 'PI_AI_ERROR', kind = 'error') {
  return { type: 'finish', reason: { kind, failure: { message, code } } }
}

function stopChunk(text) {
  return [
    { type: 'text-delta', index: 0, text },
    { type: 'finish', reason: { kind: 'stop' } },
  ]
}

/**
 * Settings and web-server stand-ins. DSH owns the real settings write chain
 * (config editor, profile patch); these tests observe the section this plugin
 * asks it to persist.
 */
class TestSettings extends Service {
  constructor(ctx) {
    super(ctx, 'settings')
    this.updates = []
  }
  configure() {
    return () => {}
  }
  async update(ns, patch) {
    this.updates.push({ ns, patch })
  }
}

class TestWebServer extends Service {
  constructor(ctx) {
    super(ctx, 'webServer')
  }
  register() {
    return () => {}
  }
}

/** Mount the real agent stack plus this plugin over one two-route adapter. */
async function mount(t, { adapter, config, providers = ['primary', 'fallback'] }) {
  const ctx = new Context()
  t.after(() => ctx.fiber.dispose())
  for (const service of [LlmRuntime, AgentRegistry, SessionStore, SessionProjectionRegistry, SystemPrompt, ToolRuntime]) {
    await ctx.plugin(service)
  }
  await ctx.plugin(AgentLoop)
  await ctx.plugin(retryPlugin)
  await ctx.plugin(TestSettings)
  await ctx.plugin(TestWebServer)
  ctx.llm.registerAdapter(providers, adapter)
  const fork = ctx.plugin(plugin, config)
  await fork
  return { ctx, fork }
}

test('a rule selects matching failures and rewrites only a different code', () => {
  const rules = [{ code: 'PI_AI_ERROR', messageContains: 'stream disconnected', treatAs: 'TRANSPORT' }]
  assert.deepEqual(reclassifyFailure(rules, { code: 'PI_AI_ERROR', message: 'x: stream disconnected before completion' }), {
    code: 'TRANSPORT',
    message: 'x: stream disconnected before completion',
  })
  assert.equal(reclassifyFailure(rules, { code: 'PI_AI_ERROR', message: 'Response incomplete' }), undefined)
  assert.equal(reclassifyFailure(rules, { code: 'AUTH', message: 'stream disconnected' }), undefined)
  assert.equal(reclassifyFailure([{ code: 'AUTH', messageContains: '', treatAs: 'AUTH' }], { code: 'AUTH', message: 'nope' }), undefined)
  assert.equal(reclassifyFailure([], { code: 'PI_AI_ERROR', message: 'stream disconnected' }), undefined)
})

test('a failure rolls when its policy retries it, when a rule selects it, or under unlimited retries', () => {
  const resolved = resolveRetryPolicy(retryPolicy, 'primary')
  assert.equal(isRollable(resolved, [], { code: 'TRANSPORT' }), true)
  assert.equal(isRollable(resolved, [], { code: 'AUTH' }), false)
  assert.equal(isRollable(resolved, [{ code: 'AUTH', messageContains: '', treatAs: '' }], { code: 'AUTH' }), true)
  assert.equal(isRollable(undefined, [], { code: 'TRANSPORT' }), false)
  assert.equal(isRollable({ mode: 'always' }, [], { code: 'AUTH' }), true)
})

test('the next candidate follows the failed route and stops at the end', () => {
  const fallbacks = [
    { provider: 'primary', model: 'a' },
    { provider: 'fallback', model: 'b' },
    { provider: 'fallback', model: 'c' },
  ]
  assert.deepEqual(nextFallback(fallbacks, { provider: 'primary', model: 'a' }), fallbacks[1])
  assert.deepEqual(nextFallback(fallbacks, { provider: 'fallback', model: 'b' }), fallbacks[2])
  assert.equal(nextFallback(fallbacks, { provider: 'fallback', model: 'c' }), undefined)
  assert.deepEqual(nextFallback(fallbacks, { provider: 'other', model: 'z' }), fallbacks[0])
  assert.equal(nextFallback([], { provider: 'primary', model: 'a' }), undefined)
})

test('a submitted section is validated at the boundary', () => {
  assert.deepEqual(
    validateConfig({
      fallbacks: [{ provider: 'fallback', model: 'b', reasoningEffort: 'high' }],
      retriableErrors: [{ code: 'AUTH', messageContains: '', treatAs: '' }],
    }),
    {
      fallbacks: [{ provider: 'fallback', model: 'b', reasoningEffort: 'high' }],
      retriableErrors: [{ code: 'AUTH', messageContains: '', treatAs: 'TRANSPORT' }],
    },
  )
  assert.throws(() => validateConfig({ fallbacks: [{ provider: '', model: 'b' }], retriableErrors: [] }), /fallbacks\[0\] requires a provider/)
  assert.throws(() => validateConfig({ fallbacks: [{ provider: 'x', model: '' }], retriableErrors: [] }), /fallbacks\[0\] requires a model/)
  assert.throws(() => validateConfig({ fallbacks: [], retriableErrors: [{ code: '' }] }), /retriableErrors\[0\] requires a code/)
  assert.throws(() => validateConfig({ retriableErrors: [] }), /fallbacks must be an array/)
})

test('the stream hook reclassifies only the configured disconnect wording', async (t) => {
  const ctx = new Context()
  t.after(() => ctx.fiber.dispose())
  await ctx.plugin(LlmRuntime)
  await ctx.plugin(AgentRegistry)
  await ctx.plugin(TestSettings)
  await ctx.plugin(TestWebServer)
  await ctx.plugin(plugin, {})
  const chunks = [
    { type: 'text-delta', index: 0, text: 'partial output' },
    errorChunk(disconnectMessage),
    errorChunk(disconnectMessage, 'AUTH'),
    errorChunk(disconnectMessage, 'PI_AI_ERROR', 'aborted'),
    errorChunk('Response incomplete: content_filter'),
    { type: 'finish', reason: { kind: 'stop' } },
  ]
  t.mock.method(ctx.llm, 'adapterStream', async function* () {
    yield* chunks
  })
  const actual = await Array.fromAsync(ctx.llm.stream({ provider: 'primary', model: 'a', messages: [] }))
  assert.equal(actual[1].reason.failure.code, 'TRANSPORT')
  for (const index of [0, 2, 3, 4, 5]) assert.equal(actual[index], chunks[index])
  assert.equal(chunks[1].reason.failure.code, 'PI_AI_ERROR')
})

test('the agent loop spends the provider budget, then continues the turn on the next model', async (t) => {
  const attempts = []
  const adapter = new LlmAdapter()
  t.mock.method(adapter, 'providerRetryPolicy', (provider) => resolveRetryPolicy(retryPolicy, provider))
  adapter.stream = async function* (options) {
    attempts.push(`${options.provider}/${options.model}`)
    if (options.provider === 'primary') {
      yield errorChunk(disconnectMessage)
      return
    }
    yield* stopChunk('answered by the fallback')
  }

  const { ctx } = await mount(t, {
    adapter,
    config: { fallbacks: [{ provider: 'fallback', model: 'model-b', reasoningEffort: '' }] },
  })
  const agent = await ctx.agentLoop.create('model-fallback-test', { provider: 'primary', model: 'model-a' })
  agent.followup(createUserMessage({ content: [{ type: 'text', text: 'continue' }], source: { kind: 'user' } }))
  await agent.whenIdle()

  assert.deepEqual(attempts, [
    'primary/model-a',
    'primary/model-a',
    'primary/model-a',
    'fallback/model-b',
  ])
  const events = agent.session.snapshotEvents()
  assert.deepEqual(
    events.filter((event) => event.type === 'llm/retry').map((event) => event.data.failure.code),
    ['TRANSPORT', 'TRANSPORT'],
  )
  const headers = events.filter((event) => event.type === 'request/header')
  assert.deepEqual(
    headers.map((event) => `${event.data.header.config.provider}/${event.data.header.config.model}`),
    ['primary/model-a', 'fallback/model-b'],
  )
  assert.equal(events.findLast((event) => event.type === 'turn/end').data.reason.kind, 'completed')
})

test('an exhausted fallback list ends the turn instead of rolling forever', async (t) => {
  const attempts = []
  const adapter = new LlmAdapter()
  t.mock.method(adapter, 'providerRetryPolicy', (provider) => resolveRetryPolicy(retryPolicy, provider))
  adapter.stream = async function* (options) {
    attempts.push(`${options.provider}/${options.model}`)
    yield errorChunk(disconnectMessage)
  }

  const { ctx } = await mount(t, {
    adapter,
    providers: ['primary', 'second', 'third'],
    config: {
      fallbacks: [
        { provider: 'second', model: 'model-b', reasoningEffort: '' },
        { provider: 'third', model: 'model-c', reasoningEffort: '' },
      ],
    },
  })
  const agent = await ctx.agentLoop.create('model-fallback-exhausted', { provider: 'primary', model: 'model-a' })
  agent.followup(createUserMessage({ content: [{ type: 'text', text: 'continue' }], source: { kind: 'user' } }))
  await agent.whenIdle()

  // One attempt plus two retries on each of the three routes, and no further roll.
  assert.deepEqual(attempts, [
    'primary/model-a', 'primary/model-a', 'primary/model-a',
    'second/model-b', 'second/model-b', 'second/model-b',
    'third/model-c', 'third/model-c', 'third/model-c',
  ])
  const events = agent.session.snapshotEvents()
  assert.deepEqual(
    events.filter((event) => event.type === 'request/header').map((event) => event.data.header.config.model),
    ['model-a', 'model-b', 'model-c'],
  )
  assert.equal(events.findLast((event) => event.type === 'turn/end').data.reason.kind, 'error')
})

test('a failure no policy or rule retries ends the turn on the selected model', async (t) => {
  const attempts = []
  const adapter = new LlmAdapter()
  t.mock.method(adapter, 'providerRetryPolicy', (provider) => resolveRetryPolicy(retryPolicy, provider))
  adapter.stream = async function* (options) {
    attempts.push(`${options.provider}/${options.model}`)
    yield errorChunk('invalid request body', 'INVALID_REQUEST')
  }

  const { ctx } = await mount(t, {
    adapter,
    config: { fallbacks: [{ provider: 'fallback', model: 'model-b', reasoningEffort: '' }] },
  })
  const agent = await ctx.agentLoop.create('model-fallback-permanent', { provider: 'primary', model: 'model-a' })
  agent.followup(createUserMessage({ content: [{ type: 'text', text: 'continue' }], source: { kind: 'user' } }))
  await agent.whenIdle()

  assert.deepEqual(attempts, ['primary/model-a'])
  assert.equal(agent.session.snapshotEvents().findLast((event) => event.type === 'turn/end').data.reason.kind, 'error')
})

test('the settings routes read, validate, and persist the section over real HTTP', async (t) => {
  const { WebServer } = await import(require.resolve('@deepseek-ai/dsh-host-webserver'))
  const ctx = new Context()
  t.after(() => ctx.fiber.dispose())
  for (const service of [LlmRuntime, AgentRegistry, SessionStore, SessionProjectionRegistry, SystemPrompt, ToolRuntime]) {
    await ctx.plugin(service)
  }
  await ctx.plugin(WebServer, { host: '127.0.0.1', port: 0 })
  await ctx.plugin(TestSettings)

  const adapter = new LlmAdapter()
  t.mock.method(adapter, 'listModels', async () => [{ provider: 'primary', id: 'model-a', name: 'Model A' }])
  adapter.stream = async function* () {
    yield* stopChunk('unused')
  }
  ctx.llm.registerAdapter(['primary', 'fallback'], adapter)

  const configured = {
    fallbacks: [{ provider: 'fallback', model: 'model-b', reasoningEffort: 'high' }],
    retriableErrors: [{ code: 'PI_AI_ERROR', messageContains: 'stream disconnected', treatAs: 'TRANSPORT' }],
  }
  await ctx.plugin(plugin, configured)

  const base = `http://127.0.0.1:${ctx.webServer.port}`
  const read = await fetch(`${base}/api/model-fallback/config`)
  assert.equal(read.status, 200)
  assert.deepEqual(await read.json(), configured)

  const directory = await (await fetch(`${base}/api/model-fallback/models`)).json()
  assert.deepEqual(directory.providers.map((provider) => provider.id).sort(), ['fallback', 'primary'])
  assert.deepEqual(directory.modelsByProvider.primary, [{ id: 'model-a', name: 'Model A' }])

  const saved = await fetch(`${base}/api/model-fallback/config`, {
    method: 'POST',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify({ fallbacks: [{ provider: 'fallback', model: 'model-c' }], retriableErrors: [] }),
  })
  assert.equal(saved.status, 200)
  assert.deepEqual(ctx.settings.updates, [{
    ns: 'model-fallback',
    patch: {
      fallbacks: [{ provider: 'fallback', model: 'model-c', reasoningEffort: '' }],
      retriableErrors: [],
    },
  }])

  const rejected = await fetch(`${base}/api/model-fallback/config`, {
    method: 'POST',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify({ fallbacks: [{ provider: '', model: 'model-c' }], retriableErrors: [] }),
  })
  assert.equal(rejected.status, 400)
  assert.match((await rejected.json()).error, /fallbacks\[0\] requires a provider/)
  assert.equal(ctx.settings.updates.length, 1)

  assert.equal((await fetch(`${base}/api/model-fallback/config`, { method: 'DELETE' })).status, 405)
  assert.equal((await fetch(`${base}/api/model-fallback/models`, { method: 'POST' })).status, 405)
})

/**
 * Collect every string in an element tree built by the stand-in above,
 * including the value and placeholder a control renders from its props.
 */
function visibleText(node) {
  if (typeof node === 'string' || typeof node === 'number') return String(node)
  if (Array.isArray(node)) return node.map(visibleText).join(' ')
  if (node === null || node === undefined || typeof node !== 'object') return ''
  const { value, placeholder } = node.props
  return [
    typeof value === 'string' ? value : '',
    typeof placeholder === 'string' ? placeholder : '',
    visibleText(node.children),
  ].join(' ')
}

test('the client bundle registers a settings page that renders both sections', async (t) => {
  let definition
  globalThis.window = { __ModuleLoader__: { load: (value) => { definition = value } } }
  t.after(() => { delete globalThis.window })
  await import('./client.js')
  assert.equal(definition.id, '@devvm/dsh-model-fallback')

  // React is a browser seed word the host supplies; this stand-in runs the real
  // component so the page's own render paths execute without a DOM.
  let stateOverride
  function createElement(type, props, ...children) {
    const node = { type, props: props || {}, children: children.flat(Infinity).filter((child) => child !== null && child !== undefined && child !== false) }
    return typeof type === 'function' ? type(node.props) : node
  }
  const react = {
    createElement,
    Fragment: 'Fragment',
    // Only the object-shaped initial state is the page state; the other hook is a counter.
    useState: (initial) => [stateOverride && typeof initial === 'object' && initial !== null ? stateOverride : initial, () => {}],
    useEffect: () => {},
  }

  let contribution
  const ctx = {
    effect: () => {},
    slots: {
      inject: (name, callback) => { assert.equal(name, 'settings.section'); callback() },
      register: (options, component) => { contribution = { options, component }; return () => {} },
    },
    locale: { getLocale: () => ({ active: 'en' }) },
  }
  const client = definition.factory((specifier) => {
    assert.equal(specifier, 'react')
    return react
  })
  assert.deepEqual(client.inject, ['slots', 'locale'])
  client.apply(ctx)

  assert.equal(contribution.options.name, 'settings.section')
  assert.equal(contribution.options.id, 'model-fallback')
  assert.equal(contribution.options.label(), 'Model Fallback')
  ctx.locale.getLocale = () => ({ active: 'zh-CN' })
  assert.equal(contribution.options.label(), '模型回退')

  ctx.locale.getLocale = () => ({ active: 'en' })
  assert.match(visibleText(contribution.component({ close() {} })), /Loading models/)

  stateOverride = {
    loading: false,
    saving: false,
    providers: [{ id: 'primary', name: 'Primary' }, { id: 'fallback', name: 'Fallback' }],
    modelsByProvider: { fallback: [{ id: 'model-b', name: 'Model B' }] },
    reasoningByModel: { 'fallback/model-b': ['low', 'high'] },
    fallbacks: [
      { provider: 'fallback', model: 'model-b', reasoningEffort: 'high' },
      { provider: 'primary', model: 'model-a', reasoningEffort: '' },
    ],
    retriableErrors: [{ code: 'PI_AI_ERROR', messageContains: 'stream disconnected', treatAs: 'TRANSPORT' }],
    notice: '',
    error: '',
  }
  const rendered = visibleText(contribution.component({ close() {} }))
  assert.match(rendered, /Fallback models/)
  assert.match(rendered, /Retriable errors/)
  for (const heading of ['Provider', 'Reasoning effort', 'Error code', 'Message contains', 'Treat as']) {
    assert.match(rendered, new RegExp(heading))
  }
  assert.match(rendered, /Model B \(model-b\)/)
  assert.match(rendered, /PI_AI_ERROR/)
  assert.match(rendered, /stream disconnected/)
  assert.match(rendered, /TRANSPORT/)
  assert.match(rendered, /Save settings/)

  // No rows means no columns to name.
  stateOverride = { ...stateOverride, fallbacks: [], retriableErrors: [] }
  const emptied = visibleText(contribution.component({ close() {} }))
  assert.match(emptied, /No fallback models configured/)
  assert.match(emptied, /No retriable errors configured/)
  assert.doesNotMatch(emptied, /Reasoning effort/)
  assert.doesNotMatch(emptied, /Message contains/)
})
