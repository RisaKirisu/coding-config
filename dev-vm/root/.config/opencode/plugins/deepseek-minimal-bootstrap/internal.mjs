import {
  glob as globFiles,
  readFile,
  stat,
} from 'node:fs/promises'
import { homedir } from 'node:os'
import { dirname, isAbsolute, join, resolve } from 'node:path'
import { fileURLToPath } from 'node:url'

import {
  createStrReplaceEditorTool,
  MINIMAL_BASH_DESCRIPTION,
  MINIMAL_TOOL_DEFINITIONS,
} from './minimal-tools.mjs'

export const TARGET_MODEL_ID = 'deepseek-v4-pro'
export const TARGET_MODEL_IDS = Object.freeze(['deepseek-v4-pro', 'deepseek-v4-flash'])
export const CONFIG_FILENAME = 'deepseek-minimal-bootstrap.json'
export const CATALOG_TOOL_CALL_THRESHOLD = 2
export const MINIMAL_SYSTEM_PROMPT = 'You are a helpful software engineer assistant.'
export const COMPRESSION_REMINDER = [
  'IMPORTANT Compression reminder: when doing compression, never compress messages from the beginning of the conversation up to and including this message.',
  'Every message from the first message through this message must remain fully intact.',
].join(' ')

export const STRUCTURED_OUTPUT_SYSTEM_PROMPT = 'IMPORTANT: The user has requested structured output. You MUST use the StructuredOutput tool to provide your final response. Do NOT respond with plain text - you MUST call the StructuredOutput tool with your answer formatted according to the schema.'

export const INJECTION_METADATA_KEY = 'dsh-anchored-standard'
export const INJECTION_METADATA_VALUE = 'promoted-context-v1'

const MARKER_HEADER = 'x-dsh-anchored-standard'
const SESSION_HEADER = 'x-dsh-anchored-session'
const ENVIRONMENT_MARKER = 'You are powered by the model named'
const BOOTSTRAP_TOOLS = new Set(['bash', 'str_replace_editor'])
const RESTORED_TOOL_EXCLUSIONS = new Set(['str_replace_editor', 'read', 'glob', 'grep', 'plan_exit'])
const UTILITY_AGENTS = new Set(['compaction', 'summary', 'title'])
const WRAPPED_FETCH = Symbol('anchored-standard-fetch')

function errorMessage(error) {
  return error instanceof Error ? error.message : String(error)
}

function modelID(model) {
  return model?.id ?? model?.modelID
}

function partID() {
  const value = globalThis.crypto?.randomUUID?.()
    ?? `${Date.now()}-${Math.random().toString(16).slice(2)}`
  return `prt_${value.replaceAll('-', '')}`
}

export function isNativeRuntimeEnabled(env = globalThis.process?.env ?? {}) {
  return ['1', 'true', 'yes', 'on'].includes(String(env.OPENCODE_EXPERIMENTAL_NATIVE_LLM ?? '').toLowerCase())
}

function toolName(tool) {
  return tool?.function?.name ?? tool?.name
}

function toolChoiceName(choice) {
  if (!choice || typeof choice !== 'object') return undefined
  return choice.function?.name ?? choice.name
}

function messagesFromResponse(response) {
  if (Array.isArray(response)) return response
  if (response?.error) throw new Error(`OpenCode session history failed: ${JSON.stringify(response.error)}`)
  if (Array.isArray(response?.data)) return response.data
  throw new TypeError('OpenCode session history response did not contain a message array')
}

function isAssistantMessage(message) {
  return (message?.info ?? message)?.role === 'assistant'
}

function isUtilityAssistant(message) {
  if (!isAssistantMessage(message)) return false

  // OpenCode 1.18.18 persists compaction (and possibly summary/title)
  // assistant messages inside the session. They are utility output, not a
  // real model reply, so they must never count toward promotion.
  const info = message?.info ?? message
  return UTILITY_AGENTS.has(info?.mode) || UTILITY_AGENTS.has(info?.agent) || info?.summary === true
}

export function countDurableToolCalls(messages) {
  let count = 0
  for (const message of messages) {
    if (!isAssistantMessage(message) || isUtilityAssistant(message)) continue
    const parts = Array.isArray(message?.parts) ? message.parts : []
    for (const part of parts) {
      if (part?.type === 'tool') count += 1
    }
  }
  return count
}

