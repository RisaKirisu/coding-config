import { tool } from "@opencode-ai/plugin"
import { readFile, writeFile } from "node:fs/promises"

const file = "/home/risa/projects/voice-dictation-cleanup/data/archive_voice_input.jsonl"

export default tool({
  description: "Remove an archived voice input JSONL record by zero-based index.",
  args: {
    index: tool.schema.number().int().min(0).describe("Zero-based index of the archived record to remove."),
  },
  async execute(args) {
    const text = await readFile(file, "utf8").catch(() => "")
    const lines = text.split("\n").filter(Boolean)
    if (args.index >= lines.length) return `No voice input record exists at index ${args.index}.`

    lines.splice(args.index, 1)
    await writeFile(file, lines.length ? `${lines.join("\n")}\n` : "", "utf8")
    return `Voice input record ${args.index} removed successfully.`
  },
})
