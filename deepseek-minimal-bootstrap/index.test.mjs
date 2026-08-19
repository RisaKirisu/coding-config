import assert from 'node:assert/strict'
import { mkdtemp, readFile, rm, writeFile } from 'node:fs/promises'
import { tmpdir } from 'node:os'
import { join } from 'node:path'
import test from 'node:test'

import AnchoredStandardPlugin from './index.mjs'
import {
  buildPromotedContext,
  COMPRESSION_REMINDER,
  createAnchoredFetch,
  createPromotionResolver,
  detectRequestProtocol,
  extractStaticSystemContext,
  INJECTION_METADATA_KEY,
  INJECTION_METADATA_VALUE,
  isNativeRuntimeEnabled,
  loadTargetModelIDs,
  MINIMAL_SYSTEM_PROMPT,
  STRUCTURED_OUTPUT_SYSTEM_PROMPT,
  TARGET_MODEL_ID,
  TARGET_MODEL_IDS,
  transformRequestBody,
} from './internal.mjs'

const MINIMAL_TOOLS = JSON.parse(
  await readFile(new URL('./fixtures/deepseek-harness-minimal-tools.json', import.meta.url), 'utf8'),
)

function tool(name) {
  return {
    type: 'function',
    function: { name, description: `${name} tool`, parameters: { type: 'object' } },
  }
}

function requestBody() {
  return {
    model: TARGET_MODEL_ID,
    messages: [
      { role: 'system', content: 'OpenCode system prompt' },
      { role: 'developer', content: 'OpenCode developer prompt' },
      { role: 'user', content: 'Inspect this repository.' },
    ],
    tools: [tool('bash'), tool('read'), tool('glob'), tool('edit'), tool('str_replace_editor')],
    tool_choice: 'auto',
    stream: true,
  }
}

function anthropicRequestBody() {
  return {
    model: TARGET_MODEL_ID,
    max_tokens: 1024,
    system: 'OpenCode system prompt',
    messages: [{ role: 'user', content: [{ type: 'text', text: 'Inspect this repository.' }] }],
    tools: [
      { name: 'bash', description: 'bash tool', input_schema: { type: 'object' } },
      { name: 'str_replace_editor', description: 'editor tool', input_schema: { type: 'object' } },
    ],
    tool_choice: { type: 'auto' },
    stream: true,
  }
}

test('Anthropic Messages requests keep top-level system and flat tools', () => {
  assert.equal(detectRequestProtocol(anthropicRequestBody()), 'anthropic')
  const transformed = transformRequestBody(anthropicRequestBody(), false, () => {}, 'anthropic')

  assert.equal(transformed.system, MINIMAL_SYSTEM_PROMPT)
  assert.deepEqual(transformed.messages, anthropicRequestBody().messages)
  assert.deepEqual(transformed.tools.map(({ name }) => name), ['bash', 'str_replace_editor'])
  assert.equal(transformed.tools[0].function, undefined)
  assert.deepEqual(transformed.tool_choice, { type: 'auto' })
})

test('Anthropic full catalog removes str_replace_editor without changing tool protocol', () => {
  const transformed = transformRequestBody(anthropicRequestBody(), true, () => {}, 'anthropic')

  assert.deepEqual(transformed.tools.map(({ name }) => name), ['bash'])
  assert.equal(transformed.tools[0].input_schema.type, 'object')
  assert.equal(transformed.tools[0].function, undefined)
})

test('Responses-shaped requests are detected for passthrough', () => {
  assert.equal(detectRequestProtocol({ model: TARGET_MODEL_ID, input: 'Inspect this repository.' }), 'responses')
})

function inProgressAssistant(parts = []) {
  return {
    info: { role: 'assistant', time: { created: 1 } },
    parts,
  }
}

function completedAssistant() {
  return {
    info: { role: 'assistant', time: { created: 1, completed: 2 }, finish: 'stop' },
    parts: [],
  }
}

function injectedUser() {
  return {
    info: { role: 'user', time: { created: 0 } },
    parts: [{
      id: 'prt_injected',
      sessionID: 'session-1',
      messageID: 'msg-0',
      type: 'text',
      text: `${COMPRESSION_REMINDER}\n\nInspect this repository.`,
      synthetic: true,
      metadata: { [INJECTION_METADATA_KEY]: INJECTION_METADATA_VALUE },
    }],
  }
}

