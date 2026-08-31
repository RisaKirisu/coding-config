import test from 'node:test';
import assert from 'node:assert/strict';
import { spawn } from 'node:child_process';
import { mkdtempSync, rmSync, existsSync, writeFileSync, readFileSync, mkdirSync, readdirSync, unlinkSync } from 'node:fs';
import { homedir, tmpdir } from 'node:os';
import { join } from 'node:path';
import { RemoteSyncManager, RSYNC_FILTER_ARGS } from './index.mjs';

function createTempDir() {
  return mkdtempSync(join(tmpdir(), 'dsh-sync-test-'));
}

async function waitFor(check, message, timeoutMs = 1000) {
  const deadline = Date.now() + timeoutMs;
  while (Date.now() < deadline) {
    if (check()) return;
    await new Promise((resolve) => setTimeout(resolve, 10));
  }
  assert.fail(message);
}

function findFile(root, name) {
  for (const entry of readdirSync(root, { withFileTypes: true })) {
    const path = join(root, entry.name);
    if (entry.isDirectory()) {
      const nested = findFile(path, name);
      if (nested) return nested;
    } else if (entry.name === name) {
      return path;
    }
  }
  return null;
}

test('Session Sync defaults to DSH Home, never the Project directory', () => {
  const projectDir = createTempDir();
  const originalCwd = process.cwd();
  const originalDshHome = process.env.DSH_HOME;
  try {
    delete process.env.DSH_HOME;
    process.chdir(projectDir);
    const manager = new RemoteSyncManager();
    assert.equal(manager.dshHome, join(homedir(), '.dsh'));
    assert.notEqual(manager.dshHome, projectDir);
  } finally {
    process.chdir(originalCwd);
    if (originalDshHome === undefined) delete process.env.DSH_HOME;
    else process.env.DSH_HOME = originalDshHome;
    rmSync(projectDir, { recursive: true, force: true });
  }
});

test('unconfigured Session Sync leaves DSH state clean', async () => {
  const tempDir = createTempDir();
  const oldSyncConfigPath = process.env.DEVVM_SYNC_CONFIG_PATH;
  process.env.DEVVM_SYNC_CONFIG_PATH = join(tempDir, 'missing-sync.json');
  try {
    const manager = new RemoteSyncManager({ dshHome: tempDir });
    assert.equal(await manager.triggerSync(), 'not_configured');
    assert.equal(existsSync(join(tempDir, '.sync-dirty')), false);
  } finally {
    if (oldSyncConfigPath === undefined) delete process.env.DEVVM_SYNC_CONFIG_PATH;
    else process.env.DEVVM_SYNC_CONFIG_PATH = oldSyncConfigPath;
    rmSync(tempDir, { recursive: true, force: true });
  }
});

