# Groq speech provider in this fork

Hex stays local by default. This fork adds one optional speech provider:
Groq. The choice is explicit, so a saved key never changes the base path by
itself. Local is the upstream path; Groq is a cloud alternative for dictation
only, with Local still available as the fallback.

## Provider model

`SpeechProvider` lives in `src/groq.rs` and has two variants: `Local` and
`Groq`. It serializes as snake_case (`"local"`, `"groq"`), defaults to
`Local`, and is saved in `AppSettings.speech_provider`. Old settings files
without that field load as `Local`, and no migration is needed.

Settings → **General → Advanced → Dictation → Dictation provider** offers
**Local** (the default) and **Groq**. `hex settings get/set speech_provider`
uses the same two values; the key itself stays read-only and redacted.

- **Local** — Hex uses its on-device model exactly as in the base fork. No
  Groq request is made, even when a key happens to be saved in settings or
  exported in the environment. Local is the explicit local-only mode.
- **Groq** — Dictation sends the complete clip to Groq only when a usable key
  is available. Without a key, or after an error, empty result, or
  cancellation, the same clip goes through the unchanged Local path. This is
  one attempt at Groq followed by one Local fallback, not a retry storm.
- The selected model is `groq_model`: `whisper-large-v3` (the default, shown as
  **Whisper Large V3**) or `whisper-large-v3-turbo` (**Whisper Large V3
  Turbo**). The model choice is used only when Dictation provider is Groq and
  a key is set.

Meetings, the local transcription API, and `hex transcribe` remain local. A
successful Groq transcript joins the ordinary processing, paste, History, and
ordered-output paths. History keeps its existing `Source` labels and schema
unchanged; provider attribution lives only in the bounded in-memory usage
ring. When the local live stream produced text and timings, those timings are
preserved as far as the existing transcript structure allows. Groq text also
goes through the existing closing-mark check.

## Key sources, precedence, and isolation

Resolution is performed when the provider is Groq; the order is:

1. A trimmed, nonempty `GROQ_API_KEY` in the process environment.
2. A trimmed, nonempty key in `<Hex data root>/groq.key`.
3. The legacy saved `groq_api_key` setting.
4. No key: the Groq selection falls back to Local.

Whitespace-only values are skipped, so a blank environment value does not
erase a saved key or turn on a request. The environment value is not written
back to settings.

`groq.key` is owner-only: `write_key_file` creates it as Unix `0600` in a
`0700` directory, and `read_key_file` rejects group/world-readable files and
files not owned by the current user on Unix. It contains just the key text;
do not put quotes, comments, or other settings in it. The legacy setting
remains supported for older installs and remains redacted on CLI reads.

`key_status()` reports only the source category (`None`, `Environment`,
`KeyFile`, `Settings`, or `Blocked`) for UI display; it never returns key
bytes. `GroqApiKey` has secret-safe `Debug`, and neither the key nor the key
file contents appear in logs, diagnostics, multipart fields, or tests. The
`hex settings` list/get output redacts `groq_api_key`, and CLI writes to it are
rejected.

**Isolation:** with `HEX_APPLICATION_SUPPORT_DIR` pointing at an overridden
data root and `HEX_GROQ_ALLOW_SYSTEM_KEYS=0`, system-level credentials are
ignored: `GROQ_API_KEY` and any external/system key source are skipped. The
`groq.key` inside that overridden root and the legacy setting still work;
`key_status()` reports `Blocked` only when a suppressed environment key exists.
Without the data-root override, or with any other value of
`HEX_GROQ_ALLOW_SYSTEM_KEYS`, normal precedence applies. This keeps a
sandboxed or redirected instance from silently borrowing a developer's
system key.

Never put a real key in documentation, tests, fixtures, screenshots, crash
reports, or version control. Tests use constructed values and loopback
request fixtures only.

## What leaves the machine

With provider Groq and a usable key, one dictation clip is sent to Groq.
Samples are encoded as mono 16 kHz audio. The multipart body carries:

- `model` (`whisper-large-v3` or `whisper-large-v3-turbo`);
- `response_format=json` and `temperature=0`;
- `language` when the dictation's language selection is known;
- `prompt` when the existing vocabulary/screen-term prompt is available;
- `file` — first a FLAC upload (`dictation.flac`, `audio/flac`).

The key appears only in the bearer `Authorization` header, never in a
multipart field. A pure-Rust encoder avoids a new audio dependency. If Groq
answers HTTP **400**, **415**, or **422**, Hex retries once with the same
samples encoded as WAV (`dictation.wav`, `audio/wav`); other failures are not
retried. The request uses Hex's existing HTTP client with no redirects, a
24 MiB preflight cap, a 1 MiB response cap, and a 60-second deadline. An
empty or malformed transcript is treated as a failure and uses the Local
fallback.

