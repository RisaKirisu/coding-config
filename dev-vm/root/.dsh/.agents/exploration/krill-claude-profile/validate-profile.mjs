import { readFileSync } from 'node:fs'
import { createRequire } from 'node:module'
import { isDeepStrictEqual } from 'node:util'

const require = createRequire('/root/.dsh/profiles/web/node_modules/')
const YAML = require('yaml')
// The loader's !!js tag is a plain scalar to this parse; only the structure is under test.
const jsTag = { tag: '!!js', resolve: (value) => value }

const profiles = ['/root/.dsh/profiles/web/cordis.patch.yml', '/root/.dsh/profiles/headless/cordis.patch.yml']
const expected = {
  displayName: 'Krill Claude',
  apiKeyEnv: 'KRILL_CLAUDE_API_KEY',
  api: 'anthropic-messages',
  baseURL: 'https://api.krill-code.net',
}
const expectedModel = {
  id: 'claude-opus-5-5',
  name: 'claude-opus-5-5 · Krill AI',
  contextWindow: 1000000,
  maxTokens: 128000,
  input: ['text', 'image'],
}

let failed = false
const check = (label, actual, wanted) => {
  const ok = isDeepStrictEqual(actual, wanted)
  if (!ok) failed = true
  console.log(`${ok ? 'ok  ' : 'FAIL'} ${label}: ${JSON.stringify(actual)}`)
}

for (const path of profiles) {
  console.log(`\n== ${path}`)
  const document = YAML.parse(readFileSync(path, 'utf8'), { customTags: [jsTag] })
  const entry = document.find((row) => row && row.id === 'llm-pi-ai')
  const route = entry?.config?.providers?.['krill-claude']
  if (!route) {
    failed = true
    console.log('FAIL krill-claude route missing')
    continue
  }
  for (const [key, value] of Object.entries(expected)) check(key, route[key], value)
  check('models.length', route.models.length, 1)
  const model = route.models[0]
  for (const [key, value] of Object.entries(expectedModel)) check(`model.${key}`, model[key], value)
  const extra = Object.keys(route).sort()
  console.log(`    route keys: ${extra.join(', ')}`)
  console.log(`    model keys: ${Object.keys(model).sort().join(', ')}`)
  check('route keys', extra, ['api', 'apiKeyEnv', 'baseURL', 'displayName', 'models'])
  // The running web profile's settings layer adds reasoningEfforts and compat
  // when it writes the user layer back, so the live file carries them while the
  // headless file states only the configured fields. The configured fields
  // above are still asserted exactly.
  const liveAdded = new Set(['compat', 'reasoningEfforts'])
  const modelKeys = Object.keys(model).sort()
  check(
    'model keys',
    modelKeys.filter((key) => !liveAdded.has(key)),
    ['contextWindow', 'id', 'input', 'maxTokens', 'name'],
  )
  console.log(`    top-level entries: ${document.map((row) => (row && row.insert ? 'insert' : row.id)).join(', ')}`)
}

console.log(`\n${failed ? 'VALIDATION FAILED' : 'VALIDATION PASSED'}`)
process.exit(failed ? 1 : 0)
