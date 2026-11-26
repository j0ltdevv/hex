# Hex — j0ltdevv fork (unofficial)

<a href="https://opencode.ai"><picture><source media="(prefers-color-scheme: dark)" srcset="https://opencode.ai/brand/badges/opencode-built-with-g-dark-20.svg"><img alt="Built with OpenCode" src="https://opencode.ai/brand/badges/opencode-built-with-g-light-20.svg" height="20" /></picture></a>

This is an **unofficial fork** of [Hex](https://github.com/anomalyco/hex),
maintained at [j0ltdevv/hex](https://github.com/j0ltdevv/hex). It is not
affiliated with, endorsed by, or supported by Anomaly Innovations. The upstream
project and its trademarks belong to their owners; Hex is MIT licensed.

Hex is native voice dictation for macOS, Windows, and Linux. Hold a key, speak,
and it pastes. This fork keeps the upstream Local path and adds one **opt-in**
cloud speech provider: **Groq**. Local remains the default and the fallback.

## Project status

| Area | Status in this fork |
| --- | --- |
| Linux x86_64 source build | Built and tested |
| Linux Groq dictation | Observed end to end: shortcut → Groq HTTP 200 → paste → History |
| macOS | Upstream target; native fork behavior not verified here |
| Windows x64 / ARM64 | Upstream targets; native fork behavior not verified here |
| Meetings | macOS live workflow; not part of the Groq change |
| Local API and TypeScript SDK | Local only; Groq is never used |
| Binary distribution | Source-only; no signed fork binaries or update feed |
| Upstream sync | Based on `anomalyco/hex` `86fd7f1`; see [`docs/UPSTREAM-SYNC.md`](docs/UPSTREAM-SYNC.md) |

## Screenshots

<img src="docs/images/output.png" width="720" alt="Hex Output pane with ordered processing steps">

<img src="tests/golden/rune-mac-settings-advanced-light.png" width="720" alt="Hex General settings with Local and Groq dictation provider">

## Privacy at a glance

| Mode | What happens to dictation audio |
| --- | --- |
| **Local** (default) | Transcribed on your device. No Groq request, even if a key is saved. |
| **Groq** with a usable key | The complete clip is sent to Groq once, on an off-worker request that runs beside the local transcription. The key is resolved when the capture is submitted; later key changes do not affect it. |
| **Groq** without a usable key, after an error, empty result, or cancellation | The same clip is transcribed locally. Cancellation stops the cloud result from being pasted. |
| **Meetings, the local API `/dictations`, and `hex transcribe`** | Local only. Groq is limited to dictation. |
| **Usage records** | A bounded in-memory ring records provider, model, outcome, samples, and duration. No key, audio, prompt, language, transcript, or monetary cost; the provider used is logged. |
| **Updates** | Source builds from this fork are marked local and never check for, download, or install an update, including an upstream Anomaly release. |

Groq is a choice, not a side effect: installing or configuring the fork does not
send audio anywhere. See [`docs/GROQ.md`](docs/GROQ.md) for key precedence,
isolation, retry behavior, and evidence limits.

## What it does

- Hold the shortcut to dictate, or choose Tap/Toggle mode. On Hyprland you can
  bind `hex dictation start` / `stop` as Lua hold-to-talk keys instead of relying
  on the GlobalShortcuts portal ([Linux notes](docs/linux.md#shortcuts)).
- Text lands at the cursor in the focused app. Paste Last uses Option-Shift-V on
  macOS and Ctrl-Alt-V on Windows/Linux.
- History keeps dictations searchable for seven days by default.
- Output runs dictations through word replacements, built-in cleanups,
  OpenCode rewrite, and ordered TypeScript transformations.
- Vocabulary supplies names and jargon to supported speech models.
- Annotate records the screen while you draw and talk on supported desktop
  platforms.
- The authenticated local API and [TypeScript SDK](sdk/typescript/README.md)
  transcribe with the local model. The SDK currently requires an explicit
  fork-built native helper.
- Meetings follows the upstream platform support and is not widened by Groq.

Speech models are cached on the device after first setup. Settings offers
Parakeet, Cohere Transcribe, Whisper large-v3-turbo, Qwen3-ASR, SenseVoice,
Nemotron, Granite, one-language models, and GGUF models of your own
([your own models](docs/features/own-models.md)). Groq's two model choices apply
only to the opt-in Groq provider.

## Quickstart: opt-in Groq

### 1. Build or install from source

```sh
git clone https://github.com/j0ltdevv/hex.git
cd hex
cargo build --release --bin hex
./target/release/hex --version
```

On Linux, `./scripts/install-linux.sh` builds and installs the same checkout into
the user-local managed layout. See the platform table below for prerequisites.

### 2. Start Hex

```sh
./target/release/hex model install
./target/release/hex start
./target/release/hex status
```

If you installed Hex on Linux, use `hex` instead of `./target/release/hex`.

### 3. Choose Groq explicitly

Local is already active. To opt in:

```sh
hex settings set speech_provider groq
hex settings set groq_model whisper-large-v3
hex settings get speech_provider
hex settings get groq_model
```

The CLI talks to the running Hex, so run `hex start` first.

### 4. Provide a key without committing it

Use either:

- the `GROQ_API_KEY` environment variable; or
- an owner-only `<Hex data root>/groq.key` file; or
- the masked Groq API key field in Settings.

Resolution order is environment → `groq.key` → legacy saved setting. The CLI
redacts `groq_api_key` and refuses to write it. Never paste a real key into an
issue, test, screenshot, or commit.

To return to the unchanged local path:

```sh
hex settings set speech_provider local
```

See [`docs/GROQ.md`](docs/GROQ.md) for the full provider contract.

## Build prerequisites

| Platform | Requirements | Supported source path |
| --- | --- | --- |
| **Linux x86_64 / ARM64** | Rust stable, C/C++ toolchain, CMake, ALSA/PipeWire compatibility, OpenBLAS, X11/Wayland development libraries, and a real Vulkan driver (not only the loader). Arch package list: [`docs/linux.md`](docs/linux.md#install-from-source). | `./scripts/install-linux.sh` or `cargo build --release --bin hex` |
| **Windows x64** | Windows 10/11, Visual Studio C++ Build Tools, CMake, stable MSVC Rust, Vulkan SDK. | `powershell -ExecutionPolicy Bypass -File .\scripts\setup-windows.ps1`, then `cargo build --release --bin hex` |
| **Windows ARM64** | Visual Studio clang-cl and Ninja; follow the ARM64 flags in [`docs/windows.md`](docs/windows.md#requirements-for-source-builds). | Same setup, then the documented ARM64 build command |
| **macOS** | macOS 15+, Xcode 26 / command-line tools, stable Rust, and the native dependencies listed in [`docs/macos.md`](docs/macos.md). | `scripts/build-app.sh`; use `HEX_RELEASE_UNSIGNED=1` for an unsigned local bundle. Upstream Developer ID/notarization tooling is not a fork release path. |

`.cargo/config.toml` sets `HEX_LOCAL_BUILD=1` for every build in this fork. The
app reports itself as a local source build and never installs an upstream update.
When portability matters, build with `-DGGML_NATIVE=OFF`; the Windows guide and
Linux release script show the supported flags.

## First run and useful commands

```sh
hex model install
hex start
hex status
hex window show
hex settings list
hex logs --process -f
hex dictation toggle
hex transcribe meeting.m4a
```

- `hex model install` downloads and verifies the selected local speech model.
- `hex start` is the only command that launches Hex; other controls talk to the
  running instance.
- `hex logs --process -f` follows Rust/native diagnostics in `process.log`.
- `hex transcribe` is always local, even when the dictation provider is Groq.
- Full CLI contract: [`docs/cli.md`](docs/cli.md).

## Architecture

Hex is one native Rust process per user and data root:

- **Capture and platform hosts** own the microphone, shortcuts, paste, HUD, and
  OS integration (`src/linux`, `src/windows`, `src/parakeet.rs`).
- **`src/transcription`** owns the local model, streaming, deadlines, ordering,
  and fallback.
- **`src/groq*`** is the fork-only additive provider: key resolution, multipart,
  FLAC/WAV encoding, cancellation, retry, and secret-safe usage records.
- **`src/control`** exposes the versioned control CLI and settings registry.
- **`src/rune_ui`** is the shared Rune window on macOS, Windows, and Linux.
- **`src/local_api`** serves authenticated loopback transcription and dictation
  control; it stays local in this fork.

The implementation authority is [`AGENTS.md`](AGENTS.md). Fork-specific deltas
and conflict hotspots are documented in [`docs/GROQ.md`](docs/GROQ.md) and
[`docs/UPSTREAM-SYNC.md`](docs/UPSTREAM-SYNC.md).

## Tests

Focused provider and integration checks:

```sh
cargo fmt --all --check
python3 scripts/code-shape.py --check
scripts/check groq::tests control::settings transcription::engine::tests linux::dictation desktop::i18n
```

UI and golden checks:

```sh
scripts/check rune_ui
VK_ICD_FILENAMES=/usr/share/vulkan/icd.d/lvp_icd.json \
  HEX_UPDATE_GOLDENS=1 cargo test --locked --bin hex rune_ui::tests::panes::platform_goldens
```

The live Groq smoke is opt-in and requires a real key plus a 16 kHz mono WAV. It
is not part of the default suite; see [`docs/GROQ.md`](docs/GROQ.md#verification-boundary).

## Known limits

- This fork publishes **no binaries, installers, signed releases, or update feed**.
- Native macOS and Windows behavior is not verified in this fork.
- Groq is only for complete-clip dictation. Meetings, `/dictations`, the local
  transcription API, and `hex transcribe` remain local.
- The local model still runs in parallel. Groq can improve the final transcript,
  but it is not a CPU/RAM offload or a model-free installation.
- The usage ring is diagnostic, not billing or invoice data.
- Queue-full, cancellation timing, fallback latency, GPU execution, and billed
  cost are not established by the offline suite.
- Translations have not had a native-speaker review.
- The TypeScript SDK still needs an explicitly supplied native helper.

## Releases and upstream sync

This repository is source-only. Builds are forced into local mode so they cannot
silently replace themselves with an Anomaly binary. There is no fork release
channel yet.

The fork tracks `anomalyco/hex` and re-applies a small behavioral delta. The
current upstream baseline is `86fd7f1`. The intended cadence is weekly and before
any future fork release. See:

- [`CHANGELOG.md`](CHANGELOG.md) for fork-specific changes;
- [`docs/FORK.md`](docs/FORK.md) for identity, scope, and support policy;
- [`docs/UPSTREAM-SYNC.md`](docs/UPSTREAM-SYNC.md) for the repeatable sync runbook.

## Contributing and support

Issues and diagnoses are welcome in this fork:
[open an issue](https://github.com/j0ltdevv/hex/issues). Never include a real key,
private transcript, or unredacted log. See [`SECURITY.md`](SECURITY.md) and
[`CONTRIBUTING.md`](CONTRIBUTING.md).

## License and attribution

Upstream Hex is copyright its authors and MIT licensed; this fork carries the
same license and preserves the upstream notice. Groq is an external service and
a trademark of Groq LLC and/or its affiliates. This fork is not affiliated with
or endorsed by Groq. See [`LICENSE`](LICENSE) and
[`THIRD_PARTY_NOTICES.md`](THIRD_PARTY_NOTICES.md).
