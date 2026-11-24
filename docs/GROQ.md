# Groq dictation in this fork

This fork adds optional Groq speech-to-text as the first transcription path for
dictation. It is a cloud alternative, not a replacement for Hex's local speech
models. With no usable key, Hex behaves as upstream. With a key, a failed Groq
request still produces a transcript through the existing local model whenever
that model is available.

## Configure it

Settings → **General → Advanced → Dictation** contains:

- **Groq API key**: saved as the optional `groq_api_key` setting.
- **Groq model**: saved as `groq_model`, with the user-facing choices
  **Whisper Large V3** and **Whisper Large V3 Turbo**.

`whisper-large-v3` is the default. The other accepted Groq model ID is
`whisper-large-v3-turbo`; the same choice can be set with
`hex settings set groq_model whisper-large-v3-turbo`.

For one launch, set the `GROQ_API_KEY` environment variable. Key selection is:

1. a trimmed, nonempty `GROQ_API_KEY`;
2. otherwise, the nonempty saved `groq_api_key`;
3. otherwise, local transcription only.

An empty or whitespace-only environment value does not erase a saved key or
turn on Groq. The environment override is not written back to settings.

The API-key setting is read-only through `hex settings`: list/get output redacts
it, so reading or enumerating settings cannot disclose it. Put a key in the
window or the environment rather than committing it to a dotfiles repository.

## What leaves the machine

When a usable key exists, Hex tries Groq first for a dictation's complete audio
clip. Audio is encoded as mono 16 kHz PCM WAV. The multipart request contains the
selected model and WAV; the key belongs only in the bearer authorization header,
never in a multipart field or form value.

A successful Groq transcript enters the same processing, paste, History, and
ordered-output path as a local transcript. Groq changes the route, not source
labels or the History schema: existing local model metadata remains.

Groq is not a global backend switch:

- no nonempty key means no Groq request;
- any Groq failure falls back to the existing local transcription path for the
  same complete clip, preserving local audio preparation and model behavior;
- meetings, the local transcription API, and `hex transcribe` remain local;
- choosing a local Speech model does not opt out of a configured Groq key: that
  model is the fallback used if Groq fails.

Consequently, configuring Groq changes the privacy boundary for dictation audio:
that audio is sent to Groq. Without a usable key, no dictation audio is sent by
this feature.

## Secret handling

The key is stored in Hex's ordinary settings file, written with owner-only
permissions. On Unix the replacement/staging file is created as `0600` before
bytes are written; on Windows it uses the profile's owner ACL. Settings list/get
redact `groq_api_key`. `AppSettings` and its snapshots hold the key in a
`GroqApiKey` wrapper whose `Debug` prints `[redacted]`, so derived debug output
cannot disclose it. The key is excluded from multipart fields and logs.
HTTP/request errors reported to the user or logs do not contain it.

Do not paste a real key into documentation, tests, fixtures, screenshots, crash
reports, or version control. Tests use constructed values and local request
fixtures only.

## Implementation map

The integration deliberately keeps Groq behind one small client module and
routes only dictation through it:

| File | Change |
| --- | --- |
| `src/groq.rs` | Key resolution, model IDs, WAV encoding, multipart request, bearer authentication, response parsing, and sanitized errors. |
| `src/groq/tests.rs` | Offline encoding, precedence, model, request, response, and redaction checks. |
| `src/transcription/engine.rs` | Cloud-first dictation transcription and fallback into the existing local engine. |
| `src/parakeet.rs` | macOS dictation-worker routing through Groq with the existing local fallback. |
| `src/main.rs` | Registers the Groq module. |
| `src/app_settings.rs` | Adds optional `groq_api_key` and `groq_model`, with Large V3 as the default. |
| `src/app_settings/persistence.rs` | Writes settings through the owner-only durable writer. |
| `src/control/settings/registry.rs` | Registers `groq_model` as a choice and `groq_api_key` as a read-only secret. |
| `src/control/settings.rs` | Redacts the API key in list/get output and rejects CLI writes. |
| `src/control/settings/choice_tests.rs` | Keeps the model choice aligned with `GroqModel` serde names. |
| `src/rune_ui.rs` | Holds and saves the secure key input alongside the model setting. |
| `src/rune_ui/general.rs` | Places the Groq rows in Advanced → Dictation. |
| `src/rune_ui/general/groq.rs` | Secure key field, model picker, labels, and local-fallback description. |
| `src/desktop/i18n/strings.rs`, `french.rs`, `hungarian.rs`, `portuguese.rs`, `russian.rs` | Adds translations for the new controls and description. |
| `docs/GROQ.md` | This fork's behavior, setup, evidence limits, and sync guidance. |
| `docs/features/dictation.md`, `docs/features/README.md` | Stable `dictate.groq-cloud-fallback` behavior map and product-map entry. |

No new network dependency is required: Groq uses Hex's existing HTTP client and
platform TLS support.

