//! `mapgen sweep` — parameter-sweep CLI for manual tuning.
//!
//! Renders N maps with one knob varied across a range, plus an
//! `index.html` that grids them for eyeball comparison. Replaces the
//! Refinery's "find good defaults" workflow at ~1/30 the cost (manual
//! pick beats SA over a piecewise objective with 5-30s eval cost).
//!
//! Initial supported knobs: `erosion_rate`, `base_precip`, `lapse_rate`,
//! `axial_tilt`. Adding a knob = one match arm in `Knob::apply` plus a
//! parse arm.

use std::{fmt::Write as _, fs, path::Path};

use anyhow::{anyhow, Context, Result};
use mapgen_render::{render, style::Style};
use mapgen_world::{
    climate::ClimateParams, erosion::ErosionParams, generate_full_with, GenerateParams,
};

#[derive(Copy, Clone, Debug)]
enum Knob {
    ErosionRate,
    BasePrecip,
    LapseRate,
    AxialTilt,
}

impl Knob {
    fn parse(s: &str) -> Result<Self> {
        match s {
            "erosion_rate" => Ok(Self::ErosionRate),
            "base_precip" => Ok(Self::BasePrecip),
            "lapse_rate" => Ok(Self::LapseRate),
            "axial_tilt" => Ok(Self::AxialTilt),
            other => Err(anyhow!(
                "unknown knob '{other}'; supported: erosion_rate, base_precip, lapse_rate, axial_tilt"
            )),
        }
    }

    fn name(self) -> &'static str {
        match self {
            Self::ErosionRate => "erosion_rate",
            Self::BasePrecip => "base_precip",
            Self::LapseRate => "lapse_rate",
            Self::AxialTilt => "axial_tilt",
        }
    }

    fn apply(self, value: f32, ep: &mut ErosionParams, cp: &mut ClimateParams) {
        match self {
            Self::ErosionRate => ep.erosion_rate = value,
            Self::BasePrecip => cp.base_precip = value,
            Self::LapseRate => cp.lapse_rate = value,
            Self::AxialTilt => cp.axial_tilt = value,
        }
    }
}

/// Parse `"lo..hi"` into `(lo, hi)`. Accepts inclusive `..=` and treats
/// it the same — every sweep is over a closed interval since `steps`
/// always emits both endpoints.
fn parse_range(s: &str) -> Result<(f32, f32)> {
    let (lo, hi) = s
        .split_once("..=")
        .or_else(|| s.split_once(".."))
        .ok_or_else(|| anyhow!("range must be `lo..hi` or `lo..=hi`, got `{s}`"))?;
    let lo: f32 = lo
        .trim()
        .parse()
        .with_context(|| format!("lo bound `{lo}` not a number"))?;
    let hi: f32 = hi
        .trim()
        .parse()
        .with_context(|| format!("hi bound `{hi}` not a number"))?;
    if !(lo.is_finite() && hi.is_finite()) {
        return Err(anyhow!("range bounds must be finite: lo={lo}, hi={hi}"));
    }
    if hi < lo {
        return Err(anyhow!("range upper bound {hi} < lower bound {lo}"));
    }
    Ok((lo, hi))
}

#[allow(clippy::too_many_arguments)]
pub fn run(
    seed: u64,
    knob_str: &str,
    range_str: &str,
    steps: usize,
    cells: usize,
    style_str: &str,
    out: &Path,
) -> Result<()> {
    if steps < 2 {
        return Err(anyhow!("--steps must be ≥ 2 (got {steps})"));
    }
    let knob = Knob::parse(knob_str)?;
    let (lo, hi) = parse_range(range_str)?;
    let style: Style = style_str
        .parse()
        .map_err(|e: String| anyhow!("style parse: {e}"))?;
    fs::create_dir_all(out).with_context(|| format!("creating {}", out.display()))?;

    let mut entries: Vec<(f32, String)> = Vec::with_capacity(steps);
    for step in 0..steps {
        let t = step as f32 / (steps - 1) as f32;
        let value = lo + (hi - lo) * t;

        let mut ep = ErosionParams::default();
        let mut cp = ClimateParams::default();
        knob.apply(value, &mut ep, &mut cp);

        let params = GenerateParams {
            seed,
            cell_count: cells,
            ..Default::default()
        };
        let world = generate_full_with(params, ep, cp);
        let svg = render(&world, style).map_err(|e| anyhow!("render: {e}"))?;

        let basename = format!("{}_{:.4}", knob.name(), value);
        let svg_path = out.join(format!("{basename}.svg"));
        let png_path = out.join(format!("{basename}.png"));

        fs::write(&svg_path, &svg).with_context(|| format!("writing {}", svg_path.display()))?;

        let (w, h) = (world.mesh.width as u32, world.mesh.height as u32);
        let png_bytes = svg_to_png(&svg, w, h)?;
        fs::write(&png_path, &png_bytes)
            .with_context(|| format!("writing {}", png_path.display()))?;

        entries.push((value, format!("{basename}.png")));
        eprintln!(
            "  step {:>2}/{steps}  {} = {:.4}",
            step + 1,
            knob.name(),
            value
        );
    }

    let html = render_index_html(knob, seed, cells, lo, hi, &entries);
    let index = out.join("index.html");
    fs::write(&index, html).with_context(|| format!("writing {}", index.display()))?;
    eprintln!("sweep grid: {}", index.display());
    Ok(())
}

