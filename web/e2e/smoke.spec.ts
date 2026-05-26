import { expect, test } from "@playwright/test";

// One end-to-end smoke covering the interactive path the Vitest unit tests
// can't: worker + wasm + DOM wiring. The app auto-generates a world once the
// engine boots (main.ts "autoload a map so the first paint is a finished
// world"), so the first map paints without interaction; we then exercise the
// Generate button, a layer lens, and the narrate-enabled state. Deliberately a
// single happy-path flow — its job is to catch a frontend that boots,
// generates, or toggles broken, end to end.
test("auto-loads a map, regenerates, applies a lens, enables narration", async ({ page }) => {
  await page.goto("/");

  const svg = page.locator("#map-content svg");
  const generate = page.locator("#generate");
  const narrate = page.locator("#narrate");

  // Engine boots asynchronously (fetches the ~1 MB wasm in a worker) and
  // auto-generates the first world — exercises generate → render → paint.
  await expect(svg).toBeVisible({ timeout: 30_000 });
  // Narration is available once a world exists (the autoloaded one).
  await expect(narrate).toBeEnabled({ timeout: 30_000 });

  // The Generate button re-runs the pipeline without error.
  await expect(generate).toBeEnabled();
  await generate.click();
  await expect(svg).toBeVisible({ timeout: 30_000 });

  // A data overlay applies via a root-<svg> class (no re-render) and survives
  // the SVG swap, since the enabled set is re-applied on every paint.
  await page.locator("#layers-panel summary").click(); // expand the panel
  await page.getByRole("button", { name: "Climate", exact: true }).click();
  await expect(svg).toHaveClass(/on-climate/, { timeout: 10_000 });
});
