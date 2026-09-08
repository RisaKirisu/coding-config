import { appendFile, mkdir, readFile, writeFile } from 'node:fs/promises'
import path from 'node:path'

export const DEFAULT_FILE_PATH = '/root/voice-dictation-cleanup/data/archive_voice_input.jsonl'

export async function countLines(filePath) {
  try {
    const text = await readFile(filePath, 'utf8')
    return text.split('\n').filter(Boolean).length
  } catch {
    return 0
  }
}

export function resolveVoiceInputFile(config, env = process.env) {
  return (typeof config?.file === 'string' && config.file.length > 0)
    ? config.file
    : (typeof config?.filePath === 'string' && config.filePath.length > 0)
    ? config.filePath
    : (env.VOICE_DICTATION_DATA_FILE || env.VOICE_DICTATION_FILE || DEFAULT_FILE_PATH)
}

export async function archiveVoiceInput(filePath, args) {
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
}

export async function removeVoiceInputRecord(filePath, args) {
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
}
