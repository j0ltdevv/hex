# Fork changelog

This file records changes made in `j0ltdevv/hex-groq` on top of upstream Hex. It is
not an upstream release history and does not imply a published binary.

## Unreleased

### Documentation

- Reworked the README around source-only installation, opt-in Groq setup,
  platform prerequisites, architecture, known limits, and support.
- Added [`docs/FORK.md`](docs/FORK.md) and
  [`docs/UPSTREAM-SYNC.md`](docs/UPSTREAM-SYNC.md).
- Labelled upstream installer, release-feed, and signed-update documentation as
  upstream-only.
- Documented Groq settings in the CLI guide and clarified SDK/local-API scope.

### SDK

- Added local native-helper packaging and `HEX_SERVICE_BINARY` resolution for
  the TypeScript SDK.

### Performance

- Recorded a headless routing benchmark on a remote Linux worker: 407 ms serial
  versus 306 ms parallel under fake API latency.

## 2026-11-26 — Groq provider baseline

Based on upstream `86fd7f1966f2cd5e83b7ff70197706945d32c661`.

### Added

- Explicit `Local` / `Groq` dictation provider; Local remains the default.
- Groq models `whisper-large-v3` and `whisper-large-v3-turbo`.
- Key resolution from `GROQ_API_KEY`, owner-only `groq.key`, then the legacy
  redacted setting.
- Groq UI, CLI settings, translations, provider attribution, and bounded
  secret-free usage records.
- Source-only fork documentation and public repository metadata.

### Fixed

- Rejected jobs no longer leave cloud uploads running.
- A successful Groq result is retained when local transcription fails.
- Cancellation is rechecked when a cloud result is already queued.
- The API key is snapshotted at submission instead of resolved later.
- macOS `/dictations` stays local.
- WAV retry is limited to HTTP 415/422.
- Unix key-file reads use `O_NOFOLLOW` and descriptor metadata checks.
- Multipart requests use a fresh unpredictable boundary.
- Groq fields appear only when the Groq provider is selected.

### Security and distribution

- Source builds force `HEX_LOCAL_BUILD=1` and cannot install upstream updates.
- Upstream installers no longer act as implicit fork distribution paths.
- Windows staged binaries are Authenticode-verified before execution.
- Automatic upstream release/deploy triggers are disabled on the fork.
- Issues, Discussions, and private vulnerability reporting are enabled.

### Verification

- Focused suite: 185 passed, 0 failed, 1 ignored on Linux.
- `cargo fmt --all --check`, code-shape, and Nix pin checks pass.
- Advanced light/dark goldens regenerated and visually inspected.
- Portable Linux release built with `GGML_NATIVE=OFF` and started locally.
- Linux Groq dictation observed end to end on 2026-11-25.

Native macOS/Windows behavior and a physical dictation on the newest portable
binary remain unverified.
