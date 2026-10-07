# Stream disconnect retry

This local server plugin reclassifies `PI_AI_ERROR` failures containing `stream disconnected before completion` as `TRANSPORT`. DSH's existing provider retry policy owns the retry limit, delay, cancellation, and durable retry events; keep those responsibilities there. Other failures and stream chunks pass through unchanged.

The Web profile loads `index.mjs` by absolute path from its patch. No package installation or Web bundle build is required. Profile HMR loads configuration changes; subsequent source edits require a restart unless module watching is enabled.

Run `node --test plugins/stream-disconnect-retry/test.mjs` from `/root/.dsh` to verify classification and bounded recovery with the installed DSH runtime.
