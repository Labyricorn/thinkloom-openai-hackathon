import assert from "node:assert/strict";
import { access, readFile, readdir, stat } from "node:fs/promises";
import path from "node:path";
import { spawn } from "node:child_process";
import test from "node:test";
import { fileURLToPath } from "node:url";

const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "..");
const releaseRoot = path.join(root, "src-tauri", "target", "release");
const packageJson = JSON.parse(await readFile(path.join(root, "package.json"), "utf8"));
const tauriConfig = JSON.parse(await readFile(path.join(root, "src-tauri", "tauri.conf.json"), "utf8"));

test("packaged Windows release launches and includes both installer formats", { skip: process.platform !== "win32", timeout: 20_000 }, async () => {
  assert.equal(tauriConfig.version, packageJson.version);
  assert.deepEqual(new Set(tauriConfig.bundle.targets), new Set(["msi", "nsis"]));

  const executable = path.join(releaseRoot, "thinkloom.exe");
  await access(executable);
  const executableBytes = await readFile(executable);
  assert.deepEqual(executableBytes.subarray(0, 2), Buffer.from("MZ"));
  assert.ok((await stat(executable)).size > 1_000_000, "release executable is unexpectedly small");

  const bundleRoot = path.join(releaseRoot, "bundle");
  const entries = await readdir(bundleRoot, { recursive: true, withFileTypes: true });
  const artifacts = entries
    .filter((entry) => entry.isFile())
    .map((entry) => path.join(entry.parentPath, entry.name));
  const msi = artifacts.find((file) => file.toLowerCase().endsWith(".msi") && file.includes(packageJson.version));
  const nsis = artifacts.find((file) => file.toLowerCase().endsWith("setup.exe") && file.includes(packageJson.version));
  assert.ok(msi, `no ${packageJson.version} MSI bundle found`);
  assert.ok(nsis, `no ${packageJson.version} NSIS bundle found`);
  assert.deepEqual((await readFile(msi)).subarray(0, 8), Buffer.from([0xd0, 0xcf, 0x11, 0xe0, 0xa1, 0xb1, 0x1a, 0xe1]));
  assert.deepEqual((await readFile(nsis)).subarray(0, 2), Buffer.from("MZ"));

  const child = spawn(executable, [], { cwd: releaseRoot, windowsHide: true, stdio: "ignore" });
  await new Promise((resolve, reject) => {
    child.once("error", reject);
    child.once("spawn", resolve);
  });
  await new Promise((resolve) => setTimeout(resolve, 2_500));
  if (child.exitCode === null) {
    assert.equal(child.kill(), true, "packaged application could not be stopped after launch smoke test");
    await new Promise((resolve) => child.once("exit", resolve));
  } else {
    assert.equal(child.exitCode, 0, "packaged application exited unsuccessfully during launch smoke test");
  }
});
