//! Overlay metadata (`overlay.json`) plus the animation/sound timings
//! shared by every platform backend. Ports the constants in
//! `lib/ignite-listener` and the `meta` dict in `lib/build.py::write_overlay`.
//!
//! The PNG layers themselves are rendered in step 2 (resvg/tiny-skia); this
//! crate owns the schema shared by the Linux runtime components.

use serde::{Deserialize, Serialize};

use crate::color::{darken, haze_for_glow};
use crate::geometry::{self, BEAM_LEN, STUB};

// --- canvas -----------------------------------------------------------
/// Extra viewBox units on every side so the haze can spread past the bitmap.
pub const OVERLAY_MARGIN: f64 = 40.0;
/// Oversampled px per viewBox unit; the daemon scales it down.
pub const OVERLAY_PX_PER_UNIT: u32 = 6;

// --- animation timings (must match ignite-listener) --------------------
pub const IGNITE_S: f64 = 0.15;
pub const RETRACT_S: f64 = 0.11;
pub const FADE_UNITS: f64 = 3.0;
pub const LIGHT_LO: f64 = 0.25;
pub const LIGHT_HI: f64 = 0.60;
pub const HUM_HAZE: f64 = 0.55;
pub const HUM_BEAM: f64 = 0.22;
pub const SOUND_DELAY_MS: u64 = 300;
pub const DOUBLE_CLICK_S: f64 = 0.40;
pub const CLASH_GAP_MS: u64 = 140;
pub const TRAIL_S: f64 = 0.60;
pub const TRAIL_ALPHA: f64 = 0.9;
pub const TRAIL_UNITS: f64 = 7.0;
pub const SURGE_S: f64 = 0.45;

/// One animated beam: output filename, emitter end, tip end.
pub(crate) type BeamSpec = (String, (f64, f64), (f64, f64));

/// One layer entry in `overlay.json`. Exactly the schema `build.py`
/// writes: dark vs. light is determined by *which list* the entry is in
/// (`layers` vs. `light_layers`), not by a flag — the daemons add their
/// own marker at load time (see `ignite-listener::selected_overlay`).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OverlayLayer {
    pub image: String,
    pub kind: String, // "haze" | "beam"
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub from: Option<[f64; 2]>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub to: Option<[f64; 2]>,
}

/// Exact schema of the `overlay.json` the Python build writes, so a Rust
/// renderer stays compatible with already-built themes and vice versa.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OverlayMeta {
    pub px_per_unit: u32,
    pub origin: [f64; 2],
    pub hotspot: [f64; 2],
    pub view_w: f64,
    pub scale: f64,
    pub sizes: Vec<u32>,
    pub layers: Vec<OverlayLayer>,
    pub light_layers: Vec<OverlayLayer>,
    /// Trail colors [dark-bg rgb, light-bg rgb], 0..1.
    pub trail_rgb: [[f64; 3]; 2],
}

impl OverlayMeta {
    pub fn new(glow_hex: &str) -> Self {
        let haze = haze_for_glow(glow_hex);
        let origin = [
            geometry::VIEW[0] - OVERLAY_MARGIN,
            geometry::VIEW[1] - OVERLAY_MARGIN,
        ];
        let tip = geometry::tip();
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

        let mut layers = vec![OverlayLayer {
            image: "overlay-base.png".into(),
            kind: "haze".into(),
            from: None,
            to: None,
        }];
        let mut beams: Vec<BeamSpec> = vec![("overlay-edge.png".into(), eb, ea)];
        for (name, e, d) in [
            ("overlay-beam-r.png", emit_r, beam_r),
            ("overlay-beam-l.png", emit_l, beam_l),
        ] {
            let stb = geometry::add(e, d, STUB);
            beams.push((name.into(), stb, geometry::add(stb, d, BEAM_LEN)));
        }
        for (name, from, to) in &beams {
            layers.push(OverlayLayer {
                image: name.clone(),
                kind: "beam".into(),
                from: Some([round3(from.0), round3(from.1)]),
                to: Some([round3(to.0), round3(to.1)]),
            });
        }
        let light_layers: Vec<OverlayLayer> = layers
            .iter()
            .map(|l| OverlayLayer {
                image: l.image.replacen("overlay-", "overlay-light-", 1),
                kind: l.kind.clone(),
                from: l.from,
                to: l.to,
            })
            .collect();

        Self {
            px_per_unit: OVERLAY_PX_PER_UNIT,
            origin,
            hotspot: [tip.0, tip.1],
            view_w: geometry::VIEW[2],
            scale: geometry::SCALE,
            sizes: geometry::SIZES.to_vec(),
            layers,
            light_layers,
            trail_rgb: [rgb01(&haze), rgb01(&darken(glow_hex, 0.20))],
        }
    }

    pub fn to_json(&self) -> String {
        serde_json::to_string(self).unwrap_or_else(|_| "{}".into())
    }
}

fn round3(v: f64) -> f64 {
    (v * 1000.0).round() / 1000.0
}

fn rgb01(hex: &str) -> [f64; 3] {
    let h = hex.strip_prefix('#').unwrap_or(hex);
    let c = |i: usize| u8::from_str_radix(&h[i..i + 2], 16).unwrap_or(255) as f64 / 255.0;
    [round3(c(0)), round3(c(2)), round3(c(4))]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn meta_schema_matches_python() {
        let m = OverlayMeta::new("#7aa2f7");
        // 1 haze + 1 edge + 2 crossguard beams, each with a light variant.
        assert_eq!(m.layers.len(), 4);
        assert_eq!(m.light_layers.len(), 4);
        assert!(m.layers[1..].iter().all(|l| l.kind == "beam"));
        assert!(m.layers[1..]
            .iter()
            .all(|l| l.from.is_some() && l.to.is_some()));
        assert_eq!(m.sizes, vec![24, 32, 48, 64]);
        let json = m.to_json();
        let v: serde_json::Value = serde_json::from_str(&json).unwrap();
        for key in [
            "px_per_unit",
            "origin",
            "hotspot",
            "layers",
            "light_layers",
            "trail_rgb",
        ] {
            assert!(v.get(key).is_some(), "missing {key}");
        }
    }
}
