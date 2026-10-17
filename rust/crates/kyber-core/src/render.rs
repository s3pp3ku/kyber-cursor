//! SVG → PNG rasterization and bloom. Port of `build.py::rasterize` and the
//! rendering half of `build.py::write_overlay`.
//!
//! The split from `geometry` is load-bearing and mirrors the Python: crisp
//! bodies render untouched; halos render separately and are blurred into
//! real bloom underneath. Overlay layers blur the *alpha only* (blurring
//! RGBA mixes transparent black into the color and leaves a dark fringe
//! over light backgrounds — see the comment in `build.py`).

use image::{GrayImage, RgbaImage};
use resvg::{tiny_skia, usvg};

use crate::color::darken;
use crate::geometry::{self, BEAM_LEN, STUB};
use crate::overlay::{OVERLAY_MARGIN, OVERLAY_PX_PER_UNIT};

/// Parsed vector art, cheap to render at any pixel size.
pub struct VectorArt {
    tree: usvg::Tree,
}

impl VectorArt {
    pub fn parse(svg: &str) -> Result<Self, String> {
        usvg::Tree::from_str(svg, &usvg::Options::default())
            .map(|tree| Self { tree })
            .map_err(|e| e.to_string())
    }

    /// Straight (non-premultiplied) RGBA bytes, `px` × `px`.
    ///
    /// Documents must carry `width == height == px` (see
    /// [`geometry::svg_sized`] / `overlay_doc`); the render is 1:1.
    pub fn render(&self, px: u32) -> Vec<u8> {
        let mut pm = tiny_skia::Pixmap::new(px, px).expect("pixmap size");
        resvg::render(
            &self.tree,
            tiny_skia::Transform::default(),
            &mut pm.as_mut(),
        );
        unpremultiply(pm.data())
    }
}

fn unpremultiply(px: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(px.len());
    for c in px.chunks_exact(4) {
        let a = c[3] as u32;
        if a == 0 {
            out.extend_from_slice(&[0, 0, 0, 0]);
        } else {
            for v in &c[0..3] {
                out.push(((((*v) as u32) * 255 + a / 2) / a).min(255) as u8);
            }
            out.push(c[3]);
        }
    }
    out
}

/// Straight-alpha "over": `src` composited on top of `dst`, in place.
fn over(dst: &mut [u8], src: &[u8]) {
    for (d, s) in dst.chunks_exact_mut(4).zip(src.chunks_exact(4)) {
        let (sa, da) = (s[3] as f32 / 255.0, d[3] as f32 / 255.0);
        let oa = sa + da * (1.0 - sa);
        if oa <= 1e-6 {
            d[0] = 0;
            d[1] = 0;
            d[2] = 0;
            d[3] = 0;
            continue;
        }
        for i in 0..3 {
            d[i] = ((s[i] as f32 * sa + d[i] as f32 * da * (1.0 - sa)) / oa).round() as u8;
        }
        d[3] = (oa * 255.0).round() as u8;
    }
}

/// Blur a whole RGBA image, then scale its alpha. Port of the `bloom`
/// helper in `build.py::rasterize`.
fn bloom_rgba(img: &[u8], px: u32, sigma: f32, strength: f32) -> Vec<u8> {
    let rgba = RgbaImage::from_raw(px, px, img.to_vec()).expect("raw rgba");
    let mut raw = image::imageops::blur(&rgba, sigma).into_raw();
    for c in raw.chunks_exact_mut(4) {
        c[3] = (c[3] as f32 * strength).round().clamp(0.0, 255.0) as u8;
    }
    raw
}

