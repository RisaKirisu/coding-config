import { tool } from "@opencode-ai/plugin"
import { appendFile, mkdir, readFile } from "node:fs/promises"
import path from "node:path"

const file = "/home/risa/projects/voice-dictation-cleanup/data/archive_voice_input.jsonl"

export default tool({
  description:
    "Archive a raw voice input transcription and its cleaned-up version as JSONL for downstream dictation analysis.",
  args: {
    raw: tool.schema
      .string()
      .min(1)
      .describe("Raw actual voice-input portion only."),
    cleaned: tool.schema
      .string()
      .min(1)
      .describe("Cleaned voice-input text; be faithful to the original message. Do not paraphrase or summarize."),
  },
  async execute(args) {
    await mkdir(path.dirname(file), { recursive: true })
    const index = await count(file)
    await appendFile(
      file,
      `${JSON.stringify({ raw: args.raw, cleaned: args.cleaned })}\n`,
      "utf8",
    )
    return `Voice input archived successfully at index ${index}.`
  },
})

async function count(file: string) {
  return readFile(file, "utf8").then(
    (text) => text.split("\n").filter(Boolean).length,
    () => 0,
  )
}
