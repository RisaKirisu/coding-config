/** Real DSH services for local tests; no model provider or fake runtime is installed. */
import { Context } from '@deepseek-ai/cordis'
import LocalSubprocess from '@deepseek-ai/dsh-subprocess-local'
import LocalBash from '@deepseek-ai/dsh-bash-local'
import Tools from '@deepseek-ai/dsh-tools'
import SystemPrompt from '@deepseek-ai/dsh-system-prompt'
import Agents from '@deepseek-ai/dsh-agent'
import Sessions from '@deepseek-ai/dsh-session'
import Llm from '@deepseek-ai/dsh-llm'
import Projections from '@deepseek-ai/dsh-session-projection'
import AgentLoop from '@deepseek-ai/dsh-agent-loop'
import Subagents from '@deepseek-ai/dsh-subagent'
import * as Spawn from '@deepseek-ai/dsh-subagent-spawn-in-process'
import * as BashTool from '@deepseek-ai/dsh-tool-bash'
import * as ShellEnv from '@deepseek-ai/dsh-shell-env'
import TokenMeter from '@deepseek-ai/dsh-token-meter'

export async function runtime() {
  const ctx = new Context()
  const fibers = []
  for (const plugin of [LocalSubprocess, LocalBash, SystemPrompt, Tools, ShellEnv, BashTool, Agents, Sessions, Llm, Projections, AgentLoop, Subagents, Spawn, TokenMeter]) {
    const fiber = ctx.plugin(plugin, {})
    fibers.push(fiber)
    await fiber.await()
  }
  return { ctx, close: async () => { for (const fiber of fibers.reverse()) await fiber.dispose() } }
}