test('real DSH persistence events start Session Sync after saved changes', async () => {
  const dshModules = '/usr/local/lib/node_modules/@deepseek-ai/dsh/node_modules/@deepseek-ai';
  const [
    { Context },
    { default: SessionStore },
    { default: JsonlSessionPersistence },
    { default: Storage },
    storageJson,
    storageDomain,
    { workspaceDomainSpec },
    { messageFeedbackDomainSpec },
    plugin,
  ] = await Promise.all([
    import(`/usr/local/lib/node_modules/@deepseek-ai/dsh/node_modules/@deepseek-ai/cordis/lib/index.js`),
    import(`${dshModules}/dsh-session/lib/index.js`),
    import(`${dshModules}/dsh-session-persistence-jsonl/lib/index.js`),
    import(`${dshModules}/dsh-storage/lib/index.js`),
    import(`${dshModules}/dsh-storage-json/lib/index.js`),
    import(`${dshModules}/dsh-storage-domain/lib/index.js`),
    import(`${dshModules}/dsh-workspace/lib/index.js`),
    import(`${dshModules}/dsh-message-feedback/lib/index.js`),
    import('./index.mjs'),
  ]);
  const tempDir = createTempDir();
  const workspaceDir = join(tempDir, 'workspace');
  const dirtyFile = join(tempDir, '.sync-dirty');
  const oldSyncConfigPath = process.env.DEVVM_SYNC_CONFIG_PATH;
  const oldProjectId = process.env.DEVVM_PROJECT_ID;
  const ctx = new Context();
  const forks = [];
  let workspaceDomain;
  let feedbackDomain;

  mkdirSync(workspaceDir);
  process.env.DEVVM_SYNC_CONFIG_PATH = join(tempDir, 'sync.json');
  process.env.DEVVM_PROJECT_ID = '00000000-0000-4000-8000-000000000001';
  writeFileSync(process.env.DEVVM_SYNC_CONFIG_PATH, JSON.stringify({
    ssh_user: 'devvm',
    ssh_host: '127.0.0.1',
    ssh_port: 1,
    ssh_key_path: join(tempDir, 'missing-key'),
    remote_sync_root: '/tmp/devvm-sync-test',
  }));

  try {
    forks.push(ctx.plugin(SessionStore));
    forks.push(ctx.plugin(JsonlSessionPersistence, {
      root: join(tempDir, 'sessions'),
      compression: 'none',
    }));
    forks.push(ctx.plugin(Storage));
    forks.push(ctx.plugin(storageJson, { root: join(tempDir, 'storages') }));
    forks.push(ctx.plugin(storageDomain, { backend: 'json' }));
    const remoteSyncFork = ctx.plugin(plugin, { dshHome: tempDir });
    forks.push(remoteSyncFork);

    await waitFor(
      () => ctx.sessions && ctx.sessionPersistence && ctx.storageDomain && ctx.remoteSync,
      'Real DSH services and Remote Sync must activate',
    );
    assert.equal(remoteSyncFork.state, 2, 'Remote Sync must be active in the real Cordis context');

    workspaceDomain = await ctx.storageDomain.open(workspaceDomainSpec);
    feedbackDomain = await ctx.storageDomain.open(messageFeedbackDomainSpec);

    const session = ctx.sessions.create('session-real-contract', { meta: { cwd: workspaceDir } });
    session.append('turn/start', { turn: 1 });
    session.append('turn/end', { turn: 1, reason: { kind: 'completed' } });

    await waitFor(
      () => existsSync(dirtyFile),
      'A real completed-turn session/event must start Session Sync',
    );
    const sessionLog = findFile(join(tempDir, 'sessions'), 'session.jsonl');
    assert.ok(sessionLog, 'Real session persistence must write session.jsonl before sync starts');
    assert.match(readFileSync(sessionLog, 'utf8'), /"type":"turn\/end"/);

    unlinkSync(dirtyFile);
    await workspaceDomain.global.set(workspaceDomain.global.get());
    await waitFor(
      () => existsSync(dirtyFile),
      'A real saved workspace domain change must start Session Sync',
    );

    unlinkSync(dirtyFile);
    await feedbackDomain.table('sessions').put(session.id, {
      session: { createdAt: session.header.createdAt, cwd: workspaceDir },
      items: [],
    });
    await waitFor(
      () => existsSync(dirtyFile),
      'A real saved message-feedback domain change must start Session Sync',
    );

    if (ctx.remoteSync.activeTransfer) await ctx.remoteSync.activeTransfer;
  } finally {
    await feedbackDomain?.close();
    await workspaceDomain?.close();
    for (const fork of forks.reverse()) await fork.dispose();
    if (oldSyncConfigPath === undefined) delete process.env.DEVVM_SYNC_CONFIG_PATH;
    else process.env.DEVVM_SYNC_CONFIG_PATH = oldSyncConfigPath;
    if (oldProjectId === undefined) delete process.env.DEVVM_PROJECT_ID;
    else process.env.DEVVM_PROJECT_ID = oldProjectId;
    rmSync(tempDir, { recursive: true, force: true });
  }
});

