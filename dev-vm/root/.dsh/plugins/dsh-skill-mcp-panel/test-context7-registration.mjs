import test from 'node:test'
import assert from 'node:assert/strict'
import { createServer } from 'node:http'
import { createRequire } from 'node:module'
import { mkdtemp, writeFile, symlink, rm } from 'node:fs/promises'
import { dirname, join } from 'node:path'
import { tmpdir } from 'node:os'
import { pathToFileURL } from 'node:url'
import { mcpServerInputSchema, toPatchRow, patchRowToView, applyServerEdit } from './lib/mcp/model.js'

const require = createRequire(process.env.DSH_PACKAGE_ENTRY)
assert.equal(require('@deepseek-ai/dsh/package.json').version, '0.2.0-rc.2')
const { boot } = await import(pathToFileURL(require.resolve('@deepseek-ai/dsh-app-boot')))
const sdk = createRequire(process.env.DSH_MCP_TEST_PACKAGE_ENTRY || import.meta.url)
const { McpServer } = await import(pathToFileURL(sdk.resolve('@modelcontextprotocol/sdk/server/mcp.js')))
const { StreamableHTTPServerTransport } = await import(pathToFileURL(sdk.resolve('@modelcontextprotocol/sdk/server/streamableHttp.js')))

test('stock rc.2 MCP uses native secret headers; upstream panel redacts and preserves them', async () => {
  const dir = await mkdtemp(join(tmpdir(), 'dsh-native-mcp-'))
  const requests = []
  const instances = new Set()
  const server = createServer(async (req, res) => {
    requests.push(req.headers.authorization)
    if (req.method !== 'POST') { res.writeHead(405).end(); return }
    const chunks = []
    for await (const chunk of req) chunks.push(chunk)
    const instance = new McpServer({ name: 'fixture', version: '1.0.0' })
    instance.registerTool('echo', { inputSchema: {} }, async () => ({ content: [{ type: 'text', text: 'echoed' }] }))
    const transport = new StreamableHTTPServerTransport({ sessionIdGenerator: undefined, enableJsonResponse: true })
    instances.add(instance)
    res.on('close', () => { instances.delete(instance); void instance.close() })
    await instance.connect(transport)
    await transport.handleRequest(req, res, JSON.parse(Buffer.concat(chunks).toString()))
  })
  await new Promise(resolve => server.listen(0, '127.0.0.1', resolve))
  let ctx
  const envKey = 'DSH_NATIVE_MCP_FIXTURE_TOKEN'
  const before = process.env[envKey]
  try {
    process.env[envKey] = 'native-test-token'
    const input = mcpServerInputSchema.parse({ serverName: 'fixture', transport: 'streamable-http',
      url: `http://127.0.0.1:${server.address().port}/mcp`, headers: { Authorization: 'Bearer native-test-token' },
      failOnStartupError: true, reconnect: { enabled: false } })
    const row = toPatchRow(input)
    const view = patchRowToView(row)
    assert.deepEqual(view.headerKeys, ['Authorization'])
    assert.ok(!JSON.stringify(view).includes('native-test-token'))
    const edited = applyServerEdit(row, mcpServerInputSchema.parse({ ...input, headers: {} }))
    assert.equal(edited.config.headers.Authorization, 'Bearer native-test-token')
    assert.equal(row.config.bearerTokenRef, undefined)
    // Native Cordis expressions read the CLI's home .env; no MCP package extension.
    row.config.headers.Authorization = { __jsExpr: `'Bearer ' + process.env.${envKey}` }
    const runtimeModules = dirname(dirname(dirname(require.resolve('@deepseek-ai/dsh-app-boot/package.json'))))
    await symlink(runtimeModules, join(dir, 'node_modules'))
    await writeFile(join(dir, 'package.json'), JSON.stringify({ type: 'module', dependencies: {
      '@deepseek-ai/dsh-system-prompt': '0.2.0-rc.2', '@deepseek-ai/dsh-tools': '0.2.0-rc.2', '@deepseek-ai/dsh-mcp-client': '0.2.0-rc.2',
    } }))
    const config = join(dir, 'cordis.yml')
    await writeFile(config, JSON.stringify([
      { id: 'system-prompt', name: '@deepseek-ai/dsh-system-prompt' },
      { id: 'tools', name: '@deepseek-ai/dsh-tools' }, row,
    ]))
    ctx = await boot('native-mcp-test', config, [], undefined, pathToFileURL(dir + '/').href)
    assert.deepEqual(ctx.tools.schemas().map(tool => tool.name), ['mcp__fixture__echo'])
    const result = await ctx.tools.execute({ callId: 'native-call', name: 'mcp__fixture__echo', arguments: {}, signal: AbortSignal.timeout(5000) })
    assert.equal(result.isError, false)
    assert.match(JSON.stringify(result), /echoed/)
    assert.ok(requests.length > 0)
    assert.ok(requests.every(value => value === 'Bearer native-test-token'))
  } finally {
    if (before === undefined) delete process.env[envKey]
    else process.env[envKey] = before
    await ctx?.fiber.dispose()
    for (const instance of instances) await instance.close()
    server.closeAllConnections()
    await new Promise(resolve => server.close(resolve))
    await rm(dir, { recursive: true, force: true })
  }
})
