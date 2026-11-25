import assert from "node:assert/strict";
import { mkdtempSync, readFileSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { dirname, join } from "node:path";
import { spawnSync } from "node:child_process";
import test from "node:test";
import { fileURLToPath } from "node:url";

const toolsDir = dirname(fileURLToPath(import.meta.url));
const script = join(toolsDir, "gen-winget-manifest.mjs");
const portableHash = "a".repeat(64);
const customHash = "b".repeat(64);

function runGenerator(args) {
  const dir = mkdtempSync(join(tmpdir(), "ddrun-winget-test-"));
  const sums = join(dir, "SHA256SUMS.txt");
  const outDir = join(dir, "winget");
  writeFileSync(
    sums,
    [
      `${portableHash}  dd-run-0.3.0-portable.zip`,
      `${customHash}  dd-run-0.3.0-custom.zip`,
      "",
    ].join("\n"),
    "utf8",
  );
  try {
    const result = spawnSync(
      process.execPath,
      [script, "--version", "0.3.0", "--sums", sums, "--out-dir", outDir, ...args],
      { encoding: "utf8" },
    );
    const installer =
      result.status === 0
        ? readFileSync(join(outDir, "Yang-SH.dd-run.installer.yaml"), "utf8")
        : "";
    const version =
      result.status === 0
        ? readFileSync(join(outDir, "Yang-SH.dd-run.yaml"), "utf8")
        : "";
    return { result, installer, version };
  } finally {
    rmSync(dir, { recursive: true, force: true });
  }
}

test("defaults to the released portable zip and extracts its hash", () => {
  const { result, installer, version } = runGenerator([]);
  assert.equal(result.status, 0, result.stderr);
  assert.match(version, /^DefaultLocale: en-US$/m);
  assert.match(
    installer,
    /InstallerUrl: https:\/\/github\.com\/Yang-SH\/dd-run\/releases\/download\/v0\.3\.0\/dd-run-0\.3\.0-portable\.zip/,
  );
  assert.ok(installer.includes(`InstallerSha256: ${portableHash}`));
  assert.match(
    installer,
    /- RelativeFilePath: dd-run-0\.3\.0\.exe\n\s+PortableCommandAlias: dd-run/,
  );
});

test("keeps explicit asset override support", () => {
  const { result, installer } = runGenerator(["--asset", "dd-run-0.3.0-custom.zip"]);
  assert.equal(result.status, 0, result.stderr);
  assert.match(installer, /dd-run-0\.3\.0-custom\.zip/);
  assert.ok(installer.includes(`InstallerSha256: ${customHash}`));
});