export function hasCompletedAssistantReply(messages) {
  return messages.some((message) => {
    if (!isAssistantMessage(message) || isUtilityAssistant(message)) return false
    const info = message?.info ?? message
    return info?.time?.completed !== undefined || info?.finish !== undefined
  })
}

/**
 * Prompt-section injection keeps its original trigger: the first durable tool
 * call or a completed assistant reply promotes the session for context
 * persistence.
 */
export function hasPromotionSignal(messages) {
  return countDurableToolCalls(messages) > 0 || hasCompletedAssistantReply(messages)
}

/**
 * The full OpenCode tool catalog unlocks only after two durable tool calls,
 * or when the assistant has completed a reply (which is what a second user
 * message follows). A single tool call alone no longer unlocks the catalog.
 */
export function hasCatalogRestoreSignal(messages) {
  return countDurableToolCalls(messages) >= CATALOG_TOOL_CALL_THRESHOLD
    || hasCompletedAssistantReply(messages)
}

export function isTargetModel(model, targetModelIDs = TARGET_MODEL_IDS) {
  return targetModelIDs.includes(modelID(model))
}

export function hasInjectedContext(messages) {
  return messages.some((message) =>
    (Array.isArray(message?.parts) ? message.parts : []).some(
      (part) => part?.type === 'text' && part?.metadata?.[INJECTION_METADATA_KEY] === INJECTION_METADATA_VALUE,
    ),
  )
}

/**
 * Extract the OpenCode operational system context from an assembled system
 * string. OpenCode 1.18.18 builds system[0] as:
 *
 *   providerPrompt + "\n" + environment + instructions + MCP + skills
 *     + structuredOutput + user.system
 *
 * The Minimal experiment keeps the provider prompt omitted, so everything
 * from the stable environment marker onward is the context we preserve. The
 * per-message structured-output prompt and user.system are stripped when the
 * matching user message context is known so only the stable base is stored.
 */
export function extractStaticSystemContext(system, userContext = {}) {
  if (typeof system !== 'string' || system.length === 0) return ''
  const start = system.indexOf(ENVIRONMENT_MARKER)
  if (start === -1) return ''

  let text = system.slice(start)
  if (typeof userContext.system === 'string' && userContext.system.length > 0) {
    const withoutTrailingNewlines = text.replace(/\n+$/, '')
    if (withoutTrailingNewlines.endsWith(userContext.system)) {
      text = withoutTrailingNewlines.slice(0, withoutTrailingNewlines.length - userContext.system.length)
    }
  }
  if (userContext.structuredOutput) {
    const withoutTrailingNewlines = text.replace(/\n+$/, '')
    if (withoutTrailingNewlines.endsWith(STRUCTURED_OUTPUT_SYSTEM_PROMPT)) {
      text = withoutTrailingNewlines.slice(0, withoutTrailingNewlines.length - STRUCTURED_OUTPUT_SYSTEM_PROMPT.length)
    }
  }
  return text.replace(/\n+$/, '')
}

/**
 * Build the single persisted context part for the first promoted user message.
 * Section order mirrors OpenCode's system assembly as closely as possible:
 * environment -> instructions -> MCP -> skills -> structured output ->
 * user.system -> compression reminder.
 */
export function buildPromotedContext({ base = '', message = {}, reminder = COMPRESSION_REMINDER }) {
  const sections = []
  const baseText = typeof base === 'string' ? base.trim() : ''
  if (baseText) sections.push(baseText)

  if (message?.format?.type === 'json_schema' && !baseText.includes(STRUCTURED_OUTPUT_SYSTEM_PROMPT)) {
    sections.push(STRUCTURED_OUTPUT_SYSTEM_PROMPT)
  }

  const system = typeof message?.system === 'string' ? message.system.trim() : ''
  if (system && !baseText.endsWith(system)) sections.push(system)

  sections.push(reminder)
  return sections.join('\n\n')
}

async function readTextFile(path) {
  try {
    const content = await readFile(path, 'utf8')
    return content.length > 0 ? content : ''
  } catch {
    return ''
  }
}

async function isFile(path) {
  try {
    return (await stat(path)).isFile()
  } catch {
    return false
  }
}

