/** Transport contracts shared by the tools and worker report validators. */
import { assertSupportedJsonSchema } from '@deepseek-ai/dsh-tools'
const text = { type: 'string' }
const strings = { type: 'array', items: text }
const object = (properties, required = Object.keys(properties)) => ({ type: 'object', properties, required, additionalProperties: false })
const array = (items) => ({ type: 'array', items })

export const CONTRACT_SCHEMA = object({
  instruction: text,
  scope: strings,
  behaviors: array(object({ id: text, observation: text, check: text }, ['id', 'observation'])),
  checks: array(object({ id: text, command: text, scope: strings, expectedExit: { type: 'integer' } })),
})

export const HANDOFF_SCHEMA = object({
  outcome: { type: 'string', enum: ['approach', 'ready-for-audit', 'needs-decision', 'blocked'] },
  report: text,
  files: strings,
  questions: strings,
  observations: strings,
  exceptions: strings,
  findings: array(object({ id: text, status: { type: 'string', enum: ['fixed', 'disputed'] }, evidence: text })),
}, ['outcome', 'report'])

export const VERDICT_SCHEMA = object({
  findings: array(object({
    impact: { type: 'string', enum: ['high', 'medium', 'low'] },
    location: text, rule: text, evidence: text, correction: text,
  })),
  prior: array(object({ id: text, status: { type: 'string', enum: ['resolved', 'open'] }, evidence: text })),
  report: text,
  blocked: text,
}, ['findings', 'prior', 'report'])

export const DECISION_SCHEMA = object({
  run_id: text,
  revision: { type: 'integer' },
  kind: { type: 'string', enum: ['approve_design', 'revise_design', 'triage', 'continue', 'accept', 'ask', 'abandon', 'inspect'] },
  instructions: text,
  dispositions: array(object({ id: text, action: { type: 'string', enum: ['fix', 'ignore'] }, reason: text })),
  confirmations: strings,
  role: { type: 'string', enum: ['builder', 'code', 'test'] },
  question: text,
  run_in_background: { type: 'boolean' },
}, ['run_id', 'revision', 'kind'])

for (const schema of [CONTRACT_SCHEMA, HANDOFF_SCHEMA, VERDICT_SCHEMA, DECISION_SCHEMA]) assertSupportedJsonSchema(schema)

/** Convert standard JSON Schema properties to defineTool's parameter spelling. */
export function parameters(schema) {
  const spec = (value) => {
    const { required, ...result } = value
    if (value.type === 'object') result.properties = parameters(value)
    if (value.type === 'array') result.items = spec(value.items)
    return result
  }
  return Object.fromEntries(Object.entries(schema.properties).map(([key, value]) => [key, {
    ...spec(value), ...(schema.required.includes(key) ? { required: true } : {}),
  }]))
}