const FULL_SYSTEM = [
  'You are opencode, an interactive CLI tool that turns natural language into working code.',
  'You are powered by the model named deepseek-v4-pro. The exact model ID is deepseek/deepseek-v4-pro',
  '<env>',
  '  Working directory: /workspace/project',
  '  Workspace root folder: /workspace',
  '  Is directory a git repo: yes',
  '  Platform: linux',
  '  Today\'s date: Mon Jan 01 2026',
  '</env>',
  'Instructions from: /workspace/AGENTS.md',
  'Be a careful engineer.',
  '<mcp_instructions>',
  '  <server name="files">',
  '    Prefer the files server for file operations.',
  '  </server>',
  '</mcp_instructions>',
  'Skills provide specialized instructions and workflows for specific tasks.',
  '<available_skills>',
  '  <skill>',
  '    <name>web</name>',
  '    <description>Fetch web pages</description>',
  '    <location>/workspace/.opencode/skill/web/SKILL.md</location>',
  '  </skill>',
  '</available_skills>',
].join('\n')

async function setup({ messages = [], provider = {}, options, agent = 'build', modelID = TARGET_MODEL_ID } = {}) {
  let history = messages
  const calls = []
  let historyReads = 0
  const upstreamFetch = async (input, init) => {
    calls.push({ input, init })
    return new Response('{}', { status: 200 })
  }
  provider.options = { ...provider.options, fetch: provider.options?.fetch ?? upstreamFetch }

  const client = {
    session: {
      async messages() {
        historyReads += 1
        if (history instanceof Error) throw history
        return { data: history }
      },
    },
  }
  const hooks = await AnchoredStandardPlugin({
    client,
    directory: '/workspace/project',
    worktree: '/workspace',
    project: { id: 'project-1', name: 'project', vcs: 'git' },
  }, options)
  const config = { provider: { deepseek: provider }, instructions: [] }
  await hooks.config(config)
  const output = { headers: {} }
  await hooks['chat.headers'](
    { sessionID: 'session-1', agent, model: { id: modelID, providerID: 'deepseek' } },
    output,
  )

  return {
    calls,
    config,
    headers: output.headers,
    historyReads: () => historyReads,
    setHistory(value) {
      history = value
    },
    async send(body = requestBody()) {
      await config.provider.deepseek.options.fetch('https://api.deepseek.com/chat/completions', {
        method: 'POST',
        headers: output.headers,
        body: JSON.stringify(body),
      })
      const call = calls.at(-1)
      return {
        body: JSON.parse(call.init.body),
        headers: new Headers(call.init.headers),
      }
    },
    async systemTransform(system, sessionID = 'session-1', modelID = TARGET_MODEL_ID) {
      const output = { system: [system] }
      await hooks['experimental.chat.system.transform'](
        { sessionID, model: { id: modelID } },
        output,
      )
      return output
    },
    async chatMessage({
      sessionID = 'session-1',
      agent = 'build',
      modelID = TARGET_MODEL_ID,
      parts = [{ type: 'text', text: 'Inspect this repository.' }],
      system,
      format,
    } = {}) {
      const output = {
        message: {
          id: 'msg-1',
          sessionID,
          role: 'user',
          agent,
          model: { providerID: 'deepseek', modelID },
          system,
          format,
        },
        parts: parts.map((part) => ({ ...part })),
      }
      await hooks['chat.message'](
        { sessionID, agent, model: { id: modelID, providerID: 'deepseek' } },
        output,
      )
      return output
    },
  }
}

test('request #1 sends the exact DeepSeek Harness Minimal prompt and tool definitions', async () => {
  const harness = await setup({ messages: [inProgressAssistant()] })
  const request = await harness.send()

  assert.deepEqual(request.body.messages, [
    { role: 'system', content: MINIMAL_SYSTEM_PROMPT },
    { role: 'user', content: 'Inspect this repository.' },
  ])
  assert.deepEqual(request.body.tools, MINIMAL_TOOLS)
  assert.equal(request.headers.has('x-dsh-anchored-standard'), false)
  assert.equal(request.headers.has('x-dsh-anchored-session'), false)
})

test('plugin entry module exports only one plugin function', async () => {
  const module = await import('./index.mjs')

  assert.deepEqual(Object.keys(module), ['default'])
  assert.equal(typeof module.default, 'function')
})

