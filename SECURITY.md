# Security

Report vulnerabilities through private vulnerability reporting on this fork:
<https://github.com/j0ltdevv/hex-groq/security/advisories/new>

Do not open a public issue for an undisclosed vulnerability. This is an
unofficial fork; Anomaly Innovations does not support or triage it.

## Scope

This repository is source-only. Builds force `HEX_LOCAL_BUILD=1` and cannot
check for, download, or install an upstream update. There is no fork signing key,
binary release, or update service.

The default Local path keeps dictation audio on the device. The optional Groq
provider is the only path in this fork that sends dictation audio to a third
party.

## Groq threat model

When provider `Groq` is explicitly selected and a usable key exists at
submission time, Hex sends the complete dictation clip, selected language, and
available vocabulary/screen-term prompt to Groq. The key is sent only in the
bearer authorization header.

The implementation snapshots the key at submission, redacts it from CLI/debug
output, cancels rejected jobs, rechecks cancellation before accepting a queued
result, and falls back to Local on missing key, request failure, empty result,
or rejection. Meetings, `/dictations`, the local transcription API, and
`hex transcribe` remain local.

If you suspect key exposure, revoke the key with the provider first, then report
the issue privately. Do not include the key itself.

## Logs and personal data

Never paste API keys, bearer tokens, private transcripts, vocabulary or
screen-term prompts, or unredacted `live.ndjson` / `process.log` output into an
issue, pull request, or chat. Redact account names and local paths, too.

Key bytes are not logged, but other captured text can be. Share only the minimum
redacted excerpt needed to reproduce a problem.

## Local execution

Custom command handlers and npm dependencies execute locally with the user's
normal permissions. They are supervised for lifecycle and bounded IPC, but they
are not sandboxed.
