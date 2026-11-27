---
"@kitlangton/hex": patch
---

`create()` now uses a locally built helper named by `HEX_SERVICE_BINARY` when no `command` is given, before falling back to an installed platform package. Documented `sdk/scripts/pack-helper.sh`, which stages a binary built from the checkout into a local `@hex-ai/service-darwin-arm64` tarball. No helper binary or npm registry release is included.
