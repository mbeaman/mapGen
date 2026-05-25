import { defineConfig, devices } from "@playwright/test";

// Smoke-only e2e: serve the production bundle and drive a real browser through
// the interactive path (worker + wasm + DOM) the Vitest unit tests can't reach.
// `vite preview` serves the built `dist/`, so a build must have run first — the
// `just web-e2e` recipe and the CI job both build before invoking this.
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
  projects: [{ name: "chromium", use: { ...devices["Desktop Chrome"] } }],
  webServer: {
    command: "npm run preview -- --port 4173 --strictPort",
    url: "http://localhost:4173",
    timeout: 120_000,
    reuseExistingServer: !process.env.CI,
  },
});
