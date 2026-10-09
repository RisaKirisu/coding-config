# Native DSH replay recording

`dsh-messages-session.v4.jsonl` is the unmodified upstream `snapshots/web/deepseek-messages-chat/session.v4.jsonl` recording from DeepSeek Harness `0.2.0-rc.2`. It records the prompt `只回复 MESSAGES_WEB_READY，不调用工具。` and the native model response `MESSAGES_WEB_READY`.

The Project-app browser fixture loads it with the published `@deepseek-ai/dsh-llm-replay@0.2.0-rc.2` plugin. The recording supplies model chunks; the daemon, native Session admission, history persistence, browser transport, and composer are real. Replay maps one live Session to this recording per process, so the interaction suite restarts only its owned test runtime before each prompt scenario.

Upstream: <https://github.com/deepseek-ai/deepseek-harness>. The upstream MIT license is preserved in `LICENSE.deepseek-harness`.
