import test from 'node:test'
import assert from 'node:assert/strict'
import {
  DEFAULT_PRESETS,
  defaultStore,
  formatPromptTag,
  normalizePreset,
  normalizeStore,
  resolvePreset,
  apply,
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

test('apply registers systemPrompt section at exact order 1 with <formatting_and_tone> tag', () => {
  let registeredSection = null
  const registeredRoutes = []

  const mockCtx = {
    systemPrompt: {
      section: (section) => {
        registeredSection = section
      },
    },
    webServer: {
      register: (route) => {
        registeredRoutes.push(route)
        return () => {}
      },
    },
  }

  const dispose = apply(mockCtx, { filePath: '/tmp/test-style-presets.json' })
  assert.equal(typeof dispose, 'function')

  // Verify section registration
  assert.ok(registeredSection, 'section should be registered')
  assert.equal(registeredSection.name, 'style:formatting-and-tone')
  assert.equal(registeredSection.order, 1, 'order must be strictly 1')
  assert.equal(typeof registeredSection.text, 'function')

  // Verify prompt generation through section.text
  const textOutput = registeredSection.text({
    agent: { session: { id: 'test-session-1' } },
  })
  assert.match(textOutput, /^<formatting_and_tone>\n[\s\S]+\n<\/formatting_and_tone>$/)

  // Verify routes registered
  assert.ok(registeredRoutes.length >= 2)
  const paths = registeredRoutes.map((r) => r.path)
  assert.ok(paths.includes('/api/style-control/presets'))
  assert.ok(paths.includes('/api/style-control/session'))
})
