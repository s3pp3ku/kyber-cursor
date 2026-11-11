//! Overlay art loading: `overlay.json` + PNG layers → premultiplied
//! tiny-skia pixmaps, with beam geometry precomputed in overlay-px.
//!
//! The cache key is the theme's `overlay.json` mtime, like the Python's
//! `_cache`. Reloads automatically after `kyber apply` recolors.

use std::path::PathBuf;

use kyber_core::overlay::{OverlayMeta, OVERLAY_PX_PER_UNIT};

fn icons_dir() -> PathBuf {
    std::env::var("HOME")
        .map(|h| PathBuf::from(h).join(".local/share/icons"))
        .unwrap_or_else(|_| PathBuf::from("/tmp"))
}

fn state_file(name: &str) -> PathBuf {
    // The same settings folder the CLI writes. (This used to fall back to
    // `~/.local/share/.config` when XDG_CONFIG_HOME is unset, which is the
    // normal case in a systemd user session: the daemon then never found
    // `active` and silently drew nothing.)
    kyber_core::paths::settings_dir()
        .unwrap_or_else(|_| icons_dir().join("..").join("kyber-cursor"))
        .join(name)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LayerKind {
    Haze,
    Beam,
}

pub struct BeamGeom {
    /// Emitter (from) in overlay-px, unit direction, length in overlay-px.
    pub fx: f32,
    pub fy: f32,
    pub dx: f32,
    pub dy: f32,
    pub len: f32,
}

pub struct Layer {
    pub kind: LayerKind,
    pub px: tiny_skia::Pixmap,
    pub beam: Option<BeamGeom>,
}

pub struct Art {
    pub meta: OverlayMeta,
    pub dark: Vec<Layer>,
    pub light: Vec<Layer>,
    pub key: (PathBuf, std::time::SystemTime),
}

fn straight_to_premult(rgba: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(rgba.len());
    for c in rgba.chunks_exact(4) {
        let a = c[3] as u32;
        out.push((c[0] as u32 * a / 255) as u8);
        out.push((c[1] as u32 * a / 255) as u8);
        out.push((c[2] as u32 * a / 255) as u8);
        out.push(c[3]);
    }
    out
}

fn load_png(folder: &std::path::Path, name: &str) -> Option<tiny_skia::Pixmap> {
    let bytes = std::fs::read(folder.join(name)).ok()?;
    let img = image::load_from_memory(&bytes).ok()?.to_rgba8();
    let (w, h) = (img.width(), img.height());
    let premult = straight_to_premult(img.as_raw());
    tiny_skia::Pixmap::from_vec(
        premult,
        tiny_skia::IntSize::from_wh(w, h).expect("layer size"),
    )
}

fn beam_geom(from: [f64; 2], to: [f64; 2], origin: [f64; 2], ppu: f32) -> BeamGeom {
    let fx = (from[0] - origin[0]) as f32 * ppu;
    let fy = (from[1] - origin[1]) as f32 * ppu;
    let tx = (to[0] - origin[0]) as f32 * ppu;
    let ty = (to[1] - origin[1]) as f32 * ppu;
    let (dx, dy) = (tx - fx, ty - fy);
    let len = (dx * dx + dy * dy).sqrt().max(1.0);
    BeamGeom {
        fx,
        fy,
        dx: dx / len,
        dy: dy / len,
        len,
    }
}

fn load_layers(
    folder: &std::path::Path,
    entries: &[kyber_core::overlay::OverlayLayer],
    meta: &OverlayMeta,
) -> Option<Vec<Layer>> {
    let mut out = Vec::new();
    for e in entries {
        let px = load_png(folder, &e.image)?;
        let (kind, beam) = if e.kind == "beam" {
            let (from, to) = (e.from?, e.to?);
            (
                LayerKind::Beam,
                Some(beam_geom(from, to, meta.origin, meta.px_per_unit as f32)),
            )
        } else {
            (LayerKind::Haze, None)
        };
        out.push(Layer { kind, px, beam });
    }
    Some(out)
}

// ---------------------------------------------------------------- tuning

/// The glow images are built with one halo color per background set: the
/// whole haze layer is that color, and the beam layers are that color with
/// the crisp blade drawn over it. That lets the daemon re-tone the halo for
/// the blade color at load time without rebuilding the theme.
///
/// Dark backgrounds: the build lifts dim colors (blue, purple, deep red) far
/// toward white so they read on black, which turns blue into lavender. Here
/// they keep more of their hue and get more opacity instead.
///
/// Light backgrounds: the build uses a pale tint, which all but vanishes on
/// white. Here the halo is the blade's hue at full saturation, deep enough to
/// stand against a white page, and stronger.
pub mod tune {
    use kyber_core::color::{luminance, parse_hex, to_hex};

    /// Dark set: lowest halo luminance before a dim color is lifted.
    pub const DARK_FLOOR: f64 = 0.15;
    /// Dark set: the build's own floor, which the opacity boost makes up to.
    pub const DARK_BUILD_FLOOR: f64 = 0.28;
    pub const DARK_MAX_GAIN: f32 = 1.45;
    /// Light set: halo luminance on white, and how much stronger it gets.
    pub const LIGHT_LUMA: f64 = 0.30;
    pub const LIGHT_GAIN: f32 = 1.6;

    fn hsv(rgb: [u8; 3]) -> (f64, f64, f64) {
        let [r, g, b] = rgb.map(|c| c as f64 / 255.0);
        let max = r.max(g).max(b);
        let min = r.min(g).min(b);
        let d = max - min;
        let h = if d == 0.0 {
            0.0
        } else if max == r {
            60.0 * (((g - b) / d).rem_euclid(6.0))
        } else if max == g {
            60.0 * ((b - r) / d + 2.0)
        } else {
            60.0 * ((r - g) / d + 4.0)
        };
        (h, if max == 0.0 { 0.0 } else { d / max }, max)
    }

    fn from_hsv(h: f64, s: f64, v: f64) -> [u8; 3] {
        let c = v * s;
        let x = c * (1.0 - ((h / 60.0).rem_euclid(2.0) - 1.0).abs());
        let m = v - c;
        let (r, g, b) = match (h / 60.0) as u32 % 6 {
            0 => (c, x, 0.0),
            1 => (x, c, 0.0),
            2 => (0.0, c, x),
            3 => (0.0, x, c),
            4 => (x, 0.0, c),
            _ => (c, 0.0, x),
        };
        [r, g, b].map(|v| ((v + m) * 255.0).round().clamp(0.0, 255.0) as u8)
    }

    /// `rgb` moved to luminance `target`: scaled toward black when brighter,
    /// mixed toward white when dimmer. Both keep the hue.
    fn at_luma(rgb: [u8; 3], target: f64) -> [u8; 3] {
        let l = luminance(&to_hex(rgb));
        let (mut lo, mut hi) = (0.0f64, 1.0f64);
        let pick = |k: f64| -> [u8; 3] {
            if l > target {
                rgb.map(|c| (c as f64 * (1.0 - k)).round() as u8)
            } else {
                rgb.map(|c| (c as f64 + (255.0 - c as f64) * k).round() as u8)
            }
        };
        for _ in 0..14 {
            let mid = (lo + hi) / 2.0;
            let ml = luminance(&to_hex(pick(mid)));
            if (ml > target) == (l > target) {
                lo = mid;
            } else {
                hi = mid;
            }
        }
        pick(hi)
    }

    /// Halo color and opacity gain for dark backgrounds.
    pub fn dark(hex: &str) -> ([u8; 3], f32) {
        let rgb = parse_hex(hex).unwrap_or([0x7a, 0xa2, 0xf7]);
        let l = luminance(hex);
        if l >= DARK_FLOOR {
            // Bright enough already: the build's own toning is fine.
            return (rgb, 1.0);
        }
        let out = at_luma(rgb, DARK_FLOOR);
        let gain = ((DARK_BUILD_FLOOR / DARK_FLOOR).sqrt() as f32).min(DARK_MAX_GAIN);
        (out, gain)
    }

    /// Halo color and opacity gain for light backgrounds.
    pub fn light(hex: &str) -> ([u8; 3], f32) {
        let rgb = parse_hex(hex).unwrap_or([0x7a, 0xa2, 0xf7]);
        let (h, s, _) = hsv(rgb);
        // Neutral blades (white, gray) stay neutral: their halo on white is
        // a soft gray edge, the only contrast a white light can have there.
        // Darkened yellow is olive; a deep gold reads as the same blade.
        let h = if (40.0..75.0).contains(&h) { 40.0 + (h - 40.0) * 0.3 } else { h };
        let vivid = if s < 0.12 { rgb } else { from_hsv(h, s.max(0.85), 1.0) };
        (at_luma(vivid, LIGHT_LUMA), LIGHT_GAIN)
    }
}

/// The one halo color the build gave this set: the haze layer is entirely it.
fn halo_tint(layers: &[Layer]) -> Option<[u8; 3]> {
    let haze = layers.iter().find(|l| l.kind == LayerKind::Haze)?;
    let p = haze.px.pixels().iter().max_by_key(|p| p.alpha())?;
    let c = p.demultiply();
    (c.alpha() > 0).then(|| [c.red(), c.green(), c.blue()])
}

/// Re-tone every pixel that is pure halo (`from`) to `to`, opacity × `gain`.
/// Pixels with the crisp blade in them are a different color and are left
/// exactly as built.
fn retint(layers: &mut [Layer], from: [u8; 3], to: [u8; 3], gain: f32) {
    let near = |c: tiny_skia::ColorU8| {
        (c.red() as i32 - from[0] as i32).abs() <= 3
            && (c.green() as i32 - from[1] as i32).abs() <= 3
            && (c.blue() as i32 - from[2] as i32).abs() <= 3
    };
    for layer in layers {
        for p in layer.px.pixels_mut() {
            if p.alpha() == 0 {
                continue;
            }
            let c = p.demultiply();
            // Very faint pixels round their color badly; at that strength
            // they can only be halo.
            if !near(c) && c.alpha() > 24 {
                continue;
            }
            let a = (c.alpha() as f32 * gain).round().clamp(0.0, 255.0) as u8;
            *p = tiny_skia::ColorU8::from_rgba(to[0], to[1], to[2], a).premultiply();
        }
    }
}

/// Re-tone both halo sets for the blade color `hex`.
pub fn tune_art(art: &mut Art, hex: &str) {
    if let Some(from) = halo_tint(&art.dark) {
        let (to, gain) = tune::dark(hex);
        if gain != 1.0 {
            retint(&mut art.dark, from, to, gain);
        }
    }
    if let Some(from) = halo_tint(&art.light) {
        let (to, gain) = tune::light(hex);
        retint(&mut art.light, from, to, gain);
    }
}

/// Load the selected theme's overlay art, or None (Kyber off / not built).
/// `kyber apply` creates `STATE/active`; `kyber off` removes it, so the
/// glow never shows over a different cursor theme.
pub fn selected_art() -> Option<Art> {
    let folder = icons_dir().join("Omarchy-Kyber-Ignited");
    let meta_path = folder.join("overlay.json");
    if !state_file("active").is_file() || !meta_path.is_file() {
        return None;
    }
    let mtime = std::fs::metadata(&meta_path)
        .and_then(|m| m.modified())
        .ok()?;
    let meta: OverlayMeta =
        serde_json::from_str(&std::fs::read_to_string(&meta_path).ok()?).ok()?;
    // Sanity: only the schema this daemon understands.
    if meta.px_per_unit != OVERLAY_PX_PER_UNIT || meta.sizes.is_empty() {
        return None;
    }
    let mut art = Art {
        dark: load_layers(&folder, &meta.layers, &meta)?,
        light: load_layers(&folder, &meta.light_layers, &meta)?,
        meta,
        key: (meta_path, mtime),
    };
    if let Ok(hex) = std::fs::read_to_string(folder.join(".accent")) {
        let hex = hex.trim();
        if kyber_core::color::parse_hex(hex).is_some() {
            tune_art(&mut art, hex);
        }
    }
    Some(art)
}

/// Overlay art for any blade color, built in memory-backed temp files the
/// same way `kyber apply` builds the theme. For the offline glow lab.
#[cfg(test)]
pub(crate) fn art_for_color(hex: &str) -> Art {
    let dir = std::env::temp_dir().join(format!(
        "kyber-lab-{}-{}",
        std::process::id(),
        hex.trim_start_matches('#')
    ));
    std::fs::create_dir_all(&dir).unwrap();
    for (name, png) in kyber_core::render::render_overlay_layers(hex) {
        std::fs::write(dir.join(name), png).unwrap();
    }
    let meta = OverlayMeta::new(hex);
    let art = Art {
        dark: load_layers(&dir, &meta.layers, &meta).expect("dark layers"),
        light: load_layers(&dir, &meta.light_layers, &meta).expect("light layers"),
        meta,
        key: (dir.clone(), std::time::UNIX_EPOCH),
    };
    let _ = std::fs::remove_dir_all(&dir);
    let mut art = art;
    if std::env::var("KYBER_LAB_BUILT").is_err() {
        tune_art(&mut art, hex);
    }
    art
}
