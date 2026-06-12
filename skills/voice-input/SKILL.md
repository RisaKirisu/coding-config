---
name: voice-input
description: Instruction for processing voice-input text. Load this whenever the user is likely inputting by voice (filler words, repetition, run-on sentences, homophone errors).
license: MIT
compatibility: opencode
metadata:
  audience: all
---

# Voice Input Recognition

When processing voice-transcribed input, follow these steps before acting on the request:

## Step 0: Scope The Voice Input

For hybrid messages, separate spoken instructions from target content such as pasted text, code, logs, quotes, or examples. Clean and archive only the spoken instruction text.

## Step 1: Identify Voice Artifacts

Recognize common transcription artifacts:
- Filler words: "so", "basically", "um", "uh", "like", "you know", "I mean", "right"
- False starts and repetition: the user restating the same phrase multiple times as they formulate their thought
- Homophones and near-homophones: words transcribed incorrectly due to similar pronunciation (e.g., "scale" instead of "skill", "mp" instead of ".md")
- Run-on structure: lack of punctuation or sentence boundaries since speech is continuous
- Self-corrections: the user saying something, then immediately rephrasing it (the later version is the intended one)

## Step 2: Extract True Intention

1. Strip all filler words and repeated phrases.
2. When the user restates something, take the last version as the intended meaning.
3. Resolve likely homophones by context (e.g., if discussing a documentation file, ".mp" likely means ".md").
4. Identify the core action the user wants performed and the key parameters/details.

## Step 3: Reconstruct as Clean Text

Rewrite the user's input internally as a clear, concise, and concrete request before act on it. If the reconstructed intent is ambiguous even after cleaning, ask clarifying questions.

## Step 4: Archive The Voice Input

After processing a voice-input user message, always call the `archive_voice_input` tool with:

- `raw`: the actual voice-input portion exactly as received.
- `cleaned`: the corrected voice-input portion only. Preserve the user's wording, order, tone, and thinking-out-loud structure as much as possible. Correct transcription mistakes and remove voice artifacts such as filler, false starts, run-on grammar, and meaningless repetition. Do not compress the message, and be faithful instead. Do not paraphrase, summarize, or reduce user's message.

Note that the `cleaned` version here isn't necessarily the same as your reconstructed clean text in step 4, as the purpose for this particular step is to gather training material for voice-input cleaning as a side-task, rather than extracting user intention.

This should ALWAYS be called regardless the mode you are in to archieve the voice input.

## Examples

### Example 1

Raw transcription:
> So I want to I want to create a new file. Um, it should be like a config file. Put it in the source folder. Actually, put it in the the uh the config folder. It should have like the date base connection sting and um the pot number.

True meaning:
Create a config file in the config/ folder containing the database connection string and port number.

### Example 2

Raw transcription:
> Can you fix the... the thing where the button doesn't doesn't work? Like when you click sub mit it just it just does nothing. I think it's in the forum component. The form component yeah.

True meaning:
Fix the submit button in the form component — clicking it currently does nothing. ("sub mit" → "submit", "forum" → "form")

### Example 3

Raw transcription:
> So basically I need you to... so there's this function called like process data or process_data and it's um it's taking two long. Can you make it like faster? Maybe use like a sink or something. It's in the you tills file. No the helpers file. The helpers file in source.

True meaning:
Optimize the process_data function in src/helpers for performance, potentially by making it async. ("two long" → "too long", "a sink" → "async", "you tills" → "utils", "source" → "src")
