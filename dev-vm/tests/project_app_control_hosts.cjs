// Verify aliases against the owned HTTPS Caddy + real native DSH fixture.
const playwrightModule = process.env.PLAYWRIGHT_MODULE || 'playwright';
const {chromium} = require(playwrightModule);
const {expect} = require(playwrightModule + '/test');
const assert = require('node:assert/strict');
const fs = require('node:fs/promises');
const path = require('node:path');
const fixtureRoot = process.env.DEVVM_BROWSER_FIXTURE_ROOT;
assert(fixtureRoot && path.resolve(fixtureRoot).startsWith(path.resolve('.agents') + path.sep));

(async () => {
  const fixture = JSON.parse(await fs.readFile(path.join(fixtureRoot, 'ready.json'), 'utf8'));
  assert(fixture.remoteControlUrls?.length);
  const browser = await chromium.launch({channel:'chrome', args:[
    '--host-resolver-rules=MAP *.devvm.test 127.0.0.2', '--no-proxy-server', '--ignore-certificate-errors',
  ]});
  const results = [];
  const project = fixture.projects[0];
  try {
    for (const origin of [...fixture.remoteControlUrls, fixture.controlUrl]) {
      const context = await browser.newContext({viewport:{width:412,height:915}, isMobile:true, hasTouch:true, ignoreHTTPSErrors:true});
      try {
        const page = await context.newPage();
        const messages = [];
        await page.exposeFunction('recordBridgeMessage', message => messages.push(message));
        await page.addInitScript(() => window.addEventListener('message', event => {
          if (event.data?.protocol === 'devvm-embed') window.recordBridgeMessage(event.data);
        }));
        await page.addLocatorHandler(page.frameLocator('.project-frame[data-active="true"]').getByRole('button', {name:'Continue',exact:true}), async button => button.click());
        await page.goto(origin);
        const label = origin === fixture.controlUrl ? 'DevVM' : new URL(origin).hostname.split('.')[0];
        await expect(page.locator('meta[name="apple-mobile-web-app-title"]')).toHaveAttribute('content', label);
        const cdp = await context.newCDPSession(page);
        const {data, errors} = await cdp.send('Page.getAppManifest');
        assert.equal(errors.length, 0, `${label}: browser manifest errors`);
        const manifest = JSON.parse(data);
        assert.equal(manifest.name, label);
        assert.equal(manifest.short_name, label);
        await cdp.detach();
        await page.evaluate(() => navigator.serviceWorker.ready);
        const cachedManifest = await page.evaluate(async () => {
          const response = await caches.match('/manifest.webmanifest');
          return response?.json();
        });
        assert.equal(cachedManifest.name, label, `${label}: cached mobile name`);
        if (await page.locator('.app').getAttribute('data-projects-open') !== 'true') await page.locator('#open-projects').click();
        await page.locator(`[data-project-id="${project.id}"]`).click();
        await page.locator('#placeholder-action:visible, .project-frame[data-active="true"]').first().waitFor();
        const launch = page.getByRole('button', {name:'Launch DSH',exact:true});
        if (await launch.count()) await launch.click();
        await expect.poll(() => messages.some(message => message.kind === 'ready' && message.projectId === project.id), {timeout:45000}).toBe(true);
        const frame = await (await page.locator('.project-frame[data-active="true"]').elementHandle()).contentFrame();
        assert.equal(await frame.evaluate(() => window.name), origin);
        assert.equal(await frame.evaluate(() => document.referrer), '');
        assert.equal(new URL(frame.url()).searchParams.has('token'), false, 'Native auth redirect must remove the token.');
        await expect(frame.locator('[data-composer-input]')).toBeVisible();
        assert((await context.cookies(frame.url())).some(cookie => cookie.httpOnly && cookie.sameSite === 'Strict'), 'Real same-site auth cookie is present.');
        // Exercise the authorized bridge after switching aliases against the same running DSH.
        await frame.getByRole('button', {name:'Open DSH sidebar',exact:true}).first().click();
        await expect(frame.locator('.pI_x6G_sidebarCol')).toBeVisible();
        results.push({control:label, manifest:manifest.name, bridge:'ready', bootstrap:'exact iframe name', referrer:'none', nativeAuth:'passed'});
      } finally {
        await context.close();
      }
    }
    const context = await browser.newContext({ignoreHTTPSErrors:true});
    try {
      const page = await context.newPage();
      for (const label of ['devvm2', 'devvm-', 'other']) {
        const response = await page.goto(`https://${label}.devvm.test`);
        assert.equal(response.status(), 400, `${label}: rejected by shipped Caddy matcher`);
      }
      // Native DSH still loads under an unauthorized sibling, but its embed bridge must stay inactive.
      const response = await context.request.get(`${fixture.apiUrl}/api/projects/${project.id}`);
      assert.equal(response.status(), 200);
      const projectView = await response.json();
      const probe = name => `<script>
        window.bridgeMessages = [];
        addEventListener('message', event => {
          if (event.data?.protocol === 'devvm-embed') window.bridgeMessages.push(event.data.kind);
        });
      </script><iframe name="${name}" referrerpolicy="no-referrer" src="${projectView.links.tailnet_dsh_url}" style="width:412px;height:915px"></iframe>`;
      await page.setContent(probe('https://other.devvm.test'));
      const native = page.frameLocator('iframe');
      await page.addLocatorHandler(native.getByRole('button', {name:'Continue',exact:true}), async button => button.click());
      await expect(native.locator('[data-composer-input]')).toBeVisible({timeout:45000});
      let frame = await (await page.locator('iframe').elementHandle()).contentFrame();
      assert.equal(await frame.evaluate(() => window.name), 'https://other.devvm.test');
      assert.equal(await frame.evaluate(() => document.referrer), '');
      assert.deepEqual(await page.evaluate(() => window.bridgeMessages), [], 'Unauthorized sibling must receive no embed announcement.');
      // An attacker can claim an allowed frame name, but browser-origin/source checks must still deny attachment.
      await page.setContent(probe(fixture.remoteControlUrls[1]));
      await expect(native.getByRole('button', {name:'Open DSH sidebar',exact:true}).first()).toBeVisible({timeout:45000});
      frame = await (await page.locator('iframe').elementHandle()).contentFrame();
      assert.equal(await frame.evaluate(() => window.name), fixture.remoteControlUrls[1]);
      await page.evaluate(({projectId, childOrigin}) => document.querySelector('iframe').contentWindow.postMessage({
        protocol:'devvm-embed', version:1, projectId, kind:'attach', channelId:'untrusted-parent',
      }, childOrigin), {projectId:project.id, childOrigin:new URL(projectView.links.tailnet_dsh_url).origin});
      await page.evaluate(() => new Promise(resolve => requestAnimationFrame(() => requestAnimationFrame(resolve))));
      assert.deepEqual(await page.evaluate(() => window.bridgeMessages), [], 'Spoofed allowed name must not receive announcements or attach replies.');
      results.push({rejected:['devvm2','devvm-','other'], untrustedParent:'bridge inactive', spoofedParent:'attachment rejected'});
    } finally {
      await context.close();
    }
    await fs.writeFile(path.join(fixtureRoot, 'control-hosts-results.json'), JSON.stringify(results, null, 2));
    console.log(JSON.stringify(results, null, 2));
  } finally {
    await browser.close();
  }
})().catch(error => {
  console.error(String(error.stack || error).replace(/([?&]token=)[^\s&"']+/g, '$1[redacted]'));
  process.exitCode = 1;
});
