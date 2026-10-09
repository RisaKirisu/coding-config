export const inject = ['webServer'];

/** Provision the Project identity and Control origin policy before client boot. */
export function apply(ctx) {
  const projectId = process.env.DEVVM_EMBED_PROJECT_ID;
  if (!projectId) return;
  const controlOrigins = JSON.parse(process.env.DEVVM_CONTROL_ORIGINS || '[]');
  if (!Array.isArray(controlOrigins) || !controlOrigins.every(origin => typeof origin === 'string' && new URL(origin).origin === origin)) {
    throw new Error('DEVVM_CONTROL_ORIGINS must be a JSON array of exact origins.');
  }
  const controlDomain = process.env.DEVVM_CONTROL_DOMAIN || null;
  if (controlDomain && (controlDomain.length > 253 || !controlDomain.split('.').every(label => /^[a-z0-9](?:[a-z0-9-]{0,61}[a-z0-9])?$/i.test(label)))) {
    throw new Error('DEVVM_CONTROL_DOMAIN must be a DNS domain.');
  }
  ctx.on('webserver/index-inject', table => {
    table.push({kind: 'global', name: '__DEVVM_EMBED__', value: {projectId, controlOrigins, controlDomain}});
  });
}
