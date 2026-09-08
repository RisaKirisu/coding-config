import {
  DEFAULT_FILE_PATH,
  archiveVoiceInput,
  countLines,
  removeVoiceInputRecord,
  resolveVoiceInputFile,
} from './handlers.mjs'

let defineTool
try {
  const mod = await import('@deepseek-ai/dsh-tools')
  defineTool = mod.defineTool
} catch {
  const { createRequire } = await import('node:module')
  const dshReq = createRequire(
    process.env.DSH_PACKAGE_ENTRY ||
    '/usr/local/lib/node_modules/@deepseek-ai/dsh/package.json',
  )
  const mod = await import(dshReq.resolve('@deepseek-ai/dsh-tools'))
  defineTool = mod.defineTool
}

export const name = 'tool-voice-input'
export const inject = ['tools']

export {
  DEFAULT_FILE_PATH,
  archiveVoiceInput,
  countLines,
  removeVoiceInputRecord,
  resolveVoiceInputFile,
}

export function apply(ctx, config) {
  const filePath = resolveVoiceInputFile(config)

  ctx.tools.register(
    defineTool({
      name: 'archive_voice_input',
      description:
        'Archive a raw voice input transcription and its cleaned-up version as JSONL for downstream dictation analysis.',
      parameters: {
        raw: {
          type: 'string',
          required: true,
          description: 'Raw voice-input text.',
        },
        cleaned: {
          type: 'string',
          required: true,
          description:
            'Cleaned voice-input text, faithful to the original wording. Do not paraphrase or summarize.',
        },
      },
      output: {
        schema: {
          type: 'object',
          additionalProperties: false,
          properties: {
            text: {
              type: 'string',
              required: true,
            },
          },
        },
        render: (_args, value) => [{ type: 'text', text: value.text }],
      },
      async execute(args) {
        return archiveVoiceInput(filePath, args)
      },
      presentCall: (args) => ({
        card: 'generic',
        title: 'Archive voice input',
        kind: 'other',
        rawInput: args,
      }),
    }),
  )

  ctx.tools.register(
    defineTool({
      name: 'remove_voice_input_record',
      description: 'Remove an archived voice input JSONL record by zero-based index.',
      parameters: {
        index: {
          type: 'integer',
          required: true,
          description: 'Zero-based index of the archived record to remove.',
        },
      },
      output: {
        schema: {
          type: 'object',
          additionalProperties: false,
          properties: {
            text: {
              type: 'string',
              required: true,
            },
          },
        },
        render: (_args, value) => [{ type: 'text', text: value.text }],
      },
      async execute(args) {
        return removeVoiceInputRecord(filePath, args)
      },
      presentCall: (args) => ({
        card: 'generic',
        title: `Remove voice input record #${args.index}`,
        kind: 'other',
        rawInput: args,
      }),
    }),
  )
}
