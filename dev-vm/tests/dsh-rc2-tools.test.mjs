import test from 'node:test'
import assert from 'node:assert/strict'
import { mkdtemp, writeFile, symlink, rm, readFile } from 'node:fs/promises'
import { createRequire } from 'node:module'
import { dirname, join } from 'node:path'
import { tmpdir } from 'node:os'
import { pathToFileURL } from 'node:url'

const require = createRequire(process.env.DSH_PACKAGE_ENTRY || new URL('../profiles/web/package.json', import.meta.url))
const { boot } = await import(pathToFileURL(require.resolve('@deepseek-ai/dsh-app-boot')))
assert.equal(require('@deepseek-ai/dsh/package.json').version, '0.2.0-rc.2')

test('configured local shell and native PTC execute through the real Loader without confinement', async () => {
  const dir = await mkdtemp(join(tmpdir(), 'dsh-rc2-tools-'))
  const homePatch = await readFile(join(process.env.DSH_HOME || '/root/.dsh', 'cordis.patch.yml'), 'utf8')
  const { parseDocument } = require('yaml')
  const home = parseDocument(homePatch).toJS()
  const localBash = home.flatMap(row => row.insert || []).find(row => row.id === 'bash-local')
  const runtimeModules = dirname(dirname(dirname(require.resolve('@deepseek-ai/dsh-app-boot/package.json'))))
  const names = ['subprocess-local', 'fs-local', 'sandbox-local', 'sandbox-policy', 'ptc-runtime-node', 'bash-local', 'session-projection']
  await symlink(runtimeModules, join(dir, 'node_modules'), 'dir')
  await writeFile(join(dir, 'package.json'), JSON.stringify({ type: 'module', dependencies: Object.fromEntries(names.map(name => ['@deepseek-ai/dsh-' + name, '0.2.0-rc.2'])) }))
  const config = join(dir, 'cordis.yml')
  await writeFile(config, JSON.stringify([
    { id: 'subprocess', name: '@deepseek-ai/dsh-subprocess-local' },
    { id: 'fs', name: '@deepseek-ai/dsh-fs-local' },
    { id: 'sandbox', name: '@deepseek-ai/dsh-sandbox-local' },
    { id: 'session-projection', name: '@deepseek-ai/dsh-session-projection' },
    { id: 'sandbox-policy', name: '@deepseek-ai/dsh-sandbox-policy', config: { mode: 'danger-full-access', workspaceRoot: dir } },
    { id: 'ptc-runtime', name: '@deepseek-ai/dsh-ptc-runtime-node' },
    localBash,
  ]))
  let ctx
  try {
    ctx = await boot('rc2-tools-test', config, [], undefined, pathToFileURL(dir + '/').href)
    assert.ok([...ctx.loader.entries()].every(entry => entry.fiber?.state === 2), 'all native services activate')
    const spec = ctx.shell.resolve({ command: 'printf upgraded', workdir: dir })
    assert.equal(spec.timeoutMs, 1200000)
    assert.equal(spec.stdoutMaxBytes, 20000)
    assert.equal(ctx.shell.resolve({ command: 'true', timeoutMs: 2400000 }).timeoutMs, 1200000)
    const execution = await ctx.shell.execute(spec)
    const result = await execution.result()
    assert.equal(result.exitCode, 0)
    assert.equal(result.stdout.text, 'upgraded')
    const ptc = await ctx.ptcRuntime.run(ctx.ptcRuntime.resolve({
      cwd: dir, program: "const fs = await import('node:fs/promises'); await fs.writeFile('ptc.txt', 'native'); return {ok:true}", bindings: [],
    }))
    assert.equal(ptc.error, undefined, JSON.stringify(ptc))
    assert.deepEqual(ptc.value, { ok: true })
    assert.equal(ptc.sandbox.mode, 'danger-full-access')
    assert.equal(await readFile(join(dir, 'ptc.txt'), 'utf8'), 'native')
  } finally { await ctx?.fiber.dispose(); await rm(dir, { recursive: true, force: true }) }
})
