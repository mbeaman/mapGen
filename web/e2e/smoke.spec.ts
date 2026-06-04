import { expect, test, type Locator } from "@playwright/test";

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

// The Faith lens (Phase 2 Diffusion surfaced): the "Faith" overlay comes from the
// shared layer manifest, and toggling it sets the root `on-faith` class. This test
// runs at the DEFAULT scale (continental) + style (ornate), so the gate it exercises
// is the ornate LAYER_STYLE rule `svg.on-faith .layer-faith{display:inline}` — NOT
// the planet-scale FAITH_LENS_STYLE (.planet-political→.planet-faith), which the Rust
// render test faith_overlay.rs pins. We assert the actual reveal: the off-by-default
// `.layer-faith` group flips computed display none→inline. Asserting only the
// `on-faith` class would pass green even if the CSS gate were mistyped and revealed
// nothing — so we pin the swap, not just the wiring.
test("the Faith lens swaps in the faith wash", async ({ page }) => {
  await page.goto("/");
  const svg = page.locator("#map-content svg");
  await expect(svg).toBeVisible({ timeout: 30_000 });

  // Off by default: the ornate faith layer ships `display="none"`.
  const faithLayer = svg.locator(".layer-faith");
  await expect(faithLayer).toHaveCSS("display", "none");

  await page.locator("#layers-panel summary").click();
  await page.getByRole("button", { name: "Faith", exact: true }).click();

  // The toggle fired (root class) AND the gate revealed the layer (display swap).
  await expect(svg).toHaveClass(/on-faith/, { timeout: 10_000 });
  await expect(faithLayer).toHaveCSS("display", "inline", { timeout: 10_000 });
});

// The Prosperity lens (v21 heatmap surfaced): the "Prosperity" overlay comes from the
// shared layer manifest, and toggling it sets the root `on-prosperity` class. As with
// Faith this runs at the DEFAULT scale (continental) + style (ornate), so it exercises
// the ornate LAYER_STYLE rule `svg.on-prosperity .layer-prosperity{display:inline}` —
// NOT the planet-scale PROSPERITY_LENS_STYLE, which the Rust render test
// prosperity_overlay.rs pins. We assert the actual reveal: the off-by-default
// `.layer-prosperity` group flips computed display none→inline. Asserting only the
// `on-prosperity` class would pass green even if the CSS gate were mistyped and
// revealed nothing — so we pin the swap, not just the wiring.
test("the Prosperity lens swaps in the prosperity wash", async ({ page }) => {
  await page.goto("/");
  const svg = page.locator("#map-content svg");
  await expect(svg).toBeVisible({ timeout: 30_000 });

  // Off by default: the ornate prosperity layer ships `display="none"`.
  const prosperityLayer = svg.locator(".layer-prosperity");
  await expect(prosperityLayer).toHaveCSS("display", "none");

  await page.locator("#layers-panel summary").click();
  await page.getByRole("button", { name: "Prosperity", exact: true }).click();

  // The toggle fired (root class) AND the gate revealed the layer (display swap).
  await expect(svg).toHaveClass(/on-prosperity/, { timeout: 10_000 });
  await expect(prosperityLayer).toHaveCSS("display", "inline", { timeout: 10_000 });
});

// width/height of the live SVG's viewBox. The planet preset is a 2:1 globe
// (2048×1024 → aspect 2.0); a continent is 2048×1280 → 1.6. So aspect > 1.8 is
// a render signal that the planet preset actually produced this map — not just
// that the dropdown moved (the breadcrumb label is a pure function of the
// dropdown, so it can't tell a real planet from a broken generation).
const viewBoxAspect = async (svg: Locator): Promise<number> => {
  const vb = (await svg.getAttribute("viewBox")) ?? "";
  const [, , w, h] = vb.trim().split(/[ ,]+/).map(Number);
  return w / h;
};