/// One cursor frame: crisp body over blurred halo. Port of
/// `build.py::rasterize`. Returns straight RGBA, `px` × `px`.
pub fn rasterize(body_svg: &str, halo_svg: &str, lit: bool, px: u32) -> Vec<u8> {
    let body = VectorArt::parse(&geometry::svg_sized(body_svg, true, px)).expect("body svg");
    let crisp = body.render(px);
    let mut out = vec![0u8; (px * px * 4) as usize];
    if !halo_svg.is_empty() {
        let halo = VectorArt::parse(&geometry::svg_sized(halo_svg, false, px)).expect("halo svg");
        let g = halo.render(px);
        if lit {
            // Wide outer haze, then a tight inner glow hugging the beams.
            let haze = bloom_rgba(&g, px, px as f32 * 0.10, 2.4);
            let inner = bloom_rgba(&g, px, px as f32 * 0.025, 1.6);
            over(&mut out, &haze);
            over(&mut out, &inner);
        } else {
            over(&mut out, &bloom_rgba(&g, px, px as f32 * 0.03, 0.8));
        }
    }
    over(&mut out, &crisp);
    out
}

/// Convenience: rasterize one cursor kind straight from its builder key.
pub fn rasterize_role(kind: &str, accent: &str, glow: &str, lit: bool, px: u32) -> Vec<u8> {
    let (body, halo) = geometry::build_shape(kind, accent, glow, lit);
    rasterize(&body, &halo, lit, px)
}

pub fn to_png(rgba: &[u8], px: u32) -> Vec<u8> {
    use image::ImageEncoder;
    let mut buf = Vec::new();
    image::codecs::png::PngEncoder::new(&mut buf)
        .write_image(rgba, px, px, image::ExtendedColorType::Rgba8)
        .expect("png encode");
    buf
}

// --- overlay layers ---------------------------------------------------
// Port of build.py::write_overlay's render/glow_of. The overlay canvas is
// bigger than the cursor frame (OVERLAY_MARGIN on every side) so the haze
// can spread past the bitmap edges.

fn overlay_viewbox() -> (f64, f64, f64, f64) {
    let (x, y, w, h) = (
        geometry::VIEW[0],
        geometry::VIEW[1],
        geometry::VIEW[2],
        geometry::VIEW[3],
    );
    let m = OVERLAY_MARGIN;
    (x - m, y - m, w + 2.0 * m, h + 2.0 * m)
}

fn overlay_doc(inner: &str, px: u32) -> String {
    let (x, y, w, h) = overlay_viewbox();
    format!(
        "<svg xmlns=\"http://www.w3.org/2000/svg\" viewBox=\"{x} {y} {w} {h}\" width=\"{px}\" height=\"{px}\">{inner}</svg>"
    )
}

fn render_overlay_doc(inner: &str, px: u32) -> VectorArt {
    VectorArt::parse(&overlay_doc(inner, px)).expect("overlay svg")
}

fn parse_hex_bytes(hex: &str) -> [u8; 3] {
    let h = hex.strip_prefix('#').unwrap_or(hex);
    let c = |i: usize| u8::from_str_radix(&h[i..i + 2], 16).unwrap_or(255);
    [c(0), c(2), c(4)]
}

/// Keep the emitted light separate so native held frames can animate it while
/// the bright core remains completely stable.
pub(crate) struct OverlayPixels {
    pub haze: Vec<u8>,
    pub core: Vec<u8>,
}

impl OverlayPixels {
    pub(crate) fn composite(&self, px: u32, ppu: f64, phase: f64) -> Vec<u8> {
        let mut out = self.haze.clone();
        shimmer_haze(&mut out, px, ppu, phase);
        over(&mut out, &self.core);
        out
    }
}

/// Smooth, periodic spatial variation in just the light. The outer fringe
/// varies more than the dense center, with no pixel-scale noise or core pulse.
pub(crate) fn shimmer_haze(rgba: &mut [u8], px: u32, ppu: f64, phase: f64) {
    let (ox, oy, _, _) = overlay_viewbox();
    let pulse = 0.015 * (2.0 * phase).sin() + 0.010 * (5.0 * phase).sin();
    for (i, pixel) in rgba.chunks_exact_mut(4).enumerate() {
        if pixel[3] == 0 {
            continue;
        }
        let x = (i % px as usize) as f64 / ppu + ox;
        let y = (i / px as usize) as f64 / ppu + oy;
        let edge = 1.0 - pixel[3] as f64 / 255.0;
        let uneven = 0.035 * (0.31 * x + 0.13 * y + phase).sin()
            + 0.025 * (0.17 * x - 0.27 * y - 2.0 * phase).cos();
        pixel[3] = (pixel[3] as f64 * (1.0 + pulse + uneven * edge))
            .round()
            .clamp(0.0, 255.0) as u8;
    }
}

