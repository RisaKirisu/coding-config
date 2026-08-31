import { spawn } from 'node:child_process';
import { readFileSync, writeFileSync, unlinkSync, existsSync, mkdirSync } from 'node:fs';
import { homedir } from 'node:os';
import { join, resolve } from 'node:path';

export const name = 'remote-sync';
export const inject = ['sessions'];

export const RSYNC_FILTER_ARGS = [
  '--include=sessions/***',
  '--exclude=storages/session_projcache.json',
  '--include=storages/***',
  '--include=attachments/',
  '--include=attachments/v1/',
  '--include=attachments/v1/objects/',
  '--include=attachments/v1/objects/***',
  '--exclude=attachments/v1/request-images/***',
  '--exclude=attachments/***',
  '--exclude=.sync-dirty',
  '--exclude=credentials/***',
  '--exclude=settings/***',
  '--exclude=plugins/***',
  '--exclude=presets/***',
  '--exclude=profiles/***',
  '--exclude=*',
];

function loadSyncConfig(config) {
  if (config?.ssh_host) return config;
  const path = process.env.DEVVM_SYNC_CONFIG_PATH || join(homedir(), '.config/devvm/sync.json');
  if (!existsSync(path)) return null;
  try {
    return JSON.parse(readFileSync(path, 'utf8'));
  } catch (error) {
    throw new Error(`Failed to read sync config from ${path}: ${error.message}`);
  }
}

export class RemoteSyncManager {
  constructor(options = {}) {
    this.dshHome = resolve(options.dshHome || process.env.DSH_HOME || join(homedir(), '.dsh'));
    this.dirtyFilePath = join(this.dshHome, '.sync-dirty');
    this.statusFilePath = join(this.dshHome, '.sync-status.json');

    let initialStatus = 'not_configured';
    let isDirty = existsSync(this.dirtyFilePath);
    let retryCount = 0;

    if (existsSync(this.statusFilePath)) {
      try {
        const data = JSON.parse(readFileSync(this.statusFilePath, 'utf8'));
        if (data.status) initialStatus = data.status;
        if (data.is_dirty !== undefined) isDirty = Boolean(data.is_dirty);
        if (data.retry_count !== undefined) retryCount = Number(data.retry_count) || 0;
      } catch (e) {}
    }

    this.status = initialStatus;
    this.isDirty = isDirty || existsSync(this.dirtyFilePath);
    this.activeTransfer = null;
    this.pendingFollowUp = false;
    this.retryCount = retryCount;
    this.syncConfig = options.syncConfig || null;
  }

  getStatus() {
    if (existsSync(this.statusFilePath)) {
      try {
        const data = JSON.parse(readFileSync(this.statusFilePath, 'utf8'));
        if (data.status && data.status !== this.status && !this.activeTransfer) {
          this.status = data.status;
        }
        if (data.is_dirty !== undefined) {
          this.isDirty = Boolean(data.is_dirty) || existsSync(this.dirtyFilePath);
        }
      } catch (e) {}
    }
    return this.status;
  }

  setStatus(newStatus, extra = {}) {
    this.status = newStatus;
    this._writeStatusFile(extra);
  }

  _writeStatusFile(extra = {}) {
    try {
      if (!existsSync(this.dshHome)) {
        mkdirSync(this.dshHome, { recursive: true });
      }
      const payload = {
        status: this.status,
        is_dirty: this.isDirty,
        retry_count: this.retryCount,
        last_error: extra.lastError || null,
        updated_at: Date.now(),
      };
      writeFileSync(this.statusFilePath, JSON.stringify(payload, null, 2) + '\n');
    } catch (e) {}
  }

  markDirty() {
    this.isDirty = true;
    try {
      if (!existsSync(this.dshHome)) {
        mkdirSync(this.dshHome, { recursive: true });
      }
      writeFileSync(this.dirtyFilePath, '1\n');
    } catch (err) {
      console.warn('Failed to mark dirty state:', err);
    }
    this._writeStatusFile();
  }

  markClean() {
    this.isDirty = false;
    try {
      if (existsSync(this.dirtyFilePath)) {
        unlinkSync(this.dirtyFilePath);
      }
    } catch (err) {
      console.warn('Failed to mark clean state:', err);
    }
    this._writeStatusFile();
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
    const syncConfig = loadSyncConfig(this.syncConfig);
    if (!syncConfig) {
      this.setStatus('not_configured');
      return this.status;
    }

    this.markDirty();
    if (this.activeTransfer) {
      this.pendingFollowUp = true;
      return this.activeTransfer;
    }

    this.retryCount = 0;
    this.activeTransfer = this._runSyncLoop(syncConfig);
    return this.activeTransfer;
  }

