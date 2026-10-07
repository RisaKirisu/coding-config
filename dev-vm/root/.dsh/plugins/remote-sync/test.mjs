import test from 'node:test';
import assert from 'node:assert/strict';
import { execFileSync, spawn } from 'node:child_process';
import { once } from 'node:events';
import { pathToFileURL } from 'node:url';
import { zstdCompressSync } from 'node:zlib';
import { boot, getDshRuntimeVersion } from '@deepseek-ai/dsh-app-boot';
import { workspaceDomainSpec } from '@deepseek-ai/dsh-workspace';
import { projectionCacheDomainSpec } from '@deepseek-ai/dsh-session-projection-cache';
import {
  cpSync,
  existsSync,
  mkdirSync,
  mkdtempSync,
  readFileSync,
  readdirSync,
  rmSync,
  statSync,
  symlinkSync,
  utimesSync,
  writeFileSync,
} from 'node:fs';
import { homedir, tmpdir } from 'node:os';
import { basename, dirname, join, relative } from 'node:path';
import {
  HEAD_MARKER_NAME,
  PROJECTION_FILTER_ARGS,
  RemoteSyncManager,
} from './index.mjs';

const PROJECT_ID = '00000000-0000-4000-8000-000000000001';
const candidateHome = process.env.DSH_HOME;
const packageEntry = process.env.DSH_PACKAGE_ENTRY;
assert.ok(candidateHome && packageEntry, 'Run with staged DSH_HOME and DSH_PACKAGE_ENTRY');
const runtimePackage = JSON.parse(readFileSync(packageEntry, 'utf8'));
const candidateCli = join(dirname(packageEntry), runtimePackage.bin.dsh);
const candidateOverlay = join(candidateHome, 'test-data', 'candidate-web.patch.yml');

const { ApiSessionList } = await import(new URL('types/list.js', import.meta.resolve('@deepseek-ai/dsh-api-session-controller')));

test('tests resolve exact rc.2 through the staged plugin dependency fallback', () => {
  assert.equal(runtimePackage.version, '0.2.0-rc.2');
  assert.equal(getDshRuntimeVersion(), '0.2.0-rc.2');
});

/** A test-only Loader composition with the real agent-owned persistence lifecycle. */
async function createDshInstance(home, remoteSyncConfig, compression = 'none') {
  const entries = [
    { id: 'session', name: '@deepseek-ai/dsh-session' },
    { id: 'agent', name: '@deepseek-ai/dsh-agent' },
    { id: 'llm', name: '@deepseek-ai/dsh-llm' },
    { id: 'tools', name: '@deepseek-ai/dsh-tools' },
    { id: 'system-prompt', name: '@deepseek-ai/dsh-system-prompt' },
    { id: 'session-projection', name: '@deepseek-ai/dsh-session-projection' },
    { id: 'agent-loop', name: '@deepseek-ai/dsh-agent-loop', config: {} },
    { id: 'session-persistence-jsonl', name: '@deepseek-ai/dsh-session-persistence-jsonl', config: { root: join(home, 'sessions'), compression } },
    { id: 'storage', name: '@deepseek-ai/dsh-storage' },
    { id: 'storage-json', name: '@deepseek-ai/dsh-storage-json', config: { root: join(home, 'storages') } },
    { id: 'storage-domain', name: '@deepseek-ai/dsh-storage-domain', config: { backend: 'json' } },
    { id: 'session-projection-cache', name: '@deepseek-ai/dsh-session-projection-cache', config: { writeEveryEvents: 200, writeIntervalMs: 5000 } },
    { id: 'session-title', name: '@deepseek-ai/dsh-session-title', config: { fallbackMaxWords: 10, fallbackMaxBytes: 100, maxTitleBytes: 200 } },
    { id: 'session-query-sqlite', name: '@deepseek-ai/dsh-session-query-sqlite', config: { path: ':memory:', openAt: 'never' } },
    { id: 'attachment-local', name: '@deepseek-ai/dsh-attachment-local', config: { dshHome: home } },
    { id: 'message-feedback', name: '@deepseek-ai/dsh-message-feedback', config: { maxNoteBytes: 4096 } },
  ];
  if (remoteSyncConfig) entries.push({ id: 'remote-sync', name: pathToFileURL(join(import.meta.dirname, 'index.mjs')).href, config: remoteSyncConfig });
  const configPath = join(home, 'test.cordis.json');
  writeFileSync(configPath, JSON.stringify(entries));
  const ctx = await boot('remote-sync-test', configPath, [], undefined, import.meta.url);
  const failures = [...ctx.loader.entries()].filter((entry) => entry.fiber?.state !== 2);
  assert.deepEqual(failures.map((entry) => entry.options.id), [], 'Every test composition entry must activate');
  return { ctx, listState: new ApiSessionList(ctx), close: () => ctx.fiber.dispose() };
}

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
  writeFileSync(join(sessionDir, 'session.v4.jsonl'), sessionBody);
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
    assert.ok(findFile(join(fixture.storeDir, 'sessions'), 'session.v4.jsonl'), 'union push must carry the session log');
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