test('rsync filter behavior - syncs whitelisted state and excludes blacklisted categories', async () => {
  const srcDir = createTempDir();
  const dstDir = createTempDir();
  try {
    // 1. Files/dirs that SHOULD be synced
    mkdirSync(join(srcDir, 'sessions', 'session-123'), { recursive: true });
    writeFileSync(join(srcDir, 'sessions', 'session-123', 'meta.json'), '{"id":"session-123"}');
    mkdirSync(join(srcDir, 'storages'), { recursive: true });
    writeFileSync(join(srcDir, 'storages', 'workspace.json'), '{"workspaces":[]}');
    writeFileSync(join(srcDir, 'storages', 'message_feedback.json'), '{"feedback":[]}');
    mkdirSync(join(srcDir, 'attachments', 'v1', 'objects', 'ab'), { recursive: true });
    writeFileSync(join(srcDir, 'attachments', 'v1', 'objects', 'ab', 'abcdef'), 'image-bytes');

    // 2. Files/dirs that MUST be excluded
    writeFileSync(join(srcDir, 'storages', 'session_projcache.json'), '{"cache":true}');
    mkdirSync(join(srcDir, 'attachments', 'v1', 'request-images', 'cd'), { recursive: true });
    writeFileSync(join(srcDir, 'attachments', 'v1', 'request-images', 'cd', 'derived'), 'derived-image');
    writeFileSync(join(srcDir, '.sync-dirty'), '1\n');
    mkdirSync(join(srcDir, 'credentials'), { recursive: true });
    writeFileSync(join(srcDir, 'credentials', 'keys.json'), 'secret');
    mkdirSync(join(srcDir, 'settings'), { recursive: true });
    writeFileSync(join(srcDir, 'settings', 'config.json'), 'settings');
    mkdirSync(join(srcDir, 'plugins'), { recursive: true });
    writeFileSync(join(srcDir, 'plugins', 'plugin.js'), 'plugin');
    mkdirSync(join(srcDir, 'presets'), { recursive: true });
    writeFileSync(join(srcDir, 'presets', 'preset.json'), 'preset');
    mkdirSync(join(srcDir, 'profiles'), { recursive: true });
    writeFileSync(join(srcDir, 'profiles', 'user.json'), 'profile');
    writeFileSync(join(srcDir, 'random-root-file.txt'), 'random');

    const srcPath = `${srcDir.replace(/\/$/, '')}/`;
    const dstPath = `${dstDir.replace(/\/$/, '')}/`;

    await new Promise((resolve, reject) => {
      const proc = spawn('rsync', ['-avz', ...RSYNC_FILTER_ARGS, srcPath, dstPath]);
      let stderr = '';
      proc.stderr.on('data', (d) => {
        stderr += d.toString();
      });
      proc.on('close', (code) => {
        if (code !== 0) reject(new Error(`rsync failed: ${stderr}`));
        else resolve();
      });
    });

    // Verify whitelisted files exist in destination
    assert.equal(existsSync(join(dstDir, 'sessions', 'session-123', 'meta.json')), true);
    assert.equal(existsSync(join(dstDir, 'storages', 'workspace.json')), true);
    assert.equal(existsSync(join(dstDir, 'storages', 'message_feedback.json')), true);
    assert.equal(existsSync(join(dstDir, 'attachments', 'v1', 'objects', 'ab', 'abcdef')), true);

    // Verify blacklisted files DO NOT exist in destination
    assert.equal(existsSync(join(dstDir, 'storages', 'session_projcache.json')), false);
    assert.equal(existsSync(join(dstDir, 'attachments', 'v1', 'request-images')), false);
    assert.equal(existsSync(join(dstDir, '.sync-dirty')), false);
    assert.equal(existsSync(join(dstDir, 'credentials')), false);
    assert.equal(existsSync(join(dstDir, 'settings')), false);
    assert.equal(existsSync(join(dstDir, 'plugins')), false);
    assert.equal(existsSync(join(dstDir, 'presets')), false);
    assert.equal(existsSync(join(dstDir, 'profiles')), false);
    assert.equal(existsSync(join(dstDir, 'random-root-file.txt')), false);
  } finally {
    rmSync(srcDir, { recursive: true, force: true });
    rmSync(dstDir, { recursive: true, force: true });
  }
});