/// Blurred solid-color haze from alpha, plus an untouched core.
fn glow_of(
    halo_inner: &str,
    crisp_inner: &str,
    col_hex: &str,
    k: f64,
    haze_opacity: f64,
    unit: u32,
    px: u32,
) -> OverlayPixels {
    let halo = render_overlay_doc(halo_inner, px);
    let g = halo.render(px);
    // Alpha-only blur: blurring RGBA would mix transparent black into the
    // color and leave a dark fringe over light backgrounds.
    let alpha: Vec<u8> = g.chunks_exact(4).map(|c| c[3]).collect();
    let gray = GrayImage::from_raw(px, px, alpha).expect("gray");
    let [r, gg, b] = parse_hex_bytes(col_hex);
    let mut out = vec![0u8; (px * px * 4) as usize];
    for (radius_units, strength) in [(9.5f32, 1.4f32), (3.6, 1.8), (1.6, 1.0)] {
        let blurred = image::imageops::blur(&gray, radius_units * unit as f32).into_raw();
        // Raise the user's accepted 6/10 glow to 8/10 in the near-blade
        // layers. Preserve the wide haze, blur radii and stable white core.
        let boost = if radius_units < 5.0 {
            (4.0 / 3.0) * (8.0 / 4.5)
        } else {
            1.0
        };
        let s = strength * 1.25 * k as f32 * boost;
        let layer: Vec<u8> = blurred
            .into_iter()
            .flat_map(|a| {
                // Treat accumulated light as optical depth instead of clipping
                // boosted alpha at 255. A clipped blur becomes a flat colored
                // shape with a visible boundary, particularly over white.
                let a = haze_alpha(a, s);
                [r, gg, b, a]
            })
            .collect();
        over(&mut out, &layer);
    }
    // Multiple blurred sources must still read as translucent emitted light.
    // Scaling the final alpha retains the entire smooth falloff (no flat cap).
    for pixel in out.chunks_exact_mut(4) {
        pixel[3] = (pixel[3] as f64 * haze_opacity).round() as u8;
    }
    let core = if crisp_inner.is_empty() {
        vec![0; out.len()]
    } else {
        render_overlay_doc(crisp_inner, px).render(px)
    };
    OverlayPixels { haze: out, core }
}

fn haze_alpha(alpha: u8, strength: f32) -> u8 {
    let depth = alpha as f32 / 255.0 * strength;
    (-(-depth).exp_m1() * 255.0).round().clamp(0.0, 255.0) as u8
}

fn feathered_rim(inner: &str) -> String {
    // The filter uses vector units, so the fade has the same logical width at
    // every export scale. Keep it off the white core and pointer body.
    let (x, y, w, h) = overlay_viewbox();
    format!(
        "<defs><filter id=\"rim-fade\" filterUnits=\"userSpaceOnUse\" x=\"{x}\" y=\"{y}\" width=\"{w}\" height=\"{h}\" color-interpolation-filters=\"sRGB\"><feGaussianBlur stdDeviation=\"1.0\"/></filter></defs><g filter=\"url(#rim-fade)\">{inner}</g>"
    )
}

fn hilt_spill(col: &str, width: f64) -> String {
    let u = geometry::axis_u();
    let m = geometry::head_back();
    geometry::line(
        geometry::add(m, u, 1.0),
        geometry::add(m, u, 19.5),
        col,
        width,
        " opacity=\"0.50\"",
    )
}