test('str_replace_editor is enabled for both DeepSeek V4 target models only', async () => {
  const hooks = await AnchoredStandardPlugin({
    client: { session: { messages: async () => ({ data: [] }) } },
  })
  await hooks.config({ provider: { deepseek: {} } })

  const target = { message: { model: { providerID: 'deepseek', modelID: TARGET_MODEL_ID } }, parts: [] }
  await hooks['chat.message'](
    { sessionID: 'target', model: target.message.model },
    target,
  )
  assert.equal(target.message.tools.str_replace_editor, true)

  const flash = { message: { model: { providerID: 'deepseek', modelID: 'deepseek-v4-flash' } }, parts: [] }
  await hooks['chat.message'](
    { sessionID: 'flash', model: flash.message.model },
    flash,
  )
  assert.equal(flash.message.tools.str_replace_editor, true)

  const other = { message: { model: { providerID: 'deepseek', modelID: 'deepseek-v4-reasoner' } }, parts: [] }
  await hooks['chat.message'](
    { sessionID: 'other', model: other.message.model },
    other,
  )
  assert.equal(other.message.tools.str_replace_editor, false)
})

test('str_replace_editor adapter executes Minimal create, view, replace, and insert calls', async () => {
  const root = await mkdtemp(join(tmpdir(), 'opencode-anchored-standard-'))
  const path = join(root, 'note.txt')
  const hooks = await AnchoredStandardPlugin({
    client: { session: { messages: async () => ({ data: [] }) } },
  })
  const editor = hooks.tool.str_replace_editor
  const permissions = []
  const context = {
    abort: new AbortController().signal,
    directory: root,
    worktree: root,
    metadata() {},
    async ask(request) {
      permissions.push(request.permission)
    },
  }

  try {
    assert.equal(
      await editor.execute({ command: 'create', path, file_text: 'alpha\nbeta\n' }, context),
      `New file created successfully at: ${path}`,
    )
    assert.equal(
      await editor.execute({ command: 'view', path, view_range: [2, 2] }, context),
      `Here's the content of ${path} with line numbers (which has a total of 3 lines) with view_range=[2, 2]:\n     2  beta\n`,
    )
    await editor.execute({ command: 'str_replace', path, old_str: 'beta', new_str: 'BETA' }, context)
    await editor.execute({ command: 'insert', path, insert_line: 1, new_str: 'middle' }, context)

    assert.equal(await readFile(path, 'utf8'), 'alpha\nmiddle\nBETA\n')
    assert.deepEqual(permissions, ['edit', 'read', 'edit', 'edit'])
  } finally {
    await rm(root, { recursive: true, force: true })
  }
})

test('native runtime flag parsing recognizes enabled and disabled values', () => {
  assert.equal(isNativeRuntimeEnabled({ OPENCODE_EXPERIMENTAL_NATIVE_LLM: 'true' }), true)
  assert.equal(isNativeRuntimeEnabled({ OPENCODE_EXPERIMENTAL_NATIVE_LLM: '1' }), true)
  assert.equal(isNativeRuntimeEnabled({ OPENCODE_EXPERIMENTAL_NATIVE_LLM: 'false' }), false)
  assert.equal(isNativeRuntimeEnabled({}), false)
})

test('target models load from a simple JSON config with a models field', async () => {
  const root = await mkdtemp(join(tmpdir(), 'opencode-anchored-standard-config-'))
  const configPath = join(root, 'models.json')
  const previous = process.env.DEEPSEEK_MINIMAL_BOOTSTRAP_CONFIG

  try {
    await writeFile(configPath, JSON.stringify({ models: ['deepseek-v4-flash', 'custom-deepseek'] }))
    process.env.DEEPSEEK_MINIMAL_BOOTSTRAP_CONFIG = configPath
    assert.deepEqual(await loadTargetModelIDs(), ['deepseek-v4-flash', 'custom-deepseek'])

    await rm(configPath)
    process.env.DEEPSEEK_MINIMAL_BOOTSTRAP_CONFIG = configPath
    assert.deepEqual(await loadTargetModelIDs(), [...TARGET_MODEL_IDS])
  } finally {
    if (previous === undefined) delete process.env.DEEPSEEK_MINIMAL_BOOTSTRAP_CONFIG
    else process.env.DEEPSEEK_MINIMAL_BOOTSTRAP_CONFIG = previous
    await rm(root, { recursive: true, force: true })
  }
})

