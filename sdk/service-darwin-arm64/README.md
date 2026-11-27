# HEX Service for Apple Silicon

> **Status:** private, local-packaging workspace. No `bin/hex-service` binary is
> committed and this package is not published to npm.

`@hex-ai/service-darwin-arm64` is the platform payload for
`@kitlangton/hex`. When installed in a consuming app, the SDK finds
`bin/hex-service` through this package's exported `package.json` and starts it
as `hex-service service --embedded`, with no `command` option and no desktop
app.

From the fork checkout on arm64 macOS:

```sh
cargo build --release --bin hex
sdk/scripts/pack-helper.sh --binary target/release/hex
```

The script copies the built executable into `bin/hex-service` (gitignored) and
writes a tarball to `sdk/dist/`. Install that tarball in the consuming app with
`npm install /path/to/tarball.tgz`. The copy must be signed and notarized before
distribution in a signed consumer app; building it does not establish a release
pipeline or a download feed.

Other platforms have no workspace package here. Build `hex` from this checkout
and point `HEX_SERVICE_BINARY` at the executable, or pass `command` to
`create()`.

The service protocol lives in
[`docs/specs/local-transcription-service.md`](../../docs/specs/local-transcription-service.md).
The repository's service-app scripts package the full desktop executable and
require explicit signing/notarization credentials (`HEX_TEAM_ID`,
`HEX_NOTARY_PROFILE`); they do not produce this npm payload. Passing their
identity guards alone does not validate a signed helper artifact.