for (const [pass, filePath] of [
  ['union', 'sessions/root/project/session-a/session.v4.jsonl'],
  ['projection', 'storages/session_projcache/sessions/session-a.json'],
  ['storage', 'storages/workspace.json'],
]) {
  test(`a failed ${pass} transfer preserves both heads and retries successfully`, async () => {
    const fixture = createFixture();
    try {
      writeLocalState(fixture.dshHome);
      seedHeadSeq(fixture.dshHome, 27);
      writeMarker(fixture.storeDir, 27, 'writer-under-test');
      const projectionPath = join(fixture.dshHome, 'storages', 'session_projcache', 'sessions', 'session-a.json');
      mkdirSync(dirname(projectionPath), { recursive: true });
      writeFileSync(projectionPath, '{"title":"Session A"}');

      // Real rsync cannot replace a nonempty directory with a regular file.
      const blockedPath = join(fixture.storeDir, filePath);
      mkdirSync(blockedPath, { recursive: true });
      writeFileSync(join(blockedPath, 'occupied'), 'block transfer');
      const markerBefore = readMarker(fixture.storeDir);
      const manager = fixture.manager();

      assert.equal(await manager.triggerSync(), 'failed');
      assert.equal(manager.headSeq, 27);
      assert.deepEqual(readMarker(fixture.storeDir), markerBefore, 'failed transfers must not publish a new head');
      assert.match(manager.lastError, /rsync push failed/);
      const persisted = JSON.parse(readFileSync(join(fixture.dshHome, STATUS_FILE_NAME), 'utf8'));
      assert.equal(persisted.status, 'failed');
      assert.equal(persisted.head_seq, 27);

      assert.equal(relative(fixture.storeDir, blockedPath), filePath);
      rmSync(blockedPath, { recursive: true });
      assert.equal(await manager.retry(), 'synchronized');
      assert.equal(manager.headSeq, 28);
      assert.equal(readMarker(fixture.storeDir).seq, 28);
      assert.equal(readMarker(fixture.storeDir).writer_id, 'writer-under-test');
      assert.equal(manager.lastError, null);
      assert.equal(readFileSync(join(fixture.storeDir, filePath), 'utf8'), readFileSync(join(fixture.dshHome, filePath), 'utf8'));
      assert.equal(readFileSync(join(fixture.storeDir, 'storages', 'workspace.json'), 'utf8'), '{"workspaces":[]}');
    } finally {
      fixture.cleanup();
    }
  });
}

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
    assert.ok(findFile(join(fixture.storeDir, 'sessions'), 'session.v4.jsonl'));
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
    writeFileSync(join(storeSession, 'session.v4.jsonl'), '{"type":"turn/start"}\n');

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
      existsSync(join(fixture.dshHome, 'sessions', 'root', 'project', 'session-store', 'session.v4.jsonl')),
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
    assert.ok(findFile(join(fixture.storeDir, 'sessions'), 'session.v4.jsonl'), 'session logs still push as a union');
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
    writeFileSync(join(localA, 'session.v4.jsonl'), longBody);
    const storeA = join(fixture.storeDir, 'sessions', 'root', 'project', 'grow-store');
    mkdirSync(storeA, { recursive: true });
    writeFileSync(join(storeA, 'session.v4.jsonl'), shortBody);
    setMtime(join(storeA, 'session.v4.jsonl'), 1000000);
    setMtime(join(localA, 'session.v4.jsonl'), 1600000000);

    const localB = join(fixture.dshHome, 'sessions', 'root', 'project', 'keep-local');
    mkdirSync(localB, { recursive: true });
    writeFileSync(join(localB, 'session.v4.jsonl'), longBody);
    setMtime(join(localB, 'session.v4.jsonl'), 1000000);
    const storeB = join(fixture.storeDir, 'sessions', 'root', 'project', 'keep-local');
    mkdirSync(storeB, { recursive: true });
    writeFileSync(join(storeB, 'session.v4.jsonl'), shortBody);
    setMtime(join(storeB, 'session.v4.jsonl'), 1600000000);

    const manager = fixture.manager();
    assert.equal(await manager.reconcile(), 'synchronized');
    assert.equal(readFileSync(join(storeA, 'session.v4.jsonl'), 'utf8'), longBody, 'the push must grow the Sync Store log');
    assert.equal(
      readFileSync(join(localB, 'session.v4.jsonl'), 'utf8'),
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
    // Five synchronizing writes and the final failure; no marker commit is attempted.
    assert.equal(clock.calls, 6);
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
    assert.ok(findFile(join(store, 'sessions'), 'session.v4.jsonl'));

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
  const fixture = createFixture();
  const workspaceDir = join(fixture.dshHome, 'workspace');
  mkdirSync(workspaceDir);
  seedHeadSeq(fixture.dshHome, 0);
  let instance;
  let workspaceDomain;
  try {
    instance = await createDshInstance(fixture.dshHome, {
      dshHome: fixture.dshHome,
      statusFilePath: join(fixture.dshHome, STATUS_FILE_NAME),
      projectId: PROJECT_ID,
      retryDelayMs: 0,
      syncConfig: { remote_sync_root: fixture.storeRoot, writer_id: 'writer-under-test' },
    });
    const { ctx } = instance;
    workspaceDomain = await ctx.storageDomain.open(workspaceDomainSpec);
    const { agent } = await ctx.agents.create({ sessionId: 'session-real-contract', meta: { cwd: workspaceDir } });
    const session = agent.session;
    session.append('turn/start', { turn: 1 });
    session.append('step/start', { turn: 1, step: 1 });
    session.append('assistant/message', { turn: 1, step: 1, stream: [], message: { id: 'feedback-target', role: 'assistant', source: { kind: 'model', provider: 'deepseek', model: 'deepseek-chat' }, content: [{ type: 'text', text: 'Saved response.' }] } }, { surfaceOp: 'append' });
    session.append('step/end', { turn: 1, step: 1 });
    session.append('turn/end', { turn: 1, reason: { kind: 'completed' } });
    const logPath = ctx.sessionPersistence.locate(session.header).path;
    assert.equal(basename(logPath), 'session.v4.jsonl');
    const storeLog = join(fixture.storeDir, relative(fixture.dshHome, logPath));
    await waitFor(
      () => existsSync(storeLog) && readFileSync(storeLog, 'utf8').includes('"type":"turn/end"'),
      'A completed-turn event must push the actual rc.2 generation into the Sync Store',
    );
    assert.equal(JSON.parse(readFileSync(storeLog, 'utf8').split('\n')[0]).version, 4);

    await workspaceDomain.global.set(workspaceDomain.global.get());
    await waitFor(
      () => existsSync(join(fixture.storeDir, 'storages', 'workspace.json')),
      'A saved workspace domain change must push the storage unit',
    );
    const feedback = await ctx.messageFeedback.put({ sessionId: session.id, messageId: 'feedback-target', rating: 'positive', ifVersion: null });
    assert.equal(feedback.ok, true);
    session.append('turn/start', { turn: 2 });
    session.append('turn/end', { turn: 2, reason: { kind: 'completed' } });
    await waitFor(
      () => readFileSync(storeLog, 'utf8').includes('"type":"feedback/message-put"'),
      'Rc.2 canonical message feedback must synchronize in the session generation',
    );
    if (ctx.remoteSync.activeTransfer) await ctx.remoteSync.activeTransfer;
    assert.ok(ctx.remoteSync.headSeq >= 1, 'each successful push advances the head sequence');
    assert.equal(readMarker(fixture.storeDir).writer_id, 'writer-under-test');
  } finally {
    await workspaceDomain?.close();
    await instance?.close();
    fixture.cleanup();
  }
});