fn svg_to_png(svg: &str, width: u32, height: u32) -> Result<Vec<u8>> {
    // Register the same TTF bytes the renderer base64-embeds, so the
    // PNG path sees identical typography to a browser rendering the
    // SVG directly. usvg ignores embedded @font-face data URLs in SVG
    // (no CSS font-loading), so fontdb is the path that makes Cinzel /
    // EB Garamond / IM Fell English actually rasterize on the CLI.
    let mut opts = usvg::Options::default();
    for (_, bytes) in mapgen_render::FONTS_TTF {
        opts.fontdb_mut().load_font_data(bytes.to_vec());
    }
    let tree = usvg::Tree::from_str(svg, &opts).map_err(|e| anyhow!("usvg parse error: {e}"))?;
    let mut pixmap = tiny_skia::Pixmap::new(width.max(1), height.max(1))
        .ok_or_else(|| anyhow!("pixmap size {width}x{height} rejected by tiny-skia"))?;
    resvg::render(&tree, tiny_skia::Transform::default(), &mut pixmap.as_mut());
    pixmap
        .encode_png()
        .map_err(|e| anyhow!("png encode error: {e}"))
}

fn render_index_html(
    knob: Knob,
    seed: u64,
    cells: usize,
    lo: f32,
    hi: f32,
    entries: &[(f32, String)],
) -> String {
    let mut html = String::with_capacity(1024 + entries.len() * 256);
    writeln!(
        html,
        r#"<!doctype html>
<html lang="en"><head><meta charset="utf-8">
<title>mapgen sweep — {knob}</title>
<style>
  body {{ font: 13px/1.4 -apple-system, system-ui, sans-serif;
         margin: 0; padding: 24px; background: #1a1a1a; color: #d8d6cf; }}
  header {{ margin-bottom: 16px; }}
  h1 {{ margin: 0 0 4px; font-weight: 500; font-size: 18px; }}
  header p {{ margin: 0; color: #8a8a8a; font-family: ui-monospace, monospace; }}
  .grid {{ display: grid; grid-template-columns: repeat(auto-fill, minmax(280px, 1fr)); gap: 16px; }}
  figure {{ margin: 0; background: #242424; border: 1px solid #303030;
            border-radius: 4px; padding: 8px; }}
  figure img {{ display: block; width: 100%; height: auto; background: #0a0a0a; }}
  figcaption {{ padding: 6px 2px 2px; font-family: ui-monospace, monospace;
                color: #b8b4a8; font-size: 12px; }}
</style></head><body>
<header>
  <h1>mapgen sweep — {knob}</h1>
  <p>seed={seed} cells={cells} steps={steps} range={lo:.4}..{hi:.4}</p>
</header>
<div class="grid">"#,
        knob = knob.name(),
        seed = seed,
        cells = cells,
        steps = entries.len(),
        lo = lo,
        hi = hi,
    )
    .unwrap();

    for (value, filename) in entries {
        writeln!(
            html,
            r#"  <figure><img src="{filename}" alt="{knob} = {value:.4}"><figcaption>{knob} = {value:.4}</figcaption></figure>"#,
            knob = knob.name(),
        )
        .unwrap();
    }

    html.push_str("</div>\n</body></html>\n");
    html
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_range_exclusive_dotdot() {
        let (lo, hi) = parse_range("0.01..0.10").unwrap();
        assert!((lo - 0.01).abs() < 1e-6);
        assert!((hi - 0.10).abs() < 1e-6);
    }

    #[test]
    fn parse_range_inclusive_dotdoteq_is_equivalent() {
        let (lo, hi) = parse_range("0.01..=0.10").unwrap();
        assert!((lo - 0.01).abs() < 1e-6);
        assert!((hi - 0.10).abs() < 1e-6);
    }

    #[test]
    fn parse_range_rejects_reversed_bounds() {
        let err = parse_range("0.10..0.01").unwrap_err().to_string();
        assert!(err.contains("upper bound"), "got: {err}");
    }

    #[test]
    fn parse_range_rejects_missing_separator() {
        assert!(parse_range("0.05").is_err());
    }

    #[test]
    fn parse_range_rejects_non_numeric() {
        assert!(parse_range("a..b").is_err());
    }

    #[test]
    fn knob_parse_rejects_unknown() {
        assert!(Knob::parse("not_a_knob").is_err());
        assert!(Knob::parse("erosion_rate").is_ok());
    }
}
