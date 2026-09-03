import { spawn } from 'node:child_process';
import { randomBytes } from 'node:crypto';
import { existsSync, mkdirSync, readFileSync, renameSync, writeFileSync } from 'node:fs';
import { homedir } from 'node:os';
import { dirname, join, resolve } from 'node:path';

export const name = 'remote-sync';
export const inject = ['sessions'];

export const DEFAULT_STATUS_FILE_PATH = '/run/devvm/sync-status.json';
export const HEAD_MARKER_NAME = '.sync-head.json';

/** Sessions and attachment objects transfer as a union: additions only, never shrinking. */
export const UNION_FILTER_ARGS = [
  '--include=sessions/***',
  '--include=attachments/',
  '--include=attachments/v1/',
  '--include=attachments/v1/objects/',
  '--include=attachments/v1/objects/***',
  '--exclude=*',
];

/** Storage units transfer as whole documents; the projection cache is rebuildable. */
export const STORAGES_FILTER_ARGS = [
  '--exclude=storages/session_projcache.json',
  '--include=storages/',
  '--include=storages/*.json',
  '--exclude=*',
];

const UNION_FLAGS = ['-az', '--update', '--append-verify'];
const NEWEST_WINS_FLAGS = ['-az', '--update'];
const REMOTE_WINS_FLAGS = ['-az'];

const MAX_PUSH_ATTEMPTS = 5;
const DEFAULT_RETRY_DELAY_MS = 1000;
const HEAD_MISMATCH_EXIT = 3;
const RSYNC_VANISHED_SOURCE_FILES = 24;
const KNOWN_STATUSES = new Set([
  'not_configured',
  'synchronizing',
  'synchronized',
  'remote_ahead',
  'degraded',
  'failed',
]);
const RETRYABLE_STATUSES = new Set(['failed', 'degraded', 'remote_ahead']);