async function findUpFile(name, start, stop) {
  let current = resolve(start)
  const root = resolve(stop)

  while (true) {
    const candidate = join(current, name)
    if (await isFile(candidate)) return candidate
    if (current === root) return undefined

    const parent = dirname(current)
    if (parent === current || (!parent.startsWith(root) && parent !== root)) return undefined
    current = parent
  }
}

function globalConfigDirectory() {
  if (process.env.OPENCODE_CONFIG_DIR) return process.env.OPENCODE_CONFIG_DIR
  const configHome = process.env.XDG_CONFIG_HOME ?? join(homedir(), '.config')
  return join(configHome, 'opencode')
}

function normalizeTargetModelIDs(value) {
  if (!Array.isArray(value)) return undefined
  const ids = [...new Set(value.map((id) => typeof id === 'string' ? id.trim() : ''))]
    .filter((id) => id.length > 0)
  return ids.length > 0 ? ids : undefined
}

function targetModelConfigCandidates() {
  const pluginDirectory = dirname(fileURLToPath(import.meta.url))
  return [
    process.env.DEEPSEEK_MINIMAL_BOOTSTRAP_CONFIG,
    join(pluginDirectory, '..', CONFIG_FILENAME),
    join(pluginDirectory, CONFIG_FILENAME),
    join(process.cwd(), CONFIG_FILENAME),
    join(globalConfigDirectory(), CONFIG_FILENAME),
  ].filter((path) => typeof path === 'string' && path.length > 0)
}

/**
 * Load the target model IDs from deepseek-minimal-bootstrap.json. The file
 * only needs a `models` array; when it is absent or invalid, both DeepSeek
 * V4 Pro and DeepSeek V4 Flash are active.
 */
export async function loadTargetModelIDs(warn = () => {}) {
  for (const path of targetModelConfigCandidates()) {
    const content = await readTextFile(path)
    if (!content) continue

    try {
      const ids = normalizeTargetModelIDs(JSON.parse(content).models)
      if (ids) return ids
      warn('invalid-target-models', `${path}: "models" must be a non-empty array of non-empty strings; using defaults`)
      return [...TARGET_MODEL_IDS]
    } catch (error) {
      warn('invalid-config', `${path} could not be parsed: ${errorMessage(error)}; using default target models`)
      return [...TARGET_MODEL_IDS]
    }
  }

  return [...TARGET_MODEL_IDS]
}

function instructionEntry(path, content) {
  return `Instructions from: ${path}\n${content}`
}

async function fetchInstruction(url) {
  try {
    const response = await fetch(url, { signal: AbortSignal.timeout(5000) })
    if (!response.ok) return ''
    const content = await response.text()
    return content ? instructionEntry(url, content) : ''
  } catch {
    return ''
  }
}

async function instructionFiles(raw, directory) {
  const home = homedir()
  const expanded = raw.startsWith('~/') ? join(home, raw.slice(2)) : raw
  const pattern = isAbsolute(expanded) ? expanded : resolve(directory, expanded)

  try {
    const matches = await globFiles(pattern)
    const files = []
    for (const match of matches) {
      const path = isAbsolute(match) ? match : resolve(directory, match)
      if (await isFile(path)) files.push(path)
    }
    return files
  } catch {
    return []
  }
}

function fallbackEnvironmentBlock({ directory, worktree, project, providerID, modelID }) {
  return [
    `You are powered by the model named ${modelID}. The exact model ID is ${providerID}/${modelID}`,
    'Here is some useful information about the environment you are running in:',
    '<env>',
    `  Working directory: ${directory}`,
    `  Workspace root folder: ${worktree}`,
    `  Is directory a git repo: ${project?.vcs === 'git' ? 'yes' : 'no'}`,
    `  Platform: ${process.platform}`,
    `  Today's date: ${new Date().toDateString()}`,
    '</env>',
  ].join('\n')
}

/**
 * Best-effort context for the rare case where a session was already promoted
 * before this plugin process observed one of its system prompts. OpenCode's
 * exact MCP instructions and skill catalog are internal and cannot be read
 * back from the plugin API, so those sections are omitted here.
 */
