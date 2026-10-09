import test from 'node:test'
import assert from 'node:assert/strict'
import { execFileSync } from 'node:child_process'
import { cpSync, existsSync, mkdirSync, mkdtempSync, readFileSync, realpathSync, rmSync } from 'node:fs'
import { createRequire } from 'node:module'
import { tmpdir } from 'node:os'
import { basename, join, resolve } from 'node:path'
import { fileURLToPath, pathToFileURL } from 'node:url'

const sourceProfile = fileURLToPath(new URL('../root/.dsh/profiles/web/', import.meta.url))
const cli = realpathSync(process.env.DEVVM_DSH_CLI || execFileSync('which', ['dsh'], { encoding: 'utf8' }).trim())
const require = createRequire(cli)
const anchor = require.resolve('@deepseek-ai/dsh/package.json')
const { loadProfileDirectory, generateConfigSchema } = await import(pathToFileURL(require.resolve('@deepseek-ai/dsh-app-boot')))

test('one native profile install supplies custom plugins and their dependencies without source fallbacks', async t => {
  const home = mkdtempSync(join(tmpdir(), 'dsh-plugin-install-'))
  t.after(() => rmSync(home, { recursive: true, force: true }))
  const profileDir = join(home, 'profiles/web')
  mkdirSync(profileDir, { recursive: true })
  for (const name of ['package.json', 'pnpm-lock.yaml', 'pnpm-workspace.yaml']) {
    cpSync(join(sourceProfile, name), join(profileDir, name))
  }
  const manifest = JSON.parse(readFileSync(join(profileDir, 'package.json'), 'utf8'))
  const packages = Object.entries(manifest.dependencies).filter(([, spec]) => spec.startsWith('file:'))
  assert.equal(packages.length, 8)
  for (const [, spec] of packages) {
    cpSync(resolve(sourceProfile, spec.slice(5)), resolve(profileDir, spec.slice(5)), {
      recursive: true,
      filter: path => basename(path) !== 'node_modules',
    })
  }
  execFileSync(process.execPath, [cli, 'plugin', '--profile', 'web', 'install', '--frozen-lockfile'], {
    env: { ...process.env, CI: 'true', DSH_HOME: home },
    stdio: 'inherit',
  })
  assert.equal(existsSync(join(home, 'plugins/node_modules')), false)
  assert.equal(existsSync(join(home, 'profiles/node_modules')), false)
  for (const [name, spec] of packages) {
    assert.equal(existsSync(resolve(profileDir, spec.slice(5), 'node_modules')), false)
    assert.ok(realpathSync(join(profileDir, 'node_modules', name)).startsWith(profileDir + '/node_modules/'))
  }

  const panel = createRequire(join(profileDir, 'node_modules/dsh-skill-mcp-panel/package.json'))
  for (const name of ['fflate', 'yaml', 'zod', '@modelcontextprotocol/sdk/client/index.js', 'chokidar']) {
    assert.ok(panel.resolve(name).startsWith(profileDir + '/node_modules/'), name + ' must be installed in the profile')
  }

  const profile = loadProfileDirectory('plugin-install-test', profileDir, anchor)
  assert.deepEqual(profile.skippedBundles, [])
  assert.deepEqual(profile.layers.map(layer => layer.packageName), manifest.dsh.profile.bundles)
  // Collect host entry modules without traversing the unrelated native preset carriers.
  const names = new Set(packages.map(([name]) => name))
  const entries = profile.layers.filter(layer => names.has(layer.packageName)).flatMap(layer =>
    layer.patches.flatMap(patch => patch.insert ?? []).filter(entry => entry.name === layer.packageName))
  assert.equal(entries.length, 7)
  const schema = await generateConfigSchema(profile, [[{ insert: entries }]], anchor)
  assert.equal(schema['x-cordis'].complete, true, JSON.stringify(schema['x-cordis'].diagnostics))
  assert.deepEqual(schema['x-cordis'].entries.map(entry => entry.name), entries.map(entry => entry.name))
})
