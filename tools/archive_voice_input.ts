import { tool } from "@opencode-ai/plugin"
import { appendFile, mkdir } from "node:fs/promises"
import path from "node:path"

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
    const dir = "/home/risa/projects/voice-dictation-cleanup/data"
    await mkdir(dir, { recursive: true })
    await appendFile(
      path.join(dir, "archive_voice_input.jsonl"),
      `${JSON.stringify({ raw: args.raw, cleaned: args.cleaned })}\n`,
      "utf8",
    )
    return "Voice input archived successfully."
  },
})