export async function buildFallbackContext({
  directory,
  worktree = directory,
  project,
  providerID = 'deepseek',
  modelID = TARGET_MODEL_ID,
  instructions = [],
}) {
  const sections = [
    fallbackEnvironmentBlock({ directory, worktree, project, providerID, modelID }),
  ]

  const globalFiles = [
    join(globalConfigDirectory(), 'AGENTS.md'),
    join(homedir(), '.claude', 'CLAUDE.md'),
  ]
  for (const file of globalFiles) {
    if (!(await isFile(file))) continue
    const content = await readTextFile(file)
    if (content) sections.push(instructionEntry(file, content))
    break
  }

  for (const name of ['AGENTS.md', 'CLAUDE.md', 'CONTEXT.md']) {
    const file = await findUpFile(name, directory, worktree)
    if (!file) continue
    const content = await readTextFile(file)
    if (content) sections.push(instructionEntry(file, content))
    break
  }

  for (const raw of instructions) {
    if (typeof raw !== 'string' || raw.length === 0) continue
    if (/^https?:\/\//i.test(raw)) {
      const entry = await fetchInstruction(raw)
      if (entry) sections.push(entry)
      continue
    }

    for (const file of await instructionFiles(raw, directory)) {
      const content = await readTextFile(file)
      if (content) sections.push(instructionEntry(file, content))
    }
  }

  return sections.join('\n\n')
}

const MINIMAL_ANTHROPIC_TOOL_DEFINITIONS = MINIMAL_TOOL_DEFINITIONS.map(({ function: definition }) => ({
  name: definition.name,
  description: definition.description,
  input_schema: definition.parameters,
}))

function providerProtocol(providerID, provider) {
  const npm = typeof provider?.npm === 'string' ? provider.npm : ''
  if (npm === '@ai-sdk/anthropic' || npm.startsWith('@ai-sdk/anthropic@')) return 'anthropic'
  if (
    npm === '@ai-sdk/openai'
    || npm.startsWith('@ai-sdk/openai@')
    || npm === '@ai-sdk/openai-compatible'
    || npm.startsWith('@ai-sdk/openai-compatible@')
  ) return 'openai'
  if (providerID === 'deepseek') return 'openai'
  return undefined
}

function toAnthropicTool(tool) {
  if (tool?.input_schema !== undefined) return tool
  const definition = tool?.function
  if (!definition?.name) return tool
  return {
    name: definition.name,
    description: definition.description,
    input_schema: definition.parameters ?? { type: 'object' },
  }
}

export function detectRequestProtocol(body, hintedProtocol) {
  if (hintedProtocol === 'anthropic' || hintedProtocol === 'openai' || hintedProtocol === 'responses') {
    return hintedProtocol
  }
  if (!body || typeof body !== 'object' || Array.isArray(body)) return undefined
  if (body.input !== undefined || body.instructions !== undefined) return 'responses'
  if (
    body.system !== undefined
    || (Array.isArray(body.tools) && body.tools.some((tool) => tool?.input_schema !== undefined))
    || ['any', 'none', 'tool'].includes(body.tool_choice?.type)
    || (Array.isArray(body.messages) && body.messages.some((message) =>
      Array.isArray(message?.content)
      && message.content.some((part) => part?.type === 'tool_use' || part?.type === 'tool_result'),
    ))
  ) return 'anthropic'
  if (
    Array.isArray(body.messages)
    && (
      (Array.isArray(body.tools) && body.tools.some((tool) => tool?.function !== undefined))
      || typeof body.tool_choice === 'string'
      || body.tool_choice?.function !== undefined
    )
  ) return 'openai'
  return undefined
}

