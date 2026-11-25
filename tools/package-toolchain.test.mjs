import assert from "node:assert/strict";
import { spawnSync } from "node:child_process";
import { readFileSync } from "node:fs";
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
        ...process.env,
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
  const env = { ...process.env };
  delete env.RUSTUP_TOOLCHAIN;
  const [toolchain, selfContained] = selectedToolchain(env);
  assert.equal(toolchain, "+stable-x86_64-pc-windows-gnu");
  assert.equal(
    selfContained,
    "/rustup/toolchains/stable-x86_64-pc-windows-gnu/lib/rustlib/x86_64-pc-windows-gnu/bin/self-contained",
  );
});
