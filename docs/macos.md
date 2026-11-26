# macOS source build

This fork is source-only on macOS. It does not publish a signed app, notarized
DMG, or update feed. The official Anomaly downloads described in upstream
documentation are not this Groq fork.

## Requirements

- macOS 15 or later.
- Xcode 26 and its command-line tools.
- Stable Rust through `rustup`.
- The native dependencies and setup steps in [`../AGENTS.md`](../AGENTS.md).

## Setup

From the repository root:

```sh
./scripts/setup.sh
./scripts/setup-parakeet.sh
```

The second script prepares the local speech-model build dependencies. Model
weights are downloaded separately by Hex on first run.

## Build an unsigned local app

```sh
HEX_RELEASE_UNSIGNED=1 scripts/build-app.sh
```

The script produces the upstream `Hex.app` / `anomaly.hex` identity as an
unsigned local bundle. This is useful for testing; it is not a distributable
signed release. `.cargo/config.toml` forces `HEX_LOCAL_BUILD=1`, so the app does
not check for or install upstream updates.

The CLI lives inside the bundle. Link it into a directory you own if you want
`hex` on `PATH`:

```sh
ln -s /path/to/Hex.app/Contents/MacOS/hex ~/.local/bin/hex
```

## Groq provider

The provider contract is the same on macOS:

- Local remains the default.
- Groq is explicit opt-in for complete-clip dictation.
- Meetings, `/dictations`, the local transcription API, and `hex transcribe`
  remain local.
- The local transcription runs in parallel and remains the fallback.

See [`GROQ.md`](GROQ.md) for setup and privacy details. Native macOS Groq,
shortcut, paste, permissions, and History behavior have not been verified in
this fork.

## Identity caution

The source bundle keeps the upstream bundle identifier `anomaly.hex` and the
shared `~/Library/Application Support/hex` data root. Do not run it beside an
official Hex install with the same identity. Test with a separate macOS user or
remove the official app first if you need an isolated smoke test.

## Verification

The shared Rust tests and UI goldens run on Linux. A native macOS build still
needs a real Mac to verify permissions, global shortcuts, microphone capture,
paste, and the Groq request path.