test('phase lookup uses the OpenCode 1.18.18 session messages call shape', async () => {
  let received
  const resolvePromotion = createPromotionResolver(
    {
      session: {
        async messages(options) {
          received = options
          return { data: [] }
        },
      },
    },
    '/workspace/project',
  )

  await resolvePromotion('ses_test')

  assert.deepEqual(received, {
    path: { id: 'ses_test' },
    query: { directory: '/workspace/project' },
  })
})

test('a single durable tool call keeps the bootstrap catalog and leaves user content unchanged', async () => {
  const harness = await setup({
    messages: [inProgressAssistant([{ type: 'tool', tool: 'read', state: { status: 'pending' } }])],
  })
  const request = await harness.send()

  assert.deepEqual(request.body.tools, MINIMAL_TOOLS)
  assert.deepEqual(request.body.messages, [
    { role: 'system', content: MINIMAL_SYSTEM_PROMPT },
    { role: 'user', content: 'Inspect this repository.' },
  ])
  assert.equal(JSON.stringify(request.body).includes(COMPRESSION_REMINDER), false)
})

test('two durable tool calls restore the full catalog', async () => {
  const harness = await setup({
    messages: [inProgressAssistant([
      { type: 'tool', tool: 'read', state: { status: 'pending' } },
      { type: 'tool', tool: 'glob', state: { status: 'pending' } },
    ])],
  })
  const request = await harness.send()

  assert.deepEqual(request.body.tools.map((entry) => entry.function.name), ['bash', 'read', 'glob', 'edit'])
  assert.deepEqual(request.body.messages, [
    { role: 'system', content: MINIMAL_SYSTEM_PROMPT },
    { role: 'user', content: 'Inspect this repository.' },
  ])
})

test('prompt-section injection still happens after the first durable tool call', async () => {
  const harness = await setup({
    messages: [inProgressAssistant([{ type: 'tool', tool: 'read', state: { status: 'pending' } }])],
  })
  await harness.systemTransform(FULL_SYSTEM)
  const original = { type: 'text', text: 'Inspect this repository.' }

  const output = await harness.chatMessage({ parts: [original] })

  assert.equal(output.parts.length, 2)
  assert.ok(output.parts[0].text.endsWith(COMPRESSION_REMINDER))
  assert.deepEqual(output.parts[1], original)
})

test('a completed text-only assistant reply also promotes without rewriting user content', async () => {
  const harness = await setup({
    messages: [completedAssistant()],
  })
  const request = await harness.send()

  assert.equal(request.body.tools.length, 4)
  assert.deepEqual(request.body.messages, [
    { role: 'system', content: MINIMAL_SYSTEM_PROMPT },
    { role: 'user', content: 'Inspect this repository.' },
  ])
})

test('compaction, summary, and title assistant messages never promote bootstrap', async () => {
  for (const mode of ['compaction', 'summary', 'title']) {
    const harness = await setup({
      messages: [{
        info: {
          role: 'assistant',
          mode,
          agent: mode,
          summary: true,
          time: { created: 1, completed: 2 },
          finish: 'stop',
        },
        parts: [],
      }],
    })
    const request = await harness.send()

    assert.deepEqual(
      request.body.tools,
      MINIMAL_TOOLS,
      `${mode} assistant output must not unlock the full catalog`,
    )
  }
})

test('promoted multipart user content is forwarded unchanged', async () => {
  const harness = await setup({
    messages: [completedAssistant()],
  })
  const body = requestBody()
  body.messages.at(-1).content = [
    { type: 'text', text: 'Inspect this image.' },
    { type: 'image_url', image_url: { url: 'data:image/png;base64,abc' } },
  ]

  const request = await harness.send(body)

  assert.equal(request.body.messages[0].content, MINIMAL_SYSTEM_PROMPT)
  assert.deepEqual(request.body.messages[1].content, [
    { type: 'text', text: 'Inspect this image.' },
    { type: 'image_url', image_url: { url: 'data:image/png;base64,abc' } },
  ])
})

test('an API retry before a durable reply remains in bootstrap', async () => {
  const harness = await setup({ messages: [inProgressAssistant()] })

  const first = await harness.send()
  const retry = await harness.send()

  assert.deepEqual(first.body.tools, MINIMAL_TOOLS)
  assert.deepEqual(retry.body.tools, MINIMAL_TOOLS)
  assert.equal(harness.historyReads(), 2)
})

