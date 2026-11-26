# Contributing to this fork

This is the unofficial [j0ltdevv/hex](https://github.com/j0ltdevv/hex) fork.
Issues, diagnoses, documentation fixes, and proposed code changes are welcome
at <https://github.com/j0ltdevv/hex/issues>. They go to this fork's maintainers,
not to upstream Anomaly.

## Before you start

- Read [`README.md`](README.md) for the source-only policy and known limits.
- Read [`docs/FORK.md`](docs/FORK.md) for the fork invariants.
- For Groq work, read [`docs/GROQ.md`](docs/GROQ.md) before changing provider,
  key, cancellation, fallback, UI, or CLI behavior.
- For upstream merges, follow [`docs/UPSTREAM-SYNC.md`](docs/UPSTREAM-SYNC.md).

## Development setup

Use the platform prerequisites in the README and platform guides:

- Linux: [`docs/linux.md`](docs/linux.md#install-from-source)
- Windows: [`docs/windows.md`](docs/windows.md#requirements-for-source-builds)
- macOS: [`AGENTS.md`](AGENTS.md) and `scripts/build-app.sh`

Build from the repository root:

```sh
cargo build --locked --bin hex
cargo run --locked --bin hex -- --version
```

`.cargo/config.toml` forces `HEX_LOCAL_BUILD=1` in this fork. Do not remove it to
make a local build check for upstream updates.

## Branch and pull-request workflow

1. Branch from `dev`.
2. Keep the change focused and preserve existing patterns.
3. Add or update tests only when behavior changes or an edge is uncertain.
4. Update the affected feature map and user documentation in the same change.
5. Open a pull request into `dev`; do not rewrite published `dev` history.
6. Merge once the relevant checks pass, then update the installed copy if you
   are validating on a real machine.

No force-push. No direct commits to `production`.

## Tests

Formatting and shape:

```sh
cargo fmt --all --check
git diff --check
python3 scripts/code-shape.py --check
python3 scripts/check-nix-git-pins.py
```

Focused provider and integration checks:

```sh
scripts/check groq::tests control::settings transcription::engine::tests linux::dictation desktop::i18n
```

UI and goldens:

```sh
scripts/check rune_ui
```

The live Groq smoke is opt-in and requires a real key plus a private 16 kHz mono
WAV. Never add a real key or private fixture to the repository or CI.

## Documentation changes

- Keep README claims aligned with observed verification.
- Label upstream installers, release feeds, and signed-update docs as
  upstream-only.
- Preserve the stable behavior ID `dictate.groq-cloud-fallback` in both feature
  maps.
- Record user-visible changes in [`CHANGELOG.md`](CHANGELOG.md).

## Secrets and personal data

Include **redacted** logs only. Remove API keys, tokens, transcripts,
vocabulary/screen-term text, local paths, and account names. Never ask a reporter
to paste a key or an unredacted log. Never commit a real key or fixture.

If the issue concerns Groq, state whether it was opt-in and whether the Local
fallback was used.

## Upstream sync

Sync at least weekly and before any future fork release. Carry `src/groq*`
additively, reapply engine/worker hunks by behavior, and preserve Local default,
fallback, cancellation, ordering, History schema, CLI/i18n parity, and secret
boundaries. Record the upstream SHA and exact verification in the pull request.

## Out of scope

This fork does not publish signed binaries or operate an update feed. Upstream
release workflows, Anomaly signing keys, Homebrew/AUR packages, and the official
download site are not fork contribution targets.