function transformAnthropicRequestBody(body, fullCatalog, warn) {
  const conversation = body.messages.filter(
    message => message?.role !== 'system' && message?.role !== 'developer',
  )
  const transformed = {
    ...body,
    system: MINIMAL_SYSTEM_PROMPT,
    messages: conversation,
  }

  if (fullCatalog) {
    if (Array.isArray(body.tools)) {
      transformed.tools = body.tools
        .filter(tool => !RESTORED_TOOL_EXCLUSIONS.has(toolName(tool)))
        .map((tool) => {
          const normalized = toAnthropicTool(tool)
          return toolName(tool) === 'bash'
            ? { ...normalized, description: MINIMAL_BASH_DESCRIPTION }
            : normalized
        })
      if (toolChoiceName(body.tool_choice) && RESTORED_TOOL_EXCLUSIONS.has(toolChoiceName(body.tool_choice))) {
        transformed.tool_choice = { type: 'auto' }
      }
    }
    return transformed
  }
  if (!Array.isArray(body.tools)) {
    warn('missing-tools', 'bootstrap request had no Anthropic-compatible tools array; exposing the original catalog')
    return transformed
  }

  const available = new Set(body.tools.map(toolName))
  const missing = [...BOOTSTRAP_TOOLS].filter((name) => !available.has(name))
  if (missing.length > 0) {
    warn('missing-bootstrap-tools', `bootstrap disabled because required tools are missing: ${missing.join(', ')}`)
    return transformed
  }

  transformed.tools = structuredClone(MINIMAL_ANTHROPIC_TOOL_DEFINITIONS)
  if (toolChoiceName(body.tool_choice) && !BOOTSTRAP_TOOLS.has(toolChoiceName(body.tool_choice))) {
    transformed.tool_choice = { type: 'auto' }
  }
  return transformed
}

export function transformRequestBody(body, fullCatalog, warn = () => {}, protocol = 'openai') {
  if (!body || typeof body !== 'object' || Array.isArray(body)) {
    throw new TypeError('request body must be a JSON object')
  }
  if (!Array.isArray(body.messages)) {
    throw new TypeError('request body must contain a messages array')
  }

  if (protocol === 'anthropic') return transformAnthropicRequestBody(body, fullCatalog, warn)
  if (protocol !== 'openai') return body

  const conversation = body.messages.filter(
    message => message?.role !== 'system' && message?.role !== 'developer',
  )
  const transformed = {
    ...body,
    messages: [{ role: 'system', content: MINIMAL_SYSTEM_PROMPT }, ...conversation],
  }

  if (fullCatalog) {
    if (Array.isArray(body.tools)) {
      transformed.tools = body.tools
        .filter(tool => !RESTORED_TOOL_EXCLUSIONS.has(toolName(tool)))
        .map(tool => toolName(tool) === 'bash'
          ? { ...tool, function: { ...tool.function, description: MINIMAL_BASH_DESCRIPTION } }
          : tool)
      if (toolChoiceName(body.tool_choice) && RESTORED_TOOL_EXCLUSIONS.has(toolChoiceName(body.tool_choice))) transformed.tool_choice = 'auto'
    }
    return transformed
  }
  if (!Array.isArray(body.tools)) {
    warn('missing-tools', 'bootstrap request had no OpenAI-compatible tools array; exposing the original catalog')
    return transformed
  }

  const available = new Set(body.tools.map(toolName))
  const missing = [...BOOTSTRAP_TOOLS].filter((name) => !available.has(name))
  if (missing.length > 0) {
    warn('missing-bootstrap-tools', `bootstrap disabled because required tools are missing: ${missing.join(', ')}`)
    return transformed
  }

  transformed.tools = structuredClone(MINIMAL_TOOL_DEFINITIONS)
  const choice = toolChoiceName(body.tool_choice)
  if (choice && !BOOTSTRAP_TOOLS.has(choice)) transformed.tool_choice = 'auto'
  return transformed
}

export function createSignalResolver(client, directory, hasSignal, warn = () => {}) {
  const resolved = new Set()

  return async (sessionID) => {
    if (resolved.has(sessionID)) return true

    try {
      const response = await client.session.messages({
        path: { id: sessionID },
        ...(directory ? { query: { directory } } : {}),
      })
      if (!hasSignal(messagesFromResponse(response))) return false
    } catch (error) {
      warn(
        'history-read-failed',
        `could not derive session phase; exposing the full catalog: ${errorMessage(error)}`,
      )
    }

    resolved.add(sessionID)
    return true
  }
}

export function createPromotionResolver(client, directory, warn = () => {}) {
  return createSignalResolver(client, directory, hasPromotionSignal, warn)
}

export function createCatalogRestoreResolver(client, directory, warn = () => {}) {
  return createSignalResolver(client, directory, hasCatalogRestoreSignal, warn)
}

/**
 * Chat-message-time inspector used for the persisted context injection. The
 * catalog-restore signal is memoized separately by createSignalResolver; this
 * reader only runs until both promotion and injection state are known.
 * Unlike the fetch path, a history failure here fails closed (no injection)
 * so a storage blip cannot mutate a bootstrap user message.
 */
