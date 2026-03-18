import test from "node:test";
import assert from "node:assert/strict";

import { assetNameFor, checksumsAssetNameFor, codexAssetSpecFor } from "../bin/install-lib.js";
import { binNameForPlatform, codexBinNameForPlatform } from "../bin/codexchat-lib.js";

test("platform matrix naming stays valid for all shipped targets", () => {
  const cases = [
    {
      appAsset: "codexchat-linux-x64",
      codexAsset: "codex-x86_64-unknown-linux-gnu.tar.gz",
      codexBin: "codex",
      platform: "linux",
      arch: "x64",
      bin: "codexchat"
    },
    {
      appAsset: "codexchat-linux-arm64",
      codexAsset: "codex-aarch64-unknown-linux-gnu.tar.gz",
      codexBin: "codex",
      platform: "linux",
      arch: "arm64",
      bin: "codexchat"
    },
    {
      appAsset: "codexchat-darwin-arm64",
      codexAsset: "codex-aarch64-apple-darwin.tar.gz",
      codexBin: "codex",
      platform: "darwin",
      arch: "arm64",
      bin: "codexchat"
    },
    {
      appAsset: "codexchat-darwin-x64",
      codexAsset: "codex-x86_64-apple-darwin.tar.gz",
      codexBin: "codex",
      platform: "darwin",
      arch: "x64",
      bin: "codexchat"
    },
    {
      appAsset: "codexchat-win32-x64.exe",
      codexAsset: "codex-x86_64-pc-windows-msvc.exe",
      codexBin: "codex.exe",
      platform: "win32",
      arch: "x64",
      bin: "codexchat.exe"
    },
    {
      appAsset: "codexchat-win32-arm64.exe",
      codexAsset: "codex-aarch64-pc-windows-msvc.exe",
      codexBin: "codex.exe",
      platform: "win32",
      arch: "arm64",
      bin: "codexchat.exe"
    }
  ];

  for (const item of cases) {
    assert.equal(binNameForPlatform(item.platform), item.bin);
    assert.equal(codexBinNameForPlatform(item.platform), item.codexBin);
    assert.equal(assetNameFor(item.platform, item.arch), item.appAsset);
    assert.equal(codexAssetSpecFor(item.platform, item.arch).asset, item.codexAsset);
    assert.equal(
      checksumsAssetNameFor(item.platform, item.arch),
      `checksums-${item.platform}-${item.arch}.txt`
    );
  }
});