test('positive catalog restoration is memoized after two durable tool calls', async () => {
  const harness = await setup({
    messages: [inProgressAssistant([
      { type: 'tool', tool: 'bash', state: { status: 'error' } },
      { type: 'tool', tool: 'read', state: { status: 'pending' } },
    ])],
  })

  await harness.send()
  await harness.send()

  assert.equal(harness.historyReads(), 1)
})

test('DeepSeek V4 Flash uses the Minimal bootstrap and restores the catalog like V4 Pro', async () => {
  const harness = await setup({ modelID: 'deepseek-v4-flash', messages: [] })
  const body = requestBody()
  body.model = 'deepseek-v4-flash'

  const bootstrap = await harness.send(body)
  assert.equal(bootstrap.body.messages[0].content, MINIMAL_SYSTEM_PROMPT)
  assert.deepEqual(bootstrap.body.tools, MINIMAL_TOOLS)

  harness.setHistory([
    inProgressAssistant([
      { type: 'tool', tool: 'read', state: { status: 'pending' } },
      { type: 'tool', tool: 'glob', state: { status: 'pending' } },
    ]),
  ])
  const restored = await harness.send(body)
  assert.deepEqual(restored.body.tools.map((entry) => entry.function.name), ['bash', 'read', 'glob', 'edit'])
})

test('DeepSeek V4 Flash receives the same persisted prompt-section injection', async () => {
  const harness = await setup({
    modelID: 'deepseek-v4-flash',
    messages: [completedAssistant()],
  })
  await harness.systemTransform(FULL_SYSTEM, 'session-1', 'deepseek-v4-flash')
  const original = { type: 'text', text: 'Inspect this repository.' }

  const output = await harness.chatMessage({ modelID: 'deepseek-v4-flash', parts: [original] })

  assert.equal(output.parts.length, 2)
  assert.ok(output.parts[0].text.includes('<available_skills>'))
  assert.ok(output.parts[0].text.endsWith(COMPRESSION_REMINDER))
  assert.deepEqual(output.parts[1], original)
})

test('non-target models are forwarded byte-for-byte', async () => {
  const harness = await setup({ modelID: 'deepseek-v4-reasoner' })
  const body = requestBody()
  body.model = 'deepseek-v4-reasoner'
  const original = JSON.stringify(body)

  await harness.config.provider.deepseek.options.fetch('https://api.deepseek.com/chat/completions', {
    method: 'POST',
    headers: harness.headers,
    body: original,
  })

  assert.equal(harness.calls.at(-1).init.body, original)
  assert.equal(harness.historyReads(), 0)
})

test('OpenCode utility agents are not modified', async () => {
  const harness = await setup({ agent: 'title' })
  const original = JSON.stringify(requestBody())

  await harness.config.provider.deepseek.options.fetch('https://api.deepseek.com/chat/completions', {
    method: 'POST',
    headers: harness.headers,
    body: original,
  })

  assert.equal(harness.calls.at(-1).init.body, original)
  assert.equal(harness.historyReads(), 0)

  const originalPart = { type: 'text', text: 'Summarize this repository.' }
  const message = await harness.chatMessage({ agent: 'title', parts: [originalPart] })

  assert.equal(message.message.tools, undefined)
  assert.deepEqual(message.parts, [originalPart])
})

test('subagent sessions receive their own bootstrap phase', async () => {
  const harness = await setup({ agent: 'general', messages: [inProgressAssistant()] })
  const request = await harness.send()

  assert.deepEqual(request.body.tools, MINIMAL_TOOLS)
})

test('missing bootstrap tools fail open to the original catalog', async () => {
  const harness = await setup({ messages: [inProgressAssistant()] })
  const body = requestBody()
  body.tools = [tool('bash'), tool('edit')]
  const request = await harness.send(body)

  assert.deepEqual(request.body.tools.map((entry) => entry.function.name), ['bash', 'edit'])
  assert.equal(request.body.messages[0].content, MINIMAL_SYSTEM_PROMPT)
})

test('history failures fail open to full tools and stay promoted', async () => {
  const harness = await setup({ messages: new Error('storage unavailable') })

  const first = await harness.send()
  const second = await harness.send()

  assert.equal(first.body.tools.length, 4)
  assert.equal(second.body.tools.length, 4)
  assert.equal(harness.historyReads(), 1)
})

