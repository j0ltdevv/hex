# Upstream sync runbook

This fork tracks [`anomalyco/hex`](https://github.com/anomalyco/hex). The current
baseline is upstream commit `86fd7f1966f2cd5e83b7ff70197706945d32c661`.
Sync weekly and before any future fork release; upstream moves quickly.

## Principles

1. Merge by behavior, not by replacing whole engine or worker files.
2. Preserve the Local default, one-attempt Groq transport, Local fallback,
   cancellation, ordering, and secret boundaries.
3. Keep meetings, `/dictations`, the local transcription API, and
   `hex transcribe` local.
4. Carry additive `src/groq*` files and module registration whole.
5. Reapply settings, CLI, i18n, and UI hunks key by key.
6. Regenerate goldens and update Rune/Nix pins in the same change when upstream
   changes them.
7. Record the upstream SHA and verification result in the sync PR.

## Standard merge workflow

Start from a clean fork checkout:

```sh
git fetch upstream --prune
git rev-list --left-right --count upstream/dev...dev
git switch -c sync/upstream-$(date +%F) dev
git merge upstream/dev
```

Resolve conflicts, then inspect the net delta:

```sh
base=$(git merge-base upstream/dev HEAD)
git diff --stat "$base"..HEAD
git diff --check "$base"..HEAD
```

Open a pull request into `dev`. Do not rewrite published `dev` history.

## Net-patch fallback

Use this when the branch has diverged enough that a direct merge is noisy:

```sh
git fetch upstream --prune
base=$(git merge-base upstream/dev HEAD)
git diff --binary "$base"..HEAD > /tmp/hex-fork.patch
git switch -c sync/upstream-$(date +%F) upstream/dev
git apply --3way /tmp/hex-fork.patch
```

The patch contains the fork delta, not upstream history. Resolve each hunk using
the hotspot rules below.

## Conflict hotspots

| Area | Required resolution |
| --- | --- |
| `src/groq*` | Carry whole; review secret handling and request bounds |
| `src/transcription/engine.rs` | Keep upstream streaming/deadlines/order; reinsert only the shared cloud-settlement contract |
| `src/parakeet.rs` | Keep Service targets local; reinsert the provider gate for UI dictation only |
| `src/linux/dictation.rs`, `src/windows/dictation*` | Keep upstream capture/output; reinsert request start, cancellation, and ordered settlement |
| `src/app_settings.rs`, persistence | Keep old files loading as Local; preserve owner-only writes |
| `src/control/settings*` | Keep provider/model choices aligned; key stays redacted and CLI read-only |
| `src/rune_ui/general*` | Keep provider row additive; cloud-only fields appear only in Groq mode |
| `src/desktop/i18n*` | Merge keys per language; keep the completeness scan covering the Groq UI module |
| `docs/GROQ.md`, feature maps | Keep `dictate.groq-cloud-fallback`; update implementation paths and evidence limits |
| `tests/golden/*advanced*` | Regenerate on Linux and inspect both light and dark |

If upstream adds a native cloud provider or changes the local-only product
boundary, stop and reconcile the product decision before merging.

## Verification

Run the focused loop after conflict resolution:

```sh
cargo fmt --all --check
python3 scripts/code-shape.py --check
python3 scripts/check-nix-git-pins.py
scripts/check groq::tests control::settings transcription::engine::tests linux::dictation desktop::i18n
scripts/check rune_ui
```

For a portable Linux build:

```sh
TRANSCRIBE_CMAKE_ARGS='-DTRANSCRIBE_VULKAN=OFF -DTRANSCRIBE_USE_SYSTEM_BLAS=OFF -DGGML_NATIVE=OFF' \
  cargo build --locked --release --bin hex
```

The live Groq smoke is separate and opt-in. Never put a real key or private
fixture in CI, documentation, or a commit.

## Recording the sync

In the pull request, record:

- upstream SHA and merge-base SHA;
- conflict files and how each intent was preserved;
- checks run and their exact results;
- goldens regenerated and inspected;
- any behavior not verified live.

Update [`../CHANGELOG.md`](../CHANGELOG.md) when the sync changes user-visible
behavior or verification coverage.