export function createSessionInspector(client, directory, warn = () => {}) {
  const promoted = new Set()
  const injected = new Set()

  const readHistory = async (sessionID) => {
    try {
      const response = await client.session.messages({
        path: { id: sessionID },
        ...(directory ? { query: { directory } } : {}),
      })
      return { messages: messagesFromResponse(response), error: false }
    } catch (error) {
      warn(
        'injection-history-read-failed',
        `could not read session history before persisting context; leaving the user message unchanged: ${errorMessage(error)}`,
      )
      return { messages: [], error: true }
    }
  }

  const inspect = async (sessionID) => {
    if (promoted.has(sessionID) && injected.has(sessionID)) {
      return { promoted: true, injected: true, error: false }
    }

    const { messages, error } = await readHistory(sessionID)
    if (error) return { promoted: false, injected: false, error: true }

    const isPromoted = hasPromotionSignal(messages)
    if (isPromoted) promoted.add(sessionID)

    const isInjected = hasInjectedContext(messages)
    if (isInjected) injected.add(sessionID)

    return { promoted: isPromoted, injected: isInjected, error: false }
  }

  const markInjected = (sessionID) => {
    promoted.add(sessionID)
    injected.add(sessionID)
  }

  return { inspect, markInjected }
}

export function createAnchoredFetch(nextFetch, isCatalogRestored, warn = () => {}, protocolHint) {
  const anchoredFetch = async (input, init = {}) => {
    const request = input instanceof Request ? input : undefined
    const headers = new Headers(init.headers ?? request?.headers)
    const marked = headers.get(MARKER_HEADER) === '1'
    const sessionID = headers.get(SESSION_HEADER)
      ?? headers.get('x-session-id')
      ?? headers.get('x-opencode-session')

    if (!marked) return nextFetch(input, init)

    headers.delete(MARKER_HEADER)
    headers.delete(SESSION_HEADER)
    const bodyText = typeof init.body === 'string'
      ? init.body
      : request
        ? await request.clone().text()
        : undefined
    const forward = (body = init.body) => {
      if (request) return nextFetch(new Request(request, { ...init, headers, body }))
      return nextFetch(input, { ...init, headers, body })
    }

    if (typeof bodyText !== 'string') {
      warn('unsupported-body', 'marked request did not have a string JSON body; forwarding it unchanged')
      return forward()
    }

    let body
    try {
      body = JSON.parse(bodyText)
    } catch (error) {
      warn('invalid-json', `marked request body was not valid JSON; forwarding it unchanged: ${errorMessage(error)}`)
      return forward(bodyText)
    }

    let fullCatalog = true
    if (sessionID) {
      fullCatalog = await isCatalogRestored(sessionID)
    } else {
      warn('missing-session', 'marked request had no session id; exposing the full catalog')
    }

    try {
      const protocol = detectRequestProtocol(body, protocolHint)
      if (!protocol || protocol === 'responses') return forward(bodyText)
      return forward(JSON.stringify(transformRequestBody(body, fullCatalog, warn, protocol)))
    } catch (error) {
      warn('transform-failed', `request transform failed; forwarding the original body: ${errorMessage(error)}`)
      return forward(bodyText)
    }
  }

  Object.defineProperty(anchoredFetch, WRAPPED_FETCH, { value: true })
  return anchoredFetch
}

function configuredProviderIDs(config, options, targetModelIDs = TARGET_MODEL_IDS) {
  const ids = new Set(['deepseek'])
  const additional = options?.providerIDs
  if (additional !== undefined) {
    if (!Array.isArray(additional) || additional.some((id) => typeof id !== 'string' || id.length === 0)) {
      throw new TypeError('opencode-anchored-standard: providerIDs must be an array of non-empty strings')
    }
    additional.forEach((id) => ids.add(id))
  }

  for (const [providerID, provider] of Object.entries(config.provider ?? {})) {
    const models = provider?.models ?? {}
    const declaresTarget = Object.entries(models).some(
      ([id, model]) => targetModelIDs.includes(id) || targetModelIDs.includes(model?.id),
    )
    if (declaresTarget) ids.add(providerID)
  }

  return ids
}

