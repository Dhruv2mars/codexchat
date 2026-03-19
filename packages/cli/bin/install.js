#!/usr/bin/env node
import {
  chmodSync,
  createWriteStream,
  existsSync,
  mkdirSync,
  readdirSync,
  readFileSync,
  renameSync,
  rmSync,
  statSync,
  writeFileSync
} from "node:fs";
import { spawnSync } from "node:child_process";
import { createHash } from "node:crypto";
import http from "node:http";
import { homedir } from "node:os";
import { basename, join } from "node:path";
import https from "node:https";

import {
  assetNameFor,
  cachePathsFor,
  checksumsAssetNameFor,
  codexAssetSpecFor,
  extractedBinaryCandidatesFor,
  packageManagerHintFromEnv,
  parseChecksumForAsset,
  pinnedCodexVersion,
  requestProtocolFor,
  resolvePackageVersion,
  shouldInstallBinary
} from "./install-lib.js";
import { resolveInstalledBin, resolveInstalledCodex, resolvePackageBinDir } from "./codexchat-lib.js";

const APP_REPO = "Dhruv2mars/codexchat";
const installRoot = process.env.CODEXCHAT_INSTALL_ROOT || join(homedir(), ".codexchat");
const binDir = join(installRoot, "bin");
const metaPath = join(installRoot, "install-meta.json");
const appDestination = resolveInstalledBin(process.env, process.platform);
const codexDestination = resolveInstalledCodex(process.env, process.platform);
const here = resolvePackageBinDir(import.meta.url);
const version = resolvePackageVersion(join(here, "..", "package.json"), process.env);
const installedMeta = readInstalledMeta(metaPath);
const installedVersion = typeof installedMeta?.version === "string" ? installedMeta.version : null;
const installedCodexVersion = typeof installedMeta?.codexVersion === "string" ? installedMeta.codexVersion : null;
const repoRoot = join(here, "..", "..", "..");
const appAsset = assetNameFor();
const checksumsAsset = checksumsAssetNameFor();
const codexSpec = codexAssetSpecFor();
const cachePaths = cachePathsFor(installRoot, version, appAsset, checksumsAsset);
const codexCacheDir = join(installRoot, "cache", codexSpec.tag);
const codexArchivePath = join(codexCacheDir, codexSpec.asset);
const needsApp = shouldInstallBinary({
  binExists: existsSync(appDestination),
  installedVersion,
  packageVersion: version
});
const needsCodex = shouldInstallBinary({
  binExists: existsSync(codexDestination),
  installedVersion: installedCodexVersion,
  packageVersion: pinnedCodexVersion()
});

if (process.env.CODEXCHAT_SKIP_DOWNLOAD === "1") process.exit(0);
if (isWorkspaceInstall()) process.exit(0);
if (!needsApp && !needsCodex) process.exit(0);

mkdirSync(binDir, { recursive: true });
mkdirSync(cachePaths.cacheDir, { recursive: true });
mkdirSync(codexCacheDir, { recursive: true });

const appReleaseBaseUrl = process.env.CODEXCHAT_RELEASE_BASE_URL
  || `https://github.com/${APP_REPO}/releases/download/v${version}`;
const codexReleaseBaseUrl = process.env.CODEXCHAT_CODEX_RELEASE_BASE_URL
  || `https://github.com/${codexSpec.repo}/releases/download/${codexSpec.tag}`;

try {
  await main();
} catch (errorValue) {
  console.error(`codexchat: install failed (${String(errorValue)})`);
  process.exit(1);
}

function readInstalledMeta(path) {
  if (!existsSync(path)) return null;
  try {
    return JSON.parse(readFileSync(path, "utf8"));
  } catch {
    return null;
  }
}

async function main() {
  if (needsApp) {
    await installAppBinary({
      asset: appAsset,
      cacheDir: cachePaths.cacheDir,
      checksumsAsset,
      checksumsUrl: `${appReleaseBaseUrl}/${checksumsAsset}`,
      destination: appDestination,
      url: `${appReleaseBaseUrl}/${appAsset}`
    });
  }

  if (needsCodex) {
    await installCodexBinary({
      archivePath: codexArchivePath,
      destination: codexDestination,
      spec: codexSpec,
      url: `${codexReleaseBaseUrl}/${codexSpec.asset}`
    });
  }

  writeFileSync(
    metaPath,
    JSON.stringify(
      {
        codexVersion: pinnedCodexVersion(),
        packageManager: packageManagerHintFromEnv(process.env),
        version
      },
      null,
      2
    )
  );
}

async function installAppBinary({ asset, cacheDir, checksumsAsset, checksumsUrl, destination, url }) {
  const tempPath = `${destination}.tmp-${Date.now()}`;
  try {
    const checksumsText = await requestText(checksumsUrl);
    await download(url, tempPath);
    verifyChecksum(tempPath, asset, checksumsText, cacheDir);
    makeExecutable(tempPath);
    renameSync(tempPath, destination);
  } catch (errorValue) {
    rmSync(tempPath, { force: true });
    throw errorValue;
  }
}

