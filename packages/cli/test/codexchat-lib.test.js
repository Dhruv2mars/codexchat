import test from "node:test";
import assert from "node:assert/strict";
import { chmodSync, mkdtempSync, realpathSync, rmSync, writeFileSync, mkdirSync, symlinkSync } from "node:fs";
import { pathToFileURL } from "node:url";
import { tmpdir } from "node:os";
import { join } from "node:path";

import {
  binNameForPlatform,
  codexBinNameForPlatform,
  defaultProbe,
  detectInstalledPackageManager,
  readInstallMeta,
  resolveInstalledCodex,
  resolveInstalledCodexVersion,
  resolveInstallMetaPath,
  resolveInstallRoot,
  resolveInstalledBin,
  resolveInstalledVersion,
  resolvePackageBinDir,
  resolveUpdateCommand,
  shouldRunUpdateCommand
} from "../bin/codexchat-lib.js";

test("codexchat lib resolves install paths and meta", () => {
  const temp = mkdtempSync(join(tmpdir(), "codexchat-lib-"));
  try {
    const env = { CODEXCHAT_INSTALL_ROOT: temp };
    mkdirSync(temp, { recursive: true });
    writeFileSync(
      resolveInstallMetaPath(env),
      JSON.stringify({ codexVersion: "rust-v0.115.0", packageManager: "bun", version: "0.1.0" })
    );

    assert.equal(resolveInstallRoot(env, "/home/test"), temp);
    assert.equal(readInstallMeta(env).version, "0.1.0");
    assert.equal(resolveInstalledVersion(env), "0.1.0");
    assert.equal(resolveInstalledCodexVersion(env), "rust-v0.115.0");
    assert.equal(resolveInstalledBin(env, "darwin", "/home/test"), join(temp, "bin", "codexchat"));
    assert.equal(resolveInstalledCodex(env, "darwin", "/home/test"), join(temp, "bin", "codex"));
    assert.equal(binNameForPlatform("win32"), "codexchat.exe");
    assert.equal(codexBinNameForPlatform("win32"), "codex.exe");
  } finally {
    rmSync(temp, { recursive: true, force: true });
  }
});

test("codexchat lib resolves update command and update arg detection", () => {
  const isolatedEnv = {
    CODEXCHAT_INSTALL_ROOT: join(tmpdir(), "codexchat-update-test-missing"),
    npm_config_user_agent: ""
  };
  assert.equal(shouldRunUpdateCommand(["update"]), true);
  assert.equal(shouldRunUpdateCommand(["chat"]), false);

  const npmUpdate = resolveUpdateCommand({ ...isolatedEnv, npm_execpath: "/tmp/npm-cli.js" });
  assert.equal(npmUpdate.command, process.execPath);
  assert.deepEqual(npmUpdate.args.slice(1), ["install", "-g", "@dhruv2mars/codexchat@latest"]);

  const bunUpdate = resolveUpdateCommand({ ...isolatedEnv, npm_execpath: "/tmp/bun" });
  assert.equal(bunUpdate.command, "bun");
  assert.deepEqual(bunUpdate.args, ["add", "-g", "@dhruv2mars/codexchat@latest"]);
});

test("codexchat lib resolves package bin dir from the real symlink target", () => {
  const temp = mkdtempSync(join(tmpdir(), "codexchat-link-"));
  try {
    const realDir = join(temp, "real", "bin");
    const linkDir = join(temp, "link");
    mkdirSync(realDir, { recursive: true });
    mkdirSync(linkDir, { recursive: true });

    const realScript = join(realDir, "codexchat.js");
    const linkedScript = join(linkDir, "codexchat");
    writeFileSync(realScript, "#!/usr/bin/env node\n");
    symlinkSync(realScript, linkedScript);

    assert.equal(resolvePackageBinDir(pathToFileURL(linkedScript).href), realpathSync(realDir));
  } finally {
    rmSync(temp, { recursive: true, force: true });
  }
});

test("codexchat lib tolerates broken install metadata", () => {
  const temp = mkdtempSync(join(tmpdir(), "codexchat-meta-"));
  try {
    const env = { CODEXCHAT_INSTALL_ROOT: temp };
    mkdirSync(temp, { recursive: true });
    writeFileSync(resolveInstallMetaPath(env), "{not-json");

    assert.equal(readInstallMeta(env), null);
    assert.equal(resolveInstalledVersion(env), null);
    assert.equal(resolveInstalledCodexVersion(env), null);
  } finally {
    rmSync(temp, { recursive: true, force: true });
  }
});

test("codexchat lib probes package managers and detects installed source manager", () => {
  const temp = mkdtempSync(join(tmpdir(), "codexchat-probe-"));
  const originalPath = process.env.PATH;
  try {
    for (const command of ["bun", "npm", "pnpm", "yarn"]) {
      if (process.platform === "win32") {
        const file = join(temp, `${command}.cmd`);
        writeFileSync(file, "@echo off\r\necho @dhruv2mars/codexchat\r\n");
      } else {
        const file = join(temp, command);
        writeFileSync(file, "#!/bin/sh\nprintf '@dhruv2mars/codexchat from %s' \"$0\"\n");
        chmodSync(file, 0o755);
      }
    }

    process.env.PATH = temp;

    assert.equal(defaultProbe("bun").status, 0);
    assert.match(defaultProbe("pnpm").stdout, /codexchat/);
    assert.match(defaultProbe("yarn").stdout, /codexchat/);
    assert.match(defaultProbe("npm").stdout, /codexchat/);
    assert.equal(defaultProbe("missing-manager").status, 1);
    assert.deepEqual(
      defaultProbe("bun", () => {
        throw new Error("boom");
      }),
      { status: 1, stdout: "" }
    );

    assert.equal(
      detectInstalledPackageManager((command) => ({
        status: 0,
        stdout: command === "yarn" ? "@dhruv2mars/codexchat" : ""
      }), "yarn"),
      "yarn"
    );
    assert.equal(
      detectInstalledPackageManager(() => ({ status: 1, stdout: "" }), null),
      null
    );
  } finally {
    process.env.PATH = originalPath;
    rmSync(temp, { recursive: true, force: true });
  }
});