// The planet zoom-out path: generate at planet scale (the planisphere becomes
// the breadcrumb root), then drill into a continent (the existing refine
// machinery produces the continental view one band in). Kept at a low cell
// count so the planet pipeline finishes well under the generate timeout.
//
// Every gate below keys off a signal that REQUIRES the underlying work to have
// happened — generation completion (#status text), a planet-shaped render
// (viewBox aspect), and refine *success* (past-tense "refined in") — never the
// breadcrumb label alone, which renders optimistically from the dropdown.
test("generates a planet, then drills into a continent", async ({ page }) => {
  await page.goto("/");

  const svg = page.locator("#map-content svg");
  const breadcrumb = page.locator("#breadcrumb");
  const status = page.locator("#status");
  const generate = page.locator("#generate");

  // Wait for the auto-loaded continental world to FINISH (button re-enabled),
  // not just paint a build-up frame — a scale switch while still busy is
  // reverted (it would no-op the generation), so it must be idle first.
  await expect(svg).toBeVisible({ timeout: 30_000 });
  await expect(generate).toBeEnabled({ timeout: 30_000 });

  // Seed 4 at 2000 cells: its largest continent "Rio" is ~1/10 of the planet, so
  // the drill must depth-size it to level 2 (see continents_spec — a level past
  // L1 from one root click is reachable ONLY via the continent re-center branch).
  await page.locator("#seed").fill("4");
  await page.locator("#cells").fill("2000");
  await page.locator("#scale").selectOption("planet");

  // #status holds the busy line for the whole generation ("Generating planet
  // seed …" — progress goes to the overlay, not here), proving planetScale rode
  // through; then flips to "Generated in …s" only on completion. Waiting both,
  // in order, gates the click on a real, finished planet generation (no race).
  await expect(status).toContainText("Generating planet");
  await expect(status).toContainText("Generated in", { timeout: 30_000 });

  // The render is actually the 2:1 planisphere (would be ~1.6 for a continent).
  expect(await viewBoxAspect(svg)).toBeGreaterThan(1.8);
  // Its root crumb reads "Planet" (not "World").
  await expect(breadcrumb.getByRole("button", { name: "Planet" })).toBeVisible();
  // Click the LARGEST continent's engraved label (its position is the landmass
  // centroid; font-size scales with the continent's share of the world, so the
  // biggest font is the biggest continent). Name-independent on purpose — the
  // generated continent names depend on the cultures, so hardcoding one is
  // brittle. The root click snaps to that continent via an async continentAt
  // round-trip, re-centers the drill on its mass and sizes the depth → level ≥2.
  // A grid-drill (the ocean fallback, or the old synchronous drill) always yields
  // L1, so asserting a level PAST L1 proves continent_at resolved the landmass
  // and the re-center/depth-sizing fired.
  const labels = page.locator("#map-content .continent-label");
  await expect(labels.first()).toBeVisible({ timeout: 30_000 });
  const n = await labels.count();
  let bestBox: { x: number; y: number; width: number; height: number } | null = null;
  let bestSize = -1;
  for (let i = 0; i < n; i++) {
    const fs = Number((await labels.nth(i).getAttribute("font-size")) ?? 0);
    if (fs > bestSize) {
      bestSize = fs;
      bestBox = await labels.nth(i).boundingBox();
    }
  }
  const box = bestBox!;
  await page.mouse.click(box.x + box.width / 2, box.y + box.height / 2);
  await expect(status).toContainText("refined in", { timeout: 30_000 });
  await expect(breadcrumb).toContainText(/L[2-6]/);
  await expect(breadcrumb.getByRole("button", { name: "Planet" })).toBeVisible();
});

