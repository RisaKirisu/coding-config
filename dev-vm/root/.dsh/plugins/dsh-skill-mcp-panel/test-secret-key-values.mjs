import test from 'node:test'
import assert from 'node:assert/strict'
import { createServer } from 'node:http'
import { mkdtemp, readFile, rm, writeFile } from 'node:fs/promises'
import { tmpdir } from 'node:os'
import { join } from 'node:path'
import { Context } from '@deepseek-ai/cordis'
import CredentialsLocal from '@deepseek-ai/dsh-credentials-local'
import Tools from '@deepseek-ai/dsh-tools'
import SystemPrompt from '@deepseek-ai/dsh-system-prompt'
import { credentialRef } from '@deepseek-ai/dsh-credentials'
import { McpServer } from '@modelcontextprotocol/sdk/server/mcp.js'
import { StreamableHTTPServerTransport } from '@modelcontextprotocol/sdk/server/streamableHttp.js'
import * as panelMcpClient from './lib/mcp/credential-client.js'
import { credentialRefForSecret, inputFromPatchRow, mcpServerInputSchema, patchRowToView, toPatchRow } from './lib/mcp/model.js'

/** MCP server that only answers requests carrying the expected x-api-key. */
async function startKeyedMcpServer(expectedKey) {
  const seenKeys = []
  const server = createServer(async (req, res) => {
    seenKeys.push(req.headers['x-api-key'])
    if (req.headers['x-api-key'] !== expectedKey.current) { res.writeHead(401).end(); return }
    if (req.method !== 'POST') { res.writeHead(405).end(); return }
    const chunks = []
    for await (const chunk of req) chunks.push(chunk)
    const instance = new McpServer({ name: 'fixture', version: '1.0.0' })
    instance.registerTool('echo', { inputSchema: {} }, async () => ({ content: [{ type: 'text', text: 'echoed' }] }))
    const transport = new StreamableHTTPServerTransport({ sessionIdGenerator: undefined, enableJsonResponse: true })
    res.on('close', () => void instance.close())
    await instance.connect(transport)
    await transport.handleRequest(req, res, JSON.parse(Buffer.concat(chunks).toString()))
  })
  await new Promise(resolve => server.listen(0, '127.0.0.1', resolve))
  return { server, seenKeys, url: `http://127.0.0.1:${server.address().port}/mcp` }
}

test('secret rows become credential references; plain rows stay in the patch row', () => {
  const input = mcpServerInputSchema.parse({
    serverName: 'exa', transport: 'streamable-http', url: 'https://mcp.example.com/mcp',
    headers: [{ key: 'x-api-key', value: 'secret-value', secret: true }, { key: 'X-Client', value: 'dsh' }],
  })
  const row = toPatchRow(input)
  assert.equal(row.name, 'dsh-skill-mcp-panel/mcp-client')
  assert.deepEqual(row.config.headers, { 'X-Client': 'dsh' })
  assert.deepEqual(row.config.secretHeaders, { 'x-api-key': credentialRefForSecret('exa', 'x-api-key') })
  assert.ok(!JSON.stringify(row).includes('secret-value'))
  assert.deepEqual(patchRowToView(row).headers, [
    { key: 'X-Client', value: 'dsh', secret: false },
    { key: 'x-api-key', secret: true },
  ])
  assert.deepEqual(inputFromPatchRow(row).headers.find(item => item.secret), { key: 'x-api-key', value: '', secret: true })
})

test('credential refs keep distinct keys distinct and are valid shell identifiers', () => {
  const refs = [credentialRefForSecret('a', 'b_c'), credentialRefForSecret('a_b', 'c'), credentialRefForSecret('exa', 'X-Api-Key')]
  assert.equal(new Set(refs).size, refs.length)
  for (const ref of refs) assert.match(ref, /^[A-Z_][A-Z0-9_]*$/)
})

test('duplicate keys are rejected', () => {
  const result = mcpServerInputSchema.safeParse({
    serverName: 'dup', transport: 'stdio', command: 'x',
    env: [{ key: 'A', value: '1' }, { key: 'A', value: '2', secret: true }],
  })
  assert.equal(result.success, false)
})

test('panel MCP entry sends the stored secret as x-api-key and restarts when it rotates', async () => {
  const dir = await mkdtemp(join(tmpdir(), 'dsh-panel-secret-'))
  const expectedKey = { current: 'first-key' }
  const { server, seenKeys, url } = await startKeyedMcpServer(expectedKey)
  const credentialsPath = join(dir, '.credentials.yaml')
  await writeFile(credentialsPath, '', { mode: 0o600 })
  const ctx = new Context()
  try {
    await ctx.plugin(CredentialsLocal, { path: credentialsPath, watch: false })
    await ctx.plugin(SystemPrompt)
    await ctx.plugin(Tools)
    const credentials = ctx.get('credentials')
    {
      const row = toPatchRow(mcpServerInputSchema.parse({
        serverName: 'fixture', transport: 'streamable-http', url,
        headers: [{ key: 'x-api-key', value: 'first-key', secret: true }],
        failOnStartupError: true, reconnect: { enabled: false },
      }))
      await credentials.set(credentialRef(row.config.secretHeaders['x-api-key']), 'first-key')
      assert.ok(!(await readFile(credentialsPath, 'utf8')).includes('fixture'))

      const fiber = ctx.plugin(panelMcpClient, row.config)
      await fiber
      assert.deepEqual(ctx.get('tools').schemas().map(tool => tool.name), ['mcp__fixture__echo'])
      const first = await ctx.get('tools').execute({ callId: 'c1', name: 'mcp__fixture__echo', arguments: {}, signal: AbortSignal.timeout(5000) })
      assert.equal(first.isError, false)
      assert.ok(seenKeys.length > 0 && seenKeys.every(key => key === 'first-key'))

      expectedKey.current = 'second-key'
      seenKeys.length = 0
      await credentials.set(credentialRef(row.config.secretHeaders['x-api-key']), 'second-key')
      await fiber.await()
      const second = await ctx.get('tools').execute({ callId: 'c2', name: 'mcp__fixture__echo', arguments: {}, signal: AbortSignal.timeout(5000) })
      assert.equal(second.isError, false)
      assert.ok(seenKeys.length > 0 && seenKeys.every(key => key === 'second-key'))
    }
  } finally {
    await ctx.fiber.dispose()
    server.closeAllConnections()
    await new Promise(resolve => server.close(resolve))
    await rm(dir, { recursive: true, force: true })
  }
})

test('panel MCP entry fails to start when a secret has no stored value', async () => {
  const dir = await mkdtemp(join(tmpdir(), 'dsh-panel-secret-'))
  const credentialsPath = join(dir, '.credentials.yaml')
  await writeFile(credentialsPath, '', { mode: 0o600 })
  const ctx = new Context()
  try {
    await ctx.plugin(CredentialsLocal, { path: credentialsPath, watch: false })
    await ctx.plugin(SystemPrompt)
    await ctx.plugin(Tools)
    const row = toPatchRow(mcpServerInputSchema.parse({
      serverName: 'missing', transport: 'streamable-http', url: 'http://127.0.0.1:9/mcp',
      headers: [{ key: 'x-api-key', value: 'unused', secret: true }],
    }))
    await assert.rejects(ctx.plugin(panelMcpClient, row.config).await(), /no stored value/)
  } finally {
    await ctx.fiber.dispose()
    await rm(dir, { recursive: true, force: true })
  }
})
