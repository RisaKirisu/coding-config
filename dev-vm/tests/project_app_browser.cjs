// Run against the explicitly owned real-runtime fixture, never a deployed daemon.
const playwrightModule = process.env.PLAYWRIGHT_MODULE || 'playwright';
const {chromium} = require(playwrightModule);
const {expect} = require(playwrightModule + '/test');
const assert = require('node:assert/strict');
const fs = require('node:fs/promises');
const path = require('node:path');

const fixtureRoot = process.env.DEVVM_BROWSER_FIXTURE_ROOT;
assert(fixtureRoot && path.resolve(fixtureRoot).startsWith(path.resolve('.agents') + path.sep), 'Use an owned .agents fixture root.');
const output = path.join(fixtureRoot, 'browser-evidence');
const sizes = [[1440,1000],[360,800],[412,915],[344,882],[768,900],[800,1280],[1280,800],[600,900],[412,500]];
const sanitize = text => String(text).replace(/([?&]token=)[^\s&"']+/g, '$1[redacted]').replace(/(cookie: )[^\n]+/gi, '$1[redacted]');

(async () => {
  const fixture = JSON.parse(await fs.readFile(path.join(fixtureRoot, 'ready.json'), 'utf8'));
  const apiUrl = new URL(fixture.apiUrl);
  assert.equal(apiUrl.hostname, '127.0.0.1');
  assert(!['3080','8100','8101','8102'].includes(apiUrl.port));
  await fs.mkdir(output, {recursive:true});
  const browserProfile = await fs.mkdtemp(path.join(output, 'chromium-profile-'));
  let context;
  let page;
  const errors = [];
  const results = [];
  const bridgeMessages = [];
  const rpcRequests = [];
  async function startBrowser() {
    context = await chromium.launchPersistentContext(browserProfile, {
      channel:'chrome', viewport:{width:1440,height:1000}, hasTouch:true,
    });
    page = context.pages()[0];
    await page.addLocatorHandler(
      page.frameLocator('.project-frame[data-active="true"]').getByRole('button', {name:'Continue',exact:true}),
      async button => { await button.click(); },
    );
    await page.exposeFunction('recordBridgeMessage', message => bridgeMessages.push(message));
    await page.addInitScript(() => window.addEventListener('message', event => {
      const message = event.data;
      if (message?.protocol === 'devvm-embed' && (
        ['selection-changed','draft-load','draft-save'].includes(message.kind) ||
        message.kind === 'result' && message.payload?.error
      )) window.recordBridgeMessage(message);
    }));
    page.on('pageerror', error => errors.push(sanitize(error.stack)));
    page.on('request', request => {
      const url = new URL(request.url());
      if (/^\/api\/(session|workspace|projects)\//.test(url.pathname) && request.method() === 'POST') {
        rpcRequests.push({path:url.pathname, body:request.postData()});
      }
    });
  }
  await startBrowser();
  const projectA = fixture.projects[0];
  const projectB = fixture.projects[1];
  const active = () => page.frameLocator('.project-frame[data-active="true"]');
  async function openProject(project) {
    const row = page.locator(`[data-project-id="${project.id}"]`);
    await row.waitFor({state:'attached'});
    if (await page.locator('.app').getAttribute('data-projects-open') !== 'true') await page.locator('#open-projects').click();
    await row.click();
    await expect(page.locator('#project-name')).toHaveText(path.basename(project.path));
    await page.locator('#placeholder-action:visible, .project-frame[data-active="true"]').first().waitFor();
    const launch = page.getByRole('button',{name:'Launch DSH',exact:true});
    if (await launch.count()) await launch.click();
    await expect.poll(async () => page.locator('.project-frame[data-active="true"]').evaluate(frame => new URL(frame.src).hostname), {timeout:30000}).toBe(`3080.${project.host}.devvm.localhost`);
    await active().locator('[data-composer-input]').waitFor({timeout:30000});
    await active().locator('[contenteditable="true"]').waitFor({timeout:25000});
  }
  async function restartSelectedProject(project) {
    const frame = await page.locator('.project-frame[data-active="true"]').elementHandle();
    const nativeRoot = await active().locator('#root').elementHandle();
    const hash = new URL(page.url()).hash;
    const draft = await active().locator('[data-composer-input]').innerText();
    await runSelectedAction(project,'Restart DSH');
    await expect(page.locator('#connection-status')).toHaveAttribute('data-state', 'connected', {timeout:30000});
    await active().locator('[contenteditable="true"]').waitFor({timeout:30000});
    await expect(active().locator('[data-composer-input]')).toHaveText(draft);
    assert.equal(new URL(page.url()).hash, hash);
    assert(await frame.evaluate(element => element.isConnected));
    assert(await nativeRoot.evaluate(element => element.isConnected));
    results.push({check:'native-restart-retains-frame-root-route-and-draft',passed:true});
  }
  async function runSelectedAction(project,label) {
    const receipt = page.waitForResponse(response => response.url() === new URL(`/api/projects/${project.id}/operations`, fixture.controlUrl).href && response.request().method() === 'POST');
    await page.locator('#project-actions').click();
    await page.locator('#action-list').getByRole('button', {name:label,exact:true}).click();
    if (await page.locator('#confirmation-dialog').isVisible()) await page.locator('#confirm-action').click();
    const operation = await (await receipt).json();
    await expect.poll(async () => {
      const response = await context.request.get(`${fixture.apiUrl}/api/projects/${project.id}/operations`);
      const snapshot = await response.json();
      return [snapshot.active, ...snapshot.recent].find(item => item?.id === operation.id)?.state;
    }, {timeout:30000}).toBe('succeeded');
    await page.getByRole('button', {name:'Close Project actions',exact:true}).click();
  }
  async function fillDraft(text) {
    const editor = active().locator('[data-composer-input]');
    await editor.click();
    await editor.fill(text);
  }
  async function stored(store) {
    return page.evaluate(async store => {
      const db = await new Promise((resolve,reject) => {
        const request = indexedDB.open('devvm-text');
        request.onsuccess = () => resolve(request.result);
        request.onerror = () => reject(request.error);
      });
      const records = await new Promise((resolve,reject) => {
        const request = db.transaction(store).objectStore(store).getAll();
        request.onsuccess = () => resolve(request.result);
        request.onerror = () => reject(request.error);
      });
      db.close();
      return records;
    },store);
  }
  async function waitDraft(text, projectId=projectA.id) {
    await expect.poll(async () => (await stored('drafts')).filter(item=>item.projectId===projectId).some(item=>item.text===text)).toBe(true);
  }
  try {
    await page.goto(fixture.controlUrl);
    await openProject(projectA);
    const editor = active().locator('[data-composer-input]');
    const draft = 'A saved draft: rotation, split screen, and constrained height.';
    await fillDraft(draft);
    await waitDraft(draft);
    const bridgeAddress = bridgeMessages.findLast(message => message.kind === 'draft-save' && message.projectId === projectA.id);
    assert(bridgeAddress);
    const forged = {
      ...bridgeAddress, commandId:crypto.randomUUID(),
      payload:{...bridgeAddress.payload,text:'Unauthenticated draft overwrite',revision:100000},
    };
    const bridgeFrame = await (await page.locator('.project-frame[data-active="true"]').elementHandle()).contentFrame();
    await page.evaluate(message => window.postMessage(message,location.origin),forged);
    for (const message of [
      {...forged,version:2},
      {...forged,projectId:crypto.randomUUID()},
      {...forged,channelId:crypto.randomUUID()},
      {...forged,instanceId:crypto.randomUUID()},
      {...forged,payload:{...forged.payload,revision:-1}},
    ]) await bridgeFrame.evaluate(({message,origin}) => parent.postMessage(message,origin),{message,origin:new URL(fixture.controlUrl).origin});
    // A valid later edit runs through the same message channel after all rejected messages.
    await fillDraft(`${draft} Verified bridge boundary.`);
    await waitDraft(`${draft} Verified bridge boundary.`);
    assert(!(await stored('drafts')).some(record => record.text === forged.payload.text));
    await fillDraft(draft);
    await waitDraft(draft);
    results.push({check:'bridge-source-project-channel-instance-and-schema-rejection',passed:true});
    const iframe = await page.locator('.project-frame[data-active="true"]').elementHandle();
    const nativeFrame = await iframe.contentFrame();
    const nativeRoot = await nativeFrame.locator('#root').elementHandle();
    for (const [width,height] of sizes) {
      await editor.focus();
      await page.setViewportSize({width,height});
      await expect(editor).toHaveText(draft);
      await expect(editor).toBeInViewport({ratio:1});
      assert(await page.locator('.project-frame[data-active="true"]').evaluate((element,original)=>element===original,iframe));
      assert(await nativeFrame.locator('#root').evaluate((element,original)=>element===original,nativeRoot));
      const outer = await page.evaluate(()=>({width:innerWidth,height:innerHeight,scroll:document.documentElement.scrollWidth,app:document.querySelector('.app').getBoundingClientRect().toJSON()}));
      const inner = await editor.evaluate(element=>({width:innerWidth,height:innerHeight,scroll:document.documentElement.scrollWidth,rect:element.getBoundingClientRect().toJSON(),font:parseFloat(getComputedStyle(element).fontSize),focused:document.activeElement===element}));
      assert(outer.scroll <= width+1,`Outer overflow at ${width}×${height}`);
      assert(inner.scroll <= inner.width+1,`Inner overflow at ${width}×${height}`);
      assert(inner.rect.right<=inner.width+1 && inner.rect.left>=-1);
      assert(inner.rect.bottom<=inner.height+1 && inner.rect.top>=-1,`Composer outside viewport at ${width}×${height}`);
      assert(inner.font>=16,`Composer font ${inner.font}px at ${width}×${height}`);
      assert(inner.focused,`Composer lost focus at ${width}×${height}`);
      await page.screenshot({path:path.join(output,`viewport-${width}x${height}.png`)});
      results.push({check:'viewport',width,height,composer:inner.rect,font:inner.font});
      await active().getByRole('button', {name: 'Open DSH sidebar', exact: true}).click();
      await expect(active().getByRole('button',{name:/^New session/i}).first()).toBeVisible();
      await page.locator('#project-title').click();
      await expect.poll(() => page.locator('#projects-panel').evaluate(element => element.inert)).toBe(false);
      if (width < 960) await page.locator('#close-projects').click();
    }
    const display = await context.newCDPSession(page);
    // Chrome 154's metrics emulator takes segments from its own combined parameters.
    await display.send('Emulation.setDeviceMetricsOverride',{width:800,height:900,screenWidth:800,screenHeight:900,deviceScaleFactor:1,mobile:true,displayFeature:{orientation:'vertical',offset:390,maskLength:20}});
    await expect.poll(() => page.evaluate(() => matchMedia('(horizontal-viewport-segments: 2)').matches)).toBe(true);
    await expect.poll(async () => (await page.locator('#workspace').boundingBox()).x).toBe(410);
    const segmentsPanel = await page.locator('#projects-panel').boundingBox();
    assert(segmentsPanel.x + segmentsPanel.width <= 390);
    await expect(editor).toHaveText(draft);
    assert(await iframe.evaluate(element => element.isConnected));
    assert(await nativeRoot.evaluate(element => element.isConnected));
    await page.screenshot({path:path.join(output,'vertical-hinge.png')});
    await display.send('Emulation.setDeviceMetricsOverride',{width:412,height:882,screenWidth:412,screenHeight:882,deviceScaleFactor:1,mobile:true,displayFeature:{orientation:'horizontal',offset:420,maskLength:20}});
    await expect.poll(() => page.evaluate(() => matchMedia('(vertical-viewport-segments: 2)').matches)).toBe(true);
    await expect.poll(async () => (await page.locator('#app').boundingBox()).y).toBe(440);
    const lowerSegment = await page.locator('#app').boundingBox();
    assert(lowerSegment.y + lowerSegment.height <= 883);
    await expect(editor).toBeInViewport({ratio:1});
    await expect(editor).toHaveText(draft);
    await page.screenshot({path:path.join(output,'horizontal-hinge.png')});
    await display.send('Emulation.clearDeviceMetricsOverride');
    await display.detach();
    await page.setViewportSize({width:412,height:500});
    await page.evaluate(() => document.documentElement.style.fontSize='20px');
    await nativeFrame.evaluate(() => document.documentElement.style.fontSize='20px');
    await expect.poll(() => editor.evaluate(element => parseFloat(getComputedStyle(element).fontSize))).toBe(20);
    await expect(editor).toHaveText(draft);
    assert(await page.evaluate(() => document.documentElement.scrollWidth <= innerWidth + 1));
    assert(await nativeFrame.evaluate(() => document.documentElement.scrollWidth <= innerWidth + 1));
    await page.screenshot({path:path.join(output,'enlarged-text.png')});
    await page.evaluate(() => document.documentElement.style.fontSize='');
    await nativeFrame.evaluate(() => document.documentElement.style.fontSize='');
    results.push({check:'reported-hinge-segments-and-enlarged-text',passed:true});
    await page.setViewportSize({width:1440,height:1000});
    const hashA = new URL(page.url()).hash;
    assert(hashA.includes('/sessions/'),'Native selection must reach the Control route.');
    await openProject(projectB);
    await fillDraft('Independent Project B draft');
    await waitDraft('Independent Project B draft',projectB.id);
    await openProject(projectA);
    await expect(active().locator('[data-composer-input]')).toHaveText(draft);
    assert.equal(new URL(page.url()).hash,hashA);
    results.push({check:'project-draft-isolation',passed:true});
    await page.reload();
    await active().locator('[contenteditable="true"]').waitFor({timeout:30000});
    await expect(active().locator('[data-composer-input]')).toHaveText(draft);
    results.push({check:'reload-draft-restore',passed:true});
    await context.close();
    await startBrowser();
    await page.goto(`${fixture.controlUrl}${hashA}`);
    await active().locator('[contenteditable="true"]').waitFor({timeout:30000});
    await expect(active().locator('[data-composer-input]')).toHaveText(draft);
    results.push({check:'browser-process-restart-draft-restore',passed:true});

    await restartSelectedProject(projectA);
    const prompt = '只回复 MESSAGES_WEB_READY，不调用工具。';
    await fillDraft(prompt);
    await waitDraft(prompt);
    await active().locator('[data-composer-input]').press('Enter');
    await expect(active().getByText('MESSAGES_WEB_READY',{exact:true})).toBeVisible({timeout:30000});
    await expect.poll(async()=> (await stored('pending')).length).toBe(0);
    results.push({check:'native-prompt-and-replay-response',passed:true});
    await page.setViewportSize({width:412,height:500});
    const messageFont = await active().getByText(prompt,{exact:true}).evaluate(element => parseFloat(getComputedStyle(element).fontSize));
    assert(messageFont >= 16,`Mobile message text is ${messageFont}px`);
    const panelControl = await active().getByRole('button',{name:'Open right sidebar',exact:true}).boundingBox();
    assert(panelControl.width >= 44 && panelControl.height >= 44);
    await active().getByRole('button',{name:'Open right sidebar',exact:true}).tap();
    await expect(active().locator('[data-sidebar-right-panel="fullscreen"][data-sidebar-right-open]')).toBeVisible();
    const panel = await active().locator('[data-sidebar-right-panel="fullscreen"][data-sidebar-right-open]').boundingBox();
    assert(panel.width > 350 && panel.width <= 412);
    await active().getByRole('button',{name:'Collapse right sidebar',exact:true}).tap();
    await expect(active().locator('[data-sidebar-right-panel][data-sidebar-right-open]')).toHaveCount(0);
    const longDraft = Array.from({length:60},(_,index) => `Draft line ${index}: phone keyboard and long text.`).join('\n');
    await fillDraft(longDraft);
    await waitDraft(longDraft);
    await expect(active().locator('[data-composer-input]')).toBeVisible();
    const inputBounds = await active().locator('[data-composer-input]').boundingBox();
    assert(inputBounds.width > 300 && inputBounds.width <= 412);
    await fillDraft('');
    await waitDraft('');
    await page.setViewportSize({width:1440,height:1000});
    results.push({check:'touch-native-panel-and-constrained-height-long-draft',passed:true});

    const previousRoute = new URL(page.url()).hash;
    await active().getByRole('button', {name: 'Open DSH sidebar', exact: true}).click();
    await active().getByRole('button', {name: /^New session/i}).first().click();
    await expect.poll(() => new URL(page.url()).hash).not.toBe(previousRoute);
    await active().locator('[contenteditable="true"]').waitFor();
    await restartSelectedProject(projectA);
    let overlappingRequestId;
    const overlapComplete = Promise.withResolvers();
    await page.route('**/api/session/prompt', async route => {
      if (overlappingRequestId) { await route.continue(); return; }
      const original = route.request().postDataJSON();
      overlappingRequestId = original.payload.args.request.requestId;
      const duplicate = {...original, rpcId: crypto.randomUUID()};
      overlapComplete.resolve(route.request().frame().evaluate(async request => {
        const response = await fetch('/api/session/prompt', {
          method:'POST', headers:{'content-type':'application/json'}, body:JSON.stringify(request),
        });
        if (!response.ok) throw new Error(`Duplicate native admission returned HTTP ${response.status}`);
        return response.json();
      }, duplicate));
      await route.continue();
    });
    await fillDraft(prompt);
    await waitDraft(prompt);
    await active().locator('[data-composer-input]').press('Enter');
    await expect.poll(() => overlappingRequestId, {timeout:30000}).toBeTruthy();
    await overlapComplete.promise;
    await expect(active().getByText('MESSAGES_WEB_READY', {exact:true})).toBeVisible({timeout:30000});
    await expect.poll(async () => (await stored('pending')).length).toBe(0);
    assert(overlappingRequestId);
    await page.unroute('**/api/session/prompt');
    await page.reload();
    await active().locator('[contenteditable="true"]').waitFor({timeout:30000});
    await expect(active().getByText(prompt, {exact:true})).toHaveCount(1);
    results.push({check:'overlapping-native-request-admission', requestId:overlappingRequestId, passed:true});
    // Preserve rendered history/title and the same document through real 401 reauthorization.
    const titleBeforeReauth = await page.locator('#session-title').innerText();
    await active().locator('body').evaluate((body, text) => {
      window.recoveryHistoryLoss = 0;
      new MutationObserver(() => { if (!body.innerText.includes(text)) window.recoveryHistoryLoss++; })
        .observe(body, {childList:true,subtree:true,characterData:true});
    }, prompt);
    await page.locator('#session-title').evaluate(element => {
      const title = element.textContent;
      window.recoveryTitleLoss = 0;
      new MutationObserver(() => { if (element.textContent !== title) window.recoveryTitleLoss++; })
        .observe(element, {childList:true,subtree:true,characterData:true});
    });
    const projectOrigin = await page.locator('.project-frame[data-active="true"]').evaluate(element => new URL(element.src).origin);
    const authResponses = [];
    const record401 = response => { if (response.status() === 401 && new URL(response.url()).origin === projectOrigin) authResponses.push(response.status()); };
    page.on('response', record401);
    await context.clearCookies({domain:new URL(projectOrigin).hostname});
    await restartSelectedProject(projectA);
    await expect(active().getByText(prompt,{exact:true})).toHaveCount(1);
    await expect(page.locator('#session-title')).toHaveText(titleBeforeReauth);
    assert.equal(await active().locator('body').evaluate(() => window.recoveryHistoryLoss), 0);
    assert.equal(await page.evaluate(() => window.recoveryTitleLoss), 0);
    assert(authResponses.length > 0, 'Reauthorization must observe a real native HTTP 401');
    page.off('response', record401);
    results.push({check:'real-401-reauthorization-retains-visible-history-and-title',passed:true});

    // Drop the browser response only after native acceptance, then let the original target settle in the background.
    const historyRoute = new URL(page.url()).hash;
    await active().getByRole('button', {name: 'Open DSH sidebar', exact: true}).click();
    await active().getByRole('button',{name:/^New session/i}).first().click();
    await expect.poll(() => new URL(page.url()).hash).not.toBe(historyRoute);
    await active().locator('[contenteditable="true"]').waitFor();
    await restartSelectedProject(projectA);
    const pendingRoute = new URL(page.url()).hash;
    const targetSession = decodeURIComponent(pendingRoute.split('/').at(-1));
    const lostAck = await context.newCDPSession(page);
    let lostRequestId;
    const acceptedResponse = Promise.withResolvers();
    const transportErrors = [];
    lostAck.on('Fetch.requestPaused', async event => {
      try {
        if (event.responseErrorReason) {
          await lostAck.send('Fetch.continueRequest',{requestId:event.requestId});
          return;
        }
        const request = JSON.parse(event.request.postData);
        lostRequestId = request.payload.args.request.requestId;
        assert.equal(request.payload.args.request.sessionId,targetSession);
        assert.equal(event.responseStatusCode,200);
        await lostAck.send('Fetch.failRequest',{requestId:event.requestId,errorReason:'Aborted'});
        acceptedResponse.resolve();
      } catch(error) { transportErrors.push(error.message); acceptedResponse.reject(error); }
    });
    await lostAck.send('Fetch.enable',{patterns:[{urlPattern:'*/api/session/prompt',requestStage:'Response'}]});
    let releaseRetries = false;
    const retryIdentities = [];
    await page.route('**/api/session/prompt', async route => {
      if (!lostRequestId || releaseRetries) { await route.continue(); return; }
      const request = route.request().postDataJSON().payload.args.request;
      retryIdentities.push({requestId:request.requestId,sessionId:request.sessionId});
      await route.abort('connectionfailed');
    });
    await fillDraft(prompt);
    await waitDraft(prompt);
    await active().locator('[data-composer-input]').press('Enter');
    await expect.poll(() => lostRequestId,{timeout:30000}).toBeTruthy();
    await acceptedResponse.promise;
    await expect.poll(async () => (await stored('pending')).length).toBe(1);
    const pending = (await stored('pending'))[0];
    assert.equal(pending.projectId,projectA.id);
    assert.equal(pending.sessionId,targetSession);
    assert.equal(pending.requestId,lostRequestId);
    await restartSelectedProject(projectA);
    await openProject(projectB);
    const backgroundDraft = 'Project B stays visible while Project A settles';
    await fillDraft(backgroundDraft);
    await waitDraft(backgroundDraft,projectB.id);
    await lostAck.send('Fetch.disable');
    await lostAck.detach();
    const backgroundRoute = new URL(page.url()).hash;
    assert.equal((await stored('pending'))[0].requestId,lostRequestId);
    await context.close();
    await startBrowser();
    await page.goto(`${fixture.controlUrl}${backgroundRoute}`);
    await active().locator('[contenteditable="true"]').waitFor({timeout:30000});
    releaseRetries = true;
    await expect.poll(async () => (await stored('pending')).length,{timeout:30000}).toBe(0);
    await expect(page.locator('#project-name')).toHaveText(path.basename(projectB.path));
    await expect(active().locator('[data-composer-input]')).toHaveText(backgroundDraft);
    assert.deepEqual(transportErrors,[]);
    assert(retryIdentities.every(item => item.requestId === lostRequestId && item.sessionId === targetSession));
    await page.unroute('**/api/session/prompt');
    await openProject(projectA);
    assert.equal(new URL(page.url()).hash,pendingRoute);
    await expect(active().getByText(prompt,{exact:true})).toHaveCount(1);
    await page.reload();
    await active().locator('[contenteditable="true"]').waitFor({timeout:30000});
    await expect(active().getByText(prompt,{exact:true})).toHaveCount(1);
    results.push({check:'lost-native-response-exact-target-background-delivery',requestId:lostRequestId,passed:true});
    results.push({check:'browser-process-restart-pending-original-target',requestId:lostRequestId,passed:true});
    results.push({check:'accepted-id-recovery-after-native-runtime-restart',requestId:lostRequestId,passed:true});

    // The owned Session log disappears while DSH is stopped; retry after restoring that exact log.
    let blockedRequest;
    await page.route('**/api/session/prompt', async route => {
      blockedRequest = route.request().postDataJSON().payload.args.request;
      await route.abort('connectionfailed');
    });
    await fillDraft(prompt);
    await waitDraft(prompt);
    await active().locator('[data-composer-input]').press('Enter');
    await expect.poll(async () => (await stored('pending')).length).toBe(1);
    await expect.poll(() => blockedRequest).toBeTruthy();
    await runSelectedAction(projectA,'Stop DSH');
    const sessionRoot = await fs.realpath(path.join(projectA.path,'.dsh-home/sessions'));
    assert(sessionRoot.startsWith(`${await fs.realpath(fixtureRoot)}/projects/`));
    const cwdDirectories = await fs.readdir(sessionRoot);
    const containing = [];
    for (const cwd of cwdDirectories) {
      const entries = await fs.readdir(path.join(sessionRoot,cwd));
      if (entries.includes(blockedRequest.sessionId)) containing.push(cwd);
    }
    assert.equal(containing.length,1);
    const sessionDirectory = await fs.realpath(path.join(sessionRoot,containing[0],blockedRequest.sessionId));
    const quarantine = path.resolve(output,`quarantined-${blockedRequest.sessionId}`);
    assert(sessionDirectory.startsWith(`${sessionRoot}/`));
    assert.equal(path.basename(sessionDirectory),blockedRequest.sessionId);
    assert.equal(path.dirname(quarantine),path.resolve(output));
    await fs.rename(sessionDirectory,quarantine);
    await page.unroute('**/api/session/prompt');
    try {
      await runSelectedAction(projectA,'Launch DSH');
      await expect(page.locator('#connection-status')).toHaveAttribute('data-state','connected',{timeout:30000});
      await expect.poll(async () => (await stored('pending'))[0]?.state,{timeout:30000}).toBe('blocked');
      const record = (await stored('pending'))[0];
      assert.equal(record.projectId,projectA.id);
      assert.equal(record.sessionId,blockedRequest.sessionId);
      assert.equal(record.requestId,blockedRequest.requestId);
      assert.equal(record.text,prompt);
      await page.locator('#open-saved-text').click();
      await expect(page.locator('#saved-text-dialog textarea')).toHaveValue(prompt);
      await page.getByRole('button',{name:'Close saved text',exact:true}).click();
    } finally {
      await runSelectedAction(projectA,'Stop DSH');
      assert.equal(await fs.realpath(quarantine),quarantine);
      assert(sessionDirectory.startsWith(`${sessionRoot}/`));
      await fs.rename(quarantine,sessionDirectory);
    }
    await runSelectedAction(projectA,'Launch DSH');
    await expect(page.locator('#connection-status')).toHaveAttribute('data-state','connected',{timeout:30000});
    const nativePromptsBeforeRetry = rpcRequests.filter(item => item.path === '/api/session/prompt').length;
    await page.locator('#open-saved-text').click();
    await page.locator('#saved-text-dialog').getByRole('button',{name:'Try again',exact:true}).click();
    await expect.poll(async () => (await stored('pending')).length,{timeout:30000}).toBe(0);
    const retryCalls = rpcRequests.filter(item => item.path === '/api/session/prompt').slice(nativePromptsBeforeRetry);
    assert(retryCalls.length > 0);
    for (const call of retryCalls) {
      const request = JSON.parse(call.body).payload.args.request;
      assert.equal(request.sessionId,blockedRequest.sessionId);
      assert.equal(request.requestId,blockedRequest.requestId);
    }
    await page.getByRole('button',{name:'Close saved text',exact:true}).click();
    await expect(active().getByText(prompt,{exact:true})).toHaveCount(2);
    results.push({check:'real-missing-session-block-and-explicit-same-id-retry',requestId:blockedRequest.requestId,passed:true});

    // A real Chromium quota failure must retain text and prevent native admission.
    const quota = await context.newCDPSession(page);
    const nativePromptsBeforeQuota = rpcRequests.filter(item => item.path === '/api/session/prompt').length;
    const quotaDraft = `Draft retained after quota failure: ${crypto.randomUUID()}`;
    if (await page.locator('#notice').isVisible()) await page.locator('#dismiss-notice').click();
    await quota.send('Storage.overrideQuotaForOrigin', {origin:new URL(fixture.controlUrl).origin, quotaSize:1});
    const quotaState = await quota.send('Storage.getUsageAndQuota',{origin:new URL(fixture.controlUrl).origin});
    assert.equal(quotaState.quota,1);
    assert.equal(quotaState.overrideActive,true);
    // Chromium 154 caches bucket write allowance for 30 seconds; use real elapsed time.
    await new Promise(resolve => setTimeout(resolve,31000));
    await fillDraft(quotaDraft);
    await expect(page.locator('#notice')).toBeVisible({timeout:30000});
    assert(!(await stored('drafts')).some(item => item.text === quotaDraft));
    await active().locator('[data-composer-input]').press('Enter');
    await expect(active().locator('[data-composer-input]')).toHaveText(quotaDraft);
    assert.equal(rpcRequests.filter(item => item.path === '/api/session/prompt').length, nativePromptsBeforeQuota);
    await expect.poll(async () => (await stored('pending')).length).toBe(0);
    await quota.send('Storage.overrideQuotaForOrigin', {origin:new URL(fixture.controlUrl).origin});
    await quota.detach();
    await fillDraft('Draft saves again after a real quota failure');
    await waitDraft('Draft saves again after a real quota failure');
    results.push({check:'real-indexeddb-quota-failure-and-recovery',passed:true});

    await page.screenshot({path:path.join(output,'final.png')});
    await fs.writeFile(path.join(output,'rpc-shapes.json'),JSON.stringify(rpcRequests,null,2));
    await fs.writeFile(path.join(output,'results.json'),JSON.stringify({results,errors},null,2));
    assert.deepEqual(errors,[],'Browser errors occurred.');
    console.log(JSON.stringify({results,errors},null,2));
  } catch (error) {
    console.error(sanitize(error.stack));
    await fs.writeFile(path.join(output,'failure-summary.json'),JSON.stringify({error:sanitize(error.stack),results,errors},null,2));
    await page.screenshot({path:path.join(output,'failure.png'),timeout:5000}).catch(screenshotError => console.error(screenshotError.message));
    console.error(JSON.stringify({outer:(await page.locator('body').innerText()).slice(0,3000),inner:(await active().locator('body').innerText().catch(()=>'')).slice(0,3000),errors},null,2));
    await fs.writeFile(path.join(output,'failure.json'),JSON.stringify({error:sanitize(error.stack),results,errors,rpcRequests,bridgeMessages,drafts:await stored('drafts'),quota:await page.evaluate(() => navigator.storage.estimate()),route:new URL(page.url()).hash},null,2));
    process.exitCode=1;
  } finally {
    await context.close();
  }
})();
