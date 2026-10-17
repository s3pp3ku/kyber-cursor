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
    std::env::var("XDG_CONFIG_HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|_| icons_dir().parent().unwrap().join(".config"))
        .join("kyber-cursor")
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
    Some(Art {
        dark: load_layers(&folder, &meta.layers, &meta)?,
        light: load_layers(&folder, &meta.light_layers, &meta)?,
        meta,
        key: (meta_path, mtime),
    })
}
