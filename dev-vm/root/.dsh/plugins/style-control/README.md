# @devvm/dsh-style-control

Agent formatting and tone control plugin for DeepSeek Harness (DSH).

## Features
- **Settings UI**: Adds a "Style Control" tab in the Settings panel for creating, editing, and deleting style presets (Default, Professional, Creative, etc.).
- **Chat UI**: Adds a style selector dropdown in the composer toolbar (`conversation.input.right`, positioned to the left of the model selector) to toggle style presets per session.
- **System Prompt Injection**: Injects the active preset's instructions into the agent system prompt at order `1`, enclosed in `<formatting_and_tone>` tags.