test('manifest, bundle patch, and client resolution contract', async () => {
  // Read and validate package.json manifest
  const pkgPath = join(import.meta.dirname, 'package.json');
  assert.ok(existsSync(pkgPath), 'package.json must exist');
  const pkg = JSON.parse(readFileSync(pkgPath, 'utf8'));

  assert.equal(pkg.name, '@devvm/dsh-remote-sync');
  assert.equal(pkg.type, 'module');
  assert.equal(pkg.main, './index.mjs');

  // Verify dsh.bundle.patch
  assert.ok(pkg.dsh?.bundle?.patch, 'dsh.bundle.patch must be declared');
  const patchRelativePath = pkg.dsh.bundle.patch;
  assert.equal(patchRelativePath, './cordis.patch.yml');
  const patchFullPath = join(import.meta.dirname, patchRelativePath);
  assert.ok(existsSync(patchFullPath), 'Declared cordis.patch.yml must exist');

  // Verify patch file content structure
  const patchContent = readFileSync(patchFullPath, 'utf8');
  assert.ok(patchContent.includes("id: remote-sync"), 'cordis.patch.yml must declare id: remote-sync');
  assert.ok(
    patchContent.includes("name: '@devvm/dsh-remote-sync'") || patchContent.includes('name: "@devvm/dsh-remote-sync"') || patchContent.includes('name: @devvm/dsh-remote-sync'),
    'cordis.patch.yml must insert host plugin @devvm/dsh-remote-sync',
  );

  // Verify dsh.client.platform
  assert.equal(pkg.dsh?.client?.platform, 'web', 'dsh.client.platform must be web');

  // Verify exports resolution
  assert.equal(pkg.exports?.['.'], './index.mjs', 'exports["."] must point to ./index.mjs');
  assert.equal(pkg.exports?.['./client'], './client.js', 'exports["./client"] must point to ./client.js');
  assert.ok(existsSync(join(import.meta.dirname, pkg.exports['.'])), 'index.mjs must exist');
  assert.ok(existsSync(join(import.meta.dirname, pkg.exports['./client'])), 'client.js must exist');
});

test('web profile integration - dump-config, profile isolation, and web client bundle resolution', async () => {
  const { execSync } = await import('node:child_process');

  // 1. dsh --profile web --dump-config includes both plugins exactly once
  const webDump = execSync('dsh --profile web --dump-config', { encoding: 'utf8' });
  const remoteSyncMatches = webDump.match(/name: ['"]?@devvm\/dsh-remote-sync['"]?/g) || [];
  const voiceInputMatches = webDump.match(/name: ['"]?@devvm\/dsh-voice-input['"]?/g) || [];
  assert.equal(remoteSyncMatches.length, 1, 'web dump-config must include @devvm/dsh-remote-sync exactly once');
  assert.equal(voiceInputMatches.length, 1, 'web dump-config must include @devvm/dsh-voice-input exactly once');
  assert.ok(webDump.includes('/root/voice-dictation-cleanup/data/archive_voice_input.jsonl'), 'voice-input config path must be preserved');

  // 2. dsh --profile headless --dump-config excludes both plugins
  const headlessDump = execSync('dsh --profile headless --dump-config', { encoding: 'utf8' });
  assert.ok(!headlessDump.includes('@devvm/dsh-remote-sync'), 'headless profile dump must exclude @devvm/dsh-remote-sync');
  assert.ok(!headlessDump.includes('@devvm/dsh-voice-input'), 'headless profile dump must exclude @devvm/dsh-voice-input');

  // 3. dsh web boots and serves client bundle
  const testPort = '3599';
  const dshProcess = spawn(
    'dsh',
    ['--profile', 'web', '--no-open', '--port', testPort],
    {
      stdio: ['ignore', 'pipe', 'pipe'],
      env: process.env,
    },
  );

  let output = '';
  let errorOutput = '';

  const bootPromise = new Promise((resolve, reject) => {
    dshProcess.stdout.on('data', (chunk) => {
      output += chunk.toString();
      if (output.includes(`dsh web: http://127.0.0.1:${testPort}`)) {
        resolve({ success: true });
      }
    });

    dshProcess.stderr.on('data', (chunk) => {
      errorOutput += chunk.toString();
    });

    dshProcess.on('exit', (code) => {
      if (code !== 0 && !output.includes(testPort)) {
        reject(new Error(`dsh exited prematurely with code ${code}: ${errorOutput}\n${output}`));
      }
    });
  });

  const timerPromise = new Promise((_, reject) =>
    setTimeout(
      () => reject(new Error(`Timeout waiting for dsh web boot. Output: ${output}, Stderr: ${errorOutput}`)),
      10000,
    ),
  );

  try {
    await Promise.race([bootPromise, timerPromise]);

    const clientRes = await fetch(`http://127.0.0.1:${testPort}/plugins/@devvm/dsh-remote-sync/client.js`);
    assert.equal(clientRes.status, 200, 'Client bundle endpoint must return HTTP 200');
    const clientText = await clientRes.text();
    assert.ok(clientText.includes('@devvm/dsh-remote-sync'), 'Client bundle text must include @devvm/dsh-remote-sync');

    const statusRes = await fetch(`http://127.0.0.1:${testPort}/api/sync/status`);
    assert.equal(statusRes.status, 200, '/api/sync/status endpoint must return HTTP 200');
  } finally {
    dshProcess.kill('SIGTERM');
  }
});

