import test from 'node:test'
import assert from 'node:assert/strict'
import { mkdtemp, rm } from 'node:fs/promises'
import path from 'node:path'
import { Context } from '/usr/local/lib/node_modules/@deepseek-ai/dsh/node_modules/@deepseek-ai/cordis/lib/index.js'
import { WebServer } from '/usr/local/lib/node_modules/@deepseek-ai/dsh/node_modules/@deepseek-ai/dsh-host-webserver/lib/index.js'
import { SystemPrompt } from '/usr/local/lib/node_modules/@deepseek-ai/dsh/node_modules/@deepseek-ai/dsh-system-prompt/lib/index.js'
import {
  DEFAULT_PRESETS,
  defaultStore,
  formatPromptTag,
  normalizePreset,
  normalizeStore,
  resolvePreset,
  apply,
  inject,
  name,
} from './index.mjs'

test('formatPromptTag encloses non-empty text in <formatting_and_tone> tag', () => {
  assert.equal(
    formatPromptTag('Be concise and clear.'),
    '<formatting_and_tone>\nBe concise and clear.\n</formatting_and_tone>'
  )
  assert.equal(formatPromptTag(''), '')
  assert.equal(formatPromptTag('   '), '')
  assert.equal(formatPromptTag(null), '')
  assert.equal(formatPromptTag(undefined), '')
})

test('defaultStore contains default presets and default active id', () => {
  const store = defaultStore()
  assert.equal(store.presets.length, 3)
  assert.equal(store.activePresetId, 'default')
  assert.equal(store.presets[0].id, 'default')
  assert.equal(store.presets[1].id, 'professional')
  assert.equal(store.presets[2].id, 'creative')
})

test('resolvePreset resolves preset for session or global active preset', () => {
  const store = defaultStore()
  // Global fallback
  const p1 = resolvePreset(store, null)
  assert.equal(p1.id, 'default')

  // Change activePresetId
  store.activePresetId = 'professional'
  const p2 = resolvePreset(store, null)
  assert.equal(p2.id, 'professional')

  // Session-specific preset override
  store.sessionPresets['session-123'] = 'creative'
  const p3 = resolvePreset(store, 'session-123')
  assert.equal(p3.id, 'creative')

  // Non-matching session falls back to activePresetId
  const p4 = resolvePreset(store, 'session-other')
  assert.equal(p4.id, 'professional')

  // Completely unknown preset falls back to first preset
  store.activePresetId = 'nonexistent-preset'
  const p5 = resolvePreset(store, null)
  assert.equal(p5.id, 'default')
})

test('normalizePreset handles missing or blank fields', () => {
  assert.deepEqual(
    normalizePreset({ id: 'custom', name: 'My Style', content: 'Do something' }, 0),
    { id: 'custom', name: 'My Style', content: 'Do something' }
  )
  const fallback = normalizePreset({}, 1)
  assert.ok(fallback.id.length > 0)
  assert.equal(fallback.name, fallback.id)
  assert.equal(fallback.content, '')
})

test('normalizeStore cleans and validates presets and selection pointers', () => {
  const clean = normalizeStore({
    presets: [
      { id: 'p1', name: 'Preset 1', content: 'Hello' },
      { id: 'p2', name: 'Preset 2', content: 'World' },
    ],
    activePresetId: 'p2',
    sessionPresets: { s1: 'p1', s2: 'invalid-id' },
  })
  assert.equal(clean.presets.length, 2)
  assert.equal(clean.activePresetId, 'p2')
  assert.equal(clean.sessionPresets.s1, 'p1')
  assert.equal(clean.sessionPresets.s2, undefined)
})

test('session selections stay independent without changing the global default', async () => {
  const fixture = await mkdtemp('/root/.dsh/.agents/exploration/style-selection/fixture-')
  const ctx = new Context()
  const webServerFiber = ctx.plugin(WebServer, {
    host: '127.0.0.1',
    port: 0,
    compression: 'none',
  })
  const systemPromptFiber = ctx.plugin(SystemPrompt, {
    includeHarnessIdentity: false,
    includeRuntimeContext: false,
    persona: '',
  })
  let pluginFiber

  try {
    await Promise.all([webServerFiber, systemPromptFiber])
    pluginFiber = ctx.plugin({ name, inject, apply }, {
      filePath: path.join(fixture, 'style-presets.json'),
    })
    await pluginFiber

    const baseUrl = `http://127.0.0.1:${ctx.webServer.port}`
    for (const [sessionId, presetId] of [['first-session', 'professional'], ['second-session', 'creative']]) {
      const response = await (await fetch(`${baseUrl}/api/style-control/session`, {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({ sessionId, presetId }),
      })).json()
      assert.deepEqual(response, { ok: true, sessionId, presetId, activePresetId: 'default' })
    }

    const state = await (await fetch(`${baseUrl}/api/style-control/presets`)).json()
    assert.equal(state.activePresetId, 'default')
    assert.deepEqual(state.sessionPresets, {
      'first-session': 'professional',
      'second-session': 'creative',
    })

    const styleFor = async (sessionId) => {
      const assembly = await ctx.systemPrompt.assemble({ agent: { session: { id: sessionId } } })
      return assembly.sections.find((entry) => entry.name === 'style:formatting-and-tone')?.text
    }
    assert.match(await styleFor('first-session'), /Maintain a professional, formal/)
    assert.match(await styleFor('second-session'), /Adopt an engaging, vivid/)
  } finally {
    await pluginFiber?.dispose()
    await systemPromptFiber.dispose()
    await webServerFiber.dispose()
    await rm(fixture, { recursive: true, force: true })
  }
})
