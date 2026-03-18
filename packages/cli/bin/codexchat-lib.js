import { existsSync, readFileSync, realpathSync } from "node:fs";
import { homedir } from "node:os";
import { dirname, join } from "node:path";
import { spawnSync } from "node:child_process";
import { fileURLToPath } from "node:url";

import { packageManagerHintFromEnv, shouldInstallBinary } from "./install-lib.js";

const PACKAGE_NAME = "@dhruv2mars/codexchat@latest";
const SUPPORTED_PACKAGE_MANAGERS = new Set(["bun", "npm", "pnpm", "yarn"]);

export { shouldInstallBinary };

export function binNameForPlatform(platform = process.platform) {
  return platform === "win32" ? "codexchat.exe" : "codexchat";
}

export function codexBinNameForPlatform(platform = process.platform) {
  return platform === "win32" ? "codex.exe" : "codex";
}

export function resolveInstallRoot(env = process.env, home = homedir()) {
  return env.CODEXCHAT_INSTALL_ROOT || join(home, ".codexchat");
}

export function resolveInstallMetaPath(env = process.env, home = homedir()) {
  return join(resolveInstallRoot(env, home), "install-meta.json");
}

export function resolveInstalledBin(env = process.env, platform = process.platform, home = homedir()) {
  return join(resolveInstallRoot(env, home), "bin", binNameForPlatform(platform));
}

export function resolveInstalledCodex(env = process.env, platform = process.platform, home = homedir()) {
  return join(resolveInstallRoot(env, home), "bin", codexBinNameForPlatform(platform));
}

export function shouldRunUpdateCommand(args) {
  return Array.isArray(args) && args[0] === "update";
}

export function readInstallMeta(env = process.env, home = homedir()) {
  const metaPath = resolveInstallMetaPath(env, home);
  if (!existsSync(metaPath)) return null;
  try {
    return JSON.parse(readFileSync(metaPath, "utf8"));
  } catch {
    return null;
  }
}

export function resolveInstalledVersion(env = process.env, home = homedir()) {
  const version = readInstallMeta(env, home)?.version;
  return typeof version === "string" && version.length > 0 ? version : null;
}

export function resolveInstalledCodexVersion(env = process.env, home = homedir()) {
  const version = readInstallMeta(env, home)?.codexVersion;
  return typeof version === "string" && version.length > 0 ? version : null;
}

export function resolvePackageBinDir(importMetaUrl) {
  return dirname(realpathSync(fileURLToPath(importMetaUrl)));
}

function updateArgsFor(manager) {
  if (manager === "bun") return ["add", "-g", PACKAGE_NAME];
  if (manager === "pnpm") return ["add", "-g", PACKAGE_NAME];
  if (manager === "yarn") return ["global", "add", PACKAGE_NAME];
  return ["install", "-g", PACKAGE_NAME];
}

export function defaultProbe(command, runner = spawnSync) {
  const args = command === "bun"
    ? ["pm", "ls", "-g"]
    : command === "pnpm"
      ? ["list", "-g", "--depth=0"]
      : command === "yarn"
        ? ["global", "list", "--depth=0"]
        : ["list", "-g", "--depth=0"];
  try {
    const result = runner(command, args, { encoding: "utf8", stdio: "pipe" });
    return {
      status: result.status ?? 1,
      stdout: String(result.stdout || "")
    };
  } catch {
    return { status: 1, stdout: "" };
  }
}

export function detectInstalledPackageManager(probe = defaultProbe, preferred = null) {
  const searchOrder = preferred && SUPPORTED_PACKAGE_MANAGERS.has(preferred)
    ? [preferred, ...[...SUPPORTED_PACKAGE_MANAGERS].filter((value) => value !== preferred)]
    : [...SUPPORTED_PACKAGE_MANAGERS];
  for (const command of searchOrder) {
    const result = probe(command);
    if (result.status !== 0) continue;
    if (result.stdout.includes("@dhruv2mars/codexchat")) {
      return command;
    }
  }
  return null;
}

export function resolveUpdateCommand(env = process.env) {
  const metaPackageManager = readInstallMeta(env)?.packageManager;
  const hintedPackageManager = packageManagerHintFromEnv(env);
  const preferred = SUPPORTED_PACKAGE_MANAGERS.has(metaPackageManager)
    ? metaPackageManager
    : SUPPORTED_PACKAGE_MANAGERS.has(hintedPackageManager)
      ? hintedPackageManager
      : null;
  const manager = preferred || detectInstalledPackageManager(defaultProbe, null) || "npm";

  if (manager === "npm") {
    const npmExecPath = env.npm_execpath;
    if (typeof npmExecPath === "string" && npmExecPath.endsWith(".js")) {
      return {
        args: [npmExecPath, ...updateArgsFor("npm")],
        command: process.execPath
      };
    }
  }

  return {
    args: updateArgsFor(manager),
    command: manager
  };
}
