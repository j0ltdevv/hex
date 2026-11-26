# About this fork

## Identity

`j0ltdevv/hex` is an unofficial fork of [`anomalyco/hex`](https://github.com/anomalyco/hex).
It is maintained independently and is not affiliated with, endorsed by, or
supported by Anomaly Innovations or Groq.

The fork keeps the upstream product, local models, UI, app identity, and data
layout. Its only product-level addition is an opt-in Groq dictation provider.

## What changes here

| Area | Fork behavior |
| --- | --- |
| Default speech provider | **Local**, unchanged from upstream |
| Cloud provider | **Groq**, explicit opt-in for complete-clip dictation |
| Groq models | `whisper-large-v3` (default) and `whisper-large-v3-turbo` |
| Key sources | `GROQ_API_KEY` → `<data root>/groq.key` → legacy redacted setting |
| Failure behavior | One Groq attempt beside the unchanged local run; Local remains the fallback |
| Local API, meetings, `hex transcribe` | Local only |
| History | Existing schema and source labels preserved; provider actually used is logged |
| Updates | Disabled for every source build by `.cargo/config.toml` |
| Distribution | Source-only; no signed binaries, installers, or update feed |

See [`GROQ.md`](GROQ.md) for the complete provider contract.

## Invariants

- Local remains the default and works without a key.
- A saved key alone never enables Groq.
- Groq audio leaves the machine only after an explicit provider choice and a
  usable key at submission time.
- Rejected or cancelled jobs never paste cloud text.
- A local transcription failure does not discard an already-successful Groq
  result.
- Meetings, `/dictations`, the local transcription API, and `hex transcribe`
  never start a Groq request.
- No key, transcript, prompt, or audio is written to the usage ring or logs.
- Source builds cannot update themselves to an upstream Anomaly artifact.

## Source-only policy

This fork does not publish binaries. The supported path is to build the checkout
or run `./scripts/install-linux.sh` on Linux. Every build is stamped as a local
build and has updates disabled.

The inherited release workflows, Anomaly release service, signing keys, Homebrew
tap, AUR package, and official download site belong to upstream. They are not
fork distribution channels. Do not reuse them to publish this fork.

A future binary release would require a separate product identity, signing key,
HTTPS manifest origin, updater verifier, and support policy. None of that is
created by this repository today.

## Data locations

| Platform | Application Support / data root |
| --- | --- |
| macOS | `~/Library/Application Support/hex` |
| Linux | `${XDG_DATA_HOME:-~/.local/share}/hex` |
| Windows | `%APPDATA%\hex` (logs and models may use `%LOCALAPPDATA%\hex`) |

`HEX_APPLICATION_SUPPORT_DIR` overrides the root. A Groq key file is named
`groq.key` inside that root and must be owner-only.

## Verification status

Observed on Linux:

- focused provider, settings, engine, Linux dictation, and i18n tests pass;
- a physical `Alt+Space` dictation reached Groq, received HTTP 200, pasted the
  returned text, and created a History entry;
- a portable release binary starts on the maintained Linux host.

Not established here:

- native macOS or Windows behavior;
- physical shortcut/paste behavior on the newest portable binary;
- queue-full, cancellation timing, fallback latency, GPU execution, or billed
  cost;
- native-speaker translation quality;
- a signed binary distribution or update channel.

See [`README.md`](../README.md#known-limits) and
[`GROQ.md`](GROQ.md#verification-boundary).

## Support

- Issues: <https://github.com/j0ltdevv/hex/issues>
- Private vulnerability reports: <https://github.com/j0ltdevv/hex/security/advisories/new>
- Security guidance: [`../SECURITY.md`](../SECURITY.md)
- Contribution workflow: [`../CONTRIBUTING.md`](../CONTRIBUTING.md)

Never include a real API key, private transcript, vocabulary/screen-term text,
or unredacted diagnostic log.

## Non-goals

- Replacing upstream Hex or claiming upstream endorsement.
- Adding a hosted Groq subscription or billing system.
- Sending meetings, local API traffic, or `hex transcribe` through Groq.
- Reusing Anomaly signing keys, package names, or update feeds.
- Generalizing to arbitrary OpenAI-compatible endpoints without a separate
  design and user demand.
