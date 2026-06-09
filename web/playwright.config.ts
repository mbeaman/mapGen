import { defineConfig, devices } from "@playwright/test";

// Smoke-only e2e: serve the production bundle and drive a real browser through
// the interactive path (worker + wasm + DOM) the Vitest unit tests can't reach.
// `vite preview` serves the built `dist/`, so the JS/TS bundle MUST be fresh. The
// webServer command builds first (`npm run build &&`) so a COLD-start `npx playwright
// test` (nothing serving :4173) can't silently test a stale JS/TS `dist/` — the exact
// trap that hid a globe double-fire bug behind a passing suite. TWO caveats, both
// local-only: (1) `reuseExistingServer` skips this build entirely if something is
// ALREADY serving :4173 — kill a lingering `vite preview` after a rebuild; (2)
// `npm run build` is `tsc && vite build`, which does NOT rebuild the wasm in
// `web/pkg` — after a Rust change run `just web-build` / `npm run build:wasm` first.
export default defineConfig({
  testDir: "e2e",
  timeout: 60_000,
  fullyParallel: true,
  forbidOnly: !!process.env.CI,
  retries: process.env.CI ? 1 : 0,
  reporter: process.env.CI ? "github" : "list",
  use: {
    baseURL: "http://localhost:4173",
    trace: "on-first-retry",
  },
  // A tall viewport so the full control sidebar (with the layer panel expanded)
  // fits without scrolling — otherwise scroll geometry can put a layer row over
  // the preset buttons' hit-point in headless.
  projects: [
    {
      name: "chromium",
      use: {
        ...devices["Desktop Chrome"],
        viewport: { width: 1280, height: 1600 },
        // The 3D globe (Scale: Globe) needs a WebGL context. Headless Chromium
        // has no GPU, so allow software rendering (SwiftShader) — newer Chrome
        // blocks it as "unsafe" for WebGL unless explicitly permitted.
        launchOptions: {
          args: [
            "--enable-unsafe-swiftshader",
            "--use-gl=angle",
            "--use-angle=swiftshader",
            "--ignore-gpu-blocklist",
          ],
        },
      },
    },
  ],
  webServer: {
    command: "npm run build && npm run preview -- --port 4173 --strictPort",
    url: "http://localhost:4173",
    timeout: 120_000,
    reuseExistingServer: !process.env.CI,
  },
});
