import assert from "node:assert/strict";
import { readFile, readdir } from "node:fs/promises";
import test from "node:test";

const root = new URL("../", import.meta.url);

test("defines a native-only Thinkloom application shell", async () => {
  const [html, packageJson, tauriConfig] = await Promise.all([
    readFile(new URL("../index.html", import.meta.url), "utf8"),
    readFile(new URL("../package.json", import.meta.url), "utf8"),
    readFile(new URL("../src-tauri/tauri.conf.json", import.meta.url), "utf8"),
  ]);

  assert.match(html, /Thinkloom — ideas into writing/i);
  assert.match(html, /\/src\/main\.tsx/);
  assert.doesNotMatch(`${html}\n${packageJson}`, /sites|next|vinext|wrangler|cloudflare/i);
  assert.match(tauriConfig, /"beforeDevCommand": "npm run dev"/);
  assert.match(tauriConfig, /"beforeBuildCommand": "npm run build"/);
  assert.match(tauriConfig, /"frontendDist": "\.\.\/dist"/);
});

test("implements the control and privacy contracts", async () => {
  const [source, css] = await Promise.all([
    readFile(new URL("../src/Thinkloom.tsx", import.meta.url), "utf8"),
    readFile(new URL("../src/globals.css", import.meta.url), "utf8"),
  ]);

  for (const phrase of ["Insert at cursor", "Replace selection", "New section", "Discard", "History recorded", "No audio retained", "Approve for this project", "Relationships, not percentages"]) {
    assert.match(source, new RegExp(phrase, "i"));
  }
  assert.match(source, /GENERATION_PARTIALLY_ACCEPTED/);
  assert.match(source, /CLOUD_PROCESSING_APPROVED/);
  assert.match(source, /store_provider_secret/);
  assert.match(css, /prefers-reduced-motion/);
  assert.match(css, /:focus-visible/);
});

test("never ships retained audio assets", async () => {
  async function walk(url) {
    const entries = await readdir(url, { withFileTypes: true });
    const files = [];
    for (const entry of entries) {
      if (["node_modules", "dist", "target", ".git"].includes(entry.name)) continue;
      const child = new URL(`${entry.name}${entry.isDirectory() ? "/" : ""}`, url);
      if (entry.isDirectory()) files.push(...await walk(child));
      else files.push(entry.name);
    }
    return files;
  }

  const files = await walk(root);
  assert.equal(files.some((name) => /\.(wav|mp3|m4a|ogg|webm)$/i.test(name)), false);
});