## Verification boundary

The behavior ID is **`dictate.groq-cloud-fallback`**. Offline checks in
`src/groq/tests.rs` establish request construction, precedence/defaults,
redaction, and fallback selection. The routing itself lives in the transcription
engine and dictation worker. The ignored live smoke was executed once with a
generated 16 kHz speech fixture and `GROQ_API_KEY`; it proved that a real Groq
transcription request returns nonempty text. It does **not** prove a full
shortcut-driven dictation, insert, or fallback-after-failure on a live machine.

`src/groq/tests.rs` also contains an ignored live-request smoke test requiring a
16 kHz mono WAV fixture and `GROQ_API_KEY`; it is not live-cloud proof unless run
and observed. The `mac-settings-advanced` golden changes with the two new rows.

Verification commands for this fork:

```sh
cargo fmt --all --check
python3 scripts/code-shape.py --check
scripts/check labs control::settings
TRANSCRIBE_CMAKE_ARGS='-DTRANSCRIBE_VULKAN=OFF -DTRANSCRIBE_USE_SYSTEM_BLAS=OFF' cargo check --locked --bin hex
TRANSCRIBE_CMAKE_ARGS='-DTRANSCRIBE_VULKAN=OFF -DTRANSCRIBE_USE_SYSTEM_BLAS=OFF' cargo test --locked --bin hex groq::tests
TRANSCRIBE_CMAKE_ARGS='-DTRANSCRIBE_VULKAN=OFF -DTRANSCRIBE_USE_SYSTEM_BLAS=OFF' cargo test --locked --bin hex control::settings
VK_ICD_FILENAMES=/usr/share/vulkan/icd.d/lvp_icd.json \
  HEX_UPDATE_GOLDENS=1 cargo test --locked --bin hex rune_ui::tests::goldens
TRANSCRIBE_CMAKE_ARGS='-DTRANSCRIBE_VULKAN=OFF -DTRANSCRIBE_USE_SYSTEM_BLAS=OFF' cargo build --locked --bin hex
```

`TRANSCRIBE_CMAKE_ARGS='-DTRANSCRIBE_VULKAN=OFF -DTRANSCRIBE_USE_SYSTEM_BLAS=OFF'` is
only for hosts without the Vulkan headers or OpenBLAS; release builds keep both enabled.
On such a host, `build.rs` still passes `-lopenblas` to the final Linux binary, so
set `LIBRARY_PATH` to a directory containing a harmless `libopenblas.so` symlink
when linking tests (no cblas symbols are needed with GGML_BLAS off).

```sh
HEX_GROQ_LIVE_FIXTURE=/path/to/16k-mono.wav \
  cargo test --locked --bin hex live_groq_fixture_transcribes_nonempty_text -- --ignored
```

## Upstream sync and merge guidance

This is a fork-only cross-cutting change. Merge upstream by behavior, not by
taking either whole file:

1. **Start with the small surfaces.** Carry `src/groq.rs`, its tests, module
   registration, the two settings fields/default, registry entries, redaction,
   and UI controls first. They are mostly additive. Keep `groq_api_key` optional
   so upstream settings remain readable and no migration is needed.
2. **Rebase the engine hunk manually.** `transcription/engine.rs` owns streaming,
   cancellation, deadlines, model leases, and fallback. Preserve upstream changes
   to those paths and insert the Groq attempt only at complete-clip dictation
   transcription. A failed attempt must enter the same local path once, with the
   original samples; do not retry Groq, bypass queue ordering, or change meeting
   and API behavior.
3. **Rebase the dictation worker hunk manually.** In `parakeet.rs`, keep accepted
   jobs, cancellation, processing, and ordered output unchanged. Verify that raw
   dictation and ordinary dictation both use the intended transcription route,
   and that History/source labels and schema remain unchanged.
4. **Preserve secret boundaries.** Keep environment precedence, trimmed nonempty
   checks, read-only/redacted CLI output, secret-safe debug formatting, and
   owner-only settings writes. Never serialize the key into diagnostics or allow
   an error body/header dump to include it.
5. **Check schema and UI drift together.** Upstream may add AppSettings fields,
   settings kinds, advanced Dictation rows, Output facts, or source enums. Keep
   `groq_model`'s registry choices and serde names in lockstep, retain the default
   for old files, and ensure the read-only secret cannot be exposed by another
   settings listing path.
6. **Re-run the focused checks after the merge, then verify live separately.**
   A green offline suite does not establish provider availability, audio quality,
   fallback latency, or a real paste. Record those observations separately before
   changing the feature map's live-proof status.

The highest-risk conflicts are the engine's local/streaming fallback, the macOS
worker's routing, settings serialization/permissions, CLI registry validation,
and the Advanced Dictation layout. Do not resolve a conflict by removing the
local fallback, widening Groq to non-dictation work, changing History/source
labels, or making the key writable/visible through generic settings output.
