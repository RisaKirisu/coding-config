export function normalizeConfig(value) {
  return {
    provider: typeof value?.provider === 'string' ? value.provider.trim() : '',
    model: typeof value?.model === 'string' && value.model.trim() ? value.model : '',
    reasoningEffort: typeof value?.reasoningEffort === 'string' ? value.reasoningEffort.trim() : '',
  }
}

export function isSubagent(agent) {
  return agent?.session?.header?.origin === 'subagent'
}
