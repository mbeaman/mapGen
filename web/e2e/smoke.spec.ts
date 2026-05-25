import { expect, test } from "@playwright/test";

// One end-to-end smoke covering the interactive path the Vitest unit tests
// can't: worker + wasm + DOM wiring. Load → engine ready → generate → the map
// paints → a layer lens applies live → narration enables. Deliberately a single
// happy-path flow (not exhaustive UI coverage) — its job is to catch a frontend
// that boots, generates, or toggles broken, end to end.
test("generate a map, apply a lens, and enable narration", async ({ page }) => {
  await page.goto("/");

  const generate = page.locator("#generate");
  const narrate = page.locator("#narrate");

  // The engine boots asynchronously (fetches the ~1 MB wasm in a worker);
  // Generate is disabled until it's ready, narration until a world exists.
  await expect(generate).toBeEnabled({ timeout: 30_000 });
  await expect(narrate).toBeDisabled();

  await generate.click();

  // The map paints into #map-content once the worker finishes generating.
  const svg = page.locator("#map-content svg");
  await expect(svg).toBeVisible({ timeout: 30_000 });

  // A data overlay applies instantly via a root-<svg> class (no re-render).
  await page.locator("#layers-panel summary").click(); // expand the panel
  await page.getByRole("button", { name: "Climate", exact: true }).click();
  await expect(svg).toHaveClass(/on-climate/);

  // Narration becomes available once a world is present.
  await expect(narrate).toBeEnabled();
});