test('reconcile.mjs always exits zero and records the outcome', async () => {
  const runReconcile = runReconcileChild;

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
    assert.ok(findFile(join(storeRoot, PROJECT_ID, 'sessions'), 'session.v4.jsonl'));
    assert.match(synchronized.stdout, /remote-sync: status synchronized/);
  } finally {
    rmSync(dshHome, { recursive: true, force: true });
    rmSync(storeRoot, { recursive: true, force: true });
  }
});

test('session projection documents synchronize in the projection pass and preserve cold listing titles', async () => {
  const fixture = createFixture();
  const workstationBHome = createTempDir();
  const sessionId = 'session-cold-title-test';
  const realTitle = 'Add Authentication Middleware';
  const workspaceCwd = join(fixture.dshHome, 'workspace');
  mkdirSync(workspaceCwd);
  let instA;
  let instB;
  let wsDomainA;

  try {
    // Create a real agent-owned rc.2 session, then checkpoint it for cold listing.
    instA = await createDshInstance(fixture.dshHome);
    wsDomainA = await instA.ctx.storageDomain.open(workspaceDomainSpec);
    await wsDomainA.table('workspaces').put('ws-1', {
      path: workspaceCwd,
      title: '.dsh',
      sessionIds: [sessionId],
      createdAt: new Date().toISOString(),
      updatedAt: new Date().toISOString(),
    });

    const { agent } = await instA.ctx.agents.create({ sessionId, meta: { cwd: workspaceCwd, agentPreset: 'standard-bash' } });
    const sessionA = agent.session;
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
    sessionA.append('step/start', { turn: 1, step: 1 });
    sessionA.append('assistant/message', {
      turn: 1,
      step: 1,
      stream: [],
      message: {
        id: 'msg-assistant-1',
        role: 'assistant',
        source: { kind: 'model', provider: 'deepseek', model: 'deepseek-chat' },
        content: [{ type: 'text', text: longBody }],
      },
    }, { surfaceOp: 'append' });
    sessionA.append('step/end', { turn: 1, step: 1 });
    sessionA.append('turn/end', { turn: 1, reason: { kind: 'completed' } });

    await instA.ctx.sessions.flush(sessionA);
    await instA.ctx.sessionProjectionCache.write(sessionA);

    const loc = instA.ctx.sessionPersistence.locate(sessionA.header);
    const fileSizeA = statSync(loc.path).size;
    assert.equal(basename(loc.path), 'session.v4.jsonl');
    assert.ok(fileSizeA > 1024, `Session generation must exceed 1024 bytes (was ${fileSizeA})`);
    const genericFileBytes = Buffer.from('portable generic attachment\n');
    const fileRef = await instA.ctx.attachments.saveFile({ data: genericFileBytes, name: 'portable.txt' });
    const imageBytes = Buffer.from('iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAIAAACQd1PeAAAACXBIWXMAAAPoAAAD6AG1e1JrAAAADElEQVQImWNgZGIGAAAOAAeCcsnOAAAAAElFTkSuQmCC', 'base64');
    const imageRef = await instA.ctx.attachments.saveImage({ data: imageBytes, mediaType: 'image/png' });

    const projDocPath = join(fixture.dshHome, 'storages', 'session_projcache', 'sessions', `${sessionId}.json`);
    assert.ok(existsSync(projDocPath), 'Session projection document must exist in Workstation A');
    const actualProjection = JSON.parse(readFileSync(projDocPath, 'utf8'));
    assert.equal(actualProjection.version, projectionCacheDomainSpec.version);
    assert.equal(actualProjection.record.identity.formatVersion, 4);

    await wsDomainA.close();
    wsDomainA = undefined;
    await instA.close();
    instA = undefined;

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
    instB = await createDshInstance(workstationBHome);
    const coldSummariesB = await instB.listState.list();
    const summaryB = coldSummariesB.find((s) => s.sessionId === sessionId);
    assert.ok(summaryB, 'Session summary must be returned in cold listing');

    const titleProjection = summaryB.projections?.values?.title;
    const computedDisplayTitle = displayTitleOf(titleProjection, summaryB.cwd, summaryB.sessionId);

    // Assert returned title projection is the real chat title, not the workspace basename
    assert.equal(titleProjection, realTitle, `Cold title projection must be "${realTitle}", got "${titleProjection}"`);
    assert.notEqual(computedDisplayTitle, workspaceTitleOf(workspaceCwd), 'Display title must not fall back to workspace basename');
    assert.equal(computedDisplayTitle, realTitle, `Computed display title must be "${realTitle}"`);

    assert.equal(instB.ctx.sessions.get(sessionId), undefined, 'cold listing must not open the session');
    const genericFilePath = instB.ctx.attachments.fileHostPath(fileRef);
    assert.deepEqual(readFileSync(genericFilePath), genericFileBytes, 'generic-file alias bytes must survive the pull');
    const objectDirectory = join(workstationBHome, 'attachments', 'v1', 'file-objects');
    const objectFiles = readdirSync(objectDirectory).flatMap((shard) => readdirSync(join(objectDirectory, shard)).map((name) => join(objectDirectory, shard, name)));
    assert.equal(objectFiles.length, 1);
    assert.deepEqual(readFileSync(objectFiles[0]), genericFileBytes, 'canonical generic-file object must also transfer');
    assert.equal(existsSync(instB.ctx.attachments.imageHostPath(imageRef)), true, 'image object must transfer');
    const { agent: reopened } = await instB.ctx.agents.resume({ resumeSessionId: sessionId });
    assert.equal(reopened.session.header.agentPreset, 'standard-bash');
    assert.equal(instB.ctx.sessionTitle.get(reopened.session).title, realTitle);
    reopened.session.append('turn/start', { turn: 2 });
    reopened.session.append('turn/end', { turn: 2, reason: { kind: 'completed' } });
    await instB.ctx.sessions.flush(reopened.session);
    const savedEvents = reopened.session.snapshotEvents();
    await instB.close();
    instB = await createDshInstance(workstationBHome);
    const { agent: restarted } = await instB.ctx.agents.resume({ resumeSessionId: sessionId });
    assert.deepEqual(restarted.session.snapshotEvents().slice(0, -1), savedEvents, 'receiving instance must persist appended events across restart');
    assert.equal(restarted.session.snapshotEvents().at(-1).type, 'session/end-seed', 'resume adds its current end-seed marker');
  } finally {
    await wsDomainA?.close();
    await instA?.close();
    await instB?.close();
    fixture.cleanup();
    rmSync(workstationBHome, { recursive: true, force: true });
  }
});


