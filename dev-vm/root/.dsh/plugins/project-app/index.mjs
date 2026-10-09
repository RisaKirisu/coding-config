export const inject = ['webServer'];

/** Provision the Project identity and exact Control origins before client boot. */
export function apply(ctx) {
  const projectId = process.env.DEVVM_EMBED_PROJECT_ID;
  if (!projectId) return;
  const controlOrigins = JSON.parse(process.env.DEVVM_CONTROL_ORIGINS || '[]');
  if (!Array.isArray(controlOrigins) || !controlOrigins.every(origin => typeof origin === 'string' && new URL(origin).origin === origin)) {
    throw new Error('DEVVM_CONTROL_ORIGINS must be a JSON array of exact origins.');
  }
  ctx.on('webserver/index-inject', table => {
    table.push({kind: 'global', name: '__DEVVM_EMBED__', value: {projectId, controlOrigins}});
  });
}