async function installCodexBinary({ archivePath, destination, spec, url }) {
  const tempExtractDir = join(codexCacheDir, `extract-${Date.now()}`);
  const tempPath = `${destination}.tmp-${Date.now()}`;
  try {
    await download(url, archivePath);
    if (spec.asset.endsWith(".tar.gz")) {
      mkdirSync(tempExtractDir, { recursive: true });
      extractTarball(archivePath, tempExtractDir);
      const extracted = findFirstFile(
        tempExtractDir,
        extractedBinaryCandidatesFor(spec.asset, spec.binName)
      );
      if (!extracted) throw new Error(`missing_codex_binary:${spec.binName}`);
      renameSync(extracted, tempPath);
    } else {
      renameSync(archivePath, tempPath);
    }
    makeExecutable(tempPath);
    renameSync(tempPath, destination);
  } catch (errorValue) {
    rmSync(tempPath, { force: true });
    throw errorValue;
  } finally {
    rmSync(tempExtractDir, { recursive: true, force: true });
    if (!spec.asset.endsWith(".exe")) {
      rmSync(archivePath, { force: true });
    }
  }
}

function extractTarball(archivePath, outDir) {
  const result = spawnSync("tar", ["-xzf", archivePath, "-C", outDir], { stdio: "pipe" });
  if (result.status !== 0) {
    throw new Error(`tar_failed:${String(result.stderr || "").trim()}`);
  }
}

function findFirstFile(root, names) {
  for (const entry of readdirSync(root)) {
    const fullPath = join(root, entry);
    const stats = statSync(fullPath);
    if (stats.isDirectory()) {
      const nested = findFirstFile(fullPath, names);
      if (nested) return nested;
      continue;
    }
    if (names.includes(basename(fullPath))) return fullPath;
  }
  return null;
}

function download(url, outputPath, redirects = 0) {
  if (redirects > 5) {
    throw new Error("too_many_redirects");
  }
  const tempPath = `${outputPath}.part`;
  rmSync(tempPath, { force: true });
  return new Promise((resolve, reject) => {
    const transport = requestProtocolFor(url) === "http:" ? http : https;
    const request = transport.get(
      url,
      { headers: { "User-Agent": "codexchat-installer" } },
      (response) => {
        if (
          response.statusCode
          && response.statusCode >= 300
          && response.statusCode < 400
          && response.headers.location
        ) {
          response.resume();
          download(response.headers.location, outputPath, redirects + 1).then(resolve, reject);
          return;
        }
        if (response.statusCode !== 200) {
          response.resume();
          reject(new Error(`http ${response.statusCode}`));
          return;
        }
        const file = createWriteStream(tempPath);
        response.pipe(file);
        file.on("finish", () => {
          file.close(() => {
            renameSync(tempPath, outputPath);
            resolve();
          });
        });
        file.on("error", reject);
      }
    );
    request.on("error", reject);
  });
}

function requestText(url, redirects = 0) {
  if (redirects > 5) {
    throw new Error("too_many_redirects");
  }
  return new Promise((resolve, reject) => {
    const transport = requestProtocolFor(url) === "http:" ? http : https;
    const request = transport.get(
      url,
      { headers: { "User-Agent": "codexchat-installer" } },
      (response) => {
        if (
          response.statusCode
          && response.statusCode >= 300
          && response.statusCode < 400
          && response.headers.location
        ) {
          response.resume();
          requestText(response.headers.location, redirects + 1).then(resolve, reject);
          return;
        }
        if (response.statusCode !== 200) {
          response.resume();
          reject(new Error(`http ${response.statusCode}`));
          return;
        }
        let data = "";
        response.setEncoding("utf8");
        response.on("data", (chunk) => {
          data += chunk;
        });
        response.on("end", () => resolve(data));
      }
    );
    request.on("error", reject);
  });
}

function verifyChecksum(filePath, asset, checksumsText, cachePath) {
  const expected = parseChecksumForAsset(checksumsText, asset);
  if (!expected) {
    throw new Error("missing_checksum");
  }
  const actual = createHash("sha256").update(readFileSync(filePath)).digest("hex");
  if (expected !== actual) {
    throw new Error(`checksum_mismatch clear cache and retry: rm -rf ${cachePath}`);
  }
}

function makeExecutable(path) {
  if (process.platform !== "win32") chmodSync(path, 0o755);
}

function isWorkspaceInstall() {
  return existsSync(join(repoRoot, "Cargo.toml")) && existsSync(join(repoRoot, "crates", "codexchat-cli", "Cargo.toml"));
}