The HTTP work runs off the engine worker (`groq::spawn` returns a one-shot
receiver). The dictation's ordered output worker polls that receiver beside
the local transcript, checks cancellation before and during the bounded
request, and settles jobs in submission order. Cancelling a job keeps the
local result and does not produce a cloud paste. This changes the transport,
not the shortcut state machine or the ordered-output path.

The privacy boundary is the provider choice plus the key: audio leaves the
machine only for a Groq dictation with a usable key. Local, missing key,
cancelled, and failed paths do not send dictation audio to Groq.

## Usage and cost records

`src/groq/usage.rs` keeps a bounded, in-memory ring of the most recent 64
records (`usage::record`, `usage::snapshot`, newest first). Each
`UsageRecord` has the timestamp, provider, model, outcome (`Succeeded`,
`WavRetry`, or `Failed`), sample count, and duration in milliseconds. It
contains no key, prompt, language, audio, multipart bytes, or transcript, and
it is not persisted to disk. Debug output is safe for logs.

Hex does **not** compute or store a monetary cost, token count, or invoice
amount in this ring. Duration and sample count are diagnostics that can help
estimate spend against Groq's published pricing; no claim about actual billed
cost follows from a local record. `usage::snapshot()` is not a billing
source.

## Implementation map

| File | Change |
| --- | --- |
| `src/groq.rs` | `SpeechProvider`, `provider_from`, `GroqModel`, redacted `GroqApiKey`, `GroqRequest`/`OwnedGroqRequest`, `Cancellation`, multipart/response handling, FLAC→WAV retry, off-worker `spawn`. |
| `src/groq/flac.rs` | Pure-Rust mono 16 kHz FLAC encoder. |
| `src/groq/keys.rs` | Key-file path/IO (`groq.key`), precedence, isolation rule, status-only helper. |
| `src/groq/usage.rs` | Bounded, non-persistent, secret-free usage ring. |
| `src/groq/tests.rs` | Deterministic provider/key/multipart/retry/cancellation/usage checks and one ignored live smoke. |
| `src/app_settings.rs` | Backward-compatible `speech_provider` field; existing `groq_api_key` and `groq_model`. |
| `src/app_settings/persistence.rs` | Owner-only writes for settings, including the legacy key setting. |
| `src/control/settings/registry.rs` | `speech_provider` as a `local`/`groq` choice; read-only/redacted API-key setting. |
| `src/control/settings.rs` | Choice apply/get for the provider and redaction/CLI-write rejection for the key. |
| `src/control/settings/choice_tests.rs` | Drift guard tying the CLI choice to `SpeechProvider::ALL`. |
| `src/rune_ui/general/groq.rs` | Provider picker, masked key field, model picker, and local/Groq descriptions. |
| `src/desktop/i18n/strings.rs`, `french.rs`, `hungarian.rs`, `portuguese.rs`, `russian.rs` | Translations for every new provider string; Traditional Chinese derives from the Simplified column in `traditional_chinese.rs`. |
| `src/transcription/engine.rs` | Restored local paths, closing-mark metadata, and a cloneable job cancellation for cloud work. |
| `src/linux/dictation.rs`, `src/windows/dictation.rs`, `src/windows/dictation/finish.rs`, `src/windows/dictation/output.rs` | Linux/Windows provider gate, off-worker request start, and ordered Local-fallback settlement. |
| `src/parakeet.rs` | macOS dictation worker uses the same shared `PendingGroq` helper. |
| `docs/GROQ.md` | This behavior, evidence limits, and sync guidance. |
| `docs/features/dictation.md`, `docs/features/README.md` | Stable `dictate.groq-cloud-fallback` behavior ID and product-map entry. |

No new Cargo dependency is required: the fork reuses Hex's HTTP client and
platform TLS support.

## Verification boundary

The behavior ID is **`dictate.groq-cloud-fallback`** in both feature maps.
Offline `groq::tests` use constructed keys and loopback HTTP fixtures; they
cover the Local default, a saved key not enabling Groq by itself, Groq
selection without a key, key precedence/isolation, owner-only key-file
permissions, multipart `language`/`prompt`/`model` fields, FLAC then WAV on
400/415/422, redacted errors and usage records, and cancellation flags.
`control::settings::choice_tests` keeps the CLI provider choice aligned with
the enum's serde names. The i18n tables have a completeness check for every
interface language. These are deterministic checks, not live-cloud proof.

