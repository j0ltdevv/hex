import { accessSync, constants, statSync } from "node:fs"
import { createRequire } from "node:module"
import { dirname, resolve } from "node:path"
import { HexError } from "./errors.js"
import type { CreateOptions } from "./types.js"

const require = createRequire(import.meta.url)

const platformPackage = (): string => {
  if (process.platform === "darwin" && process.arch === "arm64") {
    return "@hex-ai/service-darwin-arm64"
  }
  throw new HexError(
    "startup-failed",
    `HEX does not include a helper for ${process.platform}-${process.arch}`,
  )
}

const executableOrThrow = (executable: string, description: string): string => {
  try {
    if (!statSync(executable).isFile()) {
      throw new Error("not a regular file")
    }
    accessSync(executable, constants.X_OK)
  } catch (cause) {
    throw new HexError("startup-failed", `${description} is not executable: ${executable}`, { cause })
  }
  return executable
}

const packagedExecutable = (): string => {
  const packageName = platformPackage()
  let manifest: string
  try {
    manifest = require.resolve(`${packageName}/package.json`)
  } catch (cause) {
    throw new HexError("startup-failed", `The included HEX helper package ${packageName} is unavailable`, {
      cause,
    })
  }
  return executableOrThrow(
    resolve(dirname(manifest), "bin", "hex-service"),
    "The included HEX helper",
  )
}

export const resolveCommand = (
  options: Pick<CreateOptions, "command">,
): readonly [executable: string, ...arguments: readonly string[]] => {
  if (options.command) return options.command
  const localHelper = process.env.HEX_SERVICE_BINARY
  const executable = localHelper
    ? executableOrThrow(resolve(localHelper), "The HEX helper set in HEX_SERVICE_BINARY")
    : packagedExecutable()
  return [executable, "service", "--embedded"]
}
