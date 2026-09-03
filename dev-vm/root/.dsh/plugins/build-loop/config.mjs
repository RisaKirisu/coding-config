/** Settings defaults and cross-field validation for the `build-loop` namespace. Dependency-free for tests. */
import {
  DEFAULT_BUILD_PERSONA,
  DEFAULT_DENIED_TOOLS,
  DEFAULT_MAX_FIX_ROUNDS,
  DEFAULT_REVIEW_PERSONA,
  DEFAULT_TEST_PERSONA,
} from './prompts.mjs'

export const DEFAULTS = Object.freeze({
  provider: 'spawn',
  maxFixRounds: DEFAULT_MAX_FIX_ROUNDS,
  buildPersona: DEFAULT_BUILD_PERSONA,
  reviewPersona: DEFAULT_REVIEW_PERSONA,
  testPersona: DEFAULT_TEST_PERSONA,
  deniedTools: DEFAULT_DENIED_TOOLS,
})

/** Reject a saved section the loop could not run with. */
export function validateConfig(value) {
  for (const key of ['buildPersona', 'reviewPersona', 'testPersona']) {
    if (typeof value[key] !== 'string' || value[key].trim().length === 0) {
      throw new Error(`${key} must be a non-empty prompt`)
    }
  }
  if (!Number.isSafeInteger(value.maxFixRounds) || value.maxFixRounds < 0) {
    throw new Error('maxFixRounds must be a non-negative integer')
  }
  if (typeof value.provider !== 'string' || value.provider.trim().length === 0) {
    throw new Error('provider must be a non-empty provider name')
  }
  if (!Array.isArray(value.deniedTools) || !value.deniedTools.every((tool) => typeof tool === 'string')) {
    throw new Error('deniedTools must be a list of tool names')
  }
}