function shellQuote(value) {
  return `'${String(value).replace(/'/g, `'\\''`)}'`;
}

function firstLines(text, count = 3) {
  return String(text || '')
    .split('\n')
    .filter((line) => line.trim().length > 0)
    .slice(0, count)
    .join('; ');
}

function runProcess(command, args) {
  return new Promise((resolvePromise, rejectPromise) => {
    const child = spawn(command, args);
    let stdout = '';
    let stderr = '';
    child.stdout.on('data', (chunk) => {
      stdout += chunk.toString();
    });
    child.stderr.on('data', (chunk) => {
      stderr += chunk.toString();
    });
    child.on('error', (error) => {
      rejectPromise(new Error(`Failed to execute ${command}: ${error.message}`));
    });
    child.on('close', (code) => {
      resolvePromise({ code, stdout, stderr });
    });
  });
}

function delay(ms) {
  return new Promise((resolvePromise) => setTimeout(resolvePromise, ms));
}

export function loadSyncConfig(override) {
  if (override) return override;
  const path = process.env.DEVVM_SYNC_CONFIG_PATH || join(homedir(), '.config/devvm/sync.json');
  if (!existsSync(path)) return null;
  try {
    return JSON.parse(readFileSync(path, 'utf8'));
  } catch (error) {
    throw new Error(`Failed to read sync config from ${path}: ${error.message}`);
  }
}

export function resolveProjectId(override) {
  if (override) return override;
  if (process.env.DEVVM_PROJECT_ID) return process.env.DEVVM_PROJECT_ID;
  const idPath = join(process.env.DEVVM_WORKSPACE || '/root/workspace', '.devvm-id');
  if (!existsSync(idPath)) return null;
  try {
    const firstLine = readFileSync(idPath, 'utf8').split('\n')[0].trim();
    return firstLine.length > 0 ? firstLine : null;
  } catch (error) {
    console.warn(`remote-sync: failed to read ${idPath}: ${error.message}`);
    return null;
  }
}

/**
 * One Project's Sync Store directory, reached either over ssh or - when no
 * ssh_host is configured - as a local directory path. The engine above this
 * seam is identical for both transports.
 */
export class SyncStore {
  constructor(config, projectId) {
    this.projectId = projectId;
    this.root = String(config.remote_sync_root).replace(/\/+$/, '');
    this.host = config.ssh_host || '';
    this.user = config.ssh_user || 'root';
    this.port = config.ssh_port || 22;
    this.keyPath = config.ssh_key_path || join(process.env.HOME || '/root', '.ssh/id_ed25519');
    this.writerId = config.writer_id || null;
  }

  get isRemote() {
    return this.host.length > 0;
  }

  get projectDir() {
    return `${this.root}/${this.projectId}`;
  }

  sshArgs() {
    return [
      '-p',
      String(this.port),
      '-i',
      this.keyPath,
      '-o',
      'StrictHostKeyChecking=accept-new',
      '-o',
      'BatchMode=yes',
      '-o',
      'ConnectTimeout=10',
    ];
  }

  rshCommand() {
    return `ssh ${this.sshArgs().join(' ')}`;
  }

  rsyncTarget(subpath = '') {
    const path = `${this.projectDir}/${subpath}`;
    return this.isRemote ? `${this.user}@${this.host}:${path}` : path;
  }

  exec(script) {
    if (this.isRemote) {
      return runProcess('ssh', [...this.sshArgs(), `${this.user}@${this.host}`, script]);
    }
    return runProcess('sh', ['-c', script]);
  }

  async readHead() {
    const script = [
      `root=${shellQuote(this.root)}`,
      `marker=${shellQuote(`${this.projectDir}/${HEAD_MARKER_NAME}`)}`,
      'if [ ! -d "$root" ]; then echo "Sync Store root unavailable: $root" >&2; exit 4; fi',
      'if [ ! -f "$marker" ]; then echo 0; exit 0; fi',
      `seq=$(sed -n 's/.*"seq"[[:space:]]*:[[:space:]]*\\([0-9][0-9]*\\).*/\\1/p' "$marker" | head -n 1)`,
      '[ -n "$seq" ] || seq=0',
      'echo "$seq"',
    ].join('\n');
    const result = await this.exec(script);
    if (result.code !== 0) {
      throw new Error(
        `Sync Store head unreadable (exit ${result.code}): ${firstLines(result.stderr || result.stdout)}`,
      );
    }
    const seq = Number.parseInt(result.stdout.trim(), 10);
    if (!Number.isInteger(seq)) {
      throw new Error(`Sync Store head marker is not a sequence: ${result.stdout.trim()}`);
    }
    return seq;
  }

  /** Returns the new sequence, or null when the Sync Store head no longer matches. */
  async advanceHead(expectedSeq, updatedAt) {
    const marker = JSON.stringify({
      seq: expectedSeq + 1,
      writer_id: this.writerId,
      updated_at: updatedAt,
    });
    const script = [
      `dir=${shellQuote(this.projectDir)}`,
      `marker=${shellQuote(`${this.projectDir}/${HEAD_MARKER_NAME}`)}`,
      `expected=${shellQuote(String(expectedSeq))}`,
      `payload=${shellQuote(marker)}`,
      'mkdir -p "$dir" || exit 5',
      'seq=0',
      'if [ -f "$marker" ]; then',
      `  seq=$(sed -n 's/.*"seq"[[:space:]]*:[[:space:]]*\\([0-9][0-9]*\\).*/\\1/p' "$marker" | head -n 1)`,
      '  [ -n "$seq" ] || seq=0',
      'fi',
      `[ "$seq" = "$expected" ] || exit ${HEAD_MISMATCH_EXIT}`,
      'tmp="$marker.tmp.$$"',
      'printf \'%s\\n\' "$payload" > "$tmp" || exit 6',
      'mv "$tmp" "$marker" || exit 7',
      'echo "$((expected + 1))"',
    ].join('\n');
    const result = await this.exec(script);
    if (result.code === HEAD_MISMATCH_EXIT) return null;
    if (result.code !== 0) {
      throw new Error(
        `Sync Store head not advanced (exit ${result.code}): ${firstLines(result.stderr || result.stdout)}`,
      );
    }
    return expectedSeq + 1;
  }
}

export class RemoteSyncManager {
  constructor(options = {}) {
    this.dshHome = resolve(options.dshHome || process.env.DSH_HOME || join(homedir(), '.dsh'));
    this.statusFilePath = resolve(
      options.statusFilePath || process.env.DEVVM_SYNC_STATUS_PATH || DEFAULT_STATUS_FILE_PATH,
    );
    this.retryDelayMs = options.retryDelayMs ?? DEFAULT_RETRY_DELAY_MS;
    this.now = options.now || (() => new Date());
    this.configOverride = options.syncConfig || null;
    this.projectIdOverride = options.projectId || null;

    const persisted = readStatusFile(this.statusFilePath);
    this.headSeq = Number.isInteger(persisted?.head_seq) ? persisted.head_seq : null;
    this.status = KNOWN_STATUSES.has(persisted?.status) ? persisted.status : 'not_configured';
    this.lastError = null;
    this.updatedAt = typeof persisted?.updated_at === 'string' ? persisted.updated_at : null;
    this.activeTransfer = null;
    this.pendingFollowUp = false;
  }

  getStatus() {
    return this.status;
  }

  statusSnapshot() {
    return {
      status: this.status,
      head_seq: this.headSeq,
      last_error: this.lastError,
      updated_at: this.updatedAt,
    };
  }

  /** The status shape plus the identifiers the web client needs for its banner link. */
  statusResponse() {
    let config = null;
    try {
      config = loadSyncConfig(this.configOverride);
    } catch (error) {
      console.warn(`remote-sync: ${error.message}`);
    }
    return {
      ...this.statusSnapshot(),
      project_id: resolveProjectId(this.projectIdOverride),
      daemon_url: config?.daemon_url || null,
    };
  }

  async handleTurnEnd(session, sessions) {
    await sessions.flush(session);
    return this.triggerSync();
  }

  async handleDomainChanged(change) {
    if (change?.domain !== 'workspace' && change?.domain !== 'message_feedback') return null;
    return this.triggerSync();
  }

  async triggerSync() {
    const store = this._openStore();
    if (!store) return this.status;
    if (this.activeTransfer) {
      this.pendingFollowUp = true;
      return this.activeTransfer;
    }
    this.activeTransfer = this._pushUntilQuiet(store);
    return this.activeTransfer;
  }

  async retry() {
    if (!RETRYABLE_STATUSES.has(this.status)) return this.status;
    return this.triggerSync();
  }

  async checkRemoteHead() {
    if (this.activeTransfer) return this.status;
    const store = this._openStore();
    if (!store) return this.status;
    let seq;
    try {
      seq = await store.readHead();
    } catch (error) {
      // A focus-time check must not downgrade the status on its own.
      console.warn(`remote-sync: remote head check failed: ${error.message}`);
      return this.status;
    }
    if (this.headSeq === null || seq > this.headSeq) this._setStatus('remote_ahead');
    return this.status;
  }

  /** Safe to pull only because no DSH Runtime holds this Project's state open. */
  async reconcile() {
    const store = this._openStore();
    if (!store) {
      logStep(`reconciliation skipped: ${this.status}`);
      return this.status;
    }

    let seq;
    try {
      seq = await store.readHead();
    } catch (error) {
      logError(error.message);
      this._setStatus('degraded', error.message);
      return this.status;
    }
    const remoteAhead = this.headSeq === null || seq > this.headSeq;
    logStep(`Sync Store head is ${seq}, local head is ${this.headSeq}`);

    try {
      logStep('pushing sessions and attachment objects');
      await this._transfer('push', UNION_FLAGS, UNION_FILTER_ARGS, store);
      if (!remoteAhead) {
        logStep('pushing storage units');
        await this._transfer('push', NEWEST_WINS_FLAGS, STORAGES_FILTER_ARGS, store);
      }
      logStep('pulling sessions and attachment objects');
      await this._transfer('pull', UNION_FLAGS, UNION_FILTER_ARGS, store);
      logStep(remoteAhead ? 'pulling storage units (Sync Store wins)' : 'pulling storage units');
      await this._transfer(
        'pull',
        remoteAhead ? REMOTE_WINS_FLAGS : NEWEST_WINS_FLAGS,
        STORAGES_FILTER_ARGS,
        store,
      );
    } catch (error) {
      logError(error.message);
      this._setStatus('failed', error.message);
      return this.status;
    }

    this.headSeq = seq;
    this._setStatus('synchronized');
    logStep(`reconciliation finished: ${this.status}, head ${this.headSeq}`);
    return this.status;
  }

  _openStore() {
    let config;
    try {
      config = loadSyncConfig(this.configOverride);
    } catch (error) {
      this._setStatus('failed', error.message);
      return null;
    }
    if (!config?.remote_sync_root) {
      this._setStatus('not_configured');
      return null;
    }
    const projectId = resolveProjectId(this.projectIdOverride);
    if (!projectId) {
      this._setStatus('failed', 'Project ID not found (.devvm-id missing)');
      return null;
    }
    return new SyncStore(config, projectId);
  }

  async _pushUntilQuiet(store) {
    try {
      let pending = true;
      while (pending) {
        await this._pushWithRetries(store);
        pending = this.pendingFollowUp;
        this.pendingFollowUp = false;
      }
    } finally {
      this.activeTransfer = null;
    }
    return this.status;
  }

  async _pushWithRetries(store) {
    for (let attempt = 1; attempt <= MAX_PUSH_ATTEMPTS; attempt += 1) {
      this._setStatus('synchronizing');
      try {
        await this._push(store);
        return;
      } catch (error) {
        if (attempt === MAX_PUSH_ATTEMPTS) {
          this._setStatus('failed', error.message);
          return;
        }
        console.warn(`remote-sync: push attempt ${attempt} failed: ${error.message}`);
        await delay(this.retryDelayMs);
      }
    }
  }

  async _push(store) {
    // A workstation that has never reconciled is behind by definition.
    const expected = this.headSeq;
    const advanced =
      expected === null ? null : await store.advanceHead(expected, this.now().toISOString());

    if (advanced === null) {
      // Never push storage units while the Sync Store is ahead: a whole-document
      // push would drop the other workstation's session references.
      await this._transfer('push', UNION_FLAGS, UNION_FILTER_ARGS, store);
      this._setStatus('remote_ahead');
      return;
    }

    await this._transfer('push', UNION_FLAGS, UNION_FILTER_ARGS, store);
    await this._transfer('push', NEWEST_WINS_FLAGS, STORAGES_FILTER_ARGS, store);
    this.headSeq = advanced;
    this._setStatus('synchronized');
  }

  async _transfer(direction, flags, filters, store) {
    this._ensureDshHome();
    const local = `${this.dshHome}/`;
    const remote = store.rsyncTarget();
    const [source, destination] = direction === 'push' ? [local, remote] : [remote, local];
    const args = [...flags];
    if (store.isRemote) args.push('-e', store.rshCommand());
    args.push(...filters, source, destination);

    const result = await runProcess('rsync', args);
    if (result.code === RSYNC_VANISHED_SOURCE_FILES) {
      console.warn(`remote-sync: rsync ${direction} reported vanished source files`);
      return;
    }
    if (result.code !== 0) {
      throw new Error(
        `rsync ${direction} failed (exit ${result.code}): ${firstLines(result.stderr || result.stdout)}`,
      );
    }
  }

  _ensureDshHome() {
    if (!existsSync(this.dshHome)) mkdirSync(this.dshHome, { recursive: true });
  }

  _setStatus(status, lastError = null) {
    this.status = status;
    this.lastError = lastError;
    this.updatedAt = this.now().toISOString();
    this._writeStatusFile();
  }

  _writeStatusFile() {
    const payload = JSON.stringify(this.statusSnapshot(), null, 2) + '\n';
    const temporary = `${this.statusFilePath}.tmp.${randomBytes(6).toString('hex')}`;
    try {
      mkdirSync(dirname(this.statusFilePath), { recursive: true });
      writeFileSync(temporary, payload);
      renameSync(temporary, this.statusFilePath);
    } catch (error) {
      console.warn(`remote-sync: failed to write ${this.statusFilePath}: ${error.message}`);
    }
  }
}

function readStatusFile(path) {
  if (!existsSync(path)) return null;
  try {
    return JSON.parse(readFileSync(path, 'utf8'));
  } catch (error) {
    console.warn(`remote-sync: ignoring unreadable ${path}: ${error.message}`);
    return null;
  }
}

function logStep(message) {
  console.log(`remote-sync: ${message}`);
}

function logError(message) {
  console.error(`remote-sync: ${message}`);
}

function sendJson(res, statusCode, body) {
  res.writeHead(statusCode, { 'Content-Type': 'application/json' });
  res.end(JSON.stringify(body));
}

function methodNotAllowed(res) {
  sendJson(res, 405, { error: 'Method Not Allowed' });
}

export function registerWebServerRoutes(webServer, syncManager) {
  if (!webServer || typeof webServer.register !== 'function') return () => {};

  const disposers = [
    webServer.register({
      kind: 'exact',
      path: '/api/sync/status',
      handler: async (req, res) => {
        if (req.method !== 'GET' && req.method !== 'HEAD') return methodNotAllowed(res);
        sendJson(res, 200, syncManager.statusResponse());
      },
    }),
    webServer.register({
      kind: 'exact',
      path: '/api/sync/retry',
      handler: async (req, res) => {
        if (req.method !== 'POST') return methodNotAllowed(res);
        await syncManager.retry();
        sendJson(res, 200, syncManager.statusResponse());
      },
    }),
    webServer.register({
      kind: 'exact',
      path: '/api/sync/check',
      handler: async (req, res) => {
        if (req.method !== 'POST') return methodNotAllowed(res);
        await syncManager.checkRemoteHead();
        sendJson(res, 200, syncManager.statusResponse());
      },
    }),
  ];

  return () => {
    for (const dispose of disposers) dispose?.();
  };
}

export function apply(ctx, config) {
  const syncManager = new RemoteSyncManager({
    dshHome: config?.dshHome,
    statusFilePath: config?.statusFilePath,
    syncConfig: config?.syncConfig,
    retryDelayMs: config?.retryDelayMs,
    projectId: config?.projectId,
  });

  if (ctx && typeof ctx.on === 'function') {
    ctx.on('session/event', (session, event) => {
      if (event.type === 'turn/end') return syncManager.handleTurnEnd(session, ctx.sessions);
    });

    ctx.on('domain/changed', (change) => syncManager.handleDomainChanged(change));
  }

  if (ctx && typeof ctx.inject === 'function') {
    ctx.inject(['webServer'], (serverCtx) => {
      if (typeof serverCtx.effect === 'function') {
        serverCtx.effect(
          () => registerWebServerRoutes(serverCtx.webServer, syncManager),
          'remote-sync: api routes',
        );
      } else {
        registerWebServerRoutes(serverCtx.webServer, syncManager);
      }
    });
  } else if (ctx?.webServer) {
    registerWebServerRoutes(ctx.webServer, syncManager);
  }

  if (ctx && typeof ctx.provide === 'function') {
    ctx.provide('remoteSync', syncManager);
  }

  syncManager.checkRemoteHead().catch((error) => {
    console.warn(`remote-sync: startup head check failed: ${error.message}`);
  });

  return syncManager;
}
