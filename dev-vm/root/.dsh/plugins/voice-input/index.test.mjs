import assert from 'node:assert/strict'
import { existsSync, readFileSync } from 'node:fs'
import { mkdtemp, readFile, rm } from 'node:fs/promises'
import { createRequire } from 'node:module'
import { tmpdir } from 'node:os'
import { dirname, join } from 'node:path'
import test from 'node:test'
import { fileURLToPath } from 'node:url'

const req = createRequire('/usr/local/lib/node_modules/@deepseek-ai/dsh/package.json')
const { Context } = await import(req.resolve('@deepseek-ai/cordis'))
const { SystemPrompt } = await import(req.resolve('@deepseek-ai/dsh-system-prompt'))
const { ToolRuntime } = await import(req.resolve('@deepseek-ai/dsh-tools'))
const { loadOverlayPatches } = await import(req.resolve('@deepseek-ai/dsh-app-boot'))

const __dirname = dirname(fileURLToPath(import.meta.url))
const PACKAGE_DIR = __dirname
const PACKAGE_JSON_PATH = join(PACKAGE_DIR, 'package.json')

test('package.json manifest declares dsh.bundle.patch and valid exports', () => {
  const pkg = JSON.parse(readFileSync(PACKAGE_JSON_PATH, 'utf8'))

  assert.equal(pkg.name, '@devvm/dsh-voice-input', 'package name must be @devvm/dsh-voice-input')
  assert.equal(pkg.type, 'module', 'package must be ESM')
  assert.equal(pkg.main, 'index.mjs', 'main must point to index.mjs')
  assert.equal(pkg.dsh?.bundle?.patch, './cordis.patch.yml', 'dsh.bundle.patch must point to ./cordis.patch.yml')

  assert.ok(pkg.exports, 'exports must be defined')
  assert.equal(pkg.exports['.'], './index.mjs', 'exports["."] must point to ./index.mjs')
  assert.equal(pkg.exports['./cordis.patch.yml'], './cordis.patch.yml', 'exports["./cordis.patch.yml"] must resolve')
  assert.equal(pkg.exports['./package.json'], './package.json', 'exports["./package.json"] must resolve')

  assert.ok(Array.isArray(pkg.files), 'files must be an array')
  assert.ok(pkg.files.includes('index.mjs'), 'files must include index.mjs')
  assert.ok(pkg.files.includes('cordis.patch.yml'), 'files must include cordis.patch.yml')
  assert.ok(pkg.files.includes('README.md'), 'files must include README.md')

  assert.ok(existsSync(join(PACKAGE_DIR, pkg.main)), 'main file must exist on disk')
  assert.ok(existsSync(join(PACKAGE_DIR, pkg.dsh.bundle.patch)), 'bundle patch file must exist on disk')
  assert.ok(existsSync(join(PACKAGE_DIR, 'README.md')), 'README.md must exist on disk')
})

test('cordis.patch.yml is a valid DSH overlay patch that inserts plugin by package name', () => {
  const patchPath = join(PACKAGE_DIR, 'cordis.patch.yml')
  assert.ok(existsSync(patchPath), 'cordis.patch.yml must exist')

  const patches = loadOverlayPatches('test-voice-input', patchPath)
  assert.ok(Array.isArray(patches), 'patches must be a top-level array')
  assert.ok(patches.length > 0, 'patches must contain at least one patch entry')

  const insertEntry = patches.find((p) => Array.isArray(p.insert))
  assert.ok(insertEntry, 'must contain an insert patch')

  const pluginRow = insertEntry.insert.find((r) => r.name === '@devvm/dsh-voice-input')
  assert.ok(pluginRow, 'must insert row with package name @devvm/dsh-voice-input')
  assert.equal(pluginRow.id, 'tool-voice-input', 'row id should be tool-voice-input')

  for (const patch of patches) {
    if (patch.insert) {
      for (const row of patch.insert) {
        assert.ok(!row.name.startsWith('.'), `plugin name must not be a relative path: ${row.name}`)
      }
    }
  }
})

