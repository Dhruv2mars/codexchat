import test from "node:test";
import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { fileURLToPath } from "node:url";
import { join } from "node:path";

const testDir = fileURLToPath(new URL(".", import.meta.url));
const packageRoot = join(testDir, "..");
const repoRoot = join(packageRoot, "..", "..");
const ciWorkflow = join(repoRoot, ".github", "workflows", "ci.yml");

test("windows package smoke install exports skip-download env in workflow-safe form", () => {
  const text = readFileSync(ciWorkflow, "utf8");
  const packageSmoke = text.match(/  package-smoke:[\s\S]*/)?.[0] ?? "";

  assert.match(
    packageSmoke,
    /- name:\s*Install deps\s*\n\s*env:\s*\n\s*CODEXCHAT_SKIP_DOWNLOAD:\s*["']?1["']?\s*\n\s*run:\s*bun install --frozen-lockfile/
  );
  assert.doesNotMatch(packageSmoke, /CODEXCHAT_SKIP_DOWNLOAD=1 bun install/);
});
