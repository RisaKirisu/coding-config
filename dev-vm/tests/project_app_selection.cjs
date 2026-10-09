// Real selection and recovery regression against an owned native fixture.
const modulePath = process.env.PLAYWRIGHT_MODULE || 'playwright';
const {chromium} = require(modulePath);
const {expect} = require(modulePath + '/test');
const assert = require('node:assert/strict');
const fs = require('node:fs/promises');
const path = require('node:path');
const crypto = require('node:crypto');
const root = path.resolve(process.env.DEVVM_BROWSER_FIXTURE_ROOT || '');
assert(root.startsWith(path.resolve('.agents') + path.sep));
const output = path.join(root, 'selection-evidence');
const sanitize = text => String(text).replace(/([?&]token=)[^\s&"']+/g, '$1<REDACTED>').replace(/(cookie:|authorization:)[^\n]*/gi, '$1 <REDACTED>');
(async () => {
  const fixture = JSON.parse(await fs.readFile(path.join(root, 'ready.json'), 'utf8'));
  assert.equal(new URL(fixture.apiUrl).hostname, '127.0.0.1');
  assert(!['3080', '8100', '8101', '8102'].includes(new URL(fixture.apiUrl).port));
  await fs.mkdir(output, {recursive: true});
  const browser = await chromium.launch({channel: 'chrome'});
  const context = await browser.newContext({viewport: {width: 1280, height: 900}});
  const page = await context.newPage();
  const messages = [];
  const failures = [];
  await page.exposeFunction('recordSessionMessage', record => messages.push(record));
  await page.addInitScript(() => window.addEventListener('message', event => {
    const m = event.data;
    if (m?.protocol !== 'devvm-embed') return;
    if (['ready', 'selection-changed', 'select-session', 'result', 'connection-changed'].includes(m.kind)) window.recordSessionMessage({kind: m.kind, sessionId: m.payload?.sessionId, state: m.payload?.state, error: m.payload?.error, commandId: m.commandId});
  }));
  page.on('requestfailed', request => failures.push({path: new URL(request.url()).pathname, error: request.failure()?.errorText}));
  try {
    await page.goto(fixture.controlUrl);
    const project = fixture.projects[0];
    await page.locator(`[data-project-id="${project.id}"]`).click();
    const launch = page.getByRole('button', {name: 'Launch DSH', exact: true});
    await expect.poll(async () => await launch.isVisible() || await page.locator('.project-frame[data-active="true"]').count() > 0).toBe(true);
    if (await launch.isVisible()) await launch.click();
    const active = page.frameLocator('.project-frame[data-active="true"]');
    await page.addLocatorHandler(active.getByRole('button', {name: 'Continue', exact: true}), async button => button.click());
    await active.locator('[contenteditable="true"]').waitFor({timeout: 30000});
    await expect.poll(() => messages.some(message => message.kind === 'selection-changed' && message.sessionId)).toBe(true);
    const element = await page.locator('.project-frame[data-active="true"]').elementHandle();
    const child = await element.contentFrame();
    const origin = await child.evaluate(() => location.origin);
    async function call(method, args) {
      const target = new URL(origin + '/api/' + method);
      const host = target.host;
      target.hostname = '127.0.0.1';
      const cookie = (await context.cookies(origin)).map(item => item.name + '=' + item.value).join('; ');
      const response = await context.request.post(target.href, {headers: {Host: host, Cookie: cookie}, data: {type: 'client-request', rpcId: crypto.randomUUID(), method, payload: {args}}});
      const body = await response.json();
      assert.equal(body.result.ok, true, JSON.stringify(body.result));
      return body.result.value;
    }
    const {workspace} = await call('workspace/create', {request: {path: path.join(project.path, 'deepseek-harness/default-workspace')}});
    assert(workspace.workspaceId);
    const firstSession = messages.filter(message => message.kind === 'selection-changed' && message.sessionId).at(-1).sessionId;
    const documentId = await child.evaluate(() => window.__sessionWarningDocument = crypto.randomUUID());
    await context.setOffline(true);
    await expect(page.locator('#connection-status')).not.toHaveAttribute('data-state', 'connected', {timeout: 10000});
    const created = 'session-' + crypto.randomUUID();
    await call('session/create', {request: {sessionId: created, workspaceId: workspace.workspaceId}});
    const serverBefore = await call('session/list', {_request: {}});
    assert(serverBefore.items.some(item => item.sessionId === created));
    await page.evaluate(({project, session}) => {location.hash = `#/projects/${project}/sessions/${session}`; window.dispatchEvent(new PopStateEvent('popstate'));}, {project: project.id, session: created});
    await expect(page.locator('#notice-text')).toHaveText(/^Cannot verify this session:/, {timeout: 12000});
    await expect(page.locator('#notice')).toBeVisible();
    console.log('PASS: real lookup failure is retryable, not reported as deletion.');
    await context.setOffline(false);
    await expect(page.locator('#connection-status')).toHaveAttribute('data-state', 'connected', {timeout: 30000});
    await active.locator('[contenteditable="true"]').waitFor();
    assert.equal(await child.evaluate(() => window.__sessionWarningDocument), documentId);
    const serverAfter = await call('session/list', {_request: {}});
    assert(serverAfter.items.some(item => item.sessionId === created));
    await page.evaluate(({project, session}) => {location.hash = `#/projects/${project}/sessions/${session}`; window.dispatchEvent(new PopStateEvent('popstate'));}, {project: project.id, session: created});
    await expect.poll(() => messages.some(message => message.kind === 'selection-changed' && message.sessionId === created), {timeout: 15000}).toBe(true);
    await expect(page.locator('#notice')).toBeHidden();
    console.log('PASS: successful same-session recovery clears its selection warning.');
    const savedDraft = 'Selection recovery keeps this exact draft.';
    await active.locator('[contenteditable="true"]').fill(savedDraft);

    const missing = 'session-' + crypto.randomUUID();
    assert.equal(await call('session/projections', {request: {sessionId: missing}}), null);
    await page.evaluate(({project, session}) => {location.hash = `#/projects/${project}/sessions/${session}`; window.dispatchEvent(new PopStateEvent('popstate'));}, {project: project.id, session: missing});
    await expect(page.locator('#notice-text')).toHaveText('This session no longer exists in this Project.');
    await expect(page.locator('#notice')).toBeVisible();
    assert.equal(await call('session/projections', {request: {sessionId: missing}}), null);
    assert.equal(await child.evaluate(() => window.__sessionWarningDocument), documentId);
    console.log('PASS: genuinely missing target warns without creating that Session or replacing the document.');

    await page.route('**/api/projects', route => route.abort());
    await page.locator('#refresh-projects').click();
    await expect(page.locator('#notice-text')).toHaveText('Could not refresh Project status. Control Daemon unavailable.');
    await page.unroute('**/api/projects');
    const other = 'session-' + crypto.randomUUID();
    await call('session/create', {request: {sessionId: other, workspaceId: workspace.workspaceId}});
    await page.evaluate(({project, session}) => {location.hash = `#/projects/${project}/sessions/${session}`; window.dispatchEvent(new PopStateEvent('popstate'));}, {project: project.id, session: other});
    await expect.poll(() => messages.some(message => message.kind === 'selection-changed' && message.sessionId === other)).toBe(true);
    await expect(page.locator('#notice')).toBeVisible();
    await expect(page.locator('#notice-text')).toHaveText('Could not refresh Project status. Control Daemon unavailable.');
    console.log('PASS: successful selection preserves an unrelated daemon-error notice.');

    await page.evaluate(({project, session}) => {location.hash = `#/projects/${project}/sessions/${session}`; window.dispatchEvent(new PopStateEvent('popstate'));}, {project: project.id, session: created});
    await expect(active.locator('[contenteditable="true"]')).toHaveText(savedDraft);
    await expect(page.locator('#notice-text')).toHaveText('Could not refresh Project status. Control Daemon unavailable.');
    console.log('PASS: exact draft survives missing selection and valid-session switches.');

    const lateMissing = 'session-' + crypto.randomUUID();
    const lookupStarted = Promise.withResolvers();
    const releaseLookup = Promise.withResolvers();
    await page.route('**/api/session/projections', async route => {
      const request = route.request();
      if (request.postDataJSON()?.payload?.args?.request?.sessionId !== lateMissing) return route.continue();
      const target = new URL(request.url());
      const host = target.host;
      target.hostname = '127.0.0.1';
      const response = await route.fetch({url: target.href, headers: {...request.headers(), host}});
      assert.equal((await response.json()).result.value, null);
      lookupStarted.resolve();
      await releaseLookup.promise;
      await route.fulfill({response});
    });
    await page.evaluate(({project, session}) => {location.hash = `#/projects/${project}/sessions/${session}`; window.dispatchEvent(new PopStateEvent('popstate'));}, {project: project.id, session: lateMissing});
    await Promise.race([lookupStarted.promise, new Promise((_, reject) => setTimeout(() => reject(new Error('Expected real availability lookup did not start.')), 10000))]);
    const secondProject = fixture.projects[1];
    await page.locator(`[data-project-id="${secondProject.id}"]`).click();
    releaseLookup.resolve();
    await expect(page.locator('#project-name')).toHaveText(path.basename(secondProject.path));
    await expect(page.locator('#notice-text')).toHaveText('Could not refresh Project status. Control Daemon unavailable.');
    await expect(page.locator('#notice')).toBeVisible();
    await page.unroute('**/api/session/projections');
    console.log('PASS: delayed real missing response from the previous selection cannot overwrite the visible Project’s notice.');

    const startupContext = await browser.newContext({viewport: {width: 1280, height: 900}});
    const startup = await startupContext.newPage();
    const startupSelections = [];
    await startup.exposeFunction('recordStartupSelection', sessionId => startupSelections.push(sessionId));
    await startup.addInitScript(() => window.addEventListener('message', event => {
      if (event.data?.protocol === 'devvm-embed' && event.data.kind === 'selection-changed') window.recordStartupSelection(event.data.payload.sessionId);
    }));
    await startup.goto(fixture.controlUrl + '#/projects/' + project.id + '/sessions/' + created);
    const startupFrame = startup.frameLocator('.project-frame[data-active="true"]');
    await startup.addLocatorHandler(startupFrame.getByRole('button', {name: 'Continue', exact: true}), async button => button.click());
    await expect.poll(() => startupSelections.includes(created), {timeout: 30000}).toBe(true);
    await expect(startupFrame.locator('[contenteditable="true"]')).toBeVisible();
    await expect(startup.locator('#notice')).toBeHidden();
    await expect.poll(() => new URL(startup.url()).hash.endsWith('/sessions/' + created)).toBe(true);
    await startupContext.close();
    console.log('PASS: fresh-page ready handshake restores the requested existing Session without a false warning.');

    await page.screenshot({path: path.join(output, 'refresh-failure.png')});
    await fs.writeFile(path.join(output, 'refresh-failure.json'), JSON.stringify({firstSession, created, existsBefore: true, existsAfter: true, messages, failures, warning: await page.locator('#notice-text').textContent()}, null, 2));
  } catch (error) {
    await page.screenshot({path: path.join(output, 'failure.png')});
    await fs.writeFile(path.join(output, 'failure.json'), JSON.stringify({error: sanitize(error.message), hash: new URL(page.url()).hash, messages, parentText: await page.locator('body').innerText()}, null, 2));
    throw error;
  } finally {await browser.close();}
})().catch(error => {console.error(sanitize(error.message)); process.exitCode = 1;});
