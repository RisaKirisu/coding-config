import { defineTool } from '@deepseek-ai/dsh-tools'
import { appendFile, mkdir, readFile, writeFile } from 'node:fs/promises'
import path from 'node:path'

export const name = 'tool-voice-input'
export const inject = ['tools']

const DEFAULT_FILE_PATH = '/root/voice-dictation-cleanup/data/archive_voice_input.jsonl'

async function countLines(filePath) {
  try {
    const text = await readFile(filePath, 'utf8')
    return text.split('\n').filter(Boolean).length
  } catch {
    return 0
  }
}

export function apply(ctx, config) {
  const filePath =
    (typeof config?.file === 'string' && config.file.length > 0)
      ? config.file
      : (typeof config?.filePath === 'string' && config.filePath.length > 0)
      ? config.filePath
      : (process.env.VOICE_DICTATION_DATA_FILE || process.env.VOICE_DICTATION_FILE || DEFAULT_FILE_PATH)

  ctx.tools.register(
    defineTool({
      name: 'archive_voice_input',
      description:
        'Archive a raw voice input transcription and its cleaned-up version as JSONL for downstream dictation analysis.',
      parameters: {
        raw: {
          type: 'string',
          required: true,
          description: 'Raw actual voice-input portion only.',
        },
        cleaned: {
          type: 'string',
          required: true,
          description:
            'Cleaned voice-input text; be faithful to the original message. Do not paraphrase or summarize.',
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
        if (!args.raw || typeof args.raw !== 'string' || args.raw.trim().length === 0) {
          throw new Error('invalid arguments: `raw` must be a non-empty string')
        }
        if (!args.cleaned || typeof args.cleaned !== 'string' || args.cleaned.trim().length === 0) {
          throw new Error('invalid arguments: `cleaned` must be a non-empty string')
        }

        await mkdir(path.dirname(filePath), { recursive: true })
        const index = await countLines(filePath)
        await appendFile(
          filePath,
          `${JSON.stringify({ raw: args.raw, cleaned: args.cleaned })}\n`,
          'utf8',
        )
        return {
          text: `Voice input archived successfully at index ${index}.`,
        }
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
        if (typeof args.index !== 'number' || !Number.isInteger(args.index) || args.index < 0) {
          throw new Error('invalid arguments: `index` must be a non-negative integer')
        }

        const text = await readFile(filePath, 'utf8').catch(() => '')
        const lines = text.split('\n').filter(Boolean)
        if (args.index >= lines.length) {
          return {
            text: `No voice input record exists at index ${args.index}.`,
          }
        }

        lines.splice(args.index, 1)
        await mkdir(path.dirname(filePath), { recursive: true })
        await writeFile(filePath, lines.length ? `${lines.join('\n')}\n` : '', 'utf8')
        return {
          text: `Voice input record ${args.index} removed successfully.`,
        }
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
