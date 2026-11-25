# Hex — j0ltdevv fork (unofficial)

This is an **unofficial fork** of [Hex](https://github.com/anomalyco/hex),
maintained at [j0ltdevv/hex](https://github.com/j0ltdevv/hex). It is not
affiliated with, endorsed by, or supported by Anomaly Innovations. The upstream
project and its trademarks belong to their owners; Hex is MIT licensed.

Hex is local voice dictation for macOS, Windows, and Linux. Hold a key, speak,
and it pastes. This fork adds one **opt-in** cloud speech provider, Groq;
Local stays the default and the fallback.

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

- Hold the shortcut to dictate, or set Shortcut mode to Tap to start and stop
  with a press. On Hyprland you can bind `hex dictation start` / `stop` as Lua
  hold-to-talk keys instead of relying on the GlobalShortcuts portal
  ([Linux notes](docs/linux.md)).
- Text lands at the cursor in the focused app; Option-Shift-V pastes the last
  dictation again (Ctrl-Alt-V on Windows and Linux).
- History keeps dictations, searchable, for 7 days by default.
- Output runs every dictation through steps you choose (word replacements,
  built-in cleanups, your own scripts, an optional OpenCode rewrite), with
  different steps per app or website.
- Vocabulary is one list of names Hex should know; Whisper and Qwen3-ASR read
  it while transcribing, Parakeet models read it with Labs › Vocabulary help
  on, and the OpenCode rewrite uses it.
- Annotate records the screen while you draw and talk, on macOS, Linux and
  Windows (GNOME Wayland needs Labs › Annotate on GNOME).
- An authenticated local API and the [TypeScript SDK](sdk/typescript/README.md)
  transcribe with the same local model, for your own tools.
- Meetings records a call and writes its transcript on supported desktop platforms.

Speech models are cached on the device after first setup. Settings' picker
offers Parakeet, Cohere Transcribe, Whisper large-v3-turbo, Qwen3-ASR,
SenseVoice, Nemotron, Granite, one-language models, and GGUF models of your own
([your own speech models](docs/features/own-models.md)). Groq's two model
choices apply only to the opt-in Groq provider.

## Build from source

This fork publishes no binaries, installers, or update channel of its own. The
source checkout is the supported way to run it; build it with the commands
below rather than substituting a binary from another project.

```sh
git clone https://github.com/j0ltdevv/hex.git
cd hex
cargo build --release --bin hex
./target/release/hex --version
```

See [`AGENTS.md`](AGENTS.md), [`docs/linux.md`](docs/linux.md), and
[`docs/windows.md`](docs/windows.md) for native dependencies and platform
notes. `.cargo/config.toml` sets `HEX_LOCAL_BUILD=1` for every build: the app
reports itself as a source build and never checks for or installs updates, so
it cannot silently switch to upstream artifacts.

`hex transcribe meeting.m4a` prints a recording's transcript; the same `hex`
controls the running app (`hex status`, `hex dictation`, `hex settings`). See
[`docs/cli.md`](docs/cli.md). On macOS the CLI is inside the app; link it from
the app bundle to a directory you own if you want it on your `PATH`. More docs
are in [`docs/`](docs/README.md).

## Verification limits

On Linux, a physical `Alt+Space` dictation was observed on 2026-11-25 sending
audio to Groq, receiving HTTP 200, pasting the returned text, and recording it
in History. The opt-in live smoke also returned nonempty text. These observations
prove that one Linux path works; they do not establish native macOS/Windows
behavior, queue-full recovery, cancellation timing, fallback latency, audio
quality, GPU execution, or billed cost.

Offline checks cover provider settings, key precedence and isolation,
multipart fields, retry status codes, cancellation, usage redaction, CLI drift,
and compilation. The stable behavior ID is `dictate.groq-cloud-fallback`; it
records implemented behavior, not universal live validation. See
[`docs/GROQ.md`](docs/GROQ.md) for the full evidence boundary.

## Contributing

Issues and diagnoses are welcome in this fork:
[open an issue](https://github.com/j0ltdevv/hex/issues). Never include a real
key, private transcript, or unredacted log; see
[`SECURITY.md`](SECURITY.md) and [`CONTRIBUTING.md`](CONTRIBUTING.md).

Upstream Hex is copyright its authors and MIT licensed; this fork carries the
same license. See [`docs/GROQ.md`](docs/GROQ.md) for upstream-sync guidance.
