import test from "node:test";
import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { fileURLToPath } from "node:url";
import { join } from "node:path";

const testDir = fileURLToPath(new URL(".", import.meta.url));
const packageRoot = join(testDir, "..");
const repoRoot = join(packageRoot, "..", "..");
const ciWorkflow = join(repoRoot, ".github", "workflows", "ci.yml");
const smokeInstallScript = join(repoRoot, "scripts", "smoke-install.mjs");

test("windows package smoke install exports skip-download env in workflow-safe form", () => {
  const text = readFileSync(ciWorkflow, "utf8");
  const packageSmoke = text.match(/  package-smoke:[\s\S]*/)?.[0] ?? "";

  assert.match(
    packageSmoke,
    /- name:\s*Install deps\s*\n\s*env:\s*\n\s*CODEXCHAT_SKIP_DOWNLOAD:\s*["']?1["']?\s*\n\s*run:\s*bun install --frozen-lockfile/
  );
  assert.doesNotMatch(packageSmoke, /CODEXCHAT_SKIP_DOWNLOAD=1 bun install/);
});

test("install smoke uses windows-safe npm command", () => {
  const text = readFileSync(smokeInstallScript, "utf8");
  assert.match(text, /const npmCommand = process\.platform === "win32" \? "npm\.cmd" : "npm";/);
  assert.match(text, /const cliPackageRoot = join\(repoRoot, "packages", "cli"\);/);
  assert.match(text, /run\(npmCommand, \["pack", "\."\], \{ cwd: cliPackageRoot, capture: true \}\)/);
});