export async function AnchoredStandardPlugin({ client, directory, project, worktree = directory }, options = {}) {
  const warnings = new Set()
  const wrappedProviders = new Set()
  const nativeRuntime = isNativeRuntimeEnabled()
  const warn = (key, message) => {
    if (warnings.has(key)) return
    warnings.add(key)
    console.warn(`[opencode-anchored-standard] ${message}`)
  }
  const targetModelIDs = await loadTargetModelIDs(warn)
  const isCatalogRestored = createCatalogRestoreResolver(client, directory, warn)
  const inspector = createSessionInspector(client, directory, warn)

  let configuredInstructions = []
  const staticContext = new Map()
  const lastUserContext = new Map()

  return {
    tool: {
      str_replace_editor: createStrReplaceEditorTool(),
    },

    config: async (config) => {
      configuredInstructions = Array.isArray(config.instructions) ? config.instructions : []
      config.provider ??= {}
      for (const providerID of configuredProviderIDs(config, options, targetModelIDs)) {
        const provider = (config.provider[providerID] ??= {})
        const providerOptions = (provider.options ??= {})
        if (providerOptions.fetch?.[WRAPPED_FETCH]) {
          wrappedProviders.add(providerID)
          continue
        }

        const nextFetch = typeof providerOptions.fetch === 'function'
          ? providerOptions.fetch
          : globalThis.fetch.bind(globalThis)
        providerOptions.fetch = createAnchoredFetch(
          nextFetch,
          isCatalogRestored,
          warn,
          providerProtocol(providerID, provider),
        )
        wrappedProviders.add(providerID)
      }
    },

    'experimental.chat.system.transform': async (input, output) => {
      if (!input?.sessionID) return
      if (!isTargetModel(input.model, targetModelIDs)) return
      if (!Array.isArray(output?.system) || typeof output.system[0] !== 'string') return

      const context = extractStaticSystemContext(
        output.system[0],
        lastUserContext.get(input.sessionID),
      )
      if (context) staticContext.set(input.sessionID, context)
    },

    'chat.headers': async (input, output) => {
      if (!isTargetModel(input.model, targetModelIDs)) return
      if (UTILITY_AGENTS.has(input.agent)) return
      if (!wrappedProviders.has(input.model?.providerID)) return
      if (nativeRuntime) {
        warn(
          'native-runtime',
          'OPENCODE_EXPERIMENTAL_NATIVE_LLM bypasses provider fetch transforms; Anchored Standard is disabled',
        )
        return
      }

      output.headers[MARKER_HEADER] = '1'
      output.headers[SESSION_HEADER] = input.sessionID
    },

    'chat.message': async (input, output) => {
      if (UTILITY_AGENTS.has(input.agent)) return
      const selected = input.model ?? output.message.model
      const enabled = !nativeRuntime
        && isTargetModel(selected, targetModelIDs)
        && wrappedProviders.has(selected?.providerID)
      output.message.tools ??= {}
      output.message.tools.str_replace_editor = enabled
      if (!enabled) return
      if (!Array.isArray(output.parts)) return

      const sessionID = input.sessionID
      if (!sessionID || !output.message?.id) return
      lastUserContext.set(sessionID, {
        system: typeof output.message.system === 'string' ? output.message.system : '',
        structuredOutput: output.message.format?.type === 'json_schema',
      })

      const state = await inspector.inspect(sessionID)
      if (state.error) return
      if (!state.promoted || state.injected) return

      let base = staticContext.get(sessionID) ?? ''
      if (!base) {
        warn(
          'context-capture-missing',
          'OpenCode system context was not observed before promotion; persisting the plugin-assembled environment and instructions without MCP instructions or the skill catalog',
        )
        try {
          base = await buildFallbackContext({
            directory,
            worktree,
            project,
            providerID: selected?.providerID ?? 'deepseek',
            modelID: modelID(selected),
            instructions: configuredInstructions,
          })
        } catch (error) {
          warn('context-fallback-failed', `could not assemble fallback context: ${errorMessage(error)}`)
          base = ''
        }
      }

      const text = buildPromotedContext({ base, message: output.message })
      if (!text) return
      output.parts.unshift({
        id: partID(),
        type: 'text',
        sessionID,
        messageID: output.message.id,
        text,
        synthetic: true,
        metadata: { [INJECTION_METADATA_KEY]: INJECTION_METADATA_VALUE },
      })
      inspector.markInjected(sessionID)
    },
  }
}

export default AnchoredStandardPlugin
