// Focused controls checks against the owned native fixture, not a deployed daemon.
const playwrightModule = process.env.PLAYWRIGHT_MODULE || 'playwright';
const {chromium} = require(playwrightModule);
const {expect} = require(playwrightModule + '/test');
const assert = require('node:assert/strict');
const fs = require('node:fs/promises');
const path = require('node:path');
const root = path.resolve(process.env.DEVVM_BROWSER_FIXTURE_ROOT || '');
assert(root.startsWith(path.resolve('.agents') + path.sep));

(async () => {
  const fixture = JSON.parse(await fs.readFile(path.join(root, 'ready.json'), 'utf8'));
  assert.equal(new URL(fixture.apiUrl).hostname, '127.0.0.1');
  assert(!['3080', '8100', '8101', '8102'].includes(new URL(fixture.apiUrl).port));
  const output = path.join(root, 'controls-evidence');
  await fs.mkdir(output, {recursive: true});
  const browser = await chromium.launch({channel: 'chrome'});
  const context = await browser.newContext({viewport: {width: 1280, height: 800}, colorScheme: 'light'});
  const page = await context.newPage();
  page.setDefaultTimeout(7000);
  const results = [];
  const errors = [];
  page.on('pageerror', error => errors.push(error.message));
  const check = async (name, run) => {
    try { await run(); results.push({name, passed: true}); }
    catch (error) { results.push({name, passed: false, error: error.message}); }
  };
  try {
    await page.goto(fixture.controlUrl);
    const panel = page.locator('#projects-panel');
    await expect(panel).toBeVisible();
    await check('search focus uses neutral styling without outline', async () => {
      await page.locator('#project-search').focus();
      await expect(page.locator('#project-search')).toBeFocused();
      await expect(page.locator('.project-search')).toHaveCSS('outline-style', 'none');
      await expect(page.locator('#project-search')).toHaveCSS('outline-style', 'none');
      const border = await page.locator('.project-search').evaluate(element => getComputedStyle(element).borderColor);
      assert.equal(border, 'rgb(112, 112, 112)');
    });
    await check('refresh tooltip busy state and real HTTP feedback', async () => {
      const button = page.locator('#refresh-projects');
      await expect(button).toHaveAttribute('title', 'Refresh Project status');
      let release;
      const gate = new Promise(resolve => { release = resolve; });
      await page.route('**/api/projects', async route => { await gate; await route.continue(); });
      const response = page.waitForResponse(response => new URL(response.url()).pathname === '/api/projects');
      await button.click();
      await expect(button).toBeDisabled();
      await expect(button).toHaveAttribute('aria-busy', 'true');
      release();
      assert.equal((await response).status(), 200);
      await expect(page.locator('#notice-text')).toHaveText('Project status refreshed.');
      await expect(button).toBeEnabled();
      await page.unroute('**/api/projects');
      await page.locator('#dismiss-notice').click();
    });
    await check('management icon matches DevVM webpage symbol', async () => {
      const icon = await page.locator('#manage-projects svg path').getAttribute('d');
      assert.equal(icon, await page.locator('.brand svg path').getAttribute('d'));
    });
    await check('one DevVM sidebar button with descending menu lines', async () => {
      await expect(page.locator('.workspace-heading #open-sessions')).toHaveCount(0);
      await expect(page.locator('#open-projects svg path')).toHaveAttribute('d', 'M4 6h16M4 12h12M4 18h8');
    });
    // Keep control checks separate from the preference suite’s Project A.
    const project = fixture.projects[1];
    await page.locator(`[data-project-id="${project.id}"]`).click();
    const launch = page.getByRole('button', {name: 'Launch DSH', exact: true});
    await expect.poll(async () => await launch.isVisible() || await page.locator('.project-frame[data-active="true"]').count() > 0).toBe(true);
    if (await launch.isVisible()) await launch.click();
    const active = page.frameLocator('.project-frame[data-active="true"]');
    await page.addLocatorHandler(active.getByRole('button', {name: 'Continue', exact: true}), async button => { await button.click(); });
    await active.locator('[contenteditable="true"]').waitFor({timeout: 30000});
    await check('Project actions are shaded bordered selectable rows', async () => {
      await page.locator('#project-actions').click();
      const buttons = page.locator('#action-list button');
      await expect(buttons).toHaveCount(6);
      const styles = await buttons.evaluateAll(elements => elements.map(element => {
        const style = getComputedStyle(element);
        return {background: style.backgroundColor, border: style.borderWidth, radius: parseFloat(style.borderRadius)};
      }));
      assert(styles.every(style => style.background !== 'rgba(0, 0, 0, 0)' && style.border === '1px' && style.radius >= 8));
      await page.locator('#actions-dialog').screenshot({path: path.join(output, 'project-actions.png')});
      await page.getByRole('button', {name: 'Close Project actions', exact: true}).click();
    });
    if (await page.locator('#actions-dialog').isVisible()) await page.getByRole('button', {name: 'Close Project actions', exact: true}).click();
    for (const width of [1280, 412]) {
      await page.setViewportSize({width, height: 800});
      await check(`compact header and animated Project drawer at ${width}px`, async () => {
        const header = await page.locator('.workspace-heading').boundingBox();
        assert(header.height <= (width < 960 ? 48 : 44), `Header height ${header.height}px`);
        const opener = await page.locator('#open-projects').boundingBox();
        assert(opener.width >= (width < 960 ? 44 : 32));
        if (await page.locator('#app').getAttribute('data-projects-open') !== 'true') await page.locator('#open-projects').click();
        await expect(panel).toHaveCSS('visibility', 'visible');
        const middle = await panel.evaluate(element => {
          document.getElementById(matchMedia('(max-width: 959px)').matches ? 'close-projects' : 'open-projects').click();
          return new Promise(resolve => requestAnimationFrame(() => requestAnimationFrame(() => resolve(element.getBoundingClientRect().toJSON()))));
        });
        assert(middle.x < 0 && middle.x > -middle.width, `Drawer did not animate: x=${middle.x}, width=${middle.width}`);
        await expect(panel).toHaveCSS('visibility', 'hidden');
        assert(await panel.evaluate(element => element.inert));
        await page.locator('#open-projects').click();
        await expect.poll(async () => Math.round((await panel.boundingBox()).x)).toBe(0);
        await page.locator(width < 960 ? '#close-projects' : '#open-projects').click();
        await expect(panel).toHaveCSS('visibility', 'hidden');
      });
      await check(`native DSH sidebar control stays inside DSH at ${width}px`, async () => {
        const nativeButton = active.getByRole('button', {name: 'Open DSH sidebar', exact: true}).first();
        await expect(nativeButton).toBeVisible({timeout: 2000});
        await nativeButton.click();
        await expect(active.getByRole('button', {name: 'Settings', exact: true})).toBeVisible();
        await active.getByRole('button', {name: 'Collapse sidebar', exact: true}).click();
        await expect(nativeButton).toBeVisible();
      });
      await page.screenshot({path: path.join(output, `controls-${width}.png`)});
    }
    function sampleDrawerTransition(button) {
      button.click();
      return new Promise(resolve => {
        const deadline = performance.now() + 500;
        function sample() {
          const rect = document.querySelector('.pI_x6G_sidebarCol').getBoundingClientRect().toJSON();
          if (rect.x < 0 && rect.x > -rect.width || performance.now() >= deadline) resolve(rect);
          else requestAnimationFrame(sample);
        }
        requestAnimationFrame(sample);
      });
    }
    await check('phone DSH drawer has matching content width and animates both ways', async () => {
      const drawer = active.locator('.pI_x6G_sidebarCol');
      const opener = active.getByRole('button', {name: 'Open DSH sidebar', exact: true});
      await opener.click();
      await expect.poll(async () => Math.round((await drawer.boundingBox()).x)).toBe(0);
      const outside = await drawer.evaluate(element => element.clientWidth);
      const inside = await active.locator('.hHd-Xa_root').boundingBox();
      assert(outside <= 280 && Math.abs(outside - inside.width) < 1, `Drawer/content widths excluding border: ${outside}/${inside.width}`);
      const middle = await active.getByRole('button', {name: 'Collapse sidebar', exact: true}).evaluate(sampleDrawerTransition);
      assert(middle.x < 0 && middle.x > -middle.width, 'DSH drawer did not animate closed.');
      await expect(drawer).toHaveCSS('visibility', 'hidden');
      const opening = await opener.evaluate(sampleDrawerTransition);
      assert(opening.x < 0 && opening.x > -opening.width, 'DSH drawer did not animate open.');
      await expect.poll(async () => Math.round((await drawer.boundingBox()).x)).toBe(0);
      await active.getByRole('button', {name: 'Collapse sidebar', exact: true}).click();
      await expect(drawer).toHaveCSS('visibility', 'hidden');
    });
    await check('right swipe opens DSH from middle and right side of page', async () => {
      const display = await context.newCDPSession(page);
      await display.send('Emulation.setTouchEmulationEnabled', {enabled: true, maxTouchPoints: 1});
      const frame = await page.locator('.project-frame[data-active="true"]').boundingBox();
      for (const start of [130, 260]) {
        const y = frame.y + 180;
        await display.send('Input.dispatchTouchEvent', {type: 'touchStart', touchPoints: [{x: start, y}]});
        for (const step of [20, 40, 65, 90]) await display.send('Input.dispatchTouchEvent', {type: 'touchMove', touchPoints: [{x: start + step, y}]});
        await display.send('Input.dispatchTouchEvent', {type: 'touchEnd', touchPoints: []});
        await expect(active.getByRole('button', {name: 'Settings', exact: true})).toBeVisible();
        await active.getByRole('button', {name: 'Collapse sidebar', exact: true}).click();
        await expect(active.locator('.pI_x6G_sidebarCol')).toHaveCSS('visibility', 'hidden');
      }
      await display.detach();
    });
    await check('left swipe closes DSH from inside drawer and outside backdrop', async () => {
      const display = await context.newCDPSession(page);
      const frame = await page.locator('.project-frame[data-active="true"]').boundingBox();
      for (const start of [160, 360]) {
        await active.getByRole('button', {name: 'Open DSH sidebar', exact: true}).click();
        await expect.poll(async () => Math.round((await active.locator('.pI_x6G_sidebarCol').boundingBox()).x)).toBe(0);
        const y = frame.y + 230;
        await display.send('Input.dispatchTouchEvent', {type: 'touchStart', touchPoints: [{x: start, y}]});
        for (const step of [20, 40, 65, 90]) await display.send('Input.dispatchTouchEvent', {type: 'touchMove', touchPoints: [{x: start - step, y}]});
        await display.send('Input.dispatchTouchEvent', {type: 'touchEnd', touchPoints: []});
        await expect(active.locator('.pI_x6G_sidebarCol')).toHaveCSS('visibility', 'hidden');
        await expect(active.getByRole('button', {name: 'Settings', exact: true})).not.toBeVisible();
      }
      await display.detach();
    });
    await page.emulateMedia({reducedMotion: 'reduce'});
    await check('reduced motion disables drawer animation', async () => {
      await expect(panel).toHaveCSS('transition-duration', '0s');
      await expect(active.locator('.pI_x6G_sidebarCol')).toHaveCSS('transition-duration', '0s');
    });
    assert.deepEqual(errors, [], 'Unexpected browser errors.');
    await fs.writeFile(path.join(output, 'results.json'), JSON.stringify(results, null, 2));
    console.log(JSON.stringify(results, null, 2));
    assert(results.every(result => result.passed), 'Control regressions remain.');
  } finally { await browser.close(); }
})().catch(error => { console.error(error.message); process.exitCode = 1; });
