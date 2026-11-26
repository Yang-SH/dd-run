import assert from "node:assert/strict";
import { spawnSync } from "node:child_process";
import { mkdtempSync, readFileSync, rmSync, writeFileSync } from "node:fs";
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
