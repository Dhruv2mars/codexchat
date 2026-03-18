import test from "node:test";
import assert from "node:assert/strict";
import { mkdtempSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";

import {
  assetNameFor,
  cachePathsFor,
  checksumsAssetNameFor,
  codexAssetSpecFor,
  packageManagerHintFromEnv,
  parseChecksumForAsset,
  pinnedCodexVersion,
  resolvePackageVersion,
  shouldInstallBinary
} from "../bin/install-lib.js";

test("install lib resolves asset names and checksum parsing", () => {
  assert.equal(assetNameFor("darwin", "arm64"), "codexchat-darwin-arm64");
  assert.equal(assetNameFor("win32", "x64"), "codexchat-win32-x64.exe");
  assert.equal(checksumsAssetNameFor("linux", "arm64"), "checksums-linux-arm64.txt");

  const cache = cachePathsFor("/tmp/codexchat", "0.1.0", "codexchat-darwin-arm64", "checksums-darwin-arm64.txt");
  assert.equal(cache.cacheDir, "/tmp/codexchat/cache/v0.1.0");
  assert.equal(cache.cacheBinary, "/tmp/codexchat/cache/v0.1.0/codexchat-darwin-arm64");

  const checksum = parseChecksumForAsset(
    "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa *codexchat-darwin-arm64",
    "codexchat-darwin-arm64"
  );
  assert.equal(checksum, "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa");
  assert.equal(parseChecksumForAsset(null, "codexchat-darwin-arm64"), null);
  assert.equal(
    parseChecksumForAsset(
      "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa *other-asset",
      "codexchat-darwin-arm64"
    ),
    null
  );
  assert.equal(pinnedCodexVersion(), "rust-v0.115.0");
});

test("install lib detects package manager hints and install need", () => {
  assert.equal(packageManagerHintFromEnv({ npm_execpath: "/usr/local/bin/bun" }), "bun");
  assert.equal(packageManagerHintFromEnv({ npm_execpath: "/usr/local/bin/pnpm" }), "pnpm");
  assert.equal(packageManagerHintFromEnv({ npm_execpath: "/usr/local/bin/yarn" }), "yarn");
  assert.equal(packageManagerHintFromEnv({ npm_execpath: "/usr/local/bin/npm" }), "npm");
  assert.equal(packageManagerHintFromEnv({ npm_config_user_agent: "bun/1.0.0" }), "bun");
  assert.equal(packageManagerHintFromEnv({ npm_config_user_agent: "pnpm/10.0.0" }), "pnpm");
  assert.equal(packageManagerHintFromEnv({ npm_config_user_agent: "yarn/4.0.0" }), "yarn");
  assert.equal(packageManagerHintFromEnv({ npm_config_user_agent: "npm/10.0.0" }), "npm");
  assert.equal(packageManagerHintFromEnv({}), null);
  assert.equal(shouldInstallBinary({ binExists: false, installedVersion: "0.1.0", packageVersion: "0.1.0" }), true);
  assert.equal(shouldInstallBinary({ binExists: true, installedVersion: "0.0.9", packageVersion: "0.1.0" }), true);
  assert.equal(shouldInstallBinary({ binExists: true, installedVersion: "0.1.0", packageVersion: "0.1.0" }), false);
  assert.equal(shouldInstallBinary({ binExists: true, installedVersion: "0.1.0", packageVersion: "" }), false);
});

test("install lib resolves package version from package json or env fallback", () => {
  const temp = mkdtempSync(join(tmpdir(), "codexchat-install-lib-"));
  try {
    const packageJsonPath = join(temp, "package.json");
    writeFileSync(packageJsonPath, JSON.stringify({ version: "0.1.6" }));
    const emptyVersionPath = join(temp, "empty.json");
    writeFileSync(emptyVersionPath, JSON.stringify({ version: "" }));

    assert.equal(resolvePackageVersion(packageJsonPath, {}), "0.1.6");
    assert.equal(resolvePackageVersion(emptyVersionPath, { npm_package_version: "2.0.0" }), "2.0.0");
    assert.equal(resolvePackageVersion(join(temp, "missing.json"), { npm_package_version: "9.9.9" }), "9.9.9");
    assert.equal(resolvePackageVersion(join(temp, "missing.json"), {}), "0.0.0");
  } finally {
    rmSync(temp, { recursive: true, force: true });
  }
});

test("install lib maps pinned codex assets for all supported targets", () => {
  assert.deepEqual(codexAssetSpecFor("darwin", "arm64"), {
    asset: "codex-aarch64-apple-darwin.tar.gz",
    binName: "codex",
    repo: "openai/codex",
    tag: "rust-v0.115.0"
  });
  assert.deepEqual(codexAssetSpecFor("darwin", "x64"), {
    asset: "codex-x86_64-apple-darwin.tar.gz",
    binName: "codex",
    repo: "openai/codex",
    tag: "rust-v0.115.0"
  });
  assert.deepEqual(codexAssetSpecFor("linux", "arm64"), {
    asset: "codex-aarch64-unknown-linux-gnu.tar.gz",
    binName: "codex",
    repo: "openai/codex",
    tag: "rust-v0.115.0"
  });
  assert.deepEqual(codexAssetSpecFor("linux", "x64"), {
    asset: "codex-x86_64-unknown-linux-gnu.tar.gz",
    binName: "codex",
    repo: "openai/codex",
    tag: "rust-v0.115.0"
  });
  assert.deepEqual(codexAssetSpecFor("win32", "arm64"), {
    asset: "codex-aarch64-pc-windows-msvc.exe",
    binName: "codex.exe",
    repo: "openai/codex",
    tag: "rust-v0.115.0"
  });
  assert.deepEqual(codexAssetSpecFor("win32", "x64"), {
    asset: "codex-x86_64-pc-windows-msvc.exe",
    binName: "codex.exe",
    repo: "openai/codex",
    tag: "rust-v0.115.0"
  });
  assert.throws(() => codexAssetSpecFor("linux", "ppc64"), /unsupported_platform/);
  assert.equal(parseChecksumForAsset("bbbb invalid", "codexchat-darwin-arm64"), null);
});
