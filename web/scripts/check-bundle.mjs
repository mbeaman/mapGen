#!/usr/bin/env node
// Bundle guard — keeps three.js out of the entry chunk.
//
// The 3D globe pulls in three.js (~500KB raw). It MUST stay in a lazy chunk
// (loaded only when the user opens the globe), reached via the dynamic
// `import("./globe")` in main.ts, so the default SVG page stays tiny. The
// silent regression vector is a future *static* `import ... from "three"` (or
// `import "./globe"`) somewhere in the eager graph, which re-merges three into
// the entry chunk and bloats first load ~25x with nobody noticing.
//
// This runs after `vite build` and fails the build if the entry chunk grows
// past a threshold three could not fit under, and if three isn't isolated in a
// separate large chunk. Same spirit as the committed golden hashes / mollweide
// vectors: a drifted invariant fails loudly instead of rotting.
//
// Usage: node scripts/check-bundle.mjs   (after `vite build`)

import { readFileSync, readdirSync, statSync } from "node:fs";
import { join, dirname } from "node:path";
import { fileURLToPath } from "node:url";

const dist = join(dirname(fileURLToPath(import.meta.url)), "..", "dist");

// The entry chunk holds the eager app graph; three must never land here.
// Today it's ~20KB; three-merged it would be ~535KB. 120KB leaves generous
// room for the app to grow while still catching a three re-merge unambiguously.
const ENTRY_LIMIT = 120_000;
// The lazy globe chunk carries three; assert it exists and is genuinely large,
// so "code-split working" can't silently degrade to "three tree-shaken away"
// (which would mean a broken globe).
const LAZY_MIN = 300_000;

// The wasm module (worker-fetched, NOT in the eager JS graph, so the ENTRY_LIMIT
// above doesn't weigh it). It carries the whole generation pipeline + the
// worker-side resvg/usvg/tiny-skia rasterizer (`render_rgba`), ~2.13 MB at
// wasm-opt-off. Band it: the ceiling catches a future heavy dep or an accidental
// usvg `text`/fontdb re-enable (~+0.5 MB+); the floor catches the rasterizer (or
// the pipeline) getting tree-shaken away / a truncated build. Same spirit as the
// three-isolation guard — a drifted size fails loudly instead of rotting.
const WASM_MAX = 2_800_000;
const WASM_MIN = 1_500_000;

function fail(msg) {
  console.error(`✗ bundle guard: ${msg}`);
  process.exit(1);
}

let html;
try {
  html = readFileSync(join(dist, "index.html"), "utf8");
} catch {
  fail(`no dist/index.html — run \`vite build\` first`);
}

// The entry is the module script index.html loads eagerly.
const m = html.match(/<script[^>]+type="module"[^>]+src="([^"]+)"/);
if (!m) fail("could not find the entry <script type=module> in dist/index.html");
const entryRel = m[1].replace(/^\.?\//, "");
const entryPath = join(dist, entryRel);

let entrySize;
try {
  entrySize = statSync(entryPath).size;
} catch {
  fail(`entry chunk ${entryRel} not found`);
}

const assets = join(dist, "assets");
const jsFiles = readdirSync(assets)
  .filter((f) => f.endsWith(".js"))
  .map((f) => ({ name: f, size: statSync(join(assets, f)).size }));

const large = jsFiles.filter((f) => f.size >= LAZY_MIN);

console.log(`entry chunk: ${entryRel} = ${(entrySize / 1000).toFixed(1)}KB (limit ${ENTRY_LIMIT / 1000}KB)`);
for (const f of jsFiles.sort((a, b) => b.size - a.size)) {
  console.log(`  ${f.name}: ${(f.size / 1000).toFixed(1)}KB`);
}

if (entrySize > ENTRY_LIMIT) {
  fail(
    `entry chunk is ${(entrySize / 1000).toFixed(1)}KB > ${ENTRY_LIMIT / 1000}KB — ` +
      `three.js likely leaked into the eager graph (a static \`import\` of three or ./globe). ` +
      `Keep it behind the dynamic \`import("./globe")\`.`,
  );
}
if (large.length === 0) {
  fail(
    `no lazy chunk >= ${LAZY_MIN / 1000}KB — the three.js/globe chunk is missing. ` +
      `Code-splitting may have broken (or the globe import was removed).`,
  );
}

// Wasm-size band (worker-fetched module).
const wasmFiles = readdirSync(assets)
  .filter((f) => f.endsWith(".wasm"))
  .map((f) => ({ name: f, size: statSync(join(assets, f)).size }));
if (wasmFiles.length === 0) fail("no .wasm in dist/assets — the wasm-pack build is missing");
const wasm = wasmFiles.sort((a, b) => b.size - a.size)[0];
console.log(`wasm module: ${wasm.name} = ${(wasm.size / 1e6).toFixed(2)}MB (band ${WASM_MIN / 1e6}–${WASM_MAX / 1e6}MB)`);
if (wasm.size > WASM_MAX) {
  fail(
    `wasm ${wasm.name} is ${(wasm.size / 1e6).toFixed(2)}MB > ${WASM_MAX / 1e6}MB — ` +
      `a heavy dep crept in (or usvg's \`text\`/fontdb got re-enabled). Globe tiles are ` +
      `fontless: keep resvg/usvg \`default-features = false\`.`,
  );
}
if (wasm.size < WASM_MIN) {
  fail(
    `wasm ${wasm.name} is ${(wasm.size / 1e6).toFixed(2)}MB < ${WASM_MIN / 1e6}MB — ` +
      `the rasterizer or pipeline may have been tree-shaken away / the build truncated.`,
  );
}

console.log(`✓ bundle guard: entry ${(entrySize / 1000).toFixed(1)}KB, three isolated in a lazy chunk (${large.map((f) => f.name).join(", ")}), wasm ${(wasm.size / 1e6).toFixed(2)}MB`);
