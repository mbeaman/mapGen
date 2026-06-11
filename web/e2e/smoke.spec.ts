import { expect, test, type Locator } from "@playwright/test";
import { MAX_LIVE_PATCHES } from "../src/patchcache"; // the hard live-patch cap (bound assertions track it)

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

// The Trade lens (the "Sundered Lanes" surfaced): the "Trade" overlay comes from
// the shared layer manifest, and toggling it (via the "Trade" preset button) sets
// the root `on-trade` class. Like the Faith test, this runs at the DEFAULT scale
// (continental) + style (ornate), so the gate it exercises is the ornate
// LAYER_STYLE rule `svg.on-trade .layer-trade{display:inline}` — NOT the
// planet-scale TRADE_LENS_STYLE (.planet-political→.planet-trade), which the Rust
// render test trade_overlay.rs pins. We assert the actual reveal: the
// off-by-default `.layer-trade` group flips computed display none→inline.
// Asserting only the `on-trade` class would pass green even if the CSS gate were
// mistyped and revealed nothing — so we pin the swap, not just the wiring.
test("the Trade lens swaps in the sea lanes", async ({ page }) => {
  await page.goto("/");
  const svg = page.locator("#map-content svg");
  await expect(svg).toBeVisible({ timeout: 30_000 });

  // Off by default: the ornate trade layer ships `display="none"`.
  const tradeLayer = svg.locator(".layer-trade");
  await expect(tradeLayer).toHaveCSS("display", "none");

  await page.locator("#layers-panel summary").click();
  await page.getByRole("button", { name: "Trade", exact: true }).click();

  // The toggle fired (root class) AND the gate revealed the layer (display swap).
  await expect(svg).toHaveClass(/on-trade/, { timeout: 10_000 });
  await expect(tradeLayer).toHaveCSS("display", "inline", { timeout: 10_000 });
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

  // Seed 8 at 2000 cells (periodic planet, Phase 5): its largest continent is
  // ~1/11 of the planet AND its centroid sits on land, so a click on its label
  // resolves via continentAt and depth-sizes the drill to level 2 (see
  // continents_spec — a level past L1 from one root click is reachable ONLY via
  // the continent re-center branch; an ocean-centroid continent would grid-drill
  // to L1). (Was seed 4 pre-flip; under periodicity seed 4's largest continent
  // merged bigger and its centroid fell in the sea → grid-drill → L1.)
  await page.locator("#seed").fill("8");
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
  // Seed 26 at 2000 cells (periodic planet, Phase 5): a crossing seed — a polity
  // earns an overseas exclave (spans ≥2 sizable landmasses) by the present, none
  // at gen-time. (Was seed 15 pre-flip; under periodicity seed 15 has only 2
  // landmasses and no crossing, so it grew no exclave — re-derived to seed 26.)
  await page.locator("#seed").fill("26");
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

// The 3D globe tests each spin up a WebGL context (SwiftShader in headless). Run
// them SERIALLY so multiple software-GL contexts don't contend for the CPU and
// starve a globe's first paint past its timeout under the fully-parallel suite.
test.describe.serial("3D globe", () => {
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
  // setTexture). THE READY CONTRACT (#11, quality hunt): "Globe ready" is only
  // announced AFTER the texture lands, so at this point — having waited for the
  // ready status above — the attribute must ALREADY be "1" with no further wait.
  // Announce-before-texture (the old order) reds this race-free read.
  expect(await canvas.getAttribute("data-textured")).toBe("1");
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

  // A click (no drag) drills BUT STAYS IN 3D (1a free-fly): the raycaster's
  // surface UV → continentAt → the camera retargets INTO the region on the SAME
  // sphere — no 2D handoff. The discriminating signal is `data-region="1"` (set
  // ONLY by globe.enterRegion on a drill) WITH the sphere still shown and
  // `#map-content` still hidden — the exact inverse of the old "globe stepped aside
  // to a 2D sector" contract. An L-level breadcrumb proves the drill chain fired.
  await page.mouse.click(box.x + box.width / 2, box.y + box.height / 2);
  await expect(canvas).toHaveAttribute("data-region", "1", { timeout: 30_000 });
  await expect(page.locator("#breadcrumb")).toContainText(/L[1-6]/);
  await expect(canvas).toBeVisible(); // still the 3D globe, NOT a 2D sector
  await expect(page.locator("#map-content")).toBeHidden();
  // 1f entry-ease: the drill GLIDES into the flight pose instead of snapping —
  // `data-entry-eases` is a monotonic counter bumped ONLY when enterRegion starts a
  // glide (the interpolation itself is pinned off-GPU by camera.test.ts::lerpPose).
  // Remove the entryEase capture in enterRegion → never set → red.
  expect(Number(await canvas.getAttribute("data-entry-eases"))).toBeGreaterThan(0);

  // (The drill-precision contract lives in its own test below — this test's
  // earlier drag-rotate makes the click ray camera-orientation-dependent here.)

  // 1b: the refined sector lands on a curved high-detail patch over the base globe
  // — `data-patch=<level>` is set ONLY by globe.showPatch, and `data-patch-textures`
  // is its own upload counter (not the base sphere's `data-textures`). This restores
  // the sharp cartography the coarse 1a skin lacked; the refine is async, so poll.
  await expect(canvas).toHaveAttribute("data-patch", /^[1-6]$/, { timeout: 30_000 });
  expect(Number(await canvas.getAttribute("data-patch-textures"))).toBeGreaterThan(0);

  // PHASE A (off-main-thread rasterize): the patch was rasterized in the WORKER and
  // shipped as RGBA, so the main thread only blits it (putImageData) + builds the patch
  // mesh; the GPU texImage2D is DEFERRED to the next RAF render (not in this span).
  // `data-last-blit-ms` is that synchronous paint-thread cost. The old path (main-thread
  // SVG drawImage of ~12k shapes) measured ~300 ms HERE and stalled the paint thread;
  // off-thread it's single-digit. A discriminating contract: reverting to the SVG +
  // main-thread `rasterizeSvg` path either never writes the signal (null) or blows past
  // 50 ms — both RED. Not `data-textures`, which bumps identically on both paths.
  const blitMs = await canvas.getAttribute("data-last-blit-ms");
  expect(blitMs).not.toBeNull(); // the synchronous RGBA→patch blit ran
  expect(Number(blitMs)).toBeLessThan(50); // NOT the ~300 ms on-paint-thread rasterize

  // Free-fly (1a step 2): the camera is now driven by a flight state over the
  // region. A drag PANS the sub-point across the surface (OrbitControls is
  // disabled while drilled, so it can't rotate the whole globe), and the camera
  // stays outside the sphere (altitude > 0). The GPU camera can't be read from the
  // DOM, so globe.ts writes the flight state to data-* — `data-sub-lon` moving on a
  // drag is a signal ONLY flight-mode panning produces (the old OrbitControls drill
  // never wrote it).
  await expect(canvas).toHaveAttribute("data-sub-lon", /-?\d/);
  expect(Number(await canvas.getAttribute("data-altitude"))).toBeGreaterThan(0);
  const lonBefore = Number(await canvas.getAttribute("data-sub-lon"));
  await page.mouse.move(box.x + box.width / 2, box.y + box.height / 2);
  await page.mouse.down();
  await page.mouse.move(box.x + box.width / 2 - 120, box.y + box.height / 2, { steps: 10 });
  await page.mouse.up();
  await expect
    .poll(async () => Number(await canvas.getAttribute("data-sub-lon")))
    .not.toBe(lonBefore);

  // The "Globe" breadcrumb returns to the whole sphere with NO regenerate, and
  // clears the region mark (a stale data-region would false-green the return).
  await page.locator("#breadcrumb").getByRole("button", { name: "Globe" }).click();
  await expect(canvas).not.toHaveAttribute("data-region", "1");
  await expect(canvas).toBeVisible();
  await expect(page.locator("#map-content")).toBeHidden();
  // ...AND the returned overview is UPRIGHT. Flight leaves camera.up as a surface
  // tangent; without exitRegion resetting it to +Y, OrbitControls' lookAt renders
  // the globe rolled. globe.ts writes camera.up.y to data-cam-up-y on overview
  // frames; ≈1 ⇒ upright (it would be < 1 with the stale tangent up).
  await expect.poll(async () => Number(await canvas.getAttribute("data-cam-up-y"))).toBeGreaterThan(0.99);
  // ...AND narrate is re-enabled (a drill disables it — no sector chronicle; the
  // 1b patch path left it stuck disabled on return until this was fixed).
  await expect(page.locator("#narrate")).toBeEnabled();
});

// THE DRILL-PRECISION CONTRACT: the camera centres on the CLICK POINT, not the
// containing sector's centre. Fresh page (NO prior drag — the camera must be at
// the default overview pose (0,0,3.6)→origin, where a dead-centre click rays to
// exactly the sphere point (0,0,1) = lon -π/2, lat 0). The flight sub-point must
// land there within 1°. The old behaviour quantized the camera to the L3 sector
// centre — measured 25.2° off (lon -67.5°, lat -11.2°), marooning the view over
// the wrong content with the clicked land shoved to the screen edge. Revert
// navTo's focus to the sector centre → ~25° → red; a barycentric-uv pick
// (hit.uv instead of the exact hit.point) drifts mid-triangle clicks too.
test("a globe drill lands WHERE you clicked — within a degree, not a sector-centre away", async ({
  page,
}) => {
  await page.goto("/?scale=globe&cells=2000&seed=8");
  const canvas = page.locator("#globe-canvas");
  await expect(page.locator("#status")).toContainText("Globe ready", { timeout: 30_000 });
  await expect(canvas).toHaveAttribute("data-textured", "1", { timeout: 15_000 });
  const box = (await canvas.boundingBox())!;
  await page.mouse.click(box.x + box.width / 2, box.y + box.height / 2);
  await expect(canvas).toHaveAttribute("data-region", "1", { timeout: 30_000 });
  const subLon = Number(await canvas.getAttribute("data-sub-lon"));
  const subLat = Number(await canvas.getAttribute("data-sub-lat"));
  const dLon = Math.abs(((subLon + Math.PI / 2 + Math.PI) % (2 * Math.PI)) - Math.PI);
  expect((dLon * 180) / Math.PI).toBeLessThan(1);
  expect((Math.abs(subLat) * 180) / Math.PI).toBeLessThan(1);
});

// Increment 5: the style control is locked on the globe (the sphere wears the
// fixed equirectangular `globe` texture), and toggling globe↔planet reuses the one
// WebGLRenderer without exhausting the browser's GL-context pool.
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

// The globe time-slider: with border history the slider shows at the globe root,
// and scrubbing RE-textures the sphere with that year's political control —
// empires rise/fall ON the globe, the 3D mirror of the planisphere slider. A GPU
// texture can't be diffed from the DOM, so globe.ts bumps a monotonic
// `data-textures` count per upload; scrubbing must increase it (a new year frame
// was rasterized + uploaded) and the year label must leave "present".
test("globe time-slider re-textures the sphere per year", async ({ page }) => {
  await page.goto("/?scale=globe&cells=2000&seed=4");
  const status = page.locator("#status");
  const canvas = page.locator("#globe-canvas");

  await expect(status).toContainText("Globe ready", { timeout: 30_000 });
  await expect(canvas).toHaveAttribute("data-textured", "1", { timeout: 15_000 });

  // Seed 4 has border history, so the slider shows at the globe root too (it was
  // hidden in globe mode before this feature).
  await expect(page.locator("#timeslider")).not.toHaveClass(/hidden/);

  const before = Number(await canvas.getAttribute("data-textures"));
  await page.locator("#timescrub").evaluate((el: HTMLInputElement) => {
    el.value = el.min; // founding year
    el.dispatchEvent(new Event("input", { bubbles: true }));
  });
  // A fresh texture was rasterized + uploaded for the scrubbed year.
  await expect
    .poll(async () => Number(await canvas.getAttribute("data-textures")), { timeout: 15_000 })
    .toBeGreaterThan(before);
  await expect(page.locator("#timeyear")).not.toHaveText("present");

  // THE TEMPORAL CONTRACT: drilling from a scrubbed past year SNAPS the base back
  // to the present (refined tiles always render the present — without the snap,
  // present detail composites over a past base: mixed-era content with no
  // warning, and the slider label lies on return). The snap is observable as one
  // more base re-texture fired by the drill itself. Remove the snap in navTo →
  // the count never bumps → red.
  const atPast = Number(await canvas.getAttribute("data-textures"));
  const box = (await canvas.boundingBox())!;
  await page.mouse.click(box.x + box.width / 2, box.y + box.height / 2);
  await expect(canvas).toHaveAttribute("data-region", "1", { timeout: 30_000 });
  await expect
    .poll(async () => Number(await canvas.getAttribute("data-textures")), { timeout: 15_000 })
    .toBeGreaterThan(atPast);
  // Returning to the overview shows a TRUTHFUL label: the base is the present
  // again, and the slider says so.
  await page.locator("#breadcrumb").getByRole("button", { name: "Globe" }).click();
  await expect(page.locator("#timeyear")).toHaveText("present");
});

// Lens parity: a data overlay (Faith/Prosperity/Trade) can be shown on the globe,
// not just the 2D planisphere. On the globe the lens lives in the TEXTURE (the CSS
// can't reach a rasterized sphere), so toggling a lens re-rasterizes the cached
// `globe` SVG with the `on-<lens>` class injected — observable as the monotonic
// `data-textures` count bumping. Prosperity always has data (every realm has it),
// so it's the robust lens to assert at low cell counts.
test("globe lens toggle re-textures the sphere", async ({ page }) => {
  await page.goto("/?scale=globe&cells=2000&seed=11");
  const canvas = page.locator("#globe-canvas");
  await expect(page.locator("#status")).toContainText("Globe ready", { timeout: 30_000 });
  await expect(canvas).toHaveAttribute("data-textured", "1", { timeout: 15_000 });

  const before = Number(await canvas.getAttribute("data-textures"));
  await page.locator("#layers-panel summary").click(); // open the Layers panel
  const prosperity = page.getByRole("checkbox", { name: "Prosperity" });
  await prosperity.check();
  await expect(prosperity).toBeChecked();
  // The toggle re-rasterized the sphere with the lens applied (not a DOM restyle).
  await expect
    .poll(async () => Number(await canvas.getAttribute("data-textures")), { timeout: 15_000 })
    .toBeGreaterThan(before);
});

// 1e: a lens toggle while DRILLED must re-texture the base sphere AND re-stream the
// patches (they cover the surface you're looking at, with the lens baked into each
// patch raster). Pre-1e a drilled lens toggle silently no-op'd — `setLayerState`'s
// globe branch was gated `nav.level === 0`, so while drilled it fell through to the
// hidden 2D SVG and nothing on the globe changed. RED on that guard (data-textures
// never bumps while drilled); green with the broadened `if (globeScale)` + patch refresh.
test("globe lens toggle while DRILLED re-textures the base and re-streams the patches (1e)", async ({ page }) => {
  await page.goto("/?scale=globe&cells=2000&seed=8");
  const canvas = page.locator("#globe-canvas");
  await expect(page.locator("#status")).toContainText("Globe ready", { timeout: 30_000 });
  await expect(canvas).toHaveAttribute("data-rendered", "1", { timeout: 15_000 });
  const box = (await canvas.boundingBox())!;

  // Drill to L3 and let the in-view patch set fully settle.
  await page.mouse.click(box.x + box.width / 2, box.y + box.height / 2);
  await expect(canvas).toHaveAttribute("data-patch", "3", { timeout: 30_000 });
  await expect
    .poll(async () => Number(await canvas.getAttribute("data-live-patches")), { timeout: 15_000 })
    .toBeGreaterThanOrEqual(2);
  let prevKeys = "";
  await expect
    .poll(
      async () => {
        const k = (await canvas.getAttribute("data-patch-keys")) ?? "";
        const stable = k !== "" && k === prevKeys;
        prevKeys = k;
        return stable;
      },
      { timeout: 15_000 },
    )
    .toBe(true);
  const tex0 = Number(await canvas.getAttribute("data-textures"));
  const refines0 = Number(await canvas.getAttribute("data-refines"));

  // Toggle a lens WHILE DRILLED (the panel checkboxes are live, not disabled, here).
  await page.locator("#layers-panel summary").click();
  const prosperity = page.getByRole("checkbox", { name: "Prosperity" });
  await prosperity.check();
  await expect(prosperity).toBeChecked();

  // Base sphere re-textured AND the patches re-streamed under the new lens, in 3D.
  await expect
    .poll(async () => Number(await canvas.getAttribute("data-textures")), { timeout: 15_000 })
    .toBeGreaterThan(tex0);
  await expect
    .poll(async () => Number(await canvas.getAttribute("data-refines")), { timeout: 15_000 })
    .toBeGreaterThan(refines0);
  await expect(canvas).toBeVisible();
  await expect(page.locator("#map-content")).toBeHidden();
});
// Lifecycle regressions the 1a review caught that the rest of the suite is blind
// to: (1) an intermediate breadcrumb hop while drilled must STAY on the sphere —
// not fall through to the (hidden) 2D refine path; (2) regenerating while drilled
// and still on globe scale (so hide() never fires) must reset to the OVERVIEW, not
// wedge the fresh globe in the stale region with the controls dead. A globe click
// drills to GLOBE_FIRST_DRILL_LEVEL = L3, exposing intermediate L1/L2 crumbs.
test("globe drill survives an intermediate crumb hop and a regenerate", async ({ page }) => {
  await page.goto("/?scale=globe&cells=2000&seed=8");
  const status = page.locator("#status");
  const canvas = page.locator("#globe-canvas");
  await expect(status).toContainText("Globe ready", { timeout: 30_000 });
  await expect(canvas).toHaveAttribute("data-rendered", "1", { timeout: 15_000 });

  const box = (await canvas.boundingBox())!;
  await page.mouse.click(box.x + box.width / 2, box.y + box.height / 2);
  await expect(canvas).toHaveAttribute("data-region", "1", { timeout: 30_000 });
  await expect(page.locator("#breadcrumb")).toContainText(/L[3-6]/); // first drill = L3
  // Wait for the initial L3 patch to settle (busy clears) before the crumb hop —
  // otherwise navTo's `if (busy) return` guard would silently drop the click.
  await expect(canvas).toHaveAttribute("data-patch", "3", { timeout: 30_000 });
  // 1b-ii: the region-name billboard shows the drilled landmass's GROUNDED name
  // (seed 8 drills into a named continent). The name's correctness is pinned in
  // Rust (continents_spec); here we prove the consumer renders a non-empty label.
  const label = page.locator(".globe-region-label");
  await expect(label).toBeVisible();
  await expect(label).not.toHaveText("");

  // (1) Click the intermediate L2 crumb → must STAY on the 3D sphere and rebuild the
  // patch at L2; the 2D layer must NOT appear (the pre-fix guard fell through to a
  // hidden 2D refine behind it). data-patch flipping 3→2 proves the in-3D rebuild.
  await page.locator("#breadcrumb").getByRole("button", { name: /^L2 / }).click();
  await expect(canvas).toHaveAttribute("data-patch", "2", { timeout: 30_000 });
  await expect(canvas).toHaveAttribute("data-region", "1");
  await expect(canvas).toBeVisible();
  await expect(page.locator("#map-content")).toBeHidden();

  // (2) Regenerate while still drilled + still on globe scale (no hide() fires):
  // the fresh globe must return to the OVERVIEW (region cleared), not stay wedged.
  await page.locator("#generate").click();
  await expect(status).toContainText("Globe ready", { timeout: 30_000 });
  await expect(canvas).not.toHaveAttribute("data-region", "1");
  await expect(canvas).toBeVisible();
  await expect(page.locator("#map-content")).toBeHidden();
  await expect(label).toBeHidden(); // the billboard cleared with the region on reset
});

// Increment 1c: deeper drilling stays in 3D. A click on the high-detail patch
// (not a drag) drills ONE level finer and rebuilds a smaller patch — the sphere
// stays shown, the 2D layer never appears. Before 1c a patch click did nothing
// (pickable was false while drilled). A globe click drills to L3, so a patch click → L4.
// #10 (quality hunt): the drill's detail fill is a serial queue of up to 49 tiles
// (~100ms+ each) — the user must SEE the progress, not stare at a coarse base with
// a status claiming readiness. `data-pending-tiles` tracks the outstanding queue:
// it appears > 0 right after the drill settles and drains to 0 when the in-view
// set is filled (the status passes through "Loading detail… N tiles" on the same
// updates). Remove the updateStreamProgress calls → the attribute never appears →
// red.
test("drill detail fill shows live progress and drains to zero", async ({ page }) => {
  await page.goto("/?scale=globe&cells=2000&seed=8");
  const canvas = page.locator("#globe-canvas");
  await expect(page.locator("#status")).toContainText("Globe ready", { timeout: 30_000 });
  const box = (await canvas.boundingBox())!;
  await page.mouse.click(box.x + box.width / 2, box.y + box.height / 2);
  await expect(canvas).toHaveAttribute("data-region", "1", { timeout: 30_000 });
  // The queue surfaces (some tiles outstanding)…
  await expect
    .poll(async () => Number((await canvas.getAttribute("data-pending-tiles")) ?? "0"), {
      timeout: 15_000,
    })
    .toBeGreaterThan(0);
  // …and drains to zero with the region prompt restored.
  await expect
    .poll(async () => Number(await canvas.getAttribute("data-pending-tiles")), {
      timeout: 30_000,
    })
    .toBe(0);
  await expect(page.locator("#status")).toContainText("Region L", { timeout: 10_000 });

  // REGRESSION (Phase B in-flight budget refill): a STATIC drill (no pan) must still
  // fill the WHOLE in-view set, not just STREAM_BUDGET tiles. The during-motion pump
  // dies once the camera settles, so the budget slots are refilled on each tile
  // COMPLETION — without that refill only ~3 patches ever load and the rest stay coarse
  // forever (the count would still drain to 0, at 3). This asserts the fill COVERS the
  // view (» the budget of 3) and stays bounded.
  const liveFilled = Number(await canvas.getAttribute("data-live-patches"));
  expect(liveFilled).toBeGreaterThanOrEqual(8); // » STREAM_BUDGET=3 → the refill drained the in-view set
  expect(liveFilled).toBeLessThanOrEqual(MAX_LIVE_PATCHES); // still bounded by the hard cap
});

// CONTRACT (failure recovery): a tile whose refine/rasterize FAILS in the worker
// must RELEASE its budget slot, the fill must still complete, and a PERSISTENTLY
// failing sector must be BENCHED, not retried forever. The in-flight budget turned
// an orphaned reservation from "one sector stays coarse" into "1/STREAM_BUDGET of
// throughput wedged" (3 orphans freeze the stream outright); the naive fix —
// release + refill — would instead retry a deterministically failing sector ~10×/s
// forever, pegging the serial worker. ?failTiles=3 makes the first 3 DISTINCT
// sectors fail FOR REAL inside the wasm rasterizer on EVERY attempt (w=0 →
// Pixmap::new rejects → JsError → the worker's tileFailed path) — no mocks, the
// genuine end-to-end failure surface, persistent by design. Mutation-verified
// both ways — and both starve the fill at 0: drop the pendingTiles.delete → the
// budget is exhausted by the 3 orphans; drop the failedTiles memo skip → the
// doomed sectors are NEAREST-first, so the retry loop monopolises the budget head
// and nothing else ever loads (the memo is load-bearing for the fill itself, not
// just worker politeness).
test("CONTRACT: a failed tile releases its budget slot and a doomed sector is benched, not retried forever", async ({ page }) => {
  await page.goto("/?scale=globe&cells=2000&seed=8&failTiles=3");
  const canvas = page.locator("#globe-canvas");
  await expect(page.locator("#status")).toContainText("Globe ready", { timeout: 30_000 });
  const box = (await canvas.boundingBox())!;
  await page.mouse.click(box.x + box.width / 2, box.y + box.height / 2);
  await expect(canvas).toHaveAttribute("data-region", "1", { timeout: 30_000 });
  // Release half: the failures came back as tileFailed, freed their slots, and the
  // fill still covers the view AROUND the 3 doomed sectors (same ≥8 bound as the
  // static-fill regression above), bounded by the hard cap.
  await expect
    .poll(async () => Number((await canvas.getAttribute("data-live-patches")) ?? "0"), {
      timeout: 30_000,
    })
    .toBeGreaterThanOrEqual(8);
  expect(Number(await canvas.getAttribute("data-live-patches"))).toBeLessThanOrEqual(
    MAX_LIVE_PATCHES,
  );
  // The queue DRAINS to zero — only possible if the benched sectors hold no
  // reservation (an unbounded retry loop keeps ≥1 doomed key permanently pending).
  await expect
    .poll(async () => Number(await canvas.getAttribute("data-pending-tiles")), {
      timeout: 30_000,
    })
    .toBe(0);
  // Bench half: each doomed sector was retried exactly MAX_TILE_RETRIES=3 times
  // (3 sectors × 3 = 9), then benched — the counter reaches 9…
  await expect
    .poll(async () => Number((await canvas.getAttribute("data-tiles-failed")) ?? "0"), {
      timeout: 15_000,
    })
    .toBe(9);
  // …and STAYS 9 with the queue drained: any further attempt could only be a
  // memo-defeating re-request of a doomed key — give one a window to show up.
  await page.waitForTimeout(1_500);
  expect(Number(await canvas.getAttribute("data-tiles-failed"))).toBe(9);
  expect(Number(await canvas.getAttribute("data-pending-tiles"))).toBe(0);
});

// #9 (quality hunt): RE-DRILL REPRODUCIBILITY — drill, return, drill the same
// point again: the same sector must load (the path every user takes constantly;
// state corruption across the round-trip would land them somewhere else or wedge
// the stream). The Rust determinism of refine is pinned separately; this pins the
// APP state machine across the round-trip.
test("re-drilling the same point after a return lands at the same place", async ({ page }) => {
  await page.goto("/?scale=globe&cells=2000&seed=8");
  const canvas = page.locator("#globe-canvas");
  await expect(page.locator("#status")).toContainText("Globe ready", { timeout: 30_000 });
  await expect(canvas).toHaveAttribute("data-textured", "1", { timeout: 15_000 });
  const box = (await canvas.boundingBox())!;
  const cx = box.x + box.width / 2;
  const cy = box.y + box.height / 2;
  await page.mouse.click(cx, cy);
  await expect(canvas).toHaveAttribute("data-region", "1", { timeout: 30_000 });
  const lon1 = Number(await canvas.getAttribute("data-sub-lon"));
  const lat1 = Number(await canvas.getAttribute("data-sub-lat"));
  const crumb1 = await page.locator("#breadcrumb").textContent();
  // Let the drill's stream drain (explicit signal — not a sleep) so the return
  // isn't racing in-flight tiles under CI load.
  await expect
    .poll(async () => Number((await canvas.getAttribute("data-pending-tiles")) ?? "0"), {
      timeout: 30_000,
    })
    .toBe(0);
  // Return to the overview, then drill the same screen point again — only after
  // the overview camera is provably back upright (the exitRegion reset signal).
  await page.locator("#breadcrumb").getByRole("button", { name: "Globe" }).click();
  await expect(canvas).not.toHaveAttribute("data-region", "1");
  await expect
    .poll(async () => Number(await canvas.getAttribute("data-cam-up-y")), { timeout: 10_000 })
    .toBeGreaterThan(0.99);
  await page.mouse.click(cx, cy);
  await expect(canvas).toHaveAttribute("data-region", "1", { timeout: 30_000 });
  const lon2 = Number(await canvas.getAttribute("data-sub-lon"));
  const lat2 = Number(await canvas.getAttribute("data-sub-lat"));
  const crumb2 = await page.locator("#breadcrumb").textContent();
  // THE CONTRACT: the camera returns to the SAME PLACE (the user-meaningful
  // invariant) and the SAME DEPTH. We deliberately do NOT assert the exact
  // breadcrumb sector: the camera focus is the precise click point (the precision
  // fix), but WHICH L3 sector contains it flips across a 45°-wide boundary on a
  // sub-degree difference, so an exact-sector assertion would test boundary
  // quantization, not reproducibility (it flaked (2,4) vs (1,3) under CI timing
  // while the place was identical). The place (≤0.02 rad ≈ 1°) + depth is what
  // reproducibility actually means here.
  expect(Math.abs(lon2 - lon1)).toBeLessThan(0.02);
  expect(Math.abs(lat2 - lat1)).toBeLessThan(0.02);
  expect(crumb1).toMatch(/›L3 /); // depth reproduced (a fresh drill always reaches L3)
  expect(crumb2).toMatch(/›L3 /);
});

test("globe drills deeper in 3D — a patch click rebuilds a finer patch", async ({ page }) => {
  await page.goto("/?scale=globe&cells=2000&seed=8");
  const status = page.locator("#status");
  const canvas = page.locator("#globe-canvas");
  await expect(status).toContainText("Globe ready", { timeout: 30_000 });
  await expect(canvas).toHaveAttribute("data-rendered", "1", { timeout: 15_000 });

  const box = (await canvas.boundingBox())!;
  const cx = box.x + box.width / 2;
  const cy = box.y + box.height / 2;
  // First drill (→ L3 patch at the click).
  await page.mouse.click(cx, cy);
  await expect(canvas).toHaveAttribute("data-patch", "3", { timeout: 30_000 });
  // SETTLE: the fly-to + flight state must be stable so the next click takes the
  // patch-pick branch (not the overview re-pick). Poll until data-altitude holds
  // across two reads — without this the deeper drill is flaky (per the review).
  let prevAlt = "";
  await expect
    .poll(
      async () => {
        const a = (await canvas.getAttribute("data-altitude")) ?? "";
        const stable = a !== "" && a === prevAlt;
        prevAlt = a;
        return stable;
      },
      { timeout: 10_000 },
    )
    .toBe(true);

  // The L3 sector we're looking at — the deeper drill must NEST under THIS one.
  const crumb1 = (await page.locator("#breadcrumb").textContent()) ?? "";
  const l3 = crumb1.match(/L3 \(\d+,\d+\)/)?.[0] ?? "";
  expect(l3).not.toBe("");

  // Deeper drill: a PATCH click → ONE level finer (L4), STILL in 3D, and NESTED
  // under the same L3. The nesting is the signal ONLY the patch-pick path produces —
  // the pre-1c overview re-pick re-snaps and lands in a DIFFERENT branch (so this
  // assertion is false-green-proof: it goes red on the old bundle).
  await page.mouse.click(cx, cy);
  await expect(canvas).toHaveAttribute("data-patch", "4", { timeout: 30_000 });
  await expect(canvas).toBeVisible();
  await expect(page.locator("#map-content")).toBeHidden();
  const nested = new RegExp(`${l3.replace(/[()]/g, "\\$&")}.*L4 `);
  await expect(page.locator("#breadcrumb")).toContainText(nested);
});

// THE BUG YOU CAUGHT (centroid-snap): the globe drilled to the continent CENTROID,
// so every click on a continent loaded the SAME middle region — "not what I clicked
// on". This pins the fix: two DIFFERENT clicks load DIFFERENT regions. With the old
// centroid-snap both collapse to the continent centroid → identical breadcrumb → red.
test("globe drills WHERE YOU CLICK — distinct clicks on a continent load distinct regions", async ({ page }) => {
  await page.goto("/?scale=globe&cells=2000&seed=8"); // a continent fills the centre
  const status = page.locator("#status");
  const canvas = page.locator("#globe-canvas");
  const crumb = page.locator("#breadcrumb");
  await expect(status).toContainText("Globe ready", { timeout: 30_000 });
  await expect(canvas).toHaveAttribute("data-rendered", "1", { timeout: 15_000 });
  const box = (await canvas.boundingBox())!;

  // Two LAND clicks on the centre continent (data-patch="3" ⇒ a continent drill, not
  // an ocean grid-drill), offset so they fall in DIFFERENT sectors. The old centroid-
  // snap collapsed both to the continent's centre sector → identical region → this
  // goes red on that bug.
  await page.mouse.click(box.x + box.width * 0.42, box.y + box.height * 0.45);
  await expect(canvas).toHaveAttribute("data-patch", "3", { timeout: 30_000 });
  // `?? ""` (NOT a distinct "A"/"B" sentinel): if the breadcrumb format ever changes
  // and the regex misses, the explicit not-empty assert reds — distinct sentinels
  // would pass the final inequality ("A" !== "B") while asserting nothing real.
  const regionA = (await crumb.textContent())?.match(/L\d \(\d+,\d+\)/g)?.at(-1) ?? "";
  expect(regionA).not.toBe("");

  await crumb.getByRole("button", { name: "Globe" }).click();
  await expect(canvas).not.toHaveAttribute("data-region", "1", { timeout: 15_000 });
  await page.waitForTimeout(800); // let the overview camera settle before re-clicking
  await page.mouse.click(box.x + box.width * 0.5, box.y + box.height * 0.5);
  await expect(canvas).toHaveAttribute("data-patch", "3", { timeout: 30_000 });
  const regionB = (await crumb.textContent())?.match(/L\d \(\d+,\d+\)/g)?.at(-1) ?? "";
  expect(regionB).not.toBe("");

  console.log("REGION A:", regionA, "  REGION B:", regionB);
  expect(regionA).not.toBe(regionB); // you land where you click, not on one centroid
});

// THE CONTRACT (the flaw you caught): after zooming, scrolling around must KEEP the
// detail — patches stream in for the region you scroll to, not just the one drilled
// sector. This is the ST-1 streaming engine. RED on the single-patch foundation
// (no `data-live-patches`, panning loads nothing); green once streaming lands.
test("CONTRACT: scrolling after a zoom keeps detail — patches stream as you pan", async ({ page }) => {
  await page.goto("/?scale=globe&cells=2000&seed=8");
  const status = page.locator("#status");
  const canvas = page.locator("#globe-canvas");
  await expect(status).toContainText("Globe ready", { timeout: 30_000 });
  await expect(canvas).toHaveAttribute("data-rendered", "1", { timeout: 15_000 });
  const box = (await canvas.boundingBox())!;
  const cx = box.x + box.width * 0.5;
  const cy = box.y + box.height * 0.5;

  // Zoom in.
  await page.mouse.click(cx, cy);
  await expect(canvas).toHaveAttribute("data-patch", "3", { timeout: 30_000 });

  // The IN-VIEW area is covered by MULTIPLE detail patches, not one — and stays
  // within the hard cap. (Single-patch foundation: this attribute is absent → red.)
  await expect
    .poll(async () => Number(await canvas.getAttribute("data-live-patches")), { timeout: 15_000 })
    .toBeGreaterThanOrEqual(2);
  // WAIT FOR THE INITIAL DRILL BATCH TO FULLY LAND before snapshotting the live
  // sector keys. The drill's first reconcile fires up to 9 refineTile requests; if
  // we sampled now, the still-in-flight tiles (NOT the pan) would dirty the set —
  // exactly the false-green the review caught with the old cumulative-counter check.
  // Poll until the key set is STABLE across two reads, then snapshot it.
  let prevKeys = "";
  await expect
    .poll(
      async () => {
        const k = (await canvas.getAttribute("data-patch-keys")) ?? "";
        const stable = k !== "" && k === prevKeys;
        prevKeys = k;
        return stable;
      },
      { timeout: 15_000 },
    )
    .toBe(true);
  const before = new Set(((await canvas.getAttribute("data-patch-keys")) ?? "").split(",").filter(Boolean));
  expect(before.size).toBeLessThanOrEqual(MAX_LIVE_PATCHES); // bounded by the hard cap

  // Scroll/pan a FULL L3 sector across (~325px crosses one 45° sector at this
  // altitude; 450 comfortably clears it) so the post-pan window provably includes a
  // sector OUTSIDE the initial set.
  await page.mouse.move(cx, cy);
  await page.mouse.down();
  await page.mouse.move(cx - 450, cy, { steps: 18 });
  await page.mouse.up();

  // THE CONTRACT: after settling, a sector that was NOT in view at drill time has
  // streamed in — the explored area kept its fidelity. This asserts the PAN's own
  // streaming (a genuinely NEW key), not a counter the initial batch could bump.
  // Disable streamReconcile on the pan settle → no new key ever appears → red.
  await expect
    .poll(
      async () => {
        const after = ((await canvas.getAttribute("data-patch-keys")) ?? "").split(",").filter(Boolean);
        return after.some((k) => !before.has(k)); // a genuinely NEW sector streamed in
      },
      { timeout: 15_000 },
    )
    .toBe(true);
  const liveAfter = Number(await canvas.getAttribute("data-live-patches"));
  expect(liveAfter).toBeGreaterThanOrEqual(2);
  expect(liveAfter).toBeLessThanOrEqual(MAX_LIVE_PATCHES); // still bounded by the hard cap
});

test("CONTRACT (Phase B): detail follows DURING a pan — the reconcile pumps before the drag is released", async ({
  page,
}) => {
  await page.goto("/?scale=globe&cells=2000&seed=8");
  const status = page.locator("#status");
  const canvas = page.locator("#globe-canvas");
  await expect(status).toContainText("Globe ready", { timeout: 30_000 });
  await expect(canvas).toHaveAttribute("data-rendered", "1", { timeout: 15_000 });
  const box = (await canvas.boundingBox())!;
  const cx = box.x + box.width * 0.5;
  const cy = box.y + box.height * 0.5;

  // Drill, then let the initial batch settle so the camera is at rest before the pan.
  await page.mouse.click(cx, cy);
  await expect(canvas).toHaveAttribute("data-patch", "3", { timeout: 30_000 });
  let prevKeys = "";
  await expect
    .poll(
      async () => {
        const k = (await canvas.getAttribute("data-patch-keys")) ?? "";
        const stable = k !== "" && k === prevKeys;
        prevKeys = k;
        return stable;
      },
      { timeout: 15_000 },
    )
    .toBe(true);
  const before = new Set(((await canvas.getAttribute("data-patch-keys")) ?? "").split(",").filter(Boolean));
  const pumpsBefore = Number((await canvas.getAttribute("data-stream-pumps")) ?? "0");

  // THE PHASE-B CONTRACT: hold the drag DOWN and pan — the during-motion pump runs the
  // streaming reconcile WHILE the camera is still moving. Any pointermove keeps the
  // camera un-settled for SETTLE_MS (140 ms) and the pump fires every PUMP_MS (90 ms),
  // so a single drag deterministically climbs `data-stream-pumps` BEFORE the release.
  // The pre-Phase-B engine reconciled ONLY on settle (after mouse.up), so this counter
  // could never move mid-drag (disable the during-motion pump branch in globe.ts → it
  // stays at pumpsBefore → red). It counts the reconcile FIRE, not a tile landing, so
  // it does not flake on slow CI raster.
  await page.mouse.move(cx, cy);
  await page.mouse.down();
  await page.mouse.move(cx - 450, cy, { steps: 12 });
  await expect
    .poll(async () => Number((await canvas.getAttribute("data-stream-pumps")) ?? "0"), { timeout: 5_000 })
    .toBeGreaterThan(pumpsBefore);
  await page.mouse.up();

  // ...AND the pan's streaming actually LANDED content: a sector not in view at
  // drill time streamed in, and the live set stayed within the hard cap. HONESTY:
  // this poll runs after mouse.up, so it is a generic "the pan caused new detail"
  // bound — a settle-only engine would also satisfy it eventually. The during-
  // motion half of the contract is carried ENTIRELY by the data-stream-pumps
  // assertion above (polling for a tile to LAND mid-drag would flake on slow CI
  // raster, which is exactly why the pump counter counts the fire instead).
  await expect
    .poll(
      async () => {
        const after = ((await canvas.getAttribute("data-patch-keys")) ?? "").split(",").filter(Boolean);
        return after.some((k) => !before.has(k));
      },
      { timeout: 15_000 },
    )
    .toBe(true);
  expect(Number(await canvas.getAttribute("data-live-patches"))).toBeLessThanOrEqual(MAX_LIVE_PATCHES);
});
}); // test.describe.serial("3D globe")
