import { writeFile } from "node:fs/promises"
import { join } from "node:path"
import { afterEach, expect, test, vi } from "vitest"
import { HexError } from "../src/errors.js"
import { resolveCommand } from "../src/helper.js"
import { withTempDir } from "./support.js"

afterEach(() => {
  vi.unstubAllEnvs()
})

test("an explicit command wins over HEX_SERVICE_BINARY", async () => {
  vi.stubEnv("HEX_SERVICE_BINARY", "/missing/hex")
  const command = ["/custom/hex", "service", "--embedded"] as const
  expect(resolveCommand({ command })).toEqual(command)
})

test("HEX_SERVICE_BINARY supplies a local executable", async () => {
  await withTempDir("hex-helper-", async (directory) => {
    const binary = join(directory, "hex")
    await writeFile(binary, "#!/bin/sh\nexit 0\n", { mode: 0o755 })
    vi.stubEnv("HEX_SERVICE_BINARY", binary)
    expect(resolveCommand({})).toEqual([binary, "service", "--embedded"])
  })
})

test("a missing local helper is a startup failure", () => {
  vi.stubEnv("HEX_SERVICE_BINARY", "/missing/hex")
  expect(() => resolveCommand({})).toThrowError(HexError)
  expect(() => resolveCommand({})).toThrowError(/HEX_SERVICE_BINARY/)
})

test("a directory is not accepted as HEX_SERVICE_BINARY", async () => {
  await withTempDir("hex-helper-dir-", async (directory) => {
    vi.stubEnv("HEX_SERVICE_BINARY", directory)
    expect(() => resolveCommand({})).toThrowError(/not executable/)
  })
})
