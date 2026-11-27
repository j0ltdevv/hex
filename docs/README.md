# Project documentation

This is the unofficial [`j0ltdevv/hex-groq`](https://github.com/j0ltdevv/hex-groq) fork.
It builds from source, keeps Hex local by default, and adds an opt-in Groq
dictation provider. Upstream plans and release-service documents describe the
base project, not a fork binary distribution.

## Start here

- [`../README.md`](../README.md): fork identity, privacy, Groq quickstart,
  platform prerequisites, tests, and known limits.
- [`FORK.md`](FORK.md): what changes, what stays upstream, source-only policy,
  data locations, verification status, and support.
- [`GROQ.md`](GROQ.md): provider behavior, key precedence, request bounds,
  cancellation/fallback, implementation map, and evidence limits.
- [`UPSTREAM-SYNC.md`](UPSTREAM-SYNC.md): repeatable merge workflow, conflict
  hotspots, checks, and sync recording.
- [`../CHANGELOG.md`](../CHANGELOG.md): fork-specific changes and verification.

## User guides

- [`linux.md`](linux.md): Linux X11 and native Wayland requirements, source
  installation, shortcuts, and recovery. Published Anomaly builds are labelled
  upstream-only.
- [`macos.md`](macos.md): macOS source prerequisites, unsigned local app build,
  CLI linking, identity caution, and verification limits.
- [`windows.md`](windows.md): Windows x64 and ARM64 source builds, runtime
  behavior, and known issues. Official Hex installers are upstream-only.
- [`nix.md`](nix.md): Nix packaging for Linux, development, and per-user autostart.
- [`cli.md`](cli.md): the `hex` control CLI, settings, dictation commands, logs,
  and wire protocol. Fork source builds cannot check or install updates.
- [`../ios/README.md`](../ios/README.md): build and test the iOS prototype.
- [`../android/README.md`](../android/README.md): build and test Hex Mobile on Android.
- [`../sdk/typescript/README.md`](../sdk/typescript/README.md): use the public
  TypeScript transcription SDK with an explicitly supplied native helper. The
  helper is local-only; Groq is never used by the SDK or local API.
- [`../site/public/docs/transforms.md`](../site/public/docs/transforms.md):
  TypeScript transformation API (`~/.config/hex/hex.config.ts`).
- [`../site/public/docs/admin.md`](../site/public/docs/admin.md): macOS
  managed-preference feature locks (`anomaly.hex`).

## Engineering reference

- [`../AGENTS.md`](../AGENTS.md): upstream architecture, invariants,
  development commands, and diagnostics. This remains the implementation
  authority; fork deltas are documented in `GROQ.md` and `UPSTREAM-SYNC.md`.
- [`../ROADMAP.md`](../ROADMAP.md): upstream product and engineering work.
- [`design-system.md`](design-system.md): Rune + `rune-kit` + OpenCode theme
  components, Hex pieces, copy rules, and `--snap` scene verification.
- [`features/README.md`](features/README.md): user-facing capabilities,
  entry points, available verification, and gaps.
- [`perf.md`](perf.md): strobe benchmarks and isolated benchmark copies.
- [`opencode-compatibility.md`](opencode-compatibility.md): audited OpenCode V2
  source and CLI baseline, integration contract, and validation limits.
- [`specs/local-transcription-service.md`](specs/local-transcription-service.md):
  local-service protocol and lifecycle guarantees.
- [`specs/hex-words.md`](specs/hex-words.md): the `.hexwords` vocabulary and
  replacement format shared across desktop and mobile.
- [`transcript-cli.md`](transcript-cli.md): read-only `hex history` and
  `hex meetings` JSON contracts.
- [`streams.md`](streams.md): live `/streams` pushed-audio transcription contract.
- [`hex-annotation.md`](hex-annotation.md): `.hexannotation` v1 package format,
  legacy `.scrawl` discovery, and router boundaries.
- [`test-builds.md`](test-builds.md): unsigned per-push `Hex Test` artifacts and
  isolated `hex-test` data root.

## Upstream and historical material

These documents describe Anomaly's release infrastructure or earlier upstream
plans. They are useful engineering context, not installation paths for this
source-only fork:

- [`downloads.md`](downloads.md): Anomaly release-service and download hosting.
- [`releasing.md`](releasing.md): upstream release workflow and credentials.
- [`windows-signing.md`](windows-signing.md): upstream Authenticode signing.
- [`plans/release-service.md`](plans/release-service.md): Anomaly release service
  target names, `install.json` receipt, and updater client rules.
- [`plans/`](plans/): upstream architecture decisions and active plans.
- [`research/`](research/): historical research and measurements.
- [`../prototypes/app-shell/README.md`](../prototypes/app-shell/README.md):
  archived app-shell comparison prototype.

When a plan disagrees with `AGENTS.md` about shipped behavior, update the plan.
When upstream documentation conflicts with this fork's source-only or Groq
contract, the fork documents above take precedence for this repository.

## Support

- Issues: <https://github.com/j0ltdevv/hex-groq/issues>
- Private vulnerability reports:
  <https://github.com/j0ltdevv/hex-groq/security/advisories/new>
- Security guidance: [`../SECURITY.md`](../SECURITY.md)
- Contribution workflow and checks: [`../CONTRIBUTING.md`](../CONTRIBUTING.md)

Never include a real API key, private transcript, vocabulary/screen-term text,
or unredacted diagnostic log.

## Releases

This fork has no published binary release. [`releases/`](releases/) holds
upstream versioned release notes (`docs/releases/<X.Y.Z>.md`, now 3.5.x) that
generate upstream `/hex/changelog` and gate its stable releases. Preserve them
as historical records; they do not describe a fork artifact or prove that a
fork build was published.
