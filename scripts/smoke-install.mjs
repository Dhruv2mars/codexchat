#!/usr/bin/env node
import { createHash } from "node:crypto";
import {
  copyFileSync,
  existsSync,
  mkdtempSync,
  mkdirSync,
  readFileSync,
  rmSync,
  writeFileSync,
} from "node:fs";
import http from "node:http";
import { tmpdir } from "node:os";
import { dirname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";
import { spawn, spawnSync } from "node:child_process";

import {
  assetNameFor,
  checksumsAssetNameFor,
} from "../packages/cli/bin/install-lib.js";

const here = dirname(fileURLToPath(import.meta.url));
const repoRoot = resolve(here, "..");
const tempRoot = mkdtempSync(join(tmpdir(), "codexchat-install-smoke-"));
const releaseDir = join(tempRoot, "release");
const packageDir = join(tempRoot, "package");
const installRoot = join(tempRoot, "install");
const builtBinary = join(
  repoRoot,
  "target",
  "debug",
  process.platform === "win32" ? "codexchat.exe" : "codexchat",
);
const assetName = assetNameFor();
const checksumName = checksumsAssetNameFor();
const assetPath = join(releaseDir, assetName);
const packageJsonPath = join(repoRoot, "packages", "cli", "package.json");
const packageVersion = JSON.parse(readFileSync(packageJsonPath, "utf8")).version;

if (!existsSync(builtBinary)) {
  fail(`missing built binary at ${builtBinary}. run cargo build -p codexchat-cli first`);
}

mkdirSync(releaseDir, { recursive: true });
mkdirSync(packageDir, { recursive: true });
copyFileSync(builtBinary, assetPath);
writeFileSync(
  join(releaseDir, checksumName),
  `${sha256(assetPath)} *${assetName}\n`,
);

const packed = run("npm", ["pack", "./packages/cli"], { cwd: repoRoot, capture: true });
const tarball = lastNonEmptyLine(packed.stdout);
if (!tarball) {
  fail("npm pack did not return a tarball path");
}
run("tar", ["-xzf", tarball, "-C", packageDir], { cwd: repoRoot });

const extractedRoot = join(packageDir, "package");
const installer = join(extractedRoot, "bin", "install.js");
const launcher = join(extractedRoot, "bin", "codexchat.js");

const server = http.createServer((request, response) => {
  const url = new URL(request.url || "/", "http://127.0.0.1");
  const path = join(releaseDir, decodeURIComponent(url.pathname.slice(1)));
  if (!existsSync(path)) {
    response.writeHead(404);
    response.end("not found");
    return;
  }
  response.writeHead(200);
  response.end(readFileSync(path));
});

await new Promise((resolvePromise) => server.listen(0, "127.0.0.1", resolvePromise));
const { port } = server.address();
const releaseBaseUrl = `http://127.0.0.1:${port}`;

try {
  await runAsync(process.execPath, [installer], {
    cwd: repoRoot,
    env: {
      ...process.env,
      CODEXCHAT_INSTALL_ROOT: installRoot,
      CODEXCHAT_RELEASE_BASE_URL: releaseBaseUrl,
    },
  });

  const installedApp = join(
    installRoot,
    "bin",
    process.platform === "win32" ? "codexchat.exe" : "codexchat",
  );
  const installedCodex = join(
    installRoot,
    "bin",
    process.platform === "win32" ? "codex.exe" : "codex",
  );
  const metaPath = join(installRoot, "install-meta.json");

  if (!existsSync(installedApp)) fail(`missing installed app binary at ${installedApp}`);
  if (!existsSync(installedCodex)) fail(`missing installed codex bridge at ${installedCodex}`);
  if (!existsSync(metaPath)) fail(`missing install metadata at ${metaPath}`);

  const meta = JSON.parse(readFileSync(metaPath, "utf8"));
  if (meta.version !== packageVersion) {
    fail(`expected installed version ${packageVersion}, got ${meta.version}`);
  }
  if (!meta.codexVersion) {
    fail("install metadata missing codexVersion");
  }

  await runAsync(process.execPath, [launcher, "--help"], {
    cwd: repoRoot,
    env: {
      ...process.env,
      CODEXCHAT_INSTALL_ROOT: installRoot,
    },
  });
} finally {
  server.close();
}

console.log(`install smoke ok: ${process.platform}/${process.arch}`);

function sha256(path) {
  return createHash("sha256").update(readFileSync(path)).digest("hex");
}

function lastNonEmptyLine(text) {
  return text
    .split(/\r?\n/)
    .map((line) => line.trim())
    .filter(Boolean)
    .at(-1);
}

function run(command, args, options = {}) {
  const result = spawnSync(command, args, {
    cwd: options.cwd,
    encoding: "utf8",
    env: options.env || process.env,
    stdio: options.capture ? "pipe" : "inherit",
  });
  if (result.status !== 0) {
    fail(
      `${command} ${args.join(" ")} failed with code ${result.status ?? 1}\n${
        result.stderr || ""
      }`,
    );
  }
  return result;
}

function runAsync(command, args, options = {}) {
  return new Promise((resolvePromise, rejectPromise) => {
    const child = spawn(command, args, {
      cwd: options.cwd,
      env: options.env || process.env,
      stdio: "inherit",
    });
    child.on("error", rejectPromise);
    child.on("exit", (code) => {
      if (code === 0) {
        resolvePromise();
        return;
      }
      rejectPromise(new Error(`${command} ${args.join(" ")} failed with code ${code ?? 1}`));
    });
  });
}

function fail(message) {
  try {
    rmSync(tempRoot, { force: true, recursive: true });
  } catch {}
  console.error(`codexchat install smoke: ${message}`);
  process.exit(1);
}
