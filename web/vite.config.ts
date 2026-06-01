import { defineConfig } from "vitest/config";

export default defineConfig({
  base: "./",
  server: { port: 5173, strictPort: true },
  build: {
    target: "es2022",
    // Multi-page build: index.html is the SVG generator; 3d.html is the
    // live wgpu explorer (Stage 0b). Both are served at the root in dev
    // mode and emitted side-by-side into dist/ at build time.
    rollupOptions: {
      input: {
        main: "index.html",
        viewer: "3d.html",
      },
    },
  },
  // Vitest runs the pure unit tests under src/ only. The Playwright e2e specs
  // live in e2e/ and are driven by playwright.config.ts, not Vitest.
  test: { include: ["src/**/*.test.ts"] },
});
