// Fixed daemon API responses; native DSH requests continue to the owned processes.
const assert = require('node:assert/strict');
const fs = require('node:fs/promises');
const path = require('node:path');
const crypto = require('node:crypto');

async function routeProjects(page, fixture, snapshots = {}) {
  const origin = new URL(fixture.controlUrl).origin;
  const remoteOrigins = fixture.remoteControlUrls || [];
  const matches = url => [origin, ...remoteOrigins].includes(url.origin) && /^\/api\/projects(?:\/|$)/.test(url.pathname);
  const views = fixture.projects.map(project => project.view);
  const handler = async route => {
    const request = route.request();
    const pathname = new URL(request.url()).pathname;
    assert.equal(request.method(), 'GET', `Unexpected daemon request: ${request.method()} ${pathname}`);
    if (pathname === '/api/projects') return route.fulfill({json: views, headers: {'cache-control': 'no-store'}});
    const project = views.find(view => pathname === `/api/projects/${view.id}` || pathname === `/api/projects/${view.id}/operations`);
    assert(project, `Unexpected daemon API: ${pathname}`);
    const json = pathname.endsWith('/operations') ? snapshots[project.id] || {
      daemon_instance_id: project.daemon_instance_id, active: null, recent: [],
    } : project;
    await route.fulfill({json, headers: {'cache-control': 'no-store'}});
  };
  await page.route(matches, handler);
  return () => page.unroute(matches, handler);
}

async function controlProjectProcess(root, project, action) {
  assert(path.resolve(root).startsWith(path.resolve('.agents') + path.sep));
  assert(['start', 'stop', 'restart'].includes(action));
  const request_id = crypto.randomUUID();
  const requestFile = path.join(root, 'process-request.json');
  await fs.writeFile(path.join(root, 'process-request.next.json'), JSON.stringify({id: project.id, action, request_id}));
  assert.equal(path.dirname(path.resolve(requestFile)), path.resolve(root));
  await fs.rename(path.join(root, 'process-request.next.json'), requestFile);
  const resultFile = path.join(root, `process-result-${request_id}.json`);
  const deadline = Date.now() + 100000;
  while (true) {
    try { return JSON.parse(await fs.readFile(resultFile, 'utf8')); }
    catch (error) { if (error.code !== 'ENOENT') throw error; }
    assert(Date.now() < deadline, `Fixture did not complete DSH ${action}; inspect owned startup logs.`);
    await new Promise(resolve => setTimeout(resolve, 100));
  }
}

module.exports = {routeProjects, controlProjectProcess};
