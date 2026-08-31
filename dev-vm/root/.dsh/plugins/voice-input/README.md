# @devvm/dsh-voice-input

A DeepSeek Harness (DSH) plugin bundle for archiving raw voice transcriptions and their cleaned-up versions into JSONL files for downstream analysis.

## Features

- **`archive_voice_input` tool**: Records raw voice-input transcriptions paired with faithful cleaned-up text to a JSONL file.
- **`remove_voice_input_record` tool**: Removes an archived record by zero-based index.
- **Configurable target storage**: Supports custom target paths via plugin configuration or environment variables.

## Package & Bundle Manifest

- **Package Name**: `@devvm/dsh-voice-input`
- **Bundle Manifest**: Declares `dsh.bundle.patch: "./cordis.patch.yml"`.
- **Plugin Row**: Registered with ID `tool-voice-input` injecting the `tools` service.

## Configuration

Configuration can be specified in a profile `cordis.patch.yml` or through environment variables.

| Key / Env | Type | Description | Default |
|---|---|---|---|
| `file` (config) | `string` | Target JSONL file path | Defaults to `filePath` or env var |
| `filePath` (config) | `string` | Alternative key for target JSONL file path | Defaults to env var |
| `VOICE_DICTATION_DATA_FILE` | `string` | Environment variable for JSONL path | Defaults to `VOICE_DICTATION_FILE` |
| `VOICE_DICTATION_FILE` | `string` | Fallback environment variable | Defaults to `/root/voice-dictation-cleanup/data/archive_voice_input.jsonl` |

### Example Profile Configuration

In a profile's `cordis.patch.yml`:

```yaml
- id: tool-voice-input
  config:
    file: /path/to/archive_voice_input.jsonl
```

## Tools

### `archive_voice_input`
- **Parameters**:
  - `raw` (`string`, required): Raw actual voice-input transcription.
  - `cleaned` (`string`, required): Cleaned voice-input text faithful to original message.
- **Output**: Confirmation message with zero-based archive index.

### `remove_voice_input_record`
- **Parameters**:
  - `index` (`integer`, required): Zero-based index of the record to remove.
- **Output**: Confirmation message indicating removal or reporting that no record exists at that index.

## Testing

Run targeted tests using Node's built-in test runner:

```sh
node --test
```