// The planet scale survives a permalink: `?scale=planet` must reload as a
// planet. readState parses the URL on load and the app auto-generates from it,
// so the autoloaded map should itself be a planet — proving the full
// URL → readState → applyStateToControls → generate round-trip.
test("?scale=planet permalink reloads as a planet", async ({ page }) => {
  await page.goto("/?scale=planet&cells=2000");

  const svg = page.locator("#map-content svg");
  const status = page.locator("#status");

  // The dropdown reflects the URL synchronously on load (readState → controls).
  await expect(page.locator("#scale")).toHaveValue("planet");

  // The autoloaded world finished and is the 2:1 planisphere — so the parsed
  // scale propagated all the way through generation, not just to the control.
  await expect(status).toContainText("Generated in", { timeout: 30_000 });
  await expect(svg).toBeVisible();
  expect(await viewBoxAspect(svg)).toBeGreaterThan(1.8);
});

// The time-slider now lives at planet scale too: the planisphere washes in
// political control, so scrubbing animates empires rise + fall.
test("planet time-slider animates political control", async ({ page }) => {
  await page.goto("/");
  const svg = page.locator("#map-content svg");
  const status = page.locator("#status");
  const generate = page.locator("#generate");

  await expect(svg).toBeVisible({ timeout: 30_000 });
  await expect(generate).toBeEnabled({ timeout: 30_000 });
  await page.locator("#seed").fill("4");
  await page.locator("#cells").fill("2000");
  await page.locator("#scale").selectOption("planet");
  await expect(status).toContainText("Generating planet");
  await expect(status).toContainText("Generated in", { timeout: 30_000 });

  // Seed 4 has border history, so the slider is shown at planet scale (it was
  // hidden here before this feature).
  await expect(page.locator("#timeslider")).not.toHaveClass(/hidden/);

  // Scrub to the founding era and assert the political WASH actually changed —
  // borders moved, so the polygons in the .planet-political group differ. Scoping
  // to that group (not the whole SVG) keeps a broken/empty wash from passing on
  // the legend's coattails, and an empty group would never differ → this fails.
  const wash = page.locator("#map-content .planet-political");
  const present = await wash.innerHTML();
  expect(present).not.toBe(""); // the wash drew something to animate
  await page.locator("#timescrub").evaluate((el: HTMLInputElement) => {
    el.value = el.min; // founding year
    el.dispatchEvent(new Event("input", { bubbles: true }));
  });
  await expect.poll(async () => wash.innerHTML(), { timeout: 15_000 }).not.toBe(present);
});

// The Faith time-slider (Phase 2 Diffusion, replayed): with the Faith lens ON,
// scrubbing animates a faith SPREADING over history — the render-time mirror of
// the data-level `religion_at_year`. Seed 9 @ 2000 diffuses faith across water at
// this resolution (~200 recorded conversions, two faiths reaching a second
// continent), so its `.planet-faith` wash at the founding year holds fewer cells
// than at the present; scrubbing min→present changes the wash. (Seed independent
// of the native crossing-seed set, which runs at a finer planet resolution — same
// reason the exclave test below uses seed 15.) Scoped to `.planet-faith` (not the
// whole SVG) so a broken/empty wash can't pass on the legend's coattails. This is
// the live counterpart to the Rust render test (which pins the wash tracks
// religion_id); here we pin that wasm `renderAtYear` swaps the reconstructed faith
// in while scrubbing.
test("planet Faith slider animates a faith spreading over water", async ({ page }) => {
  await page.goto("/");
  const svg = page.locator("#map-content svg");
  const status = page.locator("#status");
  const generate = page.locator("#generate");

  await expect(svg).toBeVisible({ timeout: 30_000 });
  await expect(generate).toBeEnabled({ timeout: 30_000 });
  await page.locator("#seed").fill("9");
  await page.locator("#cells").fill("2000");
  await page.locator("#scale").selectOption("planet");
  await expect(status).toContainText("Generating planet");
  await expect(status).toContainText("Generated in", { timeout: 30_000 });

  // Turn on the Faith lens, then confirm the slider is available (a crossing seed
  // has a replay timeline — conquest and/or faith — so it's shown).
  await page.locator("#layers-panel summary").click();
  await page.getByRole("button", { name: "Faith", exact: true }).click();
  await expect(svg).toHaveClass(/on-faith/, { timeout: 10_000 });
  await expect(page.locator("#timeslider")).not.toHaveClass(/hidden/);

  // The present faith wash drew cells; scrubbing to the founding year reconstructs
  // a smaller (pre-diffusion) wash, so the group's contents differ.
  const wash = page.locator("#map-content .planet-faith");
  const present = await wash.innerHTML();
  expect(present).not.toBe(""); // the faith wash drew something to animate
  await page.locator("#timescrub").evaluate((el: HTMLInputElement) => {
    el.value = el.min; // founding year
    el.dispatchEvent(new Event("input", { bubbles: true }));
  });
  await expect.poll(async () => wash.innerHTML(), { timeout: 15_000 }).not.toBe(present);
});

