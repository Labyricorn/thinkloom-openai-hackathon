import assert from "node:assert/strict";
import { readFile, readdir } from "node:fs/promises";
import test from "node:test";

const root = new URL("../", import.meta.url);

async function render() {
  const workerUrl = new URL("../dist/server/index.js", import.meta.url);
  workerUrl.searchParams.set("test", `${process.pid}-${Date.now()}`);
  const { default: worker } = await import(workerUrl.href);
  return worker.fetch(new Request("http://localhost/", { headers: { accept: "text/html" } }), { ASSETS: { fetch: async () => new Response("Not found", { status: 404 }) } }, { waitUntil() {}, passThroughOnException() {} });
}

test("serves the Thinkloom product shell", async () => {
  const response = await render();
  assert.equal(response.status, 200);
  assert.match(response.headers.get("content-type") ?? "", /^text\/html\b/i);
  const html = await response.text();
  assert.match(html, /Thinkloom — ideas into writing/i);
  assert.match(html, /Gathering your threads/i);
  assert.doesNotMatch(html, /codex-preview|starter project|react-loading-skeleton/i);
});

test("implements the control and privacy contracts", async () => {
  const [source, css, layout] = await Promise.all([
    readFile(new URL("../app/thinkloom.tsx", import.meta.url), "utf8"),
    readFile(new URL("../app/globals.css", import.meta.url), "utf8"),
    readFile(new URL("../app/layout.tsx", import.meta.url), "utf8"),
  ]);
  for (const phrase of ["Insert at cursor", "Replace selection", "New section", "Discard", "History recorded", "No audio retained", "Approve for this project", "Relationships, not percentages"]) assert.match(source, new RegExp(phrase, "i"));
  assert.match(source, /GENERATION_PARTIALLY_ACCEPTED/);
  assert.match(source, /CLOUD_PROCESSING_APPROVED/);
  assert.match(source, /store_provider_secret/);
  assert.match(css, /prefers-reduced-motion/);
  assert.match(css, /:focus-visible/);
  assert.match(layout, /Thinkloom — ideas into writing/);
});

test("never ships retained audio assets", async () => {
  async function walk(url) { const entries = await readdir(url, { withFileTypes: true }); const files = []; for (const entry of entries) { if (["node_modules", "dist", "desktop-dist", "target", ".git"].includes(entry.name)) continue; const child = new URL(`${entry.name}${entry.isDirectory() ? "/" : ""}`, url); if (entry.isDirectory()) files.push(...await walk(child)); else files.push(entry.name); } return files; }
  const files = await walk(root);
  assert.equal(files.some((name) => /\.(wav|mp3|m4a|ogg|webm)$/i.test(name)), false);
});