fn beam_halo(from: (f64, f64), to: (f64, f64), col: &str, width: f64, main: bool) -> String {
    let mut halo = geometry::line(from, to, col, width, "");
    if main {
        // A wider circular source keeps the outer bloom round at the tip.
        halo.push_str(&format!(
            "<circle cx=\"{}\" cy=\"{}\" r=\"{}\" fill=\"{}\"/>",
            to.0,
            to.1,
            width * 0.65,
            col
        ));
    }
    halo
}

fn upper_blade_fill(col: &str) -> String {
    // The visible beam follows the pointer's left edge. Balance its bloom
    // across the saber axis, in the hollow above the right crossguard.
    let center = geometry::add(geometry::tip(), geometry::axis_n(), 4.0);
    geometry::line(
        geometry::add(center, geometry::axis_u(), 5.0),
        geometry::add(center, geometry::axis_u(), 20.0),
        col,
        18.0,
        " opacity=\"0.24\"",
    )
}

fn overlay_beam(from: (f64, f64), to: (f64, f64), color: &str, width: f64, light: bool) -> String {
    let tint = crate::color::glow_tint(color, light);
    let core = crate::color::blade_core_color(color);
    let contrast_opacity = if light {
        " opacity=\"0.55\""
    } else {
        " opacity=\"0.22\""
    };
    let rim = geometry::line(
        from,
        to,
        &darken(&tint, 0.65),
        3.8 * width,
        contrast_opacity,
    ) + &geometry::line(from, to, &tint, 3.0 * width, " opacity=\"0.85\"");
    // A single continuous treatment for every shade; pale colors no longer
    // lose the white center when crossing a luminance/boost threshold.
    feathered_rim(&rim)
        + &geometry::line(from, to, &core, 2.0 * width, "")
        + &geometry::line(from, to, "#ffffff", 1.1 * width, "")
}

/// Rendered overlay PNGs: `(filename, png_bytes)`. Filenames and order match
/// `build.py::write_overlay` so `OverlayMeta` stays valid.
pub fn render_overlay_layers(glow_hex: &str) -> Vec<(String, Vec<u8>)> {
    render_overlay_layers_with_unit(glow_hex, OVERLAY_PX_PER_UNIT)
}

pub(crate) fn render_overlay_layers_with_unit(glow_hex: &str, unit: u32) -> Vec<(String, Vec<u8>)> {
    let (_, _, w, _) = overlay_viewbox();
    let px = (w * unit as f64).round() as u32;
    render_overlay_parts_with_unit(glow_hex, unit)
        .into_iter()
        .map(|(name, pixels)| (name, to_png(&pixels.composite(px, unit as f64, 0.0), px)))
        .collect()
}

