# Security

Report vulnerabilities through private vulnerability reporting on this fork:
https://github.com/j0ltdevv/hex/security/advisories/new
Do not open a public issue for an undisclosed vulnerability. This is an
unofficial fork; Anomaly Innovations does not support or triage it.

Never paste API keys, bearer tokens, private transcripts, vocabulary or
screen-term prompts, or unredacted `live.ndjson` / `process.log` output into an
issue, pull request, or chat. Redact account names and local paths, too. If the
opt-in Groq provider is involved, say so: dictation audio leaves the machine
for that provider. Key bytes are never logged, but other captured text can be.

Custom command handlers and npm dependencies execute locally with the user's
normal permissions. They are supervised for lifecycle and bounded IPC, but they
are not sandboxed.
