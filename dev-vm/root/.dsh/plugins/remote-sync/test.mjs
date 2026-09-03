import test from 'node:test';
import assert from 'node:assert/strict';
import { spawn } from 'node:child_process';
import {
  existsSync,
  mkdirSync,
  mkdtempSync,
  readFileSync,
  readdirSync,
  rmSync,
  statSync,
  utimesSync,
  writeFileSync,
} from 'node:fs';
import { homedir, tmpdir } from 'node:os';
import { join } from 'node:path';
import {
  HEAD_MARKER_NAME,
  RemoteSyncManager,
  STORAGES_FILTER_ARGS,
  UNION_FILTER_ARGS,
} from './index.mjs';

const PROJECT_ID = '00000000-0000-4000-8000-000000000001';

/** Tests keep the VM-local status file inside their temp DSH Home, never in /run/devvm. */
const STATUS_FILE_NAME = 'sync-status.json';

function createTempDir() {
  return mkdtempSync(join(tmpdir(), 'dsh-sync-test-'));
}

async function waitFor(check, message, timeoutMs = 5000) {
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

/** A local Sync Store plus a local DSH Home, wired through the local transport. */
function createFixture(options = {}) {
  const dshHome = createTempDir();
  const storeRoot = createTempDir();
  const storeDir = join(storeRoot, PROJECT_ID);
  if (options.createStoreDir !== false) mkdirSync(storeDir, { recursive: true });
  return {
    dshHome,
    storeRoot,
    storeDir,
    manager(extra = {}) {
      return new RemoteSyncManager({
        dshHome,
        statusFilePath: join(dshHome, STATUS_FILE_NAME),
        projectId: PROJECT_ID,
        retryDelayMs: 0,
        syncConfig: { remote_sync_root: storeRoot, writer_id: 'writer-under-test', ...options.config },
        ...extra,
      });
    },
    cleanup() {
      rmSync(dshHome, { recursive: true, force: true });
      rmSync(storeRoot, { recursive: true, force: true });
    },
  };
}

function writeLocalState(dshHome, { sessionId = 'session-a', sessionBody = '{"type":"turn/end"}\n', workspace = '{"workspaces":[]}' } = {}) {
  const sessionDir = join(dshHome, 'sessions', 'root', 'project', sessionId);
  mkdirSync(sessionDir, { recursive: true });
  writeFileSync(join(sessionDir, 'session.jsonl'), sessionBody);
  mkdirSync(join(dshHome, 'storages'), { recursive: true });
  writeFileSync(join(dshHome, 'storages', 'workspace.json'), workspace);
  return { sessionDir };
}

function seedHeadSeq(dshHome, headSeq) {
  mkdirSync(dshHome, { recursive: true });
  writeFileSync(
    join(dshHome, STATUS_FILE_NAME),
    JSON.stringify({ status: 'synchronized', head_seq: headSeq, last_error: null, updated_at: new Date().toISOString() }, null, 2),
  );
}

function writeMarker(storeDir, seq, writerId = 'other-workstation') {
  mkdirSync(storeDir, { recursive: true });
  writeFileSync(
    join(storeDir, HEAD_MARKER_NAME),
    JSON.stringify({ seq, writer_id: writerId, updated_at: new Date().toISOString() }, null, 2),
  );
}

function readMarker(storeDir) {
  return JSON.parse(readFileSync(join(storeDir, HEAD_MARKER_NAME), 'utf8'));
}

function countingClock() {
  const state = { calls: 0 };
  state.now = () => {
    state.calls += 1;
    return new Date(1700000000000 + state.calls * 1000);
  };
  return state;
}

function setMtime(path, secondsFromEpoch) {
  utimesSync(path, secondsFromEpoch, secondsFromEpoch);
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

test('unconfigured Session Sync reports not_configured and transfers nothing', async () => {
  const dshHome = createTempDir();
  const oldSyncConfigPath = process.env.DEVVM_SYNC_CONFIG_PATH;
  process.env.DEVVM_SYNC_CONFIG_PATH = join(dshHome, 'missing-sync.json');
  try {
    const manager = new RemoteSyncManager({ dshHome, statusFilePath: join(dshHome, STATUS_FILE_NAME), projectId: PROJECT_ID });
    assert.equal(await manager.triggerSync(), 'not_configured');
    const status = JSON.parse(readFileSync(join(dshHome, STATUS_FILE_NAME), 'utf8'));
    assert.equal(status.status, 'not_configured');
    assert.equal(status.head_seq, null);
  } finally {
    if (oldSyncConfigPath === undefined) delete process.env.DEVVM_SYNC_CONFIG_PATH;
    else process.env.DEVVM_SYNC_CONFIG_PATH = oldSyncConfigPath;
    rmSync(dshHome, { recursive: true, force: true });
  }
});

test('a missing Project ID fails Session Sync instead of inventing a Sync Store directory', async () => {
  const fixture = createFixture();
  const emptyWorkspace = createTempDir();
  const oldProjectId = process.env.DEVVM_PROJECT_ID;
  const oldWorkspace = process.env.DEVVM_WORKSPACE;
  delete process.env.DEVVM_PROJECT_ID;
  process.env.DEVVM_WORKSPACE = emptyWorkspace;
  try {
    const manager = fixture.manager({ projectId: null });
    assert.equal(await manager.triggerSync(), 'failed');
    assert.equal(manager.lastError, 'Project ID not found (.devvm-id missing)');
    assert.deepEqual(readdirSync(fixture.storeDir), []);
  } finally {
    if (oldProjectId === undefined) delete process.env.DEVVM_PROJECT_ID;
    else process.env.DEVVM_PROJECT_ID = oldProjectId;
    if (oldWorkspace === undefined) delete process.env.DEVVM_WORKSPACE;
    else process.env.DEVVM_WORKSPACE = oldWorkspace;
    rmSync(emptyWorkspace, { recursive: true, force: true });
    fixture.cleanup();
  }
});

test('head protocol advances the Sync Store marker only from a known head', async () => {
  const fixture = createFixture();
  try {
    writeLocalState(fixture.dshHome);

    const fresh = fixture.manager();
    assert.equal(fresh.headSeq, null);
    assert.equal(await fresh.triggerSync(), 'remote_ahead');
    assert.equal(fresh.headSeq, null);
    assert.equal(existsSync(join(fixture.storeDir, HEAD_MARKER_NAME)), false);
    assert.ok(findFile(join(fixture.storeDir, 'sessions'), 'session.jsonl'), 'union push must carry the session log');
    assert.equal(existsSync(join(fixture.storeDir, 'storages', 'workspace.json')), false, 'storages must not be pushed while behind');

    assert.equal(await fresh.reconcile(), 'synchronized');
    assert.equal(fresh.headSeq, 0);

    assert.equal(await fresh.triggerSync(), 'synchronized');
    assert.equal(fresh.headSeq, 1);
    const marker = readMarker(fixture.storeDir);
    assert.equal(marker.seq, 1);
    assert.equal(marker.writer_id, 'writer-under-test');
    assert.equal(existsSync(join(fixture.storeDir, 'storages', 'workspace.json')), true);
  } finally {
    fixture.cleanup();
  }
});

test('a Sync Store that moved ahead suspends storage pushes but keeps session pushes', async () => {
  const fixture = createFixture();
  try {
    writeLocalState(fixture.dshHome);
    seedHeadSeq(fixture.dshHome, 0);
    writeMarker(fixture.storeDir, 5);
    mkdirSync(join(fixture.storeDir, 'storages'), { recursive: true });
    writeFileSync(join(fixture.storeDir, 'storages', 'workspace.json'), '{"workspaces":["remote"]}');

    const manager = fixture.manager();
    assert.equal(manager.headSeq, 0);
    assert.equal(await manager.triggerSync(), 'remote_ahead');
    assert.equal(manager.headSeq, 0);
    assert.equal(readMarker(fixture.storeDir).seq, 5, 'a mismatched head must not advance');
    assert.equal(
      readFileSync(join(fixture.storeDir, 'storages', 'workspace.json'), 'utf8'),
      '{"workspaces":["remote"]}',
    );
    assert.ok(findFile(join(fixture.storeDir, 'sessions'), 'session.jsonl'));

    manager._setStatus('synchronized');
    assert.equal(await manager.checkRemoteHead(), 'remote_ahead');
  } finally {
    fixture.cleanup();
  }
});

test('reconciliation at an equal head keeps the newest storage unit and gains Sync Store sessions', async () => {
  const fixture = createFixture();
  try {
    writeLocalState(fixture.dshHome, { workspace: '{"workspaces":["local"]}' });
    seedHeadSeq(fixture.dshHome, 3);
    writeMarker(fixture.storeDir, 3);

    mkdirSync(join(fixture.storeDir, 'storages'), { recursive: true });
    const storeWorkspace = join(fixture.storeDir, 'storages', 'workspace.json');
    writeFileSync(storeWorkspace, '{"workspaces":["store"]}');
    setMtime(storeWorkspace, 1000000);
    setMtime(join(fixture.dshHome, 'storages', 'workspace.json'), 1600000000);

    const storeSession = join(fixture.storeDir, 'sessions', 'root', 'project', 'session-store');
    mkdirSync(storeSession, { recursive: true });
    writeFileSync(join(storeSession, 'session.jsonl'), '{"type":"turn/start"}\n');

    const manager = fixture.manager();
    assert.equal(await manager.reconcile(), 'synchronized');
    assert.equal(manager.headSeq, 3);
    assert.equal(readFileSync(storeWorkspace, 'utf8'), '{"workspaces":["local"]}');
    assert.equal(
      existsSync(join(fixture.dshHome, 'sessions', 'root', 'project', 'session-store', 'session.jsonl')),
      true,
    );
  } finally {
    fixture.cleanup();
  }
});

test('reconciliation behind the Sync Store lets the store win storages regardless of age', async () => {
  const fixture = createFixture();
  try {
    writeLocalState(fixture.dshHome, { workspace: '{"workspaces":["local"]}' });
    seedHeadSeq(fixture.dshHome, 1);
    writeMarker(fixture.storeDir, 5);

    mkdirSync(join(fixture.storeDir, 'storages'), { recursive: true });
    const storeWorkspace = join(fixture.storeDir, 'storages', 'workspace.json');
    writeFileSync(storeWorkspace, '{"workspaces":["store"]}');
    setMtime(storeWorkspace, 1000000);
    setMtime(join(fixture.dshHome, 'storages', 'workspace.json'), 1600000000);

    const manager = fixture.manager();
    assert.equal(await manager.reconcile(), 'synchronized');
    assert.equal(manager.headSeq, 5);
    assert.equal(
      readFileSync(join(fixture.dshHome, 'storages', 'workspace.json'), 'utf8'),
      '{"workspaces":["store"]}',
      'the Sync Store wins storage units while it is ahead',
    );
    assert.equal(readFileSync(storeWorkspace, 'utf8'), '{"workspaces":["store"]}', 'storages must not be pushed');
    assert.ok(findFile(join(fixture.storeDir, 'sessions'), 'session.jsonl'), 'session logs still push as a union');
  } finally {
    fixture.cleanup();
  }
});

test('session logs never shrink: pulls skip shorter copies and pushes append', async () => {
  const fixture = createFixture();
  try {
    const longBody = '{"n":1}\n{"n":2}\n{"n":3}\n';
    const shortBody = '{"n":1}\n';
    seedHeadSeq(fixture.dshHome, 2);
    writeMarker(fixture.storeDir, 2);

    const localA = join(fixture.dshHome, 'sessions', 'root', 'project', 'grow-store');
    mkdirSync(localA, { recursive: true });
    writeFileSync(join(localA, 'session.jsonl'), longBody);
    const storeA = join(fixture.storeDir, 'sessions', 'root', 'project', 'grow-store');
    mkdirSync(storeA, { recursive: true });
    writeFileSync(join(storeA, 'session.jsonl'), shortBody);
    setMtime(join(storeA, 'session.jsonl'), 1000000);
    setMtime(join(localA, 'session.jsonl'), 1600000000);

    const localB = join(fixture.dshHome, 'sessions', 'root', 'project', 'keep-local');
    mkdirSync(localB, { recursive: true });
    writeFileSync(join(localB, 'session.jsonl'), longBody);
    setMtime(join(localB, 'session.jsonl'), 1000000);
    const storeB = join(fixture.storeDir, 'sessions', 'root', 'project', 'keep-local');
    mkdirSync(storeB, { recursive: true });
    writeFileSync(join(storeB, 'session.jsonl'), shortBody);
    setMtime(join(storeB, 'session.jsonl'), 1600000000);

    const manager = fixture.manager();
    assert.equal(await manager.reconcile(), 'synchronized');
    assert.equal(readFileSync(join(storeA, 'session.jsonl'), 'utf8'), longBody, 'the push must grow the Sync Store log');
    assert.equal(
      readFileSync(join(localB, 'session.jsonl'), 'utf8'),
      longBody,
      'a shorter Sync Store log must never replace the local one',
    );
  } finally {
    fixture.cleanup();
  }
});

test('exactly one follow-up transfer runs after an active transfer fails', async () => {
  const brokenRoot = join(createTempDir(), 'root-is-a-file');
  writeFileSync(brokenRoot, 'not a directory\n');

  async function run({ withFollowUp }) {
    const dshHome = createTempDir();
    const clock = countingClock();
    seedHeadSeq(dshHome, 0);
    writeLocalState(dshHome);
    const manager = new RemoteSyncManager({
      dshHome,
      statusFilePath: join(dshHome, STATUS_FILE_NAME),
      projectId: PROJECT_ID,
      retryDelayMs: 0,
      now: clock.now,
      syncConfig: { remote_sync_root: brokenRoot, writer_id: 'writer-under-test' },
    });
    const first = manager.triggerSync();
    if (withFollowUp) {
      const second = manager.triggerSync();
      assert.equal(manager.pendingFollowUp, true, 'a trigger during an active transfer must queue one follow-up');
      await second;
    }
    await first;
    assert.equal(manager.status, 'failed');
    rmSync(dshHome, { recursive: true, force: true });
    return clock.calls;
  }

  try {
    const single = await run({ withFollowUp: false });
    const withFollowUp = await run({ withFollowUp: true });
    assert.ok(single > 0);
    assert.equal(withFollowUp, single * 2, 'a failed transfer must still run its one queued follow-up');
  } finally {
    rmSync(brokenRoot, { force: true });
  }
});

test('an unreachable Sync Store fails a push after five attempts', async () => {
  const holder = createTempDir();
  const brokenRoot = join(holder, 'root-is-a-file');
  writeFileSync(brokenRoot, 'not a directory\n');
  const dshHome = createTempDir();
  const clock = countingClock();
  try {
    seedHeadSeq(dshHome, 0);
    writeLocalState(dshHome);
    const manager = new RemoteSyncManager({
      dshHome,
      statusFilePath: join(dshHome, STATUS_FILE_NAME),
      projectId: PROJECT_ID,
      retryDelayMs: 0,
      now: clock.now,
      syncConfig: { remote_sync_root: brokenRoot, writer_id: 'writer-under-test' },
    });
    assert.equal(await manager.triggerSync(), 'failed');
    assert.ok(manager.lastError && manager.lastError.length > 0);
    assert.equal(manager.headSeq, 0, 'a failed push must not move the local head');

    const status = JSON.parse(readFileSync(join(dshHome, STATUS_FILE_NAME), 'utf8'));
    assert.equal(status.status, 'failed');
    assert.ok(status.last_error.length > 0);
    // Five attempts, each reading the clock for its synchronizing write and its
    // head-marker timestamp, plus the final failed write.
    assert.equal(clock.calls, 11);
  } finally {
    rmSync(holder, { recursive: true, force: true });
    rmSync(dshHome, { recursive: true, force: true });
  }
});

test('reconciliation against an unreachable Sync Store is degraded and changes nothing', async () => {
  const fixture = createFixture({ createStoreDir: false });
  rmSync(fixture.storeRoot, { recursive: true, force: true });
  try {
    writeLocalState(fixture.dshHome, { workspace: '{"workspaces":["local"]}' });
    seedHeadSeq(fixture.dshHome, 4);
    const manager = fixture.manager();

    assert.equal(await manager.reconcile(), 'degraded');
    assert.equal(manager.headSeq, 4);
    assert.ok(manager.lastError.includes('Sync Store root unavailable'));
    assert.equal(existsSync(fixture.storeRoot), false, 'a degraded reconciliation must not create the Sync Store');
    assert.equal(
      readFileSync(join(fixture.dshHome, 'storages', 'workspace.json'), 'utf8'),
      '{"workspaces":["local"]}',
    );
  } finally {
    fixture.cleanup();
  }
});

test('the status file is written atomically with exactly the contract keys', async () => {
  const fixture = createFixture();
  try {
    writeLocalState(fixture.dshHome);
    seedHeadSeq(fixture.dshHome, 0);
    const manager = fixture.manager();
    assert.equal(await manager.triggerSync(), 'synchronized');

    const leftovers = readdirSync(fixture.dshHome).filter((entry) => entry.startsWith(`${STATUS_FILE_NAME}.tmp`));
    assert.deepEqual(leftovers, [], 'no temporary status files may remain');

    const status = JSON.parse(readFileSync(join(fixture.dshHome, STATUS_FILE_NAME), 'utf8'));
    assert.deepEqual(Object.keys(status).sort(), ['head_seq', 'last_error', 'status', 'updated_at']);
    assert.equal(status.status, 'synchronized');
    assert.equal(status.head_seq, 1);
    assert.equal(status.last_error, null);
    assert.equal(new Date(status.updated_at).toISOString(), status.updated_at);
  } finally {
    fixture.cleanup();
  }
});

test('a full push carries portable state only and never workstation-wide categories', async () => {
  const fixture = createFixture();
  try {
    writeLocalState(fixture.dshHome);
    seedHeadSeq(fixture.dshHome, 0);
    const home = fixture.dshHome;
    writeFileSync(join(home, 'storages', 'message_feedback.json'), '{"feedback":[]}');
    writeFileSync(join(home, 'storages', 'session_projcache.json'), '{"cache":true}');
    mkdirSync(join(home, 'attachments', 'v1', 'objects', 'ab'), { recursive: true });
    writeFileSync(join(home, 'attachments', 'v1', 'objects', 'ab', 'abcdef'), 'object-bytes');
    mkdirSync(join(home, 'attachments', 'v1', 'request-images', 'cd'), { recursive: true });
    writeFileSync(join(home, 'attachments', 'v1', 'request-images', 'cd', 'derived'), 'derived-image');
    for (const category of ['credentials', 'settings', 'plugins', 'presets', 'profiles']) {
      mkdirSync(join(home, category), { recursive: true });
      writeFileSync(join(home, category, 'file.json'), category);
    }
    writeFileSync(join(home, 'random-root-file.txt'), 'random');
    writeFileSync(join(home, HEAD_MARKER_NAME), '{"seq":999}');

    const manager = fixture.manager();
    assert.equal(await manager.triggerSync(), 'synchronized');

    const store = fixture.storeDir;
    assert.equal(existsSync(join(store, 'storages', 'workspace.json')), true);
    assert.equal(existsSync(join(store, 'storages', 'message_feedback.json')), true);
    assert.equal(existsSync(join(store, 'attachments', 'v1', 'objects', 'ab', 'abcdef')), true);
    assert.ok(findFile(join(store, 'sessions'), 'session.jsonl'));

    assert.equal(existsSync(join(store, 'storages', 'session_projcache.json')), false);
    assert.equal(existsSync(join(store, 'attachments', 'v1', 'request-images')), false);
    assert.equal(existsSync(join(store, STATUS_FILE_NAME)), false);
    assert.equal(existsSync(join(store, 'random-root-file.txt')), false);
    for (const category of ['credentials', 'settings', 'plugins', 'presets', 'profiles']) {
      assert.equal(existsSync(join(store, category)), false, `${category} must never transfer`);
    }
    assert.equal(readMarker(store).seq, 1, 'the head marker is never overwritten by a transfer');
  } finally {
    fixture.cleanup();
  }
});

test('the retry entry point starts no transfer while Session Sync is synchronized', async () => {
  const fixture = createFixture();
  try {
    writeLocalState(fixture.dshHome);
    seedHeadSeq(fixture.dshHome, 0);
    const manager = fixture.manager();
    assert.equal(await manager.triggerSync(), 'synchronized');
    const markerBefore = readMarker(fixture.storeDir);

    assert.equal(await manager.retry(), 'synchronized');
    assert.equal(manager.activeTransfer, null);
    assert.deepEqual(readMarker(fixture.storeDir), markerBefore, 'retry must not transfer while synchronized');
    assert.equal(manager.headSeq, 1);
  } finally {
    fixture.cleanup();
  }
});

test('real DSH persistence events push saved changes into the Sync Store', async () => {
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

  const dshHome = createTempDir();
  const storeRoot = createTempDir();
  const storeDir = join(storeRoot, PROJECT_ID);
  const workspaceDir = join(dshHome, 'workspace');
  const oldSyncConfigPath = process.env.DEVVM_SYNC_CONFIG_PATH;
  const oldProjectId = process.env.DEVVM_PROJECT_ID;
  const ctx = new Context();
  const forks = [];
  let workspaceDomain;
  let feedbackDomain;

  mkdirSync(workspaceDir);
  mkdirSync(storeDir, { recursive: true });
  seedHeadSeq(dshHome, 0);
  process.env.DEVVM_SYNC_CONFIG_PATH = join(dshHome, 'sync.json');
  process.env.DEVVM_PROJECT_ID = PROJECT_ID;
  writeFileSync(
    process.env.DEVVM_SYNC_CONFIG_PATH,
    JSON.stringify({ remote_sync_root: storeRoot, writer_id: 'writer-under-test' }),
  );

  try {
    forks.push(ctx.plugin(SessionStore));
    forks.push(ctx.plugin(JsonlSessionPersistence, { root: join(dshHome, 'sessions'), compression: 'none' }));
    forks.push(ctx.plugin(Storage));
    forks.push(ctx.plugin(storageJson, { root: join(dshHome, 'storages') }));
    forks.push(ctx.plugin(storageDomain, { backend: 'json' }));
    const remoteSyncFork = ctx.plugin(plugin, {
      dshHome,
      statusFilePath: join(dshHome, STATUS_FILE_NAME),
      retryDelayMs: 0,
    });
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
      () => findFile(storeDir, 'session.jsonl') !== null,
      'A real completed-turn session/event must push the session log into the Sync Store',
    );
    const storeLog = findFile(storeDir, 'session.jsonl');
    assert.match(readFileSync(storeLog, 'utf8'), /"type":"turn\/end"/);

    await workspaceDomain.global.set(workspaceDomain.global.get());
    await waitFor(
      () => existsSync(join(storeDir, 'storages', 'workspace.json')),
      'A real saved workspace domain change must push the storage unit',
    );

    await feedbackDomain.table('sessions').put(session.id, {
      session: { createdAt: session.header.createdAt, cwd: workspaceDir },
      items: [],
    });
    await waitFor(
      () => existsSync(join(storeDir, 'storages', 'message_feedback.json')),
      'A real saved message-feedback change must push the storage unit',
    );

    if (ctx.remoteSync.activeTransfer) await ctx.remoteSync.activeTransfer;
    assert.ok(ctx.remoteSync.headSeq >= 1, 'each successful push advances the head sequence');
    assert.equal(readMarker(storeDir).writer_id, 'writer-under-test');
  } finally {
    await feedbackDomain?.close();
    await workspaceDomain?.close();
    for (const fork of forks.reverse()) await fork.dispose();
    if (oldSyncConfigPath === undefined) delete process.env.DEVVM_SYNC_CONFIG_PATH;
    else process.env.DEVVM_SYNC_CONFIG_PATH = oldSyncConfigPath;
    if (oldProjectId === undefined) delete process.env.DEVVM_PROJECT_ID;
    else process.env.DEVVM_PROJECT_ID = oldProjectId;
    rmSync(dshHome, { recursive: true, force: true });
    rmSync(storeRoot, { recursive: true, force: true });
  }
});

test('reconcile.mjs always exits zero and records the outcome', async () => {
  function runReconcile(env) {
    return new Promise((resolve, reject) => {
      const child = spawn(process.execPath, ['reconcile.mjs'], {
        cwd: import.meta.dirname,
        env: { ...process.env, ...env },
        stdio: ['ignore', 'pipe', 'pipe'],
      });
      let stdout = '';
      let stderr = '';
      child.stdout.on('data', (chunk) => {
        stdout += chunk.toString();
      });
      child.stderr.on('data', (chunk) => {
        stderr += chunk.toString();
      });
      child.on('error', reject);
      child.on('close', (code) => resolve({ code, stdout, stderr }));
    });
  }

  const dshHome = createTempDir();
  const storeRoot = createTempDir();
  const missingRoot = join(storeRoot, 'absent');
  const configPath = join(dshHome, 'sync.json');
  try {
    writeLocalState(dshHome);
    const baseEnv = {
      DSH_HOME: dshHome,
      DEVVM_SYNC_STATUS_PATH: join(dshHome, STATUS_FILE_NAME),
      DEVVM_PROJECT_ID: PROJECT_ID,
      DEVVM_SYNC_CONFIG_PATH: join(dshHome, 'no-such-config.json'),
    };

    const notConfigured = await runReconcile(baseEnv);
    assert.equal(notConfigured.code, 0);
    assert.equal(JSON.parse(readFileSync(join(dshHome, STATUS_FILE_NAME), 'utf8')).status, 'not_configured');

    writeFileSync(configPath, JSON.stringify({ remote_sync_root: missingRoot, writer_id: 'writer-under-test' }));
    const degraded = await runReconcile({ ...baseEnv, DEVVM_SYNC_CONFIG_PATH: configPath });
    assert.equal(degraded.code, 0);
    assert.equal(JSON.parse(readFileSync(join(dshHome, STATUS_FILE_NAME), 'utf8')).status, 'degraded');
    assert.match(degraded.stdout, /remote-sync: status degraded/);

    writeFileSync(configPath, JSON.stringify({ remote_sync_root: storeRoot, writer_id: 'writer-under-test' }));
    const synchronized = await runReconcile({ ...baseEnv, DEVVM_SYNC_CONFIG_PATH: configPath });
    assert.equal(synchronized.code, 0);
    const status = JSON.parse(readFileSync(join(dshHome, STATUS_FILE_NAME), 'utf8'));
    assert.equal(status.status, 'synchronized');
    assert.equal(status.head_seq, 0);
    assert.ok(findFile(join(storeRoot, PROJECT_ID, 'sessions'), 'session.jsonl'));
    assert.match(synchronized.stdout, /remote-sync: status synchronized/);
  } finally {
    rmSync(dshHome, { recursive: true, force: true });
    rmSync(storeRoot, { recursive: true, force: true });
  }
});

test('filter lists keep the union and storage passes separate', () => {
  assert.equal(UNION_FILTER_ARGS.at(-1), '--exclude=*');
  assert.equal(STORAGES_FILTER_ARGS.at(-1), '--exclude=*');
  assert.ok(UNION_FILTER_ARGS.includes('--include=sessions/***'));
  assert.ok(!UNION_FILTER_ARGS.some((arg) => arg.includes('storages')));
  assert.ok(STORAGES_FILTER_ARGS.includes('--exclude=storages/session_projcache.json'));
  assert.ok(!STORAGES_FILTER_ARGS.some((arg) => arg.includes('sessions')));
});

test('manifest, bundle patch, and client resolution contract', async () => {
  const pkgPath = join(import.meta.dirname, 'package.json');
  assert.ok(existsSync(pkgPath), 'package.json must exist');
  const pkg = JSON.parse(readFileSync(pkgPath, 'utf8'));

  assert.equal(pkg.name, '@devvm/dsh-remote-sync');
  assert.equal(pkg.type, 'module');
  assert.equal(pkg.main, './index.mjs');

  assert.ok(pkg.dsh?.bundle?.patch, 'dsh.bundle.patch must be declared');
  assert.equal(pkg.dsh.bundle.patch, './cordis.patch.yml');
  const patchFullPath = join(import.meta.dirname, pkg.dsh.bundle.patch);
  assert.ok(existsSync(patchFullPath), 'Declared cordis.patch.yml must exist');

  const patchContent = readFileSync(patchFullPath, 'utf8');
  assert.ok(patchContent.includes('id: remote-sync'), 'cordis.patch.yml must declare id: remote-sync');
  assert.ok(
    patchContent.includes("name: '@devvm/dsh-remote-sync'"),
    'cordis.patch.yml must insert host plugin @devvm/dsh-remote-sync',
  );

  assert.equal(pkg.dsh?.client?.platform, 'web', 'dsh.client.platform must be web');

  assert.equal(pkg.exports?.['.'], './index.mjs');
  assert.equal(pkg.exports?.['./client'], './client.js');
  assert.equal(pkg.exports?.['./reconcile'], './reconcile.mjs');
  for (const relative of ['./index.mjs', './client.js', './reconcile.mjs']) {
    assert.ok(existsSync(join(import.meta.dirname, relative)), `${relative} must exist`);
    assert.ok(pkg.files.includes(relative.slice(2)), `${relative} must be published`);
  }
  assert.ok(statSync(join(import.meta.dirname, 'reconcile.mjs')).size > 0);
});

test('web profile integration - dump-config, profile isolation, and sync routes', async () => {
  const { execSync } = await import('node:child_process');

  const webDump = execSync('dsh --profile web --dump-config', { encoding: 'utf8' });
  const remoteSyncMatches = webDump.match(/name: ['"]?@devvm\/dsh-remote-sync['"]?/g) || [];
  const voiceInputMatches = webDump.match(/name: ['"]?@devvm\/dsh-voice-input['"]?/g) || [];
  assert.equal(remoteSyncMatches.length, 1, 'web dump-config must include @devvm/dsh-remote-sync exactly once');
  assert.equal(voiceInputMatches.length, 1, 'web dump-config must include @devvm/dsh-voice-input exactly once');
  assert.ok(
    webDump.includes('/root/voice-dictation-cleanup/data/archive_voice_input.jsonl'),
    'voice-input config path must be preserved',
  );

  const headlessDump = execSync('dsh --profile headless --dump-config', { encoding: 'utf8' });
  assert.ok(!headlessDump.includes('@devvm/dsh-remote-sync'), 'headless profile dump must exclude @devvm/dsh-remote-sync');
  assert.ok(!headlessDump.includes('@devvm/dsh-voice-input'), 'headless profile dump must exclude @devvm/dsh-voice-input');

  const testPort = '3599';
  const dshProcess = spawn('dsh', ['--profile', 'web', '--no-open', '--port', testPort], {
    stdio: ['ignore', 'pipe', 'pipe'],
    env: { ...process.env, DEVVM_SYNC_STATUS_PATH: join(createTempDir(), STATUS_FILE_NAME) },
  });

  let output = '';
  let errorOutput = '';

  const bootPromise = new Promise((resolve, reject) => {
    dshProcess.stdout.on('data', (chunk) => {
      output += chunk.toString();
      if (output.includes(`dsh web: http://127.0.0.1:${testPort}`)) resolve({ success: true });
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
    const statusBody = await statusRes.json();
    assert.deepEqual(
      Object.keys(statusBody).sort(),
      ['daemon_url', 'head_seq', 'last_error', 'project_id', 'status', 'updated_at'],
    );

    const retryRes = await fetch(`http://127.0.0.1:${testPort}/api/sync/retry`, { method: 'POST' });
    assert.equal(retryRes.status, 200, '/api/sync/retry endpoint must return HTTP 200');
    assert.ok(typeof (await retryRes.json()).status === 'string');

    const triggerRes = await fetch(`http://127.0.0.1:${testPort}/api/sync/trigger`, { method: 'POST' });
    assert.notEqual(triggerRes.status, 200, 'the removed /api/sync/trigger route must not answer');
  } finally {
    dshProcess.kill('SIGTERM');
  }
});