// The Sundered Lanes payoff, surfaced in the DOM: the planet wash groups each
// realm as <g class="realm" data-polity="N">, and an overseas exclave (a realm
// holding land on a SECOND landmass, earned by the cross-water carrier) as
// <g class="realm exclave" data-polity="N">. Seed 15 @ 2000 has exactly one such
// exclave at the present and NONE at the slider's founding (min) year — so
// scrubbing min->present makes the exclave appear. This is the machine-pointable
// version of "you can see the sundering": the e2e selects the exclave element by
// class, no pixels. Discriminator is present/absent ACROSS years, so an empty or
// always-present group can't pass.
test("scrubbing the slider reveals an overseas exclave region", async ({ page }) => {
  await page.goto("/");
  const svg = page.locator("#map-content svg");
  const status = page.locator("#status");
  const generate = page.locator("#generate");

  await expect(svg).toBeVisible({ timeout: 30_000 });
  await expect(generate).toBeEnabled({ timeout: 30_000 });
  await page.locator("#seed").fill("15");
  await page.locator("#cells").fill("2000");
  await page.locator("#scale").selectOption("planet");
  await expect(status).toContainText("Generating planet");
  await expect(status).toContainText("Generated in", { timeout: 30_000 });
  await expect(page.locator("#timeslider")).not.toHaveClass(/hidden/);

  // At the present (slider defaults to max) the earned exclave is on the map.
  const exclaves = page.locator("#map-content .planet-political .exclave[data-polity]");
  await expect(exclaves).not.toHaveCount(0, { timeout: 30_000 });

  // Scrub to the founding era (slider min): the cross-water seizure has not
  // happened yet (control_at_year undoes it), so the exclave is gone. The
  // web-first assertion auto-retries, doubling as the wait for the year-frame.
  await page.locator("#timescrub").evaluate((el: HTMLInputElement) => {
    el.value = el.min;
    el.dispatchEvent(new Event("input", { bubbles: true }));
  });
  await expect(exclaves).toHaveCount(0, { timeout: 15_000 });
});

