#!/usr/bin/env node
import { existsSync, readFileSync } from "node:fs";
import { spawnSync } from "node:child_process";
import { join } from "node:path";

import {
  resolveInstalledCodex,
  resolveInstalledCodexVersion,
  resolvePackageBinDir,
  resolveInstalledBin,
  resolveInstalledVersion,
  resolveUpdateCommand,
  shouldInstallBinary,
  shouldRunUpdateCommand
} from "./codexchat-lib.js";
import { pinnedCodexVersion } from "./install-lib.js";

const args = process.argv.slice(2);
const installedCodex = resolveInstalledCodex(process.env, process.platform);

if (shouldRunUpdateCommand(args)) {
  const update = resolveUpdateCommand(process.env);
  const result = spawnSync(update.command, update.args, { stdio: "inherit", env: process.env });
  process.exit(result.status ?? 1);
}

if (process.env.CODEXCHAT_BIN) {
  run(process.env.CODEXCHAT_BIN, args);
}

const installedBin = resolveInstalledBin(process.env, process.platform);
const packageVersion = readPackageVersion();
const installedVersion = resolveInstalledVersion(process.env);
const installedCodexVersion = resolveInstalledCodexVersion(process.env);

if (
  shouldInstallBinary({
    binExists: existsSync(installedBin),
    installedVersion,
    packageVersion
  }) || shouldInstallBinary({
    binExists: existsSync(installedCodex),
    installedVersion: installedCodexVersion,
    packageVersion: pinnedCodexVersion()
  })
) {
  console.error("codexchat: setting up native runtime...");
  const here = resolvePackageBinDir(import.meta.url);
  const installer = join(here, "install.js");
  const install = spawnSync(process.execPath, [installer], { stdio: "inherit", env: process.env });
  if (install.status !== 0 || !existsSync(installedBin) || !existsSync(installedCodex)) {
    console.error("codexchat: install missing. try reinstall: npm i -g @dhruv2mars/codexchat");
    process.exit(1);
  }
}

run(installedBin, args);

function run(bin, binArgs) {
  const result = spawnSync(bin, binArgs, {
    stdio: "inherit",
    env: {
      ...process.env,
      CODEXCHAT_CODEX_BIN: process.env.CODEXCHAT_CODEX_BIN || installedCodex
    }
  });
  process.exit(result.status ?? 1);
}

function readPackageVersion() {
  try {
    const here = resolvePackageBinDir(import.meta.url);
    const pkg = JSON.parse(readFileSync(join(here, "..", "package.json"), "utf8"));
    return pkg.version;
  } catch {
    return "";
  }
}
