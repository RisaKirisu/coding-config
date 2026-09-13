/** Approved checks use the existing bash tool. Results stay with the in-memory run. */
import { randomUUID } from 'node:crypto'

export async function runCheck(ctx, exec, run, check) {
  const startedAt = Date.now()
  const result = await ctx.tools.execute({
    callId: randomUUID(), name: 'bash', parent: exec.token, rootCallId: exec.rootCallId,
    agent: exec.agent, signal: exec.signal,
    arguments: { command: check.command, workdir: run.cwd, description: 'Run approved build verification check', timeoutMs: 30 * 60 * 1000 },
  })
  const value = result.isError ? undefined : result.value
  const record = {
    check: check.id, command: check.command, attempt: run.attempt,
    startedAt, durationMs: Date.now() - startedAt,
    exitCode: value?.exitCode ?? null,
    failed: result.isError || value?.kind !== 'foreground' || Boolean(value?.aborted || value?.timedOut || exec.signal.aborted),
    stdout: value?.stdout ?? null, stderr: value?.stderr ?? null,
    error: result.isError ? result.error.message : null,
  }
  run.checks.push(record)
  return record
}
