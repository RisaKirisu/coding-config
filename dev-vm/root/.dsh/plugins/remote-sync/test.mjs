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
  PROJECTION_FILTER_ARGS,
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

/** Run one `reconcile.mjs` as a fresh child process against the index.mjs on disk. */
function runReconcileChild(env) {
  return new Promise((resolve, reject) => {
    const child = spawn(process.execPath, ['reconcile.mjs'], {
      cwd: import.meta.dirname,
      env,
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

function workspaceTitleOf(path) {
  const trimmed = path.replace(/[/\\]+$/, '');
  const separator = Math.max(trimmed.lastIndexOf('/'), trimmed.lastIndexOf('\\'));
  return trimmed.slice(separator + 1);
}

function displayTitleOf(title, cwd, id) {
  if (title !== undefined && title !== null && title !== '') return title;
  if (cwd !== undefined && cwd !== '') {
    const base = workspaceTitleOf(cwd);
    if (base !== '') return base;
  }
  return id;
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

test('a Sync Store that moved ahead suspends storage pushes but keeps session and projection pushes', async () => {
  const fixture = createFixture();
  try {
    writeLocalState(fixture.dshHome);
    seedHeadSeq(fixture.dshHome, 0);
    writeMarker(fixture.storeDir, 5);
    mkdirSync(join(fixture.storeDir, 'storages'), { recursive: true });
    writeFileSync(join(fixture.storeDir, 'storages', 'workspace.json'), '{"workspaces":["remote"]}');
    const projDir = join(fixture.dshHome, 'storages', 'session_projcache', 'sessions');
    mkdirSync(projDir, { recursive: true });
    writeFileSync(join(projDir, 'session-ahead.json'), '{"title":"Ahead Projection"}');

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
    assert.equal(
      existsSync(join(fixture.storeDir, 'storages', 'session_projcache', 'sessions', 'session-ahead.json')),
      true,
      'projection documents must still push while the Sync Store is ahead',
    );

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

    // A projection document newer locally must reach the store (newest-wins),
    // and a stale store document must never overwrite the newer local one.
    const projDir = join(fixture.dshHome, 'storages', 'session_projcache', 'sessions');
    const storeProjDir = join(fixture.storeDir, 'storages', 'session_projcache', 'sessions');
    mkdirSync(projDir, { recursive: true });
    mkdirSync(storeProjDir, { recursive: true });
    writeFileSync(join(projDir, 'newer.json'), '{"title":"Local New"}');
    setMtime(join(projDir, 'newer.json'), 1600000000);
    writeFileSync(join(storeProjDir, 'newer.json'), '{"title":"Store Stale"}');
    setMtime(join(storeProjDir, 'newer.json'), 1000000);
    writeFileSync(join(storeProjDir, 'store-only.json'), '{"title":"Store Older"}');
    setMtime(join(storeProjDir, 'store-only.json'), 1000000);
    writeFileSync(join(projDir, 'local-only.json'), '{"title":"Local Fresh"}');
    setMtime(join(projDir, 'local-only.json'), 1600000000);
    // The newest-wins discriminator: the LOCAL checkpoint is OLDER than the
    // store's. The push must skip it (`--update`) and the pull must deliver the
    // store's newer whole record; a newest-wins pass without `--update` would
    // clobber the store with the older local document.
    writeFileSync(join(projDir, 'clobber.json'), '{"title":"Local Older"}');
    setMtime(join(projDir, 'clobber.json'), 900000000);
    writeFileSync(join(storeProjDir, 'clobber.json'), '{"title":"Store Newer"}');
    setMtime(join(storeProjDir, 'clobber.json'), 1600000000);

    const manager = fixture.manager();
    assert.equal(await manager.reconcile(), 'synchronized');
    assert.equal(manager.headSeq, 3);
    assert.equal(readFileSync(storeWorkspace, 'utf8'), '{"workspaces":["local"]}');
    assert.equal(
      existsSync(join(fixture.dshHome, 'sessions', 'root', 'project', 'session-store', 'session.jsonl')),
      true,
    );
    // The projection pass is newest-wins whole-file in both directions.
    assert.equal(
      readFileSync(join(storeProjDir, 'newer.json'), 'utf8'),
      '{"title":"Local New"}',
      'the newest projection document must be pushed to the Sync Store',
    );
    assert.equal(
      readFileSync(join(projDir, 'store-only.json'), 'utf8'),
      '{"title":"Store Older"}',
      'a projection document absent locally must be pulled from the Sync Store',
    );
    assert.equal(
      readFileSync(join(projDir, 'local-only.json'), 'utf8'),
      '{"title":"Local Fresh"}',
      'a projection document only on the local side must survive the reconcile',
    );
    assert.equal(
      readFileSync(join(projDir, 'newer.json'), 'utf8'),
      '{"title":"Local New"}',
      'a stale store projection must never overwrite the newer local one',
    );
    assert.equal(
      readFileSync(join(projDir, 'clobber.json'), 'utf8'),
      '{"title":"Store Newer"}',
      'a newer store projection must not be clobbered by an older local one (newest wins in both directions)',
    );
    assert.equal(
      readFileSync(join(storeProjDir, 'clobber.json'), 'utf8'),
      '{"title":"Store Newer"}',
      'the store must keep its newer projection document',
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
  const tempRoot = createTempDir();
  const brokenRoot = join(tempRoot, 'root-is-a-file');
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
    rmSync(tempRoot, { recursive: true, force: true });
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
    mkdirSync(join(home, 'storages', 'session_projcache', 'sessions'), { recursive: true });
    writeFileSync(join(home, 'storages', 'session_projcache', 'sessions', 'session-a.json'), '{"title":"Session A"}');
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
    assert.equal(
      existsSync(join(store, 'storages', 'session_projcache', 'sessions', 'session-a.json')),
      true,
      'per-session projection documents must transfer as portable state',
    );
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

test('session projection documents synchronize in the projection pass and preserve cold listing titles', async () => {
  const dshModules = '/usr/local/lib/node_modules/@deepseek-ai/dsh/node_modules/@deepseek-ai';
  const [
    { Context },
    { default: SessionStore },
    { default: JsonlSessionPersistence },
    { default: Storage },
    storageJson,
    storageDomain,
    { default: SessionProjection },
    { default: SessionProjectionCache },
    { default: SessionTitleService },
    { default: SessionQuery },
    { workspaceDomainSpec },
    { ApiSessionList },
  ] = await Promise.all([
    import('/usr/local/lib/node_modules/@deepseek-ai/dsh/node_modules/@deepseek-ai/cordis/lib/index.js'),
    import(`${dshModules}/dsh-session/lib/index.js`),
    import(`${dshModules}/dsh-session-persistence-jsonl/lib/index.js`),
    import(`${dshModules}/dsh-storage/lib/index.js`),
    import(`${dshModules}/dsh-storage-json/lib/index.js`),
    import(`${dshModules}/dsh-storage-domain/lib/index.js`),
    import(`${dshModules}/dsh-session-projection/lib/index.js`),
    import(`${dshModules}/dsh-session-projection-cache/lib/index.js`),
    import(`${dshModules}/dsh-session-title/lib/index.js`),
    import(`${dshModules}/dsh-session-query/lib/index.js`),
    import(`${dshModules}/dsh-workspace/lib/index.js`),
    import(`${dshModules}/dsh-api-session-controller/lib/types/list.js`),
  ]);

  async function createDshInstance(home) {
    const ctx = new Context();
    const forks = [];
    forks.push(ctx.plugin(SessionStore));
    forks.push(ctx.plugin(JsonlSessionPersistence, { root: join(home, 'sessions') }));
    forks.push(ctx.plugin(Storage));
    forks.push(ctx.plugin(storageJson, { root: join(home, 'storages') }));
    forks.push(ctx.plugin(storageDomain, { backend: 'json' }));
    forks.push(ctx.plugin(SessionProjection));
    forks.push(ctx.plugin(SessionProjectionCache, { writeEveryEvents: 200, writeIntervalMs: 5000 }));
    forks.push(ctx.plugin(SessionTitleService, { fallbackMaxWords: 10, fallbackMaxBytes: 100, maxTitleBytes: 200 }));
    forks.push(ctx.plugin(SessionQuery));
    await waitFor(
      () => ctx.sessions && ctx.sessionPersistence && ctx.storageDomain && ctx.sessionProjections && ctx.sessionProjectionCache && ctx.sessionTitle && ctx.sessionQuery,
      'DSH core services activation',
    );
    ctx.provide('agents', { get: () => undefined });
    const listState = new ApiSessionList(ctx, 1024);
    return {
      ctx,
      forks,
      listState,
      async close() {
        for (const f of forks.reverse()) await f.dispose();
      },
    };
  }

  const fixture = createFixture();
  const workstationBHome = createTempDir();
  const sessionId = 'session-cold-title-test';
  const realTitle = 'Add Authentication Middleware';
  const workspaceCwd = '/root/.dsh';

  try {
    // 1. In workstation A (fixture.dshHome), create a session whose compressed log is > 1024 bytes
    const instA = await createDshInstance(fixture.dshHome);
    const wsDomainA = await instA.ctx.storageDomain.open(workspaceDomainSpec);
    await wsDomainA.table('workspaces').put('ws-1', {
      path: workspaceCwd,
      title: '.dsh',
      sessionIds: [sessionId],
      createdAt: new Date().toISOString(),
      updatedAt: new Date().toISOString(),
    });

    const sessionA = instA.ctx.sessions.create(sessionId, { meta: { cwd: workspaceCwd } });
    sessionA.append('turn/start', { turn: 1 });
    sessionA.append('user/message', {
      id: 'msg-user-1',
      role: 'user',
      source: { kind: 'user' },
      content: [{ type: 'text', text: 'Please implement authentication middleware for the API router.' }],
    }, { surfaceOp: 'append' });
    const userMsgSeq = sessionA.snapshotEvents().at(-1).seq;
    sessionA.append('session/title', {
      title: realTitle,
      source: { kind: 'fallback' },
      messageSeqs: [userMsgSeq],
    });
    let longBody = 'Here is the comprehensive middleware implementation:\n';
    for (let i = 0; i < 400; i++) {
      longBody += `Step ${i}: configure route handler authentication with token verification and scope checks ${i * 7919} for endpoint /api/v1/resource/${i * 104729}.\n`;
    }
    sessionA.append('assistant/message', {
      message: {
        id: 'msg-assistant-1',
        role: 'assistant',
        source: { kind: 'model', provider: 'deepseek', model: 'deepseek-chat' },
        content: [{ type: 'text', text: longBody }],
      },
    }, { surfaceOp: 'append' });
    sessionA.append('turn/end', { turn: 1, reason: { kind: 'completed' } });

    await instA.ctx.sessions.flush(sessionA);
    await instA.ctx.sessionProjectionCache.write(sessionA);

    const loc = instA.ctx.sessionPersistence.locate(sessionA.header);
    const fileSizeA = statSync(loc.path).size;
    assert.ok(fileSizeA > 1024, `Compressed session log must exceed 1024 bytes (was ${fileSizeA})`);

    const projDocPath = join(fixture.dshHome, 'storages', 'session_projcache', 'sessions', `${sessionId}.json`);
    assert.ok(existsSync(projDocPath), 'Session projection document must exist in Workstation A');

    await wsDomainA.close();
    await instA.close();

    // 2. Push from Workstation A to Sync Store using real RemoteSyncManager
    const managerA = fixture.manager();
    assert.equal(await managerA.reconcile(), 'synchronized');
    assert.ok(
      existsSync(join(fixture.storeDir, 'storages', 'session_projcache', 'sessions', `${sessionId}.json`)),
      'Projection document must be transferred to Sync Store',
    );

    // 3. Pull from Sync Store to fresh Workstation B using real RemoteSyncManager
    const managerB = new RemoteSyncManager({
      dshHome: workstationBHome,
      statusFilePath: join(workstationBHome, STATUS_FILE_NAME),
      projectId: PROJECT_ID,
      retryDelayMs: 0,
      syncConfig: { remote_sync_root: fixture.storeRoot, writer_id: 'workstation-b' },
    });
    assert.equal(await managerB.reconcile(), 'synchronized');
    assert.ok(
      existsSync(join(workstationBHome, 'storages', 'session_projcache', 'sessions', `${sessionId}.json`)),
      'Projection document must be pulled into Workstation B',
    );

    // 4. Cold-list sessions in Workstation B without opening the session
    const instB = await createDshInstance(workstationBHome);
    const coldSummariesB = await instB.listState.list();
    const summaryB = coldSummariesB.find((s) => s.sessionId === sessionId);
    assert.ok(summaryB, 'Session summary must be returned in cold listing');

    const titleProjection = summaryB.projections?.values?.title;
    const computedDisplayTitle = displayTitleOf(titleProjection, summaryB.cwd, summaryB.sessionId);

    // Assert returned title projection is the real chat title, not the workspace basename
    assert.equal(titleProjection, realTitle, `Cold title projection must be "${realTitle}", got "${titleProjection}"`);
    assert.notEqual(computedDisplayTitle, workspaceTitleOf(workspaceCwd), 'Display title must not fall back to workspace basename');
    assert.equal(computedDisplayTitle, realTitle, `Computed display title must be "${realTitle}"`);

    await instB.close();
  } finally {
    fixture.cleanup();
    rmSync(workstationBHome, { recursive: true, force: true });
  }
});


test('projection documents replace whole records: append-only flags are never applied to them', async () => {
  const fixture = createFixture();
  const staleHome = createTempDir();
  try {
    seedHeadSeq(fixture.dshHome, 2);
    writeMarker(fixture.storeDir, 2);

    const sessionsDir = join(fixture.dshHome, 'storages', 'session_projcache', 'sessions');
    const storeProjDir = join(fixture.storeDir, 'storages', 'session_projcache', 'sessions');
    mkdirSync(sessionsDir, { recursive: true });
    mkdirSync(storeProjDir, { recursive: true });

    // The authoritative sender document: a whole-record JSON checkpoint.
    const freshDoc = JSON.stringify({
      version: 5,
      record: {
        identity: { createdAt: 1788000000000, cwd: '/root/.dsh' },
        rows: { title: { ver: 2, seq: 11, val: 'Add Authentication Middleware' } },
      },
    });
    writeFileSync(join(sessionsDir, 'session-x.json'), freshDoc);

    // The stale receiver document: different content and LONGER than the sender.
    // Under `--append-verify` a longer receiver is either left with its stale
    // record or spliced as stale-prefix plus sender suffix; it is never replaced
    // by the sender's whole record.
    const staleDoc = JSON.stringify({
      version: 5,
      record: {
        identity: { createdAt: 1788000000000, cwd: '/root/.dsh' },
        rows: { title: { ver: 1, seq: 9, val: 'OLD' } },
      },
      tail: { padding: 'this stale record is written longer than the fresh sender record' },
    });
    assert.ok(staleDoc.length > freshDoc.length, 'stale receiver must be longer than the sender');
    writeFileSync(join(storeProjDir, 'session-x.json'), staleDoc);
    setMtime(join(storeProjDir, 'session-x.json'), 1000000);
    setMtime(join(sessionsDir, 'session-x.json'), 1600000000);

    // Mutation: route the projection documents through the append-only union
    // flags, then reconcile in a fresh child process so the change is loaded
    // (this test file's own static import is cached before the mutation).
    const indexPath = join(import.meta.dirname, 'index.mjs');
    const original = readFileSync(indexPath, 'utf8');
    const mutated = original.replaceAll(
      'NEWEST_WINS_FLAGS, PROJECTION_FILTER_ARGS',
      'UNION_FLAGS, PROJECTION_FILTER_ARGS',
    );
    assert.notEqual(mutated, original, 'mutation must change projection transfer sites');
    writeFileSync(indexPath, mutated);
    const configPath = join(staleHome, 'sync.json');
    writeFileSync(configPath, JSON.stringify({ remote_sync_root: fixture.storeRoot, writer_id: 'mutation' }));
    let mutatedTitle;
    try {
      const mutatedResult = await runReconcileChild({
        DSH_HOME: fixture.dshHome,
        DEVVM_SYNC_STATUS_PATH: join(fixture.dshHome, STATUS_FILE_NAME),
        DEVVM_PROJECT_ID: PROJECT_ID,
        DEVVM_SYNC_CONFIG_PATH: configPath,
      });
      assert.equal(mutatedResult.code, 0, mutatedResult.stderr);
      const mutatedStore = readFileSync(join(storeProjDir, 'session-x.json'), 'utf8');
      try {
        mutatedTitle = JSON.parse(mutatedStore).record.rows.title.val;
      } catch {
        // Spliced stale-prefix plus sender suffix: also the bug.
      }
    } finally {
      writeFileSync(indexPath, original);
    }
    assert.notEqual(
      mutatedTitle,
      'Add Authentication Middleware',
      'the append-only union flags must never deliver the fresh projection record',
    );

    // Real code path: the dedicated newest-wins projection pass replaces the
    // whole record with the fresh title.
    assert.equal(await fixture.manager().reconcile(), 'synchronized');
    const storeDoc = readFileSync(join(storeProjDir, 'session-x.json'), 'utf8');
    let storeJson;
    assert.doesNotThrow(() => {
      storeJson = JSON.parse(storeDoc);
    }, `the Sync Store projection document must remain valid JSON, got: ${storeDoc}`);
    assert.equal(storeJson.record.rows.title.val, 'Add Authentication Middleware');

    // A fresh workstation must never receive a stale checkpoint; the pull
    // replaces the whole record with the newest one.
    const managerB = new RemoteSyncManager({
      dshHome: staleHome,
      statusFilePath: join(staleHome, STATUS_FILE_NAME),
      projectId: PROJECT_ID,
      retryDelayMs: 0,
      syncConfig: { remote_sync_root: fixture.storeRoot, writer_id: 'workstation-b' },
    });
    assert.equal(await managerB.reconcile(), 'synchronized');
    const pulled = readFileSync(
      join(staleHome, 'storages', 'session_projcache', 'sessions', 'session-x.json'),
      'utf8',
    );
    const pulledJson = JSON.parse(pulled);
    assert.equal(pulledJson.record.rows.title.val, 'Add Authentication Middleware');
  } finally {
    fixture.cleanup();
    rmSync(staleHome, { recursive: true, force: true });
  }
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
  const statusDir = createTempDir();
  const dshProcess = spawn('dsh', ['--profile', 'web', '--no-open', '--port', testPort], {
    stdio: ['ignore', 'pipe', 'pipe'],
    env: { ...process.env, DEVVM_SYNC_STATUS_PATH: join(statusDir, STATUS_FILE_NAME) },
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

    const directRes = await fetch(`http://127.0.0.1:${testPort}/plugins/@devvm/dsh-remote-sync/client.js`);
    assert.equal(directRes.status, 404, 'Direct client.js endpoint is removed and must return HTTP 404');

    const tokenMatch = output.match(/\?token=([^\s\r\n]+)/);
    assert.ok(tokenMatch, 'dsh web output must include launch token');
    const authRes = await fetch(`http://127.0.0.1:${testPort}/?token=${tokenMatch[1]}`, { redirect: 'manual' });
    const cookie = authRes.headers.get('set-cookie');
    const indexRes = await fetch(`http://127.0.0.1:${testPort}/`, {
      headers: cookie ? { cookie } : {},
    });
    assert.equal(indexRes.status, 200, 'Index HTML must return HTTP 200');
    const indexHtml = await indexRes.text();
    const comboMatch = indexHtml.match(/\/plugins\/\?\?@devvm\/dsh-remote-sync\/client\.js&rev=[^"'\s\\]+/);
    assert.ok(comboMatch, 'Combo URL for @devvm/dsh-remote-sync must be present in index HTML');

    const clientRes = await fetch(`http://127.0.0.1:${testPort}${comboMatch[0]}`);
    assert.equal(clientRes.status, 200, 'Client bundle combo endpoint must return HTTP 200');
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
    rmSync(statusDir, { recursive: true, force: true });
  }
});