test('plugin registers archive_voice_input and remove_voice_input_record in cordis context', async () => {
  const tmpDir = await mkdtemp(join(tmpdir(), 'voice-input-test-'))
  const testFile = join(tmpDir, 'archive.jsonl')

  try {
    const ctx = new Context()
    await ctx.plugin(SystemPrompt)
    await ctx.plugin(ToolRuntime)

    const VoicePlugin = await import(join(PACKAGE_DIR, 'index.mjs'))
    assert.equal(VoicePlugin.name, 'tool-voice-input')
    assert.deepEqual(VoicePlugin.inject, ['tools'])

    await ctx.plugin(VoicePlugin, { file: testFile })

    const tools = ctx.get('tools')
    assert.ok(tools, 'tools service must be available')

    const archiveTool = tools.get('archive_voice_input')
    assert.ok(archiveTool, 'archive_voice_input tool must be registered')
    assert.equal(archiveTool.name, 'archive_voice_input')

    const removeTool = tools.get('remove_voice_input_record')
    assert.ok(removeTool, 'remove_voice_input_record tool must be registered')
    assert.equal(removeTool.name, 'remove_voice_input_record')

    const schemas = tools.schemas()
    const names = schemas.map((s) => s.name)
    assert.ok(names.includes('archive_voice_input'))
    assert.ok(names.includes('remove_voice_input_record'))
  } finally {
    await rm(tmpDir, { recursive: true, force: true })
  }
})

test('archive_voice_input tool archives records to JSONL file and validates input', async () => {
  const tmpDir = await mkdtemp(join(tmpdir(), 'voice-input-test-'))
  const testFile = join(tmpDir, 'archive.jsonl')

  try {
    const ctx = new Context()
    await ctx.plugin(SystemPrompt)
    await ctx.plugin(ToolRuntime)

    const VoicePlugin = await import(join(PACKAGE_DIR, 'index.mjs'))
    await ctx.plugin(VoicePlugin, { file: testFile })
    const tools = ctx.get('tools')

    const res1 = await tools.execute({
      name: 'archive_voice_input',
      callId: 'call_1',
      arguments: { raw: 'um please run tests', cleaned: 'Please run tests.' },
      signal: new AbortController().signal,
    })

    assert.equal(res1.isError, false)
    assert.equal(res1.value.text, 'Voice input archived successfully at index 0.')

    const res2 = await tools.execute({
      name: 'archive_voice_input',
      callId: 'call_2',
      arguments: { raw: 'check the git status', cleaned: 'Check the git status.' },
      signal: new AbortController().signal,
    })

    assert.equal(res2.isError, false)
    assert.equal(res2.value.text, 'Voice input archived successfully at index 1.')

    const fileLines = (await readFile(testFile, 'utf8')).trim().split('\n')
    assert.equal(fileLines.length, 2)
    assert.deepEqual(JSON.parse(fileLines[0]), {
      raw: 'um please run tests',
      cleaned: 'Please run tests.',
    })
    assert.deepEqual(JSON.parse(fileLines[1]), {
      raw: 'check the git status',
      cleaned: 'Check the git status.',
    })

    const invalidRes1 = await tools.execute({
      name: 'archive_voice_input',
      callId: 'call_3',
      arguments: { raw: '  ', cleaned: 'Valid cleaned' },
      signal: new AbortController().signal,
    })
    assert.equal(invalidRes1.isError, true)
    assert.ok(invalidRes1.error.message.includes('raw'))

    const invalidRes2 = await tools.execute({
      name: 'archive_voice_input',
      callId: 'call_4',
      arguments: { raw: 'Valid raw', cleaned: '' },
      signal: new AbortController().signal,
    })
    assert.equal(invalidRes2.isError, true)
    assert.ok(invalidRes2.error.message.includes('cleaned'))
  } finally {
    await rm(tmpDir, { recursive: true, force: true })
  }
})