pub(crate) fn render_overlay_parts_with_unit(
    glow_hex: &str,
    unit: u32,
) -> Vec<(String, OverlayPixels)> {
    use crate::color::{beam_width_scale, glow_tint};

    let (_, _, w, _) = overlay_viewbox();
    let px = (w * unit as f64).round() as u32;
    let haze = glow_tint(glow_hex, false);
    let beam_w = 1.0 + (beam_width_scale(glow_hex) - 1.0) * 0.40;

    let (beam_r, beam_l) = {
        let sweep = 18f64.to_radians();
        let (ux, uy) = geometry::axis_u();
        let (nx, ny) = geometry::axis_n();
        let (c, s) = (sweep.cos(), sweep.sin());
        (
            (nx * c + ux * s, ny * c + uy * s),
            (-nx * c + ux * s, -ny * c + uy * s),
        )
    };
    let (emit_r, emit_l) = (geometry::emitter_r(), geometry::emitter_l());
    let (ea, eb) = geometry::edge_line(-0.6, 0.03, 0.80);

    // A round-capped blade halo avoids the spearhead polygon's pointed bloom.
    let base = geometry::line(ea, eb, &haze, 7.2 * beam_w * 1.25, "")
        + &hilt_spill(&haze, 9.0)
        + &upper_blade_fill(&haze);
    let mut beams: Vec<crate::overlay::BeamSpec> = vec![("overlay-edge.png".into(), eb, ea)];
    for (name, e, d) in [
        ("overlay-beam-r.png", emit_r, beam_r),
        ("overlay-beam-l.png", emit_l, beam_l),
    ] {
        let tip = geometry::add(e, d, STUB);
        beams.push((name.into(), tip, geometry::add(tip, d, BEAM_LEN)));
    }

    let mut files = Vec::new();
    files.push((
        "overlay-base.png".to_string(),
        glow_of(&base, "", &haze, 0.192, 0.98, unit, px),
    ));
    for (name, from, to) in &beams {
        let main_w = if name == "overlay-edge.png" {
            1.25
        } else {
            1.0
        };
        let halo = beam_halo(
            *from,
            *to,
            &haze,
            7.2 * beam_w * main_w,
            name == "overlay-edge.png",
        );
        let crisp = overlay_beam(*from, *to, glow_hex, beam_w * main_w, false);
        let halo_gain = if name == "overlay-edge.png" {
            0.32
        } else {
            0.38
        };
        files.push((
            name.clone(),
            glow_of(&halo, &crisp, &haze, halo_gain, 0.98, unit, px),
        ));
    }
    // Light-background set: gentler haze, crisp deeper-colored beam.
    // Lighter, more translucent-looking light lets the same fixed bank show
    // more emission over dark windows while changing white-page contrast less.
    let (lk, lc) = (
        0.32,
        crate::color::lighten(&glow_tint(glow_hex, true), 0.18),
    );
    let lbase = geometry::line(ea, eb, &lc, 7.2 * beam_w * 1.25, "")
        + &hilt_spill(&lc, 9.0)
        + &upper_blade_fill(&lc);
    files.push((
        "overlay-light-base.png".to_string(),
        glow_of(&lbase, "", &lc, lk * 0.6, 0.98, unit, px),
    ));
    for (name, from, to) in &beams {
        let lname = name.replacen("overlay-", "overlay-light-", 1);
        let main_w = if name == "overlay-edge.png" {
            1.25
        } else {
            1.0
        };
        let width = beam_w * main_w;
        let halo = beam_halo(*from, *to, &lc, 7.2 * width, name == "overlay-edge.png");
        let crisp = overlay_beam(*from, *to, glow_hex, width, true);
        let halo_gain = if name == "overlay-edge.png" { lk } else { 0.38 };
        files.push((
            lname,
            glow_of(&halo, &crisp, &lc, halo_gain, 0.98, unit, px),
        ));
    }
    files
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn shimmer_is_periodic_and_leaves_the_core_stable() {
        let px = 64;
        let haze = [50, 120, 160, 90].repeat((px * px) as usize);
        let mut core = vec![0; haze.len()];
        let center = ((px / 2 * px + px / 2) * 4) as usize;
        core[center..center + 4].copy_from_slice(&[255; 4]);
        let parts = OverlayPixels { haze, core };
        let first = parts.composite(px, 0.5, 0.0);
        assert_eq!(first, parts.composite(px, 0.5, std::f64::consts::TAU));
        let mut distinct = false;
        for i in 1..24 {
            let frame = parts.composite(px, 0.5, i as f64 / 24.0 * std::f64::consts::TAU);
            assert_eq!(&frame[center..center + 4], &[255; 4]);
            distinct |= frame != first;
        }
        assert!(distinct, "emitted light should vary across phases");
    }

    #[test]
    fn pale_and_dim_shades_keep_the_same_opaque_white_center() {
        for color in ["#000000", "#156878", "#0316ff", "#d9f5ef", "#ffffff"] {
            for light in [false, true] {
                let beam = overlay_beam((0.0, 24.0), (48.0, 24.0), color, 1.0, light);
                let rgba = render_overlay_doc(&beam, 256).render(256);
                let center = (128 * 256 + 128) * 4;
                assert_eq!(
                    &rgba[center..center + 4],
                    &[255, 255, 255, 255],
                    "{color}, light={light}"
                );
            }
        }
    }

    #[test]
    fn boosted_haze_has_no_clipped_alpha_plateau() {
        for strength in [0.6, 1.2, 2.4, 4.9] {
            let values: Vec<_> = (0..=255).map(|a| haze_alpha(a, strength)).collect();
            assert_eq!(values[0], 0);
            assert!(values.windows(2).all(|p| p[0] <= p[1]));
            assert!(values[255] < 255, "haze clips at strength {strength}");
            assert!(values[128] < values[255]);
        }
    }

    #[test]
    fn colored_rim_fades_without_blurring_white_center() {
        let px = 256;
        let rim = geometry::line((0.0, 24.0), (48.0, 24.0), "#0316ff", 6.0, "");
        let sharp = render_overlay_doc(&rim, px).render(px);
        let soft = render_overlay_doc(&feathered_rim(&rim), px).render(px);
        let alpha = |img: &[u8], y: usize| img[(y * px as usize + 128) * 4 + 3];
        let transitions = |img: &[u8]| {
            (0..px as usize)
                .filter(|&y| (1..254).contains(&alpha(img, y)))
                .count()
        };
        assert!(
            transitions(&soft) > transitions(&sharp) + 4,
            "soft={} sharp={}",
            transitions(&soft),
            transitions(&sharp)
        );
        let center = geometry::line((0.0, 24.0), (48.0, 24.0), "#ffffff", 1.5, "");
        let core = render_overlay_doc(&center, px).render(px);
        let mut combined = soft;
        over(&mut combined, &core);
        for (src, dst) in core.chunks_exact(4).zip(combined.chunks_exact(4)) {
            if src[3] == 255 {
                assert_eq!(dst, &[255, 255, 255, 255]);
            }
        }
    }

    #[test]
    fn renders_opaque_pixels() {
        let art = VectorArt::parse(&geometry::svg(
            "<circle cx=\"27\" cy=\"24\" r=\"10\" fill=\"#ff0000\"/>",
            false,
        ))
        .unwrap();
        let px = art.render(48);
        let opaque = px.chunks_exact(4).filter(|c| c[3] > 128).count();
        assert!(opaque > 200, "circle should cover pixels, got {opaque}");
    }

    #[test]
    fn bad_svg_errors() {
        assert!(VectorArt::parse("not xml at all <<<").is_err());
    }

    #[test]
    fn role_raster_has_body_and_alpha() {
        let rgba = rasterize_role("pointer:link", "#7aa2f7", "#7aa2f7", false, 24);
        assert_eq!(rgba.len(), 24 * 24 * 4);
        assert!(rgba.chunks_exact(4).any(|c| c[3] > 0));
        let png = to_png(&rgba, 24);
        assert_eq!(&png[0..8], &[137, 80, 78, 71, 13, 10, 26, 10]);
    }

    #[test]
    fn overlay_layers_render_small() {
        // unit=1 keeps the big-sigma blurs fast in debug builds.
        let files = render_overlay_layers_with_unit("#7aa2f7", 1);
        assert_eq!(files.len(), 8);
        for (name, png) in &files {
            assert!(name.ends_with(".png"), "{name}");
            assert_eq!(&png[0..8], &[137, 80, 78, 71, 13, 10, 26, 10], "{name}");
        }
    }

    #[test]
    fn dark_overlay_fades_out_before_canvas_edge() {
        for (name, png) in render_overlay_layers_with_unit("#000010", 1) {
            let img = image::load_from_memory(&png).unwrap().to_rgba8();
            let (w, h) = img.dimensions();
            let max_alpha = img
                .enumerate_pixels()
                .filter(|(x, y, _)| *x == 0 || *y == 0 || *x == w - 1 || *y == h - 1)
                .map(|(_, _, p)| p[3])
                .max()
                .unwrap();
            assert!(max_alpha <= 1, "{name} clips at alpha {max_alpha}");
        }
    }
}
