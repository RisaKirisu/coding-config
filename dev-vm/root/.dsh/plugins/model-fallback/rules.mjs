/** Rule and route decisions for model fallback, independent of Cordis wiring. */

/**
 * The provider's own stream-disconnect wording. DSH classifies it as a
 * provider error, so no retry policy retries it until it reads as transport.
 */
export const STREAM_DISCONNECT_RULE = {
  code: 'PI_AI_ERROR',
  messageContains: 'stream disconnected before completion',
  treatAs: 'TRANSPORT',
}

/**
 * Whether one configured rule selects a failure.
 * @param rule - configured `{ code, messageContains, treatAs }`.
 * @param failure - normalized provider failure.
 * @returns whether the failure matches.
 */
export function ruleMatches(rule, failure) {
  if (rule.code !== failure.code) return false
  return rule.messageContains === '' || failure.message.includes(rule.messageContains)
}

/**
 * Find the first rule selecting a failure.
 * @param rules - configured rules in order.
 * @param failure - normalized provider failure.
 * @returns the matching rule, or `undefined`.
 */
export function findRule(rules, failure) {
  return rules.find((rule) => ruleMatches(rule, failure))
}

/**
 * Rewrite a matched failure's code so the provider retry policy retries it.
 * @param rules - configured rules in order.
 * @param failure - normalized provider failure.
 * @returns the rewritten failure, or `undefined` when its code already stands.
 */
export function reclassifyFailure(rules, failure) {
  const rule = findRule(rules, failure)
  if (rule === undefined || rule.treatAs === '' || rule.treatAs === failure.code) return undefined
  return { ...failure, code: rule.treatAs }
}

/**
 * Whether a failure may roll to the next model. The provider policy owns
 * bounded retries, so reaching this decision with a normal policy means its
 * budget is spent; unlimited mode never spends one, and a configured rule
 * selects failures no policy retries.
 * @param retryPolicy - resolved policy of the adapter that served the request.
 * @param rules - configured rules in order.
 * @param failure - normalized provider failure.
 * @returns whether the failure may roll.
 */
export function isRollable(retryPolicy, rules, failure) {
  if (retryPolicy?.mode === 'always') return true
  if (retryPolicy?.retryableCodes.includes(failure.code) === true) return true
  return findRule(rules, failure) !== undefined
}

/**
 * The fallback entry following the route that just failed.
 * @param fallbacks - configured candidates in order.
 * @param current - the route that failed.
 * @returns the next candidate, or `undefined` when none remains.
 */
export function nextFallback(fallbacks, current) {
  const index = fallbacks.findIndex((entry) => entry.provider === current.provider && entry.model === current.model)
  const next = index === -1 ? 0 : index + 1
  return next >= fallbacks.length ? undefined : fallbacks[next]
}

/**
 * Validate one submitted settings document and detach its rows.
 * @param input - parsed request body.
 * @returns the detached `{ fallbacks, retriableErrors }` section.
 * @throws When a row omits the route or code it edits.
 */
export function validateConfig(input) {
  const fallbacks = input?.fallbacks
  if (!Array.isArray(fallbacks)) throw new Error('fallbacks must be an array')
  fallbacks.forEach((entry, index) => {
    if (typeof entry?.provider !== 'string' || entry.provider === '') throw new Error(`fallbacks[${index}] requires a provider`)
    if (typeof entry.model !== 'string' || entry.model === '') throw new Error(`fallbacks[${index}] requires a model`)
  })
  const retriableErrors = input?.retriableErrors
  if (!Array.isArray(retriableErrors)) throw new Error('retriableErrors must be an array')
  retriableErrors.forEach((rule, index) => {
    if (typeof rule?.code !== 'string' || rule.code === '') throw new Error(`retriableErrors[${index}] requires a code`)
  })
  return {
    fallbacks: fallbacks.map((entry) => ({
      provider: entry.provider,
      model: entry.model,
      reasoningEffort: entry.reasoningEffort ?? '',
    })),
    retriableErrors: retriableErrors.map((rule) => ({
      code: rule.code,
      messageContains: rule.messageContains ?? '',
      treatAs: rule.treatAs || 'TRANSPORT',
    })),
  }
}
