import { readFileSync } from 'node:fs'
import { writeFile, mkdir } from 'node:fs/promises'
import path from 'node:path'

export const name = 'style-control'
export const inject = ['webServer', 'systemPrompt']

export const DEFAULT_PRESETS = Object.freeze([
  {
    id: 'default',
    name: 'Default',
    content: 'Respond clearly, concisely, and accurately with balanced technical depth.',
  },
  {
    id: 'professional',
    name: 'Professional',
    content: 'Maintain a professional, formal, and precise tone. Structure answers methodically and adhere strictly to industry best practices.',
  },
  {
    id: 'creative',
    name: 'Creative',
    content: 'Adopt an engaging, vivid, and creative tone. Use expressive analogies and clear storytelling while maintaining technical accuracy.',
  },
])

export function formatPromptTag(content) {
  const trimmed = typeof content === 'string' ? content.trim() : ''
  return trimmed.length > 0 ? `<formatting_and_tone>\n${trimmed}\n</formatting_and_tone>` : ''
}

export function defaultStore() {
  return {
    presets: DEFAULT_PRESETS.map((p) => ({ ...p })),
    activePresetId: 'default',
    sessionPresets: {},
  }
}

export function resolvePreset(store, sessionId) {
  if (!store?.presets?.length) return null
  const targetId = (sessionId && store.sessionPresets?.[sessionId]) || store.activePresetId
  return store.presets.find((p) => p.id === targetId) || store.presets[0] || null
}

export function normalizePreset(p, index) {
  const id = typeof p?.id === 'string' && p.id.trim() ? p.id.trim() : `preset-${Date.now()}-${index}`
  const name = typeof p?.name === 'string' && p.name.trim() ? p.name.trim() : id
  const content = typeof p?.content === 'string' ? p.content : ''
  return { id, name, content }
}

export function normalizeStore(data) {
  const fallback = defaultStore()
  if (!data || typeof data !== 'object') return fallback

  const presets = (Array.isArray(data.presets) && data.presets.length > 0)
    ? data.presets.map(normalizePreset)
    : fallback.presets

  const validIds = new Set(presets.map((p) => p.id))
  const activePresetId = typeof data.activePresetId === 'string' && validIds.has(data.activePresetId)
    ? data.activePresetId
    : presets[0].id

  const sessionPresets = Object.fromEntries(
    Object.entries(data.sessionPresets || {}).filter(([sid, pid]) => typeof sid === 'string' && validIds.has(pid))
  )

  return { presets, activePresetId, sessionPresets }
}

export function loadStoreSync(filePath) {
  try {
    return normalizeStore(JSON.parse(readFileSync(filePath, 'utf8')))
  } catch {
    return defaultStore()
  }
}

export async function saveStoreToFile(filePath, store) {
  try {
    await mkdir(path.dirname(filePath), { recursive: true })
    await writeFile(filePath, JSON.stringify(store, null, 2), 'utf8')
  } catch (error) {
    console.error(`style-control: failed to save presets to ${filePath}:`, error)
  }
}

function json(res, status, value) {
  res.writeHead(status, { 'Content-Type': 'application/json; charset=utf-8' })
  res.end(JSON.stringify(value))
}

async function readJson(req) {
  const chunks = []
  for await (const chunk of req) chunks.push(chunk)
  return chunks.length === 0 ? {} : JSON.parse(Buffer.concat(chunks).toString('utf8'))
}

export function apply(ctx, config) {
  const dshHome = process.env.DSH_HOME || path.join(process.env.HOME || '/root', '.dsh')
  const filePath = typeof config?.filePath === 'string' && config.filePath.length > 0
    ? config.filePath
    : path.join(dshHome, 'style-presets.json')

  // Synchronous load on startup ensures zero race window
  let store = loadStoreSync(filePath)

  // Register system prompt section strictly at order: 1
  ctx.systemPrompt.section({
    name: 'style:formatting-and-tone',
    order: 1,
    text: (context) => {
      const sessionId = context?.agent?.session?.id
      const preset = resolvePreset(store, sessionId)
      return formatPromptTag(preset?.content)
    },
  })

  // Register HTTP routes
  const routes = [
    ctx.webServer.register({
      kind: 'exact',
      path: '/api/style-control/presets',
      handler: async (req, res) => {
        if (req.method === 'GET' || req.method === 'HEAD') {
          json(res, 200, store)
          return
        }
        if (req.method === 'POST') {
          try {
            const body = await readJson(req)
            store = normalizeStore({
              presets: body.presets,
              activePresetId: body.activePresetId,
              sessionPresets: store.sessionPresets,
            })
            await saveStoreToFile(filePath, store)
            json(res, 200, store)
          } catch (error) {
            json(res, 400, { error: error?.message || String(error) })
          }
          return
        }
        json(res, 405, { error: 'Method Not Allowed' })
      },
    }),
    ctx.webServer.register({
      kind: 'exact',
      path: '/api/style-control/session',
      handler: async (req, res) => {
        if (req.method !== 'POST') {
          json(res, 405, { error: 'Method Not Allowed' })
          return
        }
        try {
          const body = await readJson(req)
          const { sessionId, presetId } = body
          if (presetId && store.presets.some((p) => p.id === presetId)) {
            if (sessionId) {
              store.sessionPresets[sessionId] = presetId
              // Prune old entries if map exceeds 200 to bound memory growth
              const keys = Object.keys(store.sessionPresets)
              if (keys.length > 200) {
                delete store.sessionPresets[keys[0]]
              }
            } else {
              store.activePresetId = presetId
            }
            await saveStoreToFile(filePath, store)
            json(res, 200, { ok: true, sessionId, presetId, activePresetId: store.activePresetId })
          } else {
            json(res, 400, { error: `Unknown preset id: ${presetId}` })
          }
        } catch (error) {
          json(res, 400, { error: error?.message || String(error) })
        }
      },
    }),
    ctx.webServer.register({
      kind: 'exact',
      path: '/api/style-control/reset',
      handler: async (req, res) => {
        if (req.method !== 'POST') {
          json(res, 405, { error: 'Method Not Allowed' })
          return
        }
        store = defaultStore()
        await saveStoreToFile(filePath, store)
        json(res, 200, store)
      },
    }),
  ]

  return () => {
    routes.forEach((dispose) => dispose?.())
  }
}
