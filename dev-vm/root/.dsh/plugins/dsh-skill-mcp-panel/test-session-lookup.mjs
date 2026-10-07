// Run against a named playwright-cli browser connected to an isolated rc.2 Web fixture.
// Select a fixture workspace first, then set DSH_TEST_BROWSER to that browser name.
import test from 'node:test'
import assert from 'node:assert/strict'
import { execFile } from 'node:child_process'
import { promisify } from 'node:util'

const run = promisify(execFile)
test('skills panel sends the retained main session ID through the real rc.2 gateway', {
  skip: !process.env.DSH_TEST_BROWSER,
}, async () => {
  const script = `async page => {
    await page.getByRole('button', {name:'返回会话', exact:true}).click();
    const request = page.waitForRequest(r => r.url().endsWith('/api/skillsViewer/list') && r.method() === 'POST');
    await page.getByRole('button', {name:'技能', exact:true}).click();
    const body = (await request).postDataJSON();
    await page.getByRole('heading', {name:'技能', exact:true}).waitFor();
    if (typeof body.payload.args.sessionId !== 'string' || !body.payload.args.sessionId) throw new Error('Active session ID missing');
    return 'PASS: skills panel sends active session ID';
  }`
  const { stdout } = await run('playwright-cli', [`-s=${process.env.DSH_TEST_BROWSER}`, 'run-code', script], { timeout: 15000 })
  assert.ok(!stdout.includes('### Error'), stdout)
  assert.match(stdout, /PASS: skills panel sends active session ID/)
})
