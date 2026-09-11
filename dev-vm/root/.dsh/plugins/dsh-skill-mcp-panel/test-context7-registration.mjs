// Integration check against the installed DSH runtime and saved Context7 credential.
// Run with: node test-context7-registration.mjs
import assert from 'node:assert/strict';
import { createRequire } from 'node:module';
import { pathToFileURL } from 'node:url';
import { mcpServerInputSchema, toPatchRow, patchRowToView, inputFromPatchRow } from './lib/mcp/model.js';
const require = createRequire('/root/.dsh/profiles/web/package.json');
const load = async name => import(pathToFileURL(require.resolve(name)).href);
const { Context } = await load('@deepseek-ai/cordis');
const { default: SystemPrompt } = await load('@deepseek-ai/dsh-system-prompt');
const { default: Tools } = await load('@deepseek-ai/dsh-tools');
const { default: Credentials } = await load('@deepseek-ai/dsh-credentials-local');
const Mcp = await load('@deepseek-ai/dsh-mcp-client');
const input = mcpServerInputSchema.parse({serverName:'context7', transport:'streamable-http', url:'https://mcp.context7.com/mcp', authType:'bearer', failOnStartupError:true, reconnect:{enabled:false}});
const row = toPatchRow(input);
assert.equal(row.config.bearerTokenRef, 'MCP_636F6E7465787437_TOKEN');
assert.equal(row.config.headers.Authorization, undefined);
assert.equal(patchRowToView(row).authType, 'bearer');
assert.equal(inputFromPatchRow(row).authType, 'bearer');
const ctx = new Context();
try {
  await ctx.plugin(SystemPrompt).await();
  await ctx.plugin(Tools).await();
  await ctx.plugin(Credentials, {watch:false}).await();
  const fiber = ctx.plugin(Mcp, row.config);
  await fiber.await();
  const names = ctx.get('tools').schemas().map(tool => tool.name).sort();
  assert.deepEqual(names, ['mcp__context7__query-docs', 'mcp__context7__resolve-library-id']);
  await fiber.dispose();
  assert.equal(ctx.get('tools').schemas().length, 0);
  console.log('PASS: bearer reference survives panel round-trip; both MCP tools register and dispose.');
} finally {
  await ctx.fiber.dispose();
}
