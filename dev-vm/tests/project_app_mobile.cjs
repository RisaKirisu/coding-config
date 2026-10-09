// Run only against the owned real-runtime fixture described in docs/project-app.md.
const {chromium} = require(process.env.PLAYWRIGHT_MODULE || 'playwright');
const {expect} = require((process.env.PLAYWRIGHT_MODULE || 'playwright') + '/test');
const assert = require('node:assert/strict');
const fs = require('node:fs/promises');
const path = require('node:path');
const root = path.resolve(process.env.DEVVM_BROWSER_FIXTURE_ROOT || '');
assert(root.startsWith(path.resolve('.agents') + path.sep));

(async () => {
  const fixture = JSON.parse(await fs.readFile(path.join(root, 'ready.json'), 'utf8'));
  assert.equal(new URL(fixture.apiUrl).hostname, '127.0.0.1');
  assert(!['3080', '8100', '8101', '8102'].includes(new URL(fixture.apiUrl).port));
  await fs.mkdir(path.join(root, 'mobile-evidence'), {recursive: true});
  const browser = await chromium.launch({channel: 'chrome'});
  const context = await browser.newContext({viewport: {width: 412, height: 915}, hasTouch: true, isMobile: true});
  const page = await context.newPage();
  const results = [];
  const check = async (name, run) => {
    try { await run(); results.push({name, passed: true}); }
    catch (error) {
      const frame = page.frames().find(frame => frame.url().includes('3080.'));
      results.push({name, passed: false, error: error.message, snapshot: frame ? await frame.locator('body').ariaSnapshot() : null});
    }
  };
  try {
    await page.goto(fixture.controlUrl);
    const project = fixture.projects[0];
    let releaseFrame;
    const loading = new Promise(resolve => { releaseFrame = resolve; });
    await page.route(url => url.hostname === `3080.${project.host}.devvm.localhost` && url.pathname === '/', async route => {
      await loading;
      await route.continue();
    });
    await page.locator('#open-projects').click();
    await page.locator(`[data-project-id="${project.id}"]`).click();
    const launch = page.getByRole('button', {name: 'Launch DSH', exact: true});
    await expect.poll(async () => await launch.isVisible() || await page.locator('.project-frame[data-active="true"]').count() > 0).toBe(true);
    if (await launch.isVisible()) await launch.click();
    const status = page.locator('#connection-status');
    await expect(status).toHaveAttribute('data-state', /connecting|connected/);
    await check('connection indicator stays round', async () => {
      // Observe the actual status button while native transport is still loading.
      const box = await status.boundingBox();
      assert(box, 'Loading status was not visible.');
      assert(Math.abs(box.width - box.height) < 1, `Loading indicator is ${box.width}×${box.height}px.`);
      const dot = await status.evaluate(element => {
        const style = getComputedStyle(element, '::before');
        return {width: style.width, height: style.height, radius: style.borderRadius};
      });
      assert.deepEqual(dot, {width: '8px', height: '8px', radius: '50%'});
    });
    releaseFrame();
    const active = page.frameLocator('.project-frame[data-active="true"]');
    const continueButton = active.getByRole('button', {name: 'Continue', exact: true});
    await continueButton.waitFor({state: 'visible', timeout: 15000}).catch(() => {});
    if (await continueButton.isVisible()) await continueButton.click();
    const editor = active.locator('[data-composer-input]');
    await editor.waitFor({timeout: 30000});
    await expect(editor).toHaveAttribute('contenteditable', 'true');
    await expect(status).toHaveAttribute('data-state', 'connected');
    await check('DSH navigation button is inside its native header', async () => {
      const box = await active.getByRole('button', {name: 'Open DSH sidebar', exact: true}).boundingBox();
      assert(box && box.x < 60 && box.width >= 44 && box.height >= 44, `DSH navigation button: ${JSON.stringify(box)}`);
    });
    const sessionButton = active.getByRole('button', {name: /^New session/i}).first();
    const display = await context.newCDPSession(page);
    const frameBox = await page.locator('.project-frame[data-active="true"]').boundingBox();
    await check('right swipe opens native DSH sidebar', async () => {
      const y = frameBox.y + 180;
      await display.send('Input.dispatchTouchEvent', {type: 'touchStart', touchPoints: [{x: 18, y}]});
      for (const x of [35, 60, 95, 130, 170]) await display.send('Input.dispatchTouchEvent', {type: 'touchMove', touchPoints: [{x, y}]});
      await display.send('Input.dispatchTouchEvent', {type: 'touchEnd', touchPoints: []});
      await expect(sessionButton).toBeVisible({timeout: 2000});
      assert.equal(await page.locator('.app').getAttribute('data-projects-open'), 'false');
    });
    await check('native header button opens full DSH navigation', async () => {
      const close = active.getByRole('button', {name: 'Close Sessions', exact: true});
      if (await close.isVisible()) await close.click({position: {x: 380, y: 20}});
      await active.getByRole('button', {name: 'Open DSH sidebar', exact: true}).click();
      await expect(sessionButton).toBeVisible();
      await expect(active.getByRole('button', {name: 'Settings', exact: true})).toBeVisible();
      await close.click({position: {x: 380, y: 20}});
      await expect(sessionButton).not.toBeVisible();
    });
    await check('Project picker closes native navigation', async () => {
      await active.getByRole('button', {name: 'Open DSH sidebar', exact: true}).click();
      await expect(sessionButton).toBeVisible();
      await page.locator('#project-title').click();
      await expect(page.locator('#projects-panel')).toBeVisible();
      await expect(sessionButton).not.toBeVisible();
      await page.locator('#close-projects').click();
    });
    await check('vertical gestures keep navigation closed and horizontal gestures open it', async () => {
      const swipe = async (startX, startY, endX, endY) => {
        await display.send('Input.dispatchTouchEvent', {type: 'touchStart', touchPoints: [{x: startX, y: startY}]});
        for (let step = 1; step <= 5; step++) await display.send('Input.dispatchTouchEvent', {
          type: 'touchMove', touchPoints: [{x: startX + (endX - startX) * step / 5, y: startY + (endY - startY) * step / 5}],
        });
        await display.send('Input.dispatchTouchEvent', {type: 'touchEnd', touchPoints: []});
      };
      await swipe(18, frameBox.y + 250, 24, frameBox.y + 120);
      await expect(sessionButton).not.toBeVisible();
      await swipe(120, frameBox.y + 180, 260, frameBox.y + 180);
      await expect(sessionButton).toBeVisible();
      await active.getByRole('button', {name: 'Collapse sidebar', exact: true}).click();
      await expect(sessionButton).not.toBeVisible();
    });
    await editor.fill('只回复 MESSAGES_WEB_READY，不调用工具。');
    await editor.press('Enter');
    await expect(active.getByText('MESSAGES_WEB_READY', {exact: true})).toBeVisible({timeout: 30000});
    await check('keyboard layout policy and constrained viewport retain header and draft', async () => {
      const viewport = await page.locator('meta[name="viewport"]').getAttribute('content');
      assert(viewport.includes('interactive-widget=resizes-content'), 'Top-level document does not request keyboard layout resizing.');
      const frame = await page.locator('.project-frame[data-active="true"]').elementHandle();
      const nativeRoot = await active.locator('#root').elementHandle();
      const route = new URL(page.url()).hash;
      const draft = 'Mobile keyboard keeps this draft and its Session.';
      await editor.fill(draft);
      await editor.focus();
      for (const height of [500, 360, 915]) {
        await page.setViewportSize({width: 412, height});
        await expect.poll(() => page.evaluate(() => document.querySelector('.app').getBoundingClientRect().height)).toBe(height);
        await expect(page.locator('.workspace-heading')).toBeInViewport({ratio: 1});
        await expect(editor).toBeInViewport({ratio: 1});
        await expect(editor).toHaveText(draft);
        assert(await editor.evaluate(element => document.activeElement === element));
        assert(await frame.evaluate(element => element.isConnected));
        assert(await nativeRoot.evaluate(element => element.isConnected));
        assert.equal(new URL(page.url()).hash, route);
        assert.equal(await page.evaluate(() => scrollY), 0);
        await page.screenshot({path: path.join(root, 'mobile-evidence', `keyboard-height-${height}.png`)});
      }
    });
    await check('pinch zoom preserves app height and allows viewport panning', async () => {
      const height = await page.locator('.app').evaluate(element => element.getBoundingClientRect().height);
      await display.send('Emulation.setPageScaleFactor', {pageScaleFactor: 1.5});
      await expect.poll(() => page.evaluate(() => visualViewport.scale)).toBe(1.5);
      assert.equal(await page.locator('.app').evaluate(element => element.getBoundingClientRect().height), height);
      await display.send('Emulation.setPageScaleFactor', {pageScaleFactor: 1});
    });
    await page.screenshot({path: path.join(root, 'mobile-evidence', 'phone.png')});
    await fs.writeFile(path.join(root, 'mobile-evidence', 'results.json'), JSON.stringify(results, null, 2));
    console.log(JSON.stringify(results, null, 2));
    assert(results.every(result => result.passed), 'Mobile regressions remain.');
  } finally { await browser.close(); }
})().catch(error => { console.error(error.message); process.exitCode = 1; });
