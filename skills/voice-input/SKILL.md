---
name: voice-input
description: Instruction for processing voice-input text. Load this whenever the user is likely inputting by voice (filler words, repetition, run-on sentences, homophone errors).
license: MIT
compatibility: opencode
metadata:
  audience: all
---

# Voice Input Recognition

Voice processing produces two separate outputs:

- An internal interpretation used to understand and act on the request.
- A training record containing `raw`, the voice-transcribed text exactly as received, and `cleaned`, a faithful transcript with only necessary corrections.

Never substitute the internal interpretation for `cleaned`. Follow these steps:

## Step 0: Scope The Voice Input

For hybrid messages, separate spoken instructions from target content such as pasted text, code, logs, quotes, or examples. Clean and archive only the spoken instruction text.

## Step 1: Interpret The Request

Determine the user's intent internally so you can respond correctly. This interpretation may be concise or reorganized. If the intent remains ambiguous, ask a clarifying question.

## Step 2: Produce The Cleaned Transcript

Create `cleaned` directly from `raw`. It is a faithful transcript produced with the minimum edits needed to recover the user's intended speech. Preserve intended information, wording, order, tone, and thinking-aloud structure.

Preserving wording does not mean preserving transcription errors. Correct a word or phrase when context strongly indicates that the transcription is wrong, including technical terms and homophones.

Allowed edits:

- Correct probable transcription and word-recognition errors.
- Add punctuation, capitalization, and sentence boundaries.
- Remove empty fillers, stutters, abandoned false starts, and accidental repetition.
- Resolve clear self-corrections to the user's final intended wording.

Do not paraphrase or stylistically rewrite, summarize or compress, or reorder the user's thoughts. When uncertain whether something is an error or intentional speech, preserve it.

## Step 3: Archive The Voice Input

After processing a voice-input user message, always call the `archive_voice_input` tool with:

- `raw`: the actual voice-input portion exactly as received.
- `cleaned`: the faithful transcript produced in Step 2.

Always archive voice input regardless of the active response mode.

## Example

Raw transcription:
> So basically I need you to... so there's this function called like process data or process_data and it's um it's taking two long. Can you make it like faster? Maybe use like a sink or something. It's in the you tills file. No the helpers file. The helpers file in source.

Cleaned transcript:
> There's this function called `process_data`, and it's taking too long. Can you make it faster? Maybe use async or something. It's in the helpers file in `src`.

Internal interpretation, not archived:
> Optimize the `process_data` function in `src/helpers` for performance, potentially by making it async.
