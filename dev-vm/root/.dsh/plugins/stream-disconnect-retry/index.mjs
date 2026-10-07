export const name = 'stream-disconnect-retry'
export const inject = ['llm']

export function apply(ctx) {
  ctx.on('llm/stream', async function* (_options, next) {
    for await (const chunk of next()) {
      if (
        chunk.type === 'finish' &&
        chunk.reason.kind === 'error' &&
        chunk.reason.failure.code === 'PI_AI_ERROR' &&
        chunk.reason.failure.message.includes('stream disconnected before completion')
      ) {
        yield {
          ...chunk,
          reason: {
            ...chunk.reason,
            failure: { ...chunk.reason.failure, code: 'TRANSPORT' },
          },
        }
      } else {
        yield chunk
      }
    }
  })
}