test('remove_voice_input_record tool removes record by index and handles edge cases', async () => {
  const tmpDir = await mkdtemp(join(tmpdir(), 'voice-input-test-'))
  const testFile = join(tmpDir, 'archive.jsonl')

  try {
    const ctx = new Context()
    await ctx.plugin(SystemPrompt)
    await ctx.plugin(ToolRuntime)

    const VoicePlugin = await import(join(PACKAGE_DIR, 'index.mjs'))
    await ctx.plugin(VoicePlugin, { file: testFile })
    const tools = ctx.get('tools')

    for (let i = 0; i < 3; i++) {
      await tools.execute({
        name: 'archive_voice_input',
        callId: `call_seed_${i}`,
        arguments: { raw: `raw ${i}`, cleaned: `cleaned ${i}` },
        signal: new AbortController().signal,
      })
    }

    const removeRes = await tools.execute({
      name: 'remove_voice_input_record',
      callId: 'call_rem_1',
      arguments: { index: 1 },
      signal: new AbortController().signal,
    })
    assert.equal(removeRes.isError, false)
    assert.equal(removeRes.value.text, 'Voice input record 1 removed successfully.')

    const remaining = (await readFile(testFile, 'utf8')).trim().split('\n').map((l) => JSON.parse(l))
    assert.equal(remaining.length, 2)
    assert.equal(remaining[0].raw, 'raw 0')
    assert.equal(remaining[1].raw, 'raw 2')

    const outOfBoundsRes = await tools.execute({
      name: 'remove_voice_input_record',
      callId: 'call_rem_oob',
      arguments: { index: 10 },
      signal: new AbortController().signal,
    })
    assert.equal(outOfBoundsRes.isError, false)
    assert.equal(outOfBoundsRes.value.text, 'No voice input record exists at index 10.')

    const invalidIndexRes = await tools.execute({
      name: 'remove_voice_input_record',
      callId: 'call_rem_neg',
      arguments: { index: -1 },
      signal: new AbortController().signal,
    })
    assert.equal(invalidIndexRes.isError, true)
    assert.ok(invalidIndexRes.error.message.includes('index'))
  } finally {
    await rm(tmpDir, { recursive: true, force: true })
  }
})

test('configuration respects filePath and environment variable overrides', async () => {
  const tmpDir = await mkdtemp(join(tmpdir(), 'voice-input-test-'))
  const fileA = join(tmpDir, 'fileA.jsonl')
  const fileB = join(tmpDir, 'fileB.jsonl')
  const fileC = join(tmpDir, 'fileC.jsonl')

  try {
    const VoicePlugin = await import(join(PACKAGE_DIR, 'index.mjs'))

    // 1. config.filePath
    const ctx1 = new Context()
    await ctx1.plugin(SystemPrompt)
    await ctx1.plugin(ToolRuntime)
    await ctx1.plugin(VoicePlugin, { filePath: fileA })
    await ctx1.get('tools').execute({
      name: 'archive_voice_input',
      callId: 'c1',
      arguments: { raw: 'r1', cleaned: 'c1' },
      signal: new AbortController().signal,
    })
    assert.ok(existsSync(fileA), 'fileA should be created by config.filePath')

    // 2. VOICE_DICTATION_DATA_FILE
    const prevEnvData = process.env.VOICE_DICTATION_DATA_FILE
    process.env.VOICE_DICTATION_DATA_FILE = fileB
    try {
      const ctx2 = new Context()
      await ctx2.plugin(SystemPrompt)
      await ctx2.plugin(ToolRuntime)
      await ctx2.plugin(VoicePlugin, {})
      await ctx2.get('tools').execute({
        name: 'archive_voice_input',
        callId: 'c2',
        arguments: { raw: 'r2', cleaned: 'c2' },
        signal: new AbortController().signal,
      })
      assert.ok(existsSync(fileB), 'fileB should be created by VOICE_DICTATION_DATA_FILE')
    } finally {
      if (prevEnvData !== undefined) process.env.VOICE_DICTATION_DATA_FILE = prevEnvData
      else delete process.env.VOICE_DICTATION_DATA_FILE
    }

    // 3. VOICE_DICTATION_FILE
    const prevEnv = process.env.VOICE_DICTATION_FILE
    process.env.VOICE_DICTATION_FILE = fileC
    try {
      const ctx3 = new Context()
      await ctx3.plugin(SystemPrompt)
      await ctx3.plugin(ToolRuntime)
      await ctx3.plugin(VoicePlugin, {})
      await ctx3.get('tools').execute({
        name: 'archive_voice_input',
        callId: 'c3',
        arguments: { raw: 'r3', cleaned: 'c3' },
        signal: new AbortController().signal,
      })
      assert.ok(existsSync(fileC), 'fileC should be created by VOICE_DICTATION_FILE')
    } finally {
      if (prevEnv !== undefined) process.env.VOICE_DICTATION_FILE = prevEnv
      else delete process.env.VOICE_DICTATION_FILE
    }
  } finally {
    await rm(tmpDir, { recursive: true, force: true })
  }
})
