/** Worker prompts are the instructions/*.md files, read when a run starts so each run pins the text it was built with. */
import { readFileSync } from 'node:fs'
const instruction = (name) => readFileSync(new URL('./instructions/' + name + '.md', import.meta.url), 'utf8')

export function loadInstructions() {
  return {
    common: instruction('common'),
    builder: instruction('builder'),
    reviewer: instruction('code-reviewer'),
    tester: instruction('test-auditor'),
    auditorFindings: instruction('auditor-findings'),
  }
}