  async _runSyncLoop(syncConfig) {
    let keepRunning = true;

    while (keepRunning) {
      this.setStatus('synchronizing');
      let success = false;
      let lastErr = null;

      while (this.retryCount < 5) {
        try {
          await this._defaultRunner(syncConfig);
          success = true;
          break;
        } catch (err) {
          this.retryCount++;
          lastErr = err;
          if (this.retryCount < 5) {
            await new Promise((resolve) => setTimeout(resolve, 1000));
          }
        }
      }

      if (success) {
        this.markClean();
        this.retryCount = 0;
        this.setStatus('synchronized');

        if (this.pendingFollowUp) {
          this.pendingFollowUp = false;
          keepRunning = true;
        } else {
          keepRunning = false;
        }
      } else {
        // After failed attempts, remain in failed status, retaining dirty local state
        this.setStatus('failed', { lastError: lastErr?.message });
        this.pendingFollowUp = false;
        keepRunning = false;
      }
    }

    this.activeTransfer = null;
    return this.status;
  }

  async _defaultRunner(syncConfig) {
    const dshHome = this.dshHome;
    let projectId = syncConfig.projectId || process.env.DEVVM_PROJECT_ID;

    if (!projectId) {
      const devvmIdPath = join(process.env.DEVVM_WORKSPACE || '/root/workspace', '.devvm-id');
      if (existsSync(devvmIdPath)) {
        try {
          projectId = readFileSync(devvmIdPath, 'utf8').trim();
        } catch (e) {}
      }
    }

    if (!projectId) {
      projectId = 'default';
    }

    const sshUser = syncConfig.ssh_user || 'root';
    const sshHost = syncConfig.ssh_host;
    const sshPort = syncConfig.ssh_port || 22;
    const sshKeyPath =
      syncConfig.ssh_key_path || join(process.env.HOME || '/root', '.ssh/id_ed25519');
    const remoteSyncRoot = syncConfig.remote_sync_root || '/var/lib/devvm-sync';

    const source = `${dshHome.replace(/\/$/, '')}/`;
    const destination = `${sshUser}@${sshHost}:${remoteSyncRoot.replace(/\/$/, '')}/${projectId}/`;

    const rsh = `ssh -p ${sshPort} -i ${sshKeyPath} -o StrictHostKeyChecking=accept-new -o BatchMode=yes`;

    const args = [
      '-avz',
      '-e',
      rsh,
      ...RSYNC_FILTER_ARGS,
      source,
      destination,
    ];

    return new Promise((resolve, reject) => {
      const proc = spawn('rsync', args);
      let stdout = '';
      let stderr = '';

      proc.stdout.on('data', (chunk) => {
        stdout += chunk.toString();
      });
      proc.stderr.on('data', (chunk) => {
        stderr += chunk.toString();
      });

      proc.on('error', (err) => {
        reject(new Error(`Failed to execute rsync: ${err.message}`));
      });

      proc.on('close', (code) => {
        if (code !== 0) {
          reject(new Error(`rsync process failed with exit code ${code}: ${stderr || stdout}`));
        } else {
          resolve({ stdout, stderr });
        }
      });
    });
  }
}

function registerWebServerRoutes(webServer, syncManager) {
  if (!webServer || typeof webServer.register !== 'function') return () => {};

  const d1 = webServer.register({
    kind: 'exact',
    path: '/api/sync/trigger',
    handler: async (req, res) => {
      if (req.method !== 'POST') {
        res.writeHead(405, { 'Content-Type': 'application/json' });
        res.end(JSON.stringify({ error: 'Method Not Allowed' }));
        return;
      }
      await syncManager.triggerSync();
      res.writeHead(200, { 'Content-Type': 'application/json' });
      res.end(JSON.stringify({ status: syncManager.getStatus() }));
    },
  });

  const d2 = webServer.register({
    kind: 'exact',
    path: '/api/sync/status',
    handler: async (req, res) => {
      if (req.method !== 'GET' && req.method !== 'HEAD') {
        res.writeHead(405, { 'Content-Type': 'application/json' });
        res.end(JSON.stringify({ error: 'Method Not Allowed' }));
        return;
      }
      res.writeHead(200, { 'Content-Type': 'application/json' });
      res.end(JSON.stringify({ status: syncManager.getStatus(), isDirty: syncManager.isDirty }));
    },
  });

  return () => {
    d1?.();
    d2?.();
  };
}

export function apply(ctx, config) {
  const syncManager = new RemoteSyncManager({
    dshHome: config?.dshHome,
    syncConfig: config,
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

  return syncManager;
}
