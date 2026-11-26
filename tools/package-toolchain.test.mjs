import assert from "node:assert/strict";
import { spawnSync } from "node:child_process";
import { mkdirSync, mkdtempSync, readFileSync, readdirSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { dirname, join } from "node:path";
import test from "node:test";
import { fileURLToPath } from "node:url";

const toolsDir = dirname(fileURLToPath(import.meta.url));
const packageScript = readFileSync(join(toolsDir, "package.sh"), "utf8");
const selection = packageScript
  .split(/\r?\n/)
  .filter((line) => /^(TOOLCHAIN_NAME|TOOLCHAIN|SELF_CONTAINED)=/.test(line))
  .join("\n");

function selectedToolchain(env) {
  const result = spawnSync(
    "bash",
    [
      "-c",
      `${selection}\nprintf '%s\\n%s\\n' "$TOOLCHAIN" "$SELF_CONTAINED"`,
    ],
    {
      encoding: "utf8",
      env: {
        HOME: "/home/ddrun-test",
        RUSTUP_HOME: "/rustup",
        ...env,
      },
    },
  );
  assert.equal(result.status, 0, result.stderr);
  return result.stdout.trim().split("\n");
}

test("honors RUSTUP_TOOLCHAIN and derives the self-contained path", () => {
  const [toolchain, selfContained] = selectedToolchain({
    RUSTUP_TOOLCHAIN: "1.98.0-x86_64-pc-windows-gnu",
  });
  assert.equal(toolchain, "+1.98.0-x86_64-pc-windows-gnu");
  assert.equal(
    selfContained,
    "/rustup/toolchains/1.98.0-x86_64-pc-windows-gnu/lib/rustlib/x86_64-pc-windows-gnu/bin/self-contained",
  );
});

test("defaults to stable windows-gnu when RUSTUP_TOOLCHAIN is unset", () => {
  const [toolchain, selfContained] = selectedToolchain({});
  assert.equal(toolchain, "+stable-x86_64-pc-windows-gnu");
  assert.equal(
    selfContained,
    "/rustup/toolchains/stable-x86_64-pc-windows-gnu/lib/rustlib/x86_64-pc-windows-gnu/bin/self-contained",
  );
});

test("does not inherit RUSTUP_TOOLCHAIN when the caller omits it", () => {
  const previous = process.env.RUSTUP_TOOLCHAIN;
  process.env.RUSTUP_TOOLCHAIN = "1.98.0-x86_64-pc-windows-gnu";
  try {
    const [toolchain, selfContained] = selectedToolchain({});
    assert.equal(toolchain, "+stable-x86_64-pc-windows-gnu");
    assert.equal(
      selfContained,
      "/rustup/toolchains/stable-x86_64-pc-windows-gnu/lib/rustlib/x86_64-pc-windows-gnu/bin/self-contained",
    );
  } finally {
    if (previous === undefined) {
      delete process.env.RUSTUP_TOOLCHAIN;
    } else {
      process.env.RUSTUP_TOOLCHAIN = previous;
    }
  }
});

test("default case stays isolated from a parent BASH_ENV", () => {
  const dir = mkdtempSync(join(tmpdir(), "ddrun-toolchain-env-"));
  const bashEnv = join(dir, "bash-env");
  writeFileSync(bashEnv, "export RUSTUP_TOOLCHAIN=1.98.0-x86_64-pc-windows-gnu\n");
  try {
    const env = {
      ...process.env,
      BASH_ENV: bashEnv.replaceAll("\\", "/"),
      RUSTUP_TOOLCHAIN: "1.98.0-x86_64-pc-windows-gnu",
    };
    delete env.NODE_TEST_CONTEXT;
    const result = spawnSync(
      process.execPath,
      [
        "--test",
        "--test-name-pattern=defaults to stable windows-gnu when RUSTUP_TOOLCHAIN is unset",
        fileURLToPath(import.meta.url),
      ],
      {
        encoding: "utf8",
        env,
      },
    );
    assert.equal(result.status, 0, `${result.stdout}\n${result.stderr}`);
    assert.match(result.stdout, /defaults to stable windows-gnu/);
  } finally {
    rmSync(dir, { recursive: true, force: true });
  }
});

function packageFunction(name) {
  const match = packageScript.match(new RegExp(`${name}\\(\\) \\{[\\s\\S]*?^\\}`, "m"));
  assert.ok(match, `package.sh 缺少 ${name}()`);
  return match[0];
}

function runBash(script, { cwd, env = {} } = {}) {
  return spawnSync("bash", ["-c", script], {
    cwd,
    encoding: "utf8",
    env: {
      PATH: process.env.PATH,
      HOME: cwd,
      TMPDIR: cwd,
      TEMP: cwd,
      TMP: cwd,
      ...env,
    },
  });
}

test("package output is cleaned before a new build", () => {
  const dir = mkdtempSync(join(tmpdir(), "ddrun-package-clean-"));
  const dist = join(dir, "dist");
  const stale = join(dist, "stale.sig");
  mkdirSync(dist, { recursive: true });
  writeFileSync(stale, "stale", { flag: "w" });
  try {
    const result = runBash(`${packageFunction("clean_package_output")}\nclean_package_output`, {
      cwd: dir,
    });
    assert.equal(result.status, 0, result.stderr);
    assert.throws(() => readFileSync(stale, "utf8"), { code: "ENOENT" });
  } finally {
    rmSync(dir, { recursive: true, force: true });
  }
});

test("signing key accepts 64-character hex content without logging it", () => {
  const dir = mkdtempSync(join(tmpdir(), "ddrun-package-key-"));
  const seed = "ab".repeat(32);
  try {
    const result = runBash(
      `${packageFunction("prepare_signing_key")}\nprepare_signing_key\nIFS= read -r material < "$SIGN_KEY_FILE"\nrm -f "$SIGN_KEY_FILE"\nprintf '%s' "$material"`,
      { cwd: dir, env: { DDRUN_SIGN_KEY: seed } },
    );
    assert.equal(result.status, 0, result.stderr);
    assert.equal(result.stdout, seed);
    assert.equal(result.stderr.includes(seed), false);
  } finally {
    rmSync(dir, { recursive: true, force: true });
  }
});

test("signing key accepts an existing seed file", () => {
  const dir = mkdtempSync(join(tmpdir(), "ddrun-package-key-file-"));
  const seed = "cd".repeat(32);
  const keyFile = join(dir, "seed.hex");
  writeFileSync(keyFile, seed);
  try {
    const result = runBash(
      `${packageFunction("prepare_signing_key")}\nprepare_signing_key\nprintf '%s' "$SIGN_KEY_FILE"`,
      { cwd: dir, env: { DDRUN_SIGN_KEY: keyFile } },
    );
    assert.equal(result.status, 0, result.stderr);
    assert.equal(result.stdout.trim(), keyFile);
  } finally {
    rmSync(dir, { recursive: true, force: true });
  }
});

test("formal packaging fails when the signing key is missing", () => {
  const dir = mkdtempSync(join(tmpdir(), "ddrun-package-key-required-"));
  try {
    const result = runBash(
      `${packageFunction("prepare_signing_key")}\nprepare_signing_key`,
      { cwd: dir, env: { DDRUN_PACKAGE_REQUIRE_SIGNATURE: "1" } },
    );
    assert.notEqual(result.status, 0);
    assert.match(result.stderr, /正式发布缺少签名密钥/);
  } finally {
    rmSync(dir, { recursive: true, force: true });
  }
});

test("optional packaging allows a missing signing key", () => {
  const dir = mkdtempSync(join(tmpdir(), "ddrun-package-key-optional-"));
  try {
    const result = runBash(
      `${packageFunction("prepare_signing_key")}\nprepare_signing_key\nprintf '%s' "$SIGN_KEY_FILE"`,
      { cwd: dir },
    );
    assert.equal(result.status, 0, result.stderr);
    assert.equal(result.stdout, "");
  } finally {
    rmSync(dir, { recursive: true, force: true });
  }
});

test("invalid signing key material fails closed", () => {
  const dir = mkdtempSync(join(tmpdir(), "ddrun-package-key-invalid-"));
  try {
    const result = runBash(
      `${packageFunction("prepare_signing_key")}\nprepare_signing_key`,
      { cwd: dir, env: { DDRUN_SIGN_KEY: "not-a-seed" } },
    );
    assert.notEqual(result.status, 0);
    assert.match(result.stderr, /DDRUN_SIGN_KEY/);
  } finally {
    rmSync(dir, { recursive: true, force: true });
  }
});

test("release workflow uploads and attests each platform candidate", () => {
  const workflow = readFileSync(join(toolsDir, "../.github/workflows/release.yml"), "utf8").replace(/\r\n/g, "\n");
  for (const job of ["release", "release-linux", "release-macos"]) {
    const start = workflow.indexOf(`  ${job}:`);
    assert.notEqual(start, -1, `缺少 ${job} job`);
    const rest = workflow.slice(start + `  ${job}:`.length);
    const nextJob = rest.search(/\n  [A-Za-z0-9_-]+:\n/);
    const body = rest.slice(0, nextJob === -1 ? undefined : nextJob);
    assert.match(body, /actions\/attest-build-provenance@/);
    assert.match(body, /actions\/upload-artifact@/);
    assert.doesNotMatch(body, /Attach to GitHub Release/, `${job} 不应直接公开 Release`);
    assert.match(body, /contents: read/);
  }
  assert.match(workflow, /candidate-sums:/);
  assert.match(workflow, /github\.event_name == 'workflow_dispatch'/);
  assert.match(packageScript, /--verify-ext-sig/);
});

test("tag release is drafted and published only after all platforms succeed", () => {
  const workflow = readFileSync(join(toolsDir, "../.github/workflows/release.yml"), "utf8").replace(/\r\n/g, "\n");
  const start = workflow.indexOf("  publish:");
  assert.notEqual(start, -1, "缺少统一 publish job");
  const rest = workflow.slice(start + "  publish:".length);
  const nextJob = rest.search(/\n  [A-Za-z0-9_-]+:\n/);
  const body = rest.slice(0, nextJob === -1 ? undefined : nextJob);
  assert.match(body, /needs: \[release, release-linux, release-macos\]/);
  assert.match(body, /github\.event_name == 'push'/);
  assert.match(body, /gh attestation verify/);
  assert.match(body, /draft: true/);
  assert.match(body, /gh release edit .*--draft=false/);
  assert.match(body, /DDRUN_SIGN_KEY/);
});

test("signing secret is scoped away from non-Windows and summary jobs", () => {
  const workflow = readFileSync(join(toolsDir, "../.github/workflows/release.yml"), "utf8").replace(/\r\n/g, "\n");
  const jobsStart = workflow.indexOf("\njobs:");
  assert.notEqual(jobsStart, -1);
  assert.doesNotMatch(workflow.slice(0, jobsStart), /DDRUN_SIGN_KEY/);
  for (const job of ["release-linux", "release-macos", "candidate-sums"]) {
    const start = workflow.indexOf(`  ${job}:`);
    const rest = workflow.slice(start + `  ${job}:`.length);
    const nextJob = rest.search(/\n  [A-Za-z0-9_-]+:\n/);
    const body = rest.slice(0, nextJob === -1 ? undefined : nextJob);
    assert.doesNotMatch(body, /DDRUN_SIGN_KEY/, `${job} 不应继承私钥`);
  }
  for (const job of ["release", "publish"]) {
    const start = workflow.indexOf(`  ${job}:`);
    const rest = workflow.slice(start + `  ${job}:`.length);
    const nextJob = rest.search(/\n  [A-Za-z0-9_-]+:\n/);
    const body = rest.slice(0, nextJob === -1 ? undefined : nextJob);
    assert.doesNotMatch(body.split("steps:")[0], /DDRUN_SIGN_KEY/, `${job} 原始 secret 不应在 job 级`);
    assert.match(body, /name: Require signing key for tagged release[\s\S]*?env:\s+DDRUN_SIGN_KEY/);
  }
});

test("candidate summary verifies upstream attestations before hashing", () => {
  const workflow = readFileSync(join(toolsDir, "../.github/workflows/release.yml"), "utf8").replace(/\r\n/g, "\n");
  const start = workflow.indexOf("  candidate-sums:");
  const rest = workflow.slice(start + "  candidate-sums:".length);
  const nextJob = rest.search(/\n  [A-Za-z0-9_-]+:\n/);
  const body = rest.slice(0, nextJob === -1 ? undefined : nextJob);
  assert.match(body, /^\s+gh attestation verify /m);
  assert.match(body, /sha256sum -c SHA256SUMS\.txt/);
});

test("temporary signing key is removed by the real EXIT trap", () => {
  const dir = mkdtempSync(join(tmpdir(), "ddrun-package-key-cleanup-"));
  try {
    const result = runBash(
      `SIGN_KEY_FILE=""\nSIGN_KEY_TEMP=""\n${packageFunction("cleanup_signing_key")}\n${packageFunction("prepare_signing_key")}\ntrap cleanup_signing_key EXIT\nprepare_signing_key\ntest -f "$SIGN_KEY_FILE"`,
      { cwd: dir, env: { DDRUN_SIGN_KEY: "ef".repeat(32) } },
    );
    assert.equal(result.status, 0, result.stderr);
    assert.deepEqual(readdirSync(dir), []);
  } finally {
    rmSync(dir, { recursive: true, force: true });
  }
});

test("signing key hex boundaries and uppercase are handled", () => {
  const dir = mkdtempSync(join(tmpdir(), "ddrun-package-key-boundary-"));
  try {
    for (const seed of ["AB".repeat(32), "ab".repeat(32)]) {
      const result = runBash(
        `${packageFunction("prepare_signing_key")}\nprepare_signing_key\ntest -f "$SIGN_KEY_FILE"`,
        { cwd: dir, env: { DDRUN_SIGN_KEY: seed } },
      );
      assert.equal(result.status, 0, result.stderr);
    }
    for (const seed of ["ab".repeat(31), "ab".repeat(33), "g".repeat(64)]) {
      const result = runBash(
        `${packageFunction("prepare_signing_key")}\nprepare_signing_key`,
        { cwd: dir, env: { DDRUN_SIGN_KEY: seed } },
      );
      assert.equal(result.status, 1, seed);
    }
  } finally {
    rmSync(dir, { recursive: true, force: true });
  }
});

test("signing key file paths with spaces are preserved", () => {
  const dir = mkdtempSync(join(tmpdir(), "ddrun package key "));
  const keyFile = join(dir, "seed with spaces.hex");
  writeFileSync(keyFile, "cd".repeat(32));
  try {
    const result = runBash(
      `${packageFunction("prepare_signing_key")}\nprepare_signing_key\ntest "$SIGN_KEY_FILE" = "$DDRUN_SIGN_KEY"`,
      { cwd: dir, env: { DDRUN_SIGN_KEY: keyFile } },
    );
    assert.equal(result.status, 0, result.stderr);
  } finally {
    rmSync(dir, { recursive: true, force: true });
  }
});

test("only Windows packaging prepares a signing key", () => {
  const normalized = packageScript.replace(/\r\n/g, "\n");
  const windowsBranch = normalized.indexOf('if [ "$OS_NAME" = "Linux" ]');
  const prepareCall = normalized.indexOf("\nprepare_signing_key\n");
  assert.notEqual(windowsBranch, -1);
  assert.ok(prepareCall > windowsBranch, "非 Windows 分支必须在校验密钥前 exit");
  assert.match(normalized, /DDRUN_REQUIRE_ZIP/);
  assert.match(normalized, /DDRUN_REQUIRE_ZIP:-0\}" = "1"[\s\S]*?exit 1/);
});

test("portable zip uses an explicit member allowlist and verifies signatures", () => {
  const script = readFileSync(join(toolsDir, "portable_zip.ps1"), "utf8");
  assert.match(script, /com\.ddrun\.filesearch\.json/);
  assert.match(script, /dd-ext-search\.exe/);
  assert.match(script, /unexpected|不允许|未知成员/);
  assert.match(script, /--verify-ext-sig/);
});
