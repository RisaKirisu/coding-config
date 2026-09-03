import { RemoteSyncManager } from './index.mjs';

const manager = new RemoteSyncManager();
let status = 'failed';

try {
  status = await manager.reconcile();
} catch (error) {
  status = manager.getStatus();
  console.error(`remote-sync: reconciliation crashed: ${error?.message ?? error}`);
}

console.log(`remote-sync: status ${status}`);

// The DSH Runtime launch must never be blocked by Session Sync.
process.exit(0);