test('rc.2 migrates copied V3 state and rsync preserves committed generations while excluding staging in both directions', async () => {
  for (const compression of ['none', 'zstd']) {
    const fixture = createFixture();
    const receiverHome = createTempDir();
    let instance;
    let receiver;
    try {
      const sessionId = `session-v3-${compression}`;
      instance = await createDshInstance(fixture.dshHome, undefined, compression);
      const header = { type: 'session', version: 3, id: sessionId, createdAt: 1, isSeeded: false, delegationDepth: 0, agentPreset: 'standard-bash' };
      const successorPath = instance.ctx.sessionPersistence.locate(header).path;
      const directory = dirname(successorPath);
      mkdirSync(directory, { recursive: true });
      const predecessorPath = join(directory, `session.v3.jsonl${compression === 'zstd' ? '.zstd' : ''}`);
      const events = [
        { type: 'turn/start', data: { turn: 1 } },
        { type: 'turn/end', data: { turn: 1, reason: { kind: 'completed' } } },
      ].map((event, seq) => ({ ...event, seq, time: seq + 10 }));
      const headerLine = JSON.stringify(header) + '\n';
      const eventLines = events.map((row) => JSON.stringify(row)).join('\n') + '\n';
      const predecessorBytes = compression === 'zstd'
        ? Buffer.concat([zstdCompressSync(Buffer.from(headerLine)), zstdCompressSync(Buffer.from(eventLines))])
        : Buffer.from(headerLine + eventLines);
      writeFileSync(predecessorPath, predecessorBytes);
      const predecessorStat = statSync(predecessorPath, { bigint: true });
      const { agent } = await instance.ctx.agents.resume({ resumeSessionId: sessionId });
      assert.equal(agent.session.header.version, 4);
      assert.equal(agent.session.header.agentPreset, 'standard-bash');
      agent.session.append('turn/start', { turn: 2 });
      agent.session.append('turn/end', { turn: 2, reason: { kind: 'completed' } });
      await instance.ctx.sessions.flush(agent.session);
      const savedEvents = agent.session.snapshotEvents();
      assert.ok(existsSync(successorPath), 'rc.2 must publish its V4 successor');
      assert.deepEqual(readFileSync(predecessorPath), predecessorBytes);
      const after = statSync(predecessorPath, { bigint: true });
      for (const field of ['dev', 'ino', 'size', 'mtimeNs', 'ctimeNs']) assert.equal(after[field], predecessorStat[field], `migration must preserve predecessor ${field}`);
      await instance.close();
      instance = undefined;

      const stagingNames = ['session.lock', 'session.migration.0123456789ab.jsonl.tmp', 'session.migration.0123456789ab.jsonl.zstd.tmp', 'session.v3.jsonl.0123456789ab.tmp', 'session.v4.jsonl.0123456789ab.tmp', 'session.v4.jsonl.zstd.0123456789ab.tmp'];
      for (const name of stagingNames) writeFileSync(join(directory, name), 'local staging');
      assert.equal(await fixture.manager().reconcile(), 'synchronized');
      const storeDirectory = join(fixture.storeDir, relative(fixture.dshHome, directory));
      assert.deepEqual(readFileSync(join(storeDirectory, basename(predecessorPath))), predecessorBytes, 'committed V3 must remain union-eligible');
      assert.deepEqual(readFileSync(join(storeDirectory, basename(successorPath))), readFileSync(successorPath), 'committed V4 must remain union-eligible');
      for (const name of stagingNames) {
        assert.equal(existsSync(join(storeDirectory, name)), false, `${name} must not push`);
        writeFileSync(join(storeDirectory, name), 'remote staging');
      }
      const receiverManager = new RemoteSyncManager({
        dshHome: receiverHome, statusFilePath: join(receiverHome, STATUS_FILE_NAME), projectId: PROJECT_ID, retryDelayMs: 0,
        syncConfig: { remote_sync_root: fixture.storeRoot, writer_id: 'receiver' },
      });
      assert.equal(await receiverManager.reconcile(), 'synchronized');
      const receiverDirectory = join(receiverHome, relative(fixture.dshHome, directory));
      for (const name of stagingNames) assert.equal(existsSync(join(receiverDirectory, name)), false, `${name} must not pull`);
      assert.deepEqual(readFileSync(join(receiverDirectory, basename(predecessorPath))), predecessorBytes);
      receiver = await createDshInstance(receiverHome, undefined, compression);
      const { agent: reopened } = await receiver.ctx.agents.resume({ resumeSessionId: sessionId });
      assert.deepEqual(reopened.session.snapshotEvents().slice(0, -1), savedEvents, 'receiving instance must reopen the committed successor with appended events');
      assert.equal(reopened.session.snapshotEvents().at(-1).type, 'session/end-seed');
      assert.equal(reopened.session.header.agentPreset, 'standard-bash');
    } finally {
      await instance?.close();
      await receiver?.close();
      fixture.cleanup();
      rmSync(receiverHome, { recursive: true, force: true });
    }
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
      version: projectionCacheDomainSpec.version,
      record: {
        identity: { formatVersion: 4, createdAt: 1788000000000, cwd: fixture.dshHome, isSeeded: false, inheritedEventCount: 0 },
        rows: { title: { ver: 1, seq: 11, val: 'Add Authentication Middleware' } },
      },
    });
    writeFileSync(join(sessionsDir, 'session-x.json'), freshDoc);

    // The stale receiver document: different content and LONGER than the sender.
    // Under `--append-verify` a longer receiver is either left with its stale
    // record or spliced as stale-prefix plus sender suffix; it is never replaced
    // by the sender's whole record.
    const staleDoc = JSON.stringify({
      version: projectionCacheDomainSpec.version,
      record: {
        identity: { formatVersion: 4, createdAt: 1788000000000, cwd: fixture.dshHome, isSeeded: false, inheritedEventCount: 0 },
        rows: { title: { ver: 1, seq: 9, val: 'OLD' } },
      },
      tail: { padding: 'this stale record is written longer than the fresh sender record' },
    });
    assert.ok(staleDoc.length > freshDoc.length, 'stale receiver must be longer than the sender');
    writeFileSync(join(storeProjDir, 'session-x.json'), staleDoc);
    setMtime(join(storeProjDir, 'session-x.json'), 1000000);
    setMtime(join(sessionsDir, 'session-x.json'), 1600000000);

    // Demonstrate the regression with real rsync without mutating plugin source.
    execFileSync('rsync', ['-az', '--update', '--append-verify', ...PROJECTION_FILTER_ARGS, `${fixture.dshHome}/`, `${fixture.storeDir}/`]);
    let mutatedTitle;
    try {
      mutatedTitle = JSON.parse(readFileSync(join(storeProjDir, 'session-x.json'), 'utf8')).record.rows.title.val;
    } catch {
      // A stale-prefix/sender-suffix splice is also the regression.
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
  const root = createTempDir();
  const home = join(root, 'home');
  const workspace = join(root, 'workspace');
  const archivePath = join(home, 'test-data', 'voice.jsonl');
  const storeRoot = join(home, 'test-data', 'sync-store');
  const statusPath = join(home, 'test-data', STATUS_FILE_NAME);
  const isolationOverlay = join(root, 'web-isolation.patch.json');
  let dshProcess;
  let exited;
  let bootTimeout;
  try {
    mkdirSync(workspace);
    mkdirSync(storeRoot, { recursive: true });
    writeFileSync(archivePath, '');
    writeFileSync(statusPath, JSON.stringify({ status: 'failed', head_seq: 0, last_error: 'fixture retry', updated_at: new Date().toISOString() }));
    writeMarker(join(storeRoot, PROJECT_ID), 0, 'web-fixture');
    cpSync(join(candidateHome, 'cordis.patch.yml'), join(home, 'cordis.patch.yml'));
    cpSync(join(candidateHome, 'style-presets.json'), join(home, 'style-presets.json'));
    for (const profile of ['web', 'headless']) {
      const profileDir = join(home, 'profiles', profile);
      mkdirSync(profileDir, { recursive: true });
      for (const filename of ['package.json', 'cordis.yml', 'cordis.patch.yml']) {
        cpSync(join(candidateHome, 'profiles', profile, filename), join(profileDir, filename));
      }
      symlinkSync(join(candidateHome, 'profiles', profile, 'node_modules'), join(profileDir, 'node_modules'));
    }
    symlinkSync(join(candidateHome, 'profiles', 'node_modules'), join(home, 'profiles', 'node_modules'));
    writeFileSync(isolationOverlay, JSON.stringify([
      { id: 'tool-voice-input', config: { file: archivePath } },
      { id: 'remote-sync', config: { dshHome: home, statusFilePath: statusPath, projectId: PROJECT_ID, retryDelayMs: 0, syncConfig: { remote_sync_root: storeRoot, writer_id: 'web-fixture' } } },
      { id: 'style-control', config: { filePath: join(home, 'style-presets.json') } },
    ]));
    const env = {
      ...process.env, DSH_HOME: home, DSH_PACKAGE_ENTRY: packageEntry, DSH_TELEMETRY_DISABLED: '1',
      npm_config_prefix: join(home, 'test-data', 'npm-global'),
      NPM_CONFIG_PREFIX: join(home, 'test-data', 'npm-global'),
      DEVVM_WORKSPACE: workspace, DEVVM_PROJECT_ID: PROJECT_ID, DEVVM_SYNC_STATUS_PATH: statusPath,
      DEVVM_SYNC_CONFIG_PATH: join(home, 'missing-sync-config.json'),
    };
    const webArgs = ['--profile', 'web', '--patch', candidateOverlay, '--patch', isolationOverlay];
    const webDump = execFileSync(process.execPath, [candidateCli, ...webArgs, '--dump-config'], { cwd: workspace, env, encoding: 'utf8' });
    assert.equal((webDump.match(/name: ['"]?@devvm\/dsh-remote-sync['"]?/g) || []).length, 1);
    assert.equal((webDump.match(/name: ['"]?@devvm\/dsh-voice-input['"]?/g) || []).length, 1);
    assert.ok(webDump.includes(archivePath), 'Web must use the isolated voice archive');
    assert.ok(webDump.includes(storeRoot), 'Web must use the isolated local Sync Store');
    assert.ok(webDump.includes(statusPath), 'Web must use the isolated status file');
    assert.match(webDump, /id: panel-mcp-context7\n(?:[^\n]*\n)*?\s+disabled: true/, 'only the fixture disables production Context7');
    const headlessDump = execFileSync(process.execPath, [candidateCli, '--profile', 'headless', '--dump-config'], { cwd: workspace, env, encoding: 'utf8' });
    assert.ok(!headlessDump.includes('@devvm/dsh-remote-sync'), 'headless must exclude Remote Sync');
    assert.ok(!headlessDump.includes('@devvm/dsh-voice-input'), 'headless must exclude Voice Input');

    dshProcess = spawn(process.execPath, [candidateCli, ...webArgs, '--no-open', '--host', '127.0.0.1', '--port', '0'], {
      cwd: workspace, env, stdio: ['ignore', 'pipe', 'pipe'],
    });
    exited = once(dshProcess, 'close');
    let output = '';
    let errorOutput = '';
    const ready = new Promise((resolve, reject) => {
      dshProcess.stdout.on('data', (chunk) => {
        output += chunk.toString();
        const match = output.match(/dsh web: (http:\/\/127\.0\.0\.1:\d+\/[^\s]*)/);
        if (match) resolve(new URL(match[1]));
      });
      dshProcess.stderr.on('data', (chunk) => { errorOutput += chunk.toString(); });
      dshProcess.on('error', reject);
      dshProcess.on('close', (code) => reject(new Error(`dsh exited before ready (${code}): ${errorOutput}\n${output}`)));
      bootTimeout = setTimeout(() => reject(new Error(`Timeout waiting for staged Web: ${errorOutput}\n${output}`)), 30000);
    });
    const readyUrl = await ready;
    clearTimeout(bootTimeout);
    assert.notEqual(readyUrl.port, '0', 'the ready URL must report the assigned port');
    assert.ok(readyUrl.searchParams.has('token'), 'the ready URL must include its launch token');
    const authRes = await fetch(readyUrl, { redirect: 'manual' });
    assert.equal(authRes.status, 303);
    const cookie = authRes.headers.get('set-cookie')?.split(';')[0];
    assert.ok(cookie, 'launch token exchange must issue an authentication cookie');
    const headers = { cookie };
    const origin = readyUrl.origin;
    const directRes = await fetch(`${origin}/plugins/@devvm/dsh-remote-sync/client.js`, { headers });
    assert.equal(directRes.status, 404, 'Direct client.js endpoint must remain removed');
    const indexRes = await fetch(`${origin}/`, { headers });
    assert.equal(indexRes.status, 200);
    const indexHtml = await indexRes.text();
    const comboUrl = [...indexHtml.matchAll(/plugins\/\?\?[^"'\s\\]+/g)]
      .map(([url]) => url.replaceAll('&amp;', '&'))
      .find((url) => url.includes('@devvm/dsh-remote-sync/client.js'));
    assert.ok(comboUrl, 'Remote Sync must be included in the advertised rc.2 client batches');
    const clientRes = await fetch(new URL(comboUrl, `${origin}/`), { headers });
    assert.equal(clientRes.status, 200);
    assert.ok((await clientRes.text()).includes('@devvm/dsh-remote-sync'));
    const statusRes = await fetch(`${origin}/api/sync/status`, { headers });
    assert.equal(statusRes.status, 200);
    const statusBody = await statusRes.json();
    assert.deepEqual(Object.keys(statusBody).sort(), ['daemon_url', 'head_seq', 'last_error', 'project_id', 'status', 'updated_at']);
    assert.equal(statusBody.project_id, PROJECT_ID);
    const retryRes = await fetch(`${origin}/api/sync/retry`, { method: 'POST', headers });
    assert.equal(retryRes.status, 200);
    assert.equal((await retryRes.json()).status, 'synchronized');
    assert.ok(existsSync(statusPath), 'the status route must operate on the isolated home');
    assert.ok(existsSync(join(storeRoot, PROJECT_ID, HEAD_MARKER_NAME)), 'retry must publish only to the isolated store');
    const checkRes = await fetch(`${origin}/api/sync/check`, { method: 'POST', headers });
    assert.equal(checkRes.status, 200);
    assert.equal((await checkRes.json()).status, 'synchronized');
    const triggerRes = await fetch(`${origin}/api/sync/trigger`, { method: 'POST', headers });
    assert.notEqual(triggerRes.status, 200, 'the removed trigger route must not answer');
  } finally {
    clearTimeout(bootTimeout);
    if (dshProcess) {
      if (dshProcess.exitCode === null && dshProcess.signalCode === null) dshProcess.kill('SIGTERM');
      const killTimeout = setTimeout(() => dshProcess.kill('SIGKILL'), 5000);
      try { await exited; } finally { clearTimeout(killTimeout); }
    }
    rmSync(root, { recursive: true, force: true });
  }
});
