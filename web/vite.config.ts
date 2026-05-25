import { defineConfig } from "vitest/config";

export default defineConfig({
  base: "./",
  server: { port: 5173, strictPort: true },
  build: { target: "es2022" },
  // Vitest runs the pure unit tests under src/ only. The Playwright e2e specs
  // live in e2e/ and are driven by playwright.config.ts, not Vitest.
  test: { include: ["src/**/*.test.ts"] },
});
