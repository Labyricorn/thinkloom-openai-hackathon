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
  assert.match(source, /New empty project/);
  assert.match(source, /function createEmptyProject/);
  assert.match(source, /turns: \[\], ideas: \[\], manuscript: ""/);
  assert.match(source, /New project cancelled\. Your current project is unchanged\./);
  assert.match(source, /purpose: "conversation"/);
  assert.match(source, /did not reply/);
  assert.doesNotMatch(source, /suggests a useful tension\. What changes when you see it as a shared condition/);
  assert.match(css, /prefers-reduced-motion/);
  assert.match(css, /:focus-visible/);
  assert.match(css, /html,body,#root\{[^}]*height:100%[^}]*overflow:hidden/);
  assert.match(css, /\.app-shell\{[^}]*height:100dvh[^}]*overflow:hidden/);
  assert.match(css, /\.ideation-layout\{height:100%;min-height:0/);
});

function contrastRatio(foreground, background) {
  const luminance = (hex) => {
    const channels = hex.slice(1).match(/.{2}/g).map((value) => Number.parseInt(value, 16) / 255);
    const linear = channels.map((value) => value <= 0.04045 ? value / 12.92 : ((value + 0.055) / 1.055) ** 2.4);
    return 0.2126 * linear[0] + 0.7152 * linear[1] + 0.0722 * linear[2];
  };
  const values = [luminance(foreground), luminance(background)].sort((a, b) => b - a);
  return (values[0] + 0.05) / (values[1] + 0.05);
}

function themeToken(theme, name) {
  return theme.match(new RegExp(`--${name}:(#[0-9a-f]{6})`, "i"))?.[1];
}

test("editable surfaces maintain high contrast in light and dark themes", async () => {
  const css = await readFile(new URL("../src/globals.css", import.meta.url), "utf8");
  const lightTheme = css.match(/^:root\{([^}]*)\}/)?.[1];
  const darkTheme = css.match(/@media\(prefers-color-scheme:dark\)\{:root\{([^}]*)\}/)?.[1];
  assert.ok(lightTheme && darkTheme, "Both system themes must define color tokens");

  for (const [name, theme] of [["light", lightTheme], ["dark", darkTheme]]) {
    const background = themeToken(theme, "field-bg");
    const foreground = themeToken(theme, "field-ink");
    const placeholder = themeToken(theme, "field-muted");
    assert.ok(background && foreground && placeholder, `${name} field tokens must be complete`);
    assert.ok(contrastRatio(foreground, background) >= 7, `${name} editable text must meet WCAG AAA contrast`);
    assert.ok(contrastRatio(placeholder, background) >= 7, `${name} placeholder text must meet WCAG AAA contrast`);
  }

  assert.match(css, /\.edit-title,.idea-card textarea\{[^}]*background:var\(--field-bg\)[^}]*color:var\(--field-ink\)/);
  assert.match(css, /\.format-toolbar button\{[^}]*background:var\(--field-bg\)[^}]*color:var\(--field-ink\)/);
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
