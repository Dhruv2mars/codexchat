#!/usr/bin/env node
import { existsSync } from "node:fs";
import { spawnSync } from "node:child_process";
import { fileURLToPath } from "node:url";
import { join } from "node:path";

const here = fileURLToPath(new URL(".", import.meta.url));
const tests = spawnSync(
  process.execPath,
  [
    "--test",
    join(here, "..", "test", "codexchat-lib.test.js"),
    join(here, "..", "test", "docs-contract.test.js"),
    join(here, "..", "test", "install-lib.test.js"),
    join(here, "..", "test", "platform-matrix.test.js"),
    join(here, "..", "test", "release-contract.test.js")
  ],
  { stdio: "inherit", env: process.env }
);
if (tests.status !== 0) process.exit(tests.status ?? 1);

const repoRoot = join(here, "..", "..", "..");
const builtBinary = join(
  repoRoot,
  "target",
  "debug",
  process.platform === "win32" ? "codexchat.exe" : "codexchat"
);
if (!existsSync(builtBinary)) {
  console.error(`codexchat cli test: missing rust bin at ${builtBinary}`);
  console.error("run: bun run build");
  process.exit(1);
}

const launcher = join(here, "..", "bin", "codexchat.js");
const result = spawnSync(process.execPath, [launcher, "--help"], {
  stdio: "inherit",
  env: {
    ...process.env,
    CODEXCHAT_BIN: builtBinary,
    CODEXCHAT_SKIP_DOWNLOAD: "1"
  }
});
process.exit(result.status ?? 1);