// The 3D globe view (Increment 1): Scale: Globe mounts a three.js sphere over
// the (hidden) SVG layer. The discriminating signal is a REAL WebGL context on
// #globe-canvas — proving the lazily-imported three.js renderer actually mounted,
// not merely that the canvas element exists. (No pixel assertions: headless
// SwiftShader pixel readback is unreliable; correctness of the eventual texture +
// drill is pinned by Vitest unit tests in later increments.)
test("globe scale mounts a 3D sphere and renders a frame", async ({ page }) => {
  await page.goto("/?scale=globe&cells=2000");

  const status = page.locator("#status");
  const canvas = page.locator("#globe-canvas");

  // The dropdown reflects the URL synchronously; generation finishes as a globe.
  await expect(page.locator("#scale")).toHaveValue("globe");
  await expect(status).toContainText("Generating globe");
  await expect(status).toContainText("Globe ready", { timeout: 30_000 });

  // The load-bearing signal: the globe RENDERED A FRAME. globe.ts sets
  // data-rendered="1" after the first renderer.render(), which only happens once
  // the lazily-imported three.js WebGLRenderer instantiated (it throws if WebGL
  // is unavailable, leaving the canvas hidden) and the RAF loop ran. This proves
  // mount + paint — strictly more than "a WebGL context exists" (a bare canvas
  // reports that with no globe at all).
  await expect(canvas).toHaveAttribute("data-rendered", "1", { timeout: 15_000 });
  // And it's textured with the REAL world (the flat biomes render), not the
  // placeholder graticule: setTexture sets data-textured only when a rasterized
  // world texture is applied (the constructor's placeholder never goes through
  // setTexture). So this fails if the generate→biomes-svg→rasterize→setTexture
  // pipeline broke and the sphere fell back to the placeholder.
  await expect(canvas).toHaveAttribute("data-textured", "1", { timeout: 15_000 });
  // The sphere is shown over the hidden SVG layer.
  await expect(canvas).toBeVisible();
  await expect(page.locator("#map-content")).toBeHidden();
  // The breadcrumb root reads "Globe".
  await expect(page.locator("#breadcrumb").getByRole("button", { name: "Globe" })).toBeVisible();

  // A drag over the globe rotates it (OrbitControls) and must NOT drill — we stay
  // at the Globe root with the sphere shown.
  const box = (await canvas.boundingBox())!;
  await page.mouse.move(box.x + box.width / 2, box.y + box.height / 2);
  await page.mouse.down();
  await page.mouse.move(box.x + box.width / 2 + 80, box.y + box.height / 2 + 20, { steps: 8 });
  await page.mouse.up();
  await expect(page.locator("#breadcrumb").getByRole("button", { name: "Globe" })).toBeVisible();
  await expect(canvas).toBeVisible(); // still the globe, not a drilled SVG sector

  // A click (no drag) drills: the raycaster's surface UV → uvToWorld → the SAME
  // continentAt + refine the 2D planet uses, handing off to the 2D SVG sector.
  // "refined in" + an L-level prove the whole chain fired (a click that did
  // nothing, or a globe that swallowed the event, would leave us at the root).
  await page.mouse.click(box.x + box.width / 2, box.y + box.height / 2);
  await expect(status).toContainText("refined in", { timeout: 30_000 });
  await expect(page.locator("#breadcrumb")).toContainText(/L[1-6]/);
  await expect(page.locator("#map-content")).toBeVisible(); // the 2D sector
  await expect(canvas).toBeHidden(); // the globe stepped aside

  // The "Globe" breadcrumb returns to the sphere with NO regenerate.
  await page.locator("#breadcrumb").getByRole("button", { name: "Globe" }).click();
  await expect(canvas).toBeVisible();
  await expect(page.locator("#map-content")).toBeHidden();
});

// Increment 5: the style control is locked on the globe (the sphere is always
// biomes-textured), and toggling globe↔planet reuses the one WebGLRenderer
// without exhausting the browser's GL-context pool.
test("globe locks the style control and survives scale toggling", async ({ page }) => {
  await page.goto("/?scale=globe&cells=2000&seed=4");
  const status = page.locator("#status");
  const canvas = page.locator("#globe-canvas");

  await expect(status).toContainText("Globe ready", { timeout: 30_000 });
  await expect(canvas).toHaveAttribute("data-rendered", "1", { timeout: 15_000 });
  // Style is disabled while the sphere is shown (it doesn't apply).
  await expect(page.locator("#style")).toBeDisabled();

  // Toggle planet → globe twice; the reused renderer must keep mounting/painting.
  for (let i = 0; i < 2; i++) {
    await page.locator("#scale").selectOption("planet");
    await expect(status).toContainText("Generated in", { timeout: 30_000 });
    await expect(page.locator("#map-content svg")).toBeVisible();
    await expect(page.locator("#style")).toBeEnabled(); // style applies in 2D again

    await page.locator("#scale").selectOption("globe");
    await expect(status).toContainText("Globe ready", { timeout: 30_000 });
    await expect(canvas).toBeVisible();
    await expect(canvas).toHaveAttribute("data-textured", "1", { timeout: 15_000 });
    await expect(page.locator("#style")).toBeDisabled();
  }
});
