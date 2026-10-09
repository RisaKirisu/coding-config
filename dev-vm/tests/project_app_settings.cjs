// Real preferences over local and non-loopback origins in an owned native fixture.
const playwrightModule = process.env.PLAYWRIGHT_MODULE || 'playwright';
const {chromium} = require(playwrightModule);
const {expect} = require(playwrightModule + '/test');
const assert = require('node:assert/strict');
const fs = require('node:fs/promises');
const path = require('node:path');
const root = path.resolve(process.env.DEVVM_BROWSER_FIXTURE_ROOT || '');
assert(root.startsWith(path.resolve('.agents') + path.sep));
const yaml = require(path.join(path.dirname(root), 'stage/cli/node_modules/js-yaml'));
const schema = yaml.DEFAULT_SCHEMA.extend(new yaml.Type('tag:yaml.org,2002:js', {kind: 'scalar'}));
const sanitize = text => String(text).replace(/([?&]token=)[^\s&"']+/g, '$1<REDACTED>');

(async () => {
  const fixture = JSON.parse(await fs.readFile(path.join(root, 'ready.json'), 'utf8'));
  assert.equal(new URL(fixture.apiUrl).hostname, '127.0.0.1');
  assert(!['3080', '8100', '8101', '8102'].includes(new URL(fixture.apiUrl).port));
  const output = path.join(root, 'settings-evidence');
  await fs.mkdir(output, {recursive: true});
  const browser = await chromium.launch({channel: 'chrome', args: ['--host-resolver-rules=MAP *.devvm.test 127.0.0.1']});
  let context = await browser.newContext({viewport: {width: 1280, height: 900}, locale: 'en-US'});
  let page = await context.newPage();
  const results = [];
  const check = async (name, run) => {
    try { await run(); results.push({name, passed: true}); }
    catch (error) { results.push({name, passed: false, error: sanitize(error.message)}); }
  };
  async function settings(target) {
    if (!await target.getByRole('dialog', {name: /^(Settings|设置)$/}).isVisible()) {
      await target.getByRole('button', {name: /^(Settings|设置)$/, exact: true}).click();
    }
  }
  async function choose(target, mode, locale) {
    await settings(target);
    const busy = target.getByRole('button', {name: /^(Queue|Steer|排队发送|插话发送)$/});
    const label = await busy.textContent();
    if (!label.includes(mode === 'steer' ? 'Steer' : 'Queue') && !label.includes(mode === 'steer' ? '插话发送' : '排队发送')) {
      const isChinese = await target.locator('html').getAttribute('lang') === 'zh-CN';
      await busy.click();
      await target.getByText(isChinese ? (mode === 'steer' ? '插话发送' : '排队发送') : (mode === 'steer' ? 'Steer' : 'Queue'), {exact: true}).click();
    }
    const language = target.getByRole('button', {name: /^(English|中文)$/});
    const desired = locale === 'zh' ? '中文' : 'English';
    if (await language.textContent() !== desired) {
      await language.click();
      await target.getByText(desired, {exact: true}).click();
    }
    await expect(target.locator('html')).toHaveAttribute('lang', locale === 'zh' ? 'zh-CN' : 'en');
    await expect(target.getByRole('button', {name: locale === 'zh' ? (mode === 'steer' ? '插话发送' : '排队发送') : (mode === 'steer' ? 'Steer' : 'Queue'), exact: true})).toBeVisible();
  }
  async function current(target, mode = 'steer', locale = 'zh') {
    await settings(target);
    await expect(target.locator('html')).toHaveAttribute('lang', locale === 'zh' ? 'zh-CN' : 'en');
    await expect(target.getByRole('button', {name: locale === 'zh' ? (mode === 'steer' ? '插话发送' : '排队发送') : (mode === 'steer' ? 'Steer' : 'Queue'), exact: true})).toBeVisible();
  }
  async function stored() {
    const file = path.join(fixture.projects[0].path, '.dsh-home/profiles/embedded-browser/cordis.patch.yml');
    const rows = yaml.load(await fs.readFile(file, 'utf8'), {schema});
    return {mode: rows.find(row => row.id === 'ui-conversation')?.config?.busyEnter, locale: rows.find(row => row.id === 'locale')?.config?.preference};
  }
  async function waitStored(mode = 'steer', locale = 'zh') {
    await expect.poll(stored).toEqual({mode, locale});
  }
  try {
    await page.goto(fixture.controlUrl);
    const project = fixture.projects[0];
    await page.locator(`[data-project-id="${project.id}"]`).click();
    const launch = page.getByRole('button', {name: 'Launch DSH', exact: true});
    await expect.poll(async () => await launch.isVisible() || await page.locator('.project-frame[data-active="true"]').count() > 0).toBe(true);
    if (await launch.isVisible()) await launch.click();
    const embedded = page.frameLocator('.project-frame[data-active="true"]');
    await page.addLocatorHandler(embedded.getByRole('button', {name: 'Continue', exact: true}), async button => button.click());
    await embedded.locator('[contenteditable="true"]').waitFor({timeout: 30000});
    await embedded.getByRole('button', {name: 'Open DSH sidebar', exact: true}).click();
    await choose(embedded, 'steer', 'zh');
    await waitStored();
    results.push({name: 'loopback choices commit to the actual profile', passed: true});
    async function remoteUrl() {
      const projects = await (await context.request.get(`${fixture.apiUrl}/api/projects`)).json();
      const record = projects.find(record => record.id === project.id);
      const url = new URL(record.links.local_dsh_url);
      url.hostname = `settings.${record.project_host}.devvm.test`;
      url.port = fixture.ingressPort;
      return url.href;
    }
    async function loadRemote(url) {
      await page.goto(url);
      await page.addLocatorHandler(page.getByRole('button', {name: /^(Continue|继续)$/, exact: true}), async button => button.click());
      await page.locator('[contenteditable="true"]').waitFor({timeout: 30000});
    }
    await loadRemote(await remoteUrl());
    await check('non-loopback page reads saved Steer and Chinese', () => current(page));
    await choose(page, 'steer', 'zh');
    await page.reload();
    await check('reload retains saved Steer and Chinese', () => current(page));
    await choose(page, 'queue', 'en');
    await check('remote edits commit to the profile', () => waitStored('queue', 'en'));
    await page.reload();
    await check('reload honors explicit changes without forcing defaults', () => current(page, 'queue', 'en'));
    await choose(page, 'steer', 'zh');
    await waitStored();
    await context.close();
    context = await browser.newContext({viewport: {width: 1280, height: 900}, locale: 'en-US'});
    page = await context.newPage();
    await loadRemote(await remoteUrl());
    await check('fresh browser context reads saved choices', () => current(page));
    await context.setOffline(true);
    await check('offline state retains displayed choices', () => current(page));
    await context.setOffline(false);
    await check('reconnect retains saved choices', () => current(page));
    const response = await context.request.post(`${fixture.apiUrl}/api/projects/${project.id}/dsh/restart`);
    assert(response.ok(), 'Owned runtime restart failed.');
    await expect.poll(async () => {
      const records = await (await context.request.get(`${fixture.apiUrl}/api/projects`)).json();
      return records.find(record => record.id === project.id).dsh_status;
    }, {timeout: 30000}).toBe('running');
    await loadRemote(await remoteUrl());
    await check('runtime restart retains saved choices', () => current(page));
    await waitStored();
    await fs.writeFile(path.join(output, 'results.json'), JSON.stringify(results, null, 2));
    console.log(JSON.stringify(results, null, 2));
    assert(results.every(result => result.passed), 'Preferences reverted at a real persistence boundary.');
  } finally { await browser.close(); }
})().catch(error => {console.error(sanitize(error.message)); process.exitCode = 1;});