test('configured providers declaring the target model are wrapped', async () => {
  let forwarded
  const upstreamFetch = async (_input, init) => {
    forwarded = init
    return new Response('{}', { status: 200 })
  }
  const hooks = await AnchoredStandardPlugin({
    client: { session: { messages: async () => ({ data: [inProgressAssistant()] }) } },
  })
  const config = {
    provider: {
      gateway: {
        models: { [TARGET_MODEL_ID]: {} },
        options: { fetch: upstreamFetch },
      },
    },
  }
  await hooks.config(config)
  const output = { headers: {} }
  await hooks['chat.headers'](
    { sessionID: 'custom-session', agent: 'build', model: { id: TARGET_MODEL_ID, providerID: 'gateway' } },
    output,
  )
  await config.provider.gateway.options.fetch('https://gateway.example/chat/completions', {
    headers: output.headers,
    body: JSON.stringify(requestBody()),
  })

  assert.deepEqual(JSON.parse(forwarded.body).tools, MINIMAL_TOOLS)
  assert.ok(config.provider.deepseek)
})

test('Anthropic providers select the Anthropic request transform from provider metadata', async () => {
  let forwarded
  const upstreamFetch = async (input, init) => {
    forwarded = { input, init }
    return new Response('{}', { status: 200 })
  }
  const hooks = await AnchoredStandardPlugin({
    client: { session: { messages: async () => ({ data: [] }) } },
  })
  const config = {
    provider: {
      'krill-china': {
        npm: '@ai-sdk/anthropic',
        models: { [TARGET_MODEL_ID]: {} },
        options: { fetch: upstreamFetch },
      },
    },
  }
  await hooks.config(config)
  const output = { headers: {} }
  await hooks['chat.headers'](
    { sessionID: 'custom-session', agent: 'build', model: { id: TARGET_MODEL_ID, providerID: 'krill-china' } },
    output,
  )
  await config.provider['krill-china'].options.fetch('https://krill.example/messages', {
    headers: output.headers,
    body: JSON.stringify(anthropicRequestBody()),
  })

  const body = JSON.parse(forwarded.init.body)
  assert.equal(forwarded.input, 'https://krill.example/messages')
  assert.equal(body.system, MINIMAL_SYSTEM_PROMPT)
  assert.deepEqual(body.tools.map(({ name }) => name), ['bash', 'str_replace_editor'])
  assert.equal(body.tools[0].function, undefined)
})

test('a forced removed tool choice resets to auto during bootstrap', async () => {
  const harness = await setup({ messages: [inProgressAssistant()] })
  const body = requestBody()
  body.tool_choice = { type: 'function', function: { name: 'edit' } }
  const request = await harness.send(body)

  assert.equal(request.body.tool_choice, 'auto')
})

test('target-model requests through unwrapped providers receive no private headers', async () => {
  const hooks = await AnchoredStandardPlugin({
    client: { session: { messages: async () => ({ data: [] }) } },
  })
  await hooks.config({ provider: {} })
  const output = { headers: {} }

  await hooks['chat.headers'](
    { sessionID: 'session-1', agent: 'build', model: { id: TARGET_MODEL_ID, providerID: 'other' } },
    output,
  )

  assert.deepEqual(output.headers, {})
})

test('Request-object inputs are transformed without leaking private headers', async () => {
  let forwarded
  const fetch = createAnchoredFetch(
    async (input, init) => {
      forwarded = { input, init }
      return new Response('{}', { status: 200 })
    },
    async () => false,
  )
  const request = new Request('https://api.deepseek.com/chat/completions', {
    method: 'POST',
    headers: {
      'content-type': 'application/json',
      'x-dsh-anchored-standard': '1',
      'x-dsh-anchored-session': 'request-session',
    },
    body: JSON.stringify(requestBody()),
  })

  await fetch(request)

  assert.ok(forwarded.input instanceof Request)
  assert.equal(forwarded.init, undefined)
  assert.equal(forwarded.input.headers.has('x-dsh-anchored-standard'), false)
  assert.equal(forwarded.input.headers.has('x-dsh-anchored-session'), false)
  const body = await forwarded.input.json()
  assert.deepEqual(body.tools, MINIMAL_TOOLS)
})