`src/groq/tests.rs` also contains one **ignored** live smoke. It is opt-in:
set `HEX_GROQ_LIVE_FIXTURE` to a 16 kHz mono WAV and `GROQ_API_KEY` to a real
key, then run the named ignored test. No real key or fixture is needed for the
default suite, and neither value is printed. A previously observed live
request showed that a real Groq transcription can return nonempty text; that
does **not** prove provider selection in a shortcut-driven dictation, paste,
fallback latency, History entries on a live machine, audio quality, or cost. The
UI row changes the `mac-settings-advanced` golden; no native end-to-end run or
native-speaker translation review is implied by the offline checks.

Focused checks for this fork (recorded as locators, not as results of this
document edit):

```sh
cargo fmt --all --check
python3 scripts/code-shape.py --check
TRANSCRIBE_CMAKE_ARGS='-DTRANSCRIBE_VULKAN=OFF -DTRANSCRIBE_USE_SYSTEM_BLAS=OFF' cargo test --locked --bin hex groq::tests
TRANSCRIBE_CMAKE_ARGS='-DTRANSCRIBE_VULKAN=OFF -DTRANSCRIBE_USE_SYSTEM_BLAS=OFF' cargo test --locked --bin hex control::settings
TRANSCRIBE_CMAKE_ARGS='-DTRANSCRIBE_VULKAN=OFF -DTRANSCRIBE_USE_SYSTEM_BLAS=OFF' cargo check --locked --bin hex
VK_ICD_FILENAMES=/usr/share/vulkan/icd.d/lvp_icd.json \
  HEX_UPDATE_GOLDENS=1 cargo test --locked --bin hex rune_ui::tests::goldens
```

`TRANSCRIBE_CMAKE_ARGS='-DTRANSCRIBE_VULKAN=OFF -DTRANSCRIBE_USE_SYSTEM_BLAS=OFF'`
is only for hosts without Vulkan headers or OpenBLAS; release builds keep
both enabled. On such a host, `build.rs` still passes `-lopenblas` to the
final Linux binary, so set `LIBRARY_PATH` to a directory containing a
harmless `libopenblas.so` symlink when linking tests (no cblas symbols are
needed with GGML_BLAS off).

The opt-in smoke is separate from the default suite:

```sh
HEX_GROQ_LIVE_FIXTURE=/path/to/16k-mono.wav \
  cargo test --locked --bin hex live_groq_fixture_transcribes_nonempty_text -- --ignored
```

## Upstream sync and merge guidance

This is a fork-only cross-cutting change. Merge upstream by behavior, not by
taking either whole file:

1. **Start with additive surfaces.** Carry `src/groq.rs` and its submodules,
   the module registration, `speech_provider` with its `Local` default, the
   existing key/model settings, registry entries, redaction, and UI controls.
   Keep the provider field optional-on-read so old settings stay valid.
2. **Rebase engine and worker hunks manually.** `transcription/engine.rs`
   owns streaming, cancellation, deadlines, model leases, order, and
   fallback. Preserve those upstream paths; insert only a provider-gated
   cloud attempt at complete-clip dictation transcription, consume the
   off-worker receiver without blocking, fall back to Local once with the
   original samples, and leave meetings, local API, and CLI transcribe alone.
   Apply the same gate in `parakeet.rs` without changing accepted jobs,
   cancellation, processing, or ordered output.
3. **Preserve secret boundaries.** Keep the resolution order, trimmed
   nonempty checks, `groq.key` permissions and isolation rule, CLI
   redaction/read-only behavior, secret-safe `Debug`, and owner-only settings
   writes. Do not let a new settings path, error body, header dump, or usage
   record expose key bytes, prompts, language, audio, or transcripts.
4. **Check schema, CLI, and UI drift together.** Upstream may add
   `AppSettings` fields, settings kinds, Advanced Dictation rows, source
   enums, or History fields. Keep the provider serde names, CLI choices, and
   translations in lockstep, retain the default for old files, and keep every
   new `tr(...)` key present in every i18n table.
5. **Re-run focused checks after the merge, then verify live separately.**
   A green offline suite does not establish provider availability, real
   fallback latency, UI layout, translation quality, or billed cost. Record
   those observations separately before changing the feature map's
   live-proof status.

The highest-risk conflicts are the engine's local/streaming fallback and
off-worker transport, the macOS worker's routing, key-file permissions and
isolation, settings serialization/permissions, CLI registry validation,
the unchanged History schema, and the Advanced Dictation layout. Do
not resolve a conflict by removing the Local default or fallback, widening
Groq to non-dictation work, changing History's source labels, or making the
key writable or visible in generic settings output.