test('static system context drops the provider prompt and per-message additions', () => {
  const system = [
    'You are opencode, an interactive CLI tool.',
    FULL_SYSTEM,
    STRUCTURED_OUTPUT_SYSTEM_PROMPT,
    'Be strict about tests.',
  ].join('\n')
  const context = extractStaticSystemContext(system, {
    system: 'Be strict about tests.',
    structuredOutput: true,
  })

  assert.equal(context.includes('You are opencode'), false)
  assert.ok(context.startsWith('You are powered by the model named'))
  assert.ok(context.includes('<env>'))
  assert.ok(context.includes('<mcp_instructions>'))
  assert.ok(context.includes('<available_skills>'))
  assert.equal(context.includes(STRUCTURED_OUTPUT_SYSTEM_PROMPT), false)
  assert.equal(context.endsWith('Be strict about tests.'), false)
})

test('promoted context keeps OpenCode section order and puts the reminder last', () => {
  const text = buildPromotedContext({
    base: FULL_SYSTEM,
    message: {
      system: 'Be strict about tests.',
      format: { type: 'json_schema', schema: {} },
    },
  })

  const env = text.indexOf('<env>')
  const structured = text.indexOf(STRUCTURED_OUTPUT_SYSTEM_PROMPT)
  const userSystem = text.indexOf('Be strict about tests.')
  const reminder = text.indexOf(COMPRESSION_REMINDER)

  assert.ok(env !== -1 && structured !== -1 && userSystem !== -1 && reminder !== -1)
  assert.ok(env < structured, 'environment must precede the structured-output prompt')
  assert.ok(structured < userSystem, 'structured-output prompt must precede user.system')
  assert.ok(userSystem < reminder, 'user.system must precede the compression reminder')
  assert.ok(text.endsWith(COMPRESSION_REMINDER))
})

test('chat.message persists captured context and reminder once on the first promoted user message', async () => {
  const harness = await setup({ messages: [completedAssistant()] })
  await harness.systemTransform(FULL_SYSTEM)
  const original = { type: 'text', text: 'Inspect this repository.' }

  const output = await harness.chatMessage({ parts: [original] })

  assert.equal(output.parts.length, 2)
  const injected = output.parts[0]
  assert.equal(injected.type, 'text')
  assert.equal(injected.synthetic, true)
  assert.equal(injected.messageID, 'msg-1')
  assert.equal(injected.sessionID, 'session-1')
  assert.ok(injected.id.startsWith('prt_'))
  assert.deepEqual(injected.metadata, { [INJECTION_METADATA_KEY]: INJECTION_METADATA_VALUE })
  assert.equal(injected.text.includes('You are opencode'), false)
  assert.ok(injected.text.includes('<env>'))
  assert.ok(injected.text.includes('Instructions from: /workspace/AGENTS.md'))
  assert.ok(injected.text.includes('<mcp_instructions>'))
  assert.ok(injected.text.includes('<available_skills>'))
  assert.ok(injected.text.endsWith(COMPRESSION_REMINDER))
  assert.deepEqual(output.parts[1], original)
})

test('bootstrap user messages are never annotated with context or reminder', async () => {
  const harness = await setup({ messages: [] })
  await harness.systemTransform(FULL_SYSTEM)
  const original = { type: 'text', text: 'Inspect this repository.' }

  const output = await harness.chatMessage({ parts: [original] })

  assert.deepEqual(output.parts, [original])
})

test('context injection is idempotent for sessions that already carry it', async () => {
  const harness = await setup({ messages: [injectedUser(), completedAssistant()] })
  const original = { type: 'text', text: 'Continue the task.' }

  const output = await harness.chatMessage({ parts: [original] })

  assert.deepEqual(output.parts, [original])
  assert.equal(harness.historyReads(), 1)
})

test('chat.message falls back to environment and instructions when no system was observed', async () => {
  const harness = await setup({ messages: [completedAssistant()] })
  const original = { type: 'text', text: 'Inspect this repository.' }

  const output = await harness.chatMessage({ parts: [original], system: 'Be strict about tests.' })

  assert.equal(output.parts.length, 2)
  const injected = output.parts[0]
  assert.ok(injected.text.includes('You are powered by the model named'))
  assert.ok(injected.text.includes('<env>'))
  assert.ok(injected.text.includes('Working directory: /workspace/project'))
  assert.ok(injected.text.includes('Be strict about tests.'))
  assert.ok(injected.text.endsWith(COMPRESSION_REMINDER))
  assert.equal(injected.text.includes('<mcp_instructions>'), false)
  assert.equal(injected.text.includes('<available_skills>'), false)
  assert.deepEqual(output.parts[1], original)
})
