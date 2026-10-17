//! Experimental single-image cursor frames shared by native trials.
//! Exporting them does not install or change the running cursor.
use std::path::Path;

use image::{imageops, RgbaImage};

use crate::{geometry, overlay::OverlayMeta, render};

const STEPS: u32 = 16;
const HOLD_FRAMES: u32 = 24;
const HOLD_INTERVAL_MS: u32 = 80;

/// Export both background variants with a fixed hotspot and transparent padding.
/// `nominal` is the Xcursor size loaded at `scale` (48 at 2x for a 24px cursor).
pub fn export(out: &Path, color: &str, nominal: u32, scale: u32) -> Result<(), String> {
    if !crate::color::is_valid_hex(color)
        || !color.starts_with('#')
        || !(1..=4).contains(&scale)
        || !geometry::SIZES.contains(&nominal)
    {
        return Err("invalid color, size, or scale".into());
    }
    std::fs::create_dir_all(out).map_err(|e| e.to_string())?;
    let meta = OverlayMeta::new(color);
    let body_px = (nominal as f64 * geometry::SCALE).round() as u32;
    let ppu = body_px as f64 / geometry::VIEW[2];
    let side = ((geometry::VIEW[2] + 2.0 * crate::overlay::OVERLAY_MARGIN) * ppu).ceil() as u32;
    if side > 256 {
        return Err("native cursor exceeds the 256px trial limit".into());
    }
    let hx = ((meta.hotspot[0] - meta.origin[0]) * ppu).round() as u32;
    let hy = ((meta.hotspot[1] - meta.origin[1]) * ppu).round() as u32;
    let body = RgbaImage::from_raw(
        body_px,
        body_px,
        render::rasterize_role("pointer", color, color, false, body_px),
    )
    .unwrap();
    let body_offset = (crate::overlay::OVERLAY_MARGIN * ppu).round() as i64;
    // Render at 2 px/unit, then resample once, never in the compositor's input path.
    let files = render::render_overlay_parts_with_unit(color, 2);
    for (bank, specs) in [("dark", &meta.layers), ("light", &meta.light_layers)] {
        let mut layers = Vec::new();
        for spec in specs {
            let pixels = &files
                .iter()
                .find(|(name, _)| *name == spec.image)
                .unwrap()
                .1;
            // Preserve the exact vector-to-pixel ratio, including the fractional last pixel.
            let inner_side =
                ((geometry::VIEW[2] + 2.0 * crate::overlay::OVERLAY_MARGIN) * ppu).round() as u32;
            let resample = |bytes: &[u8]| {
                let source_side =
                    ((geometry::VIEW[2] + 2.0 * crate::overlay::OVERLAY_MARGIN) * 2.0) as u32;
                let source = RgbaImage::from_raw(source_side, source_side, bytes.to_vec()).unwrap();
                let image = imageops::resize(
                    &source,
                    inner_side,
                    inner_side,
                    imageops::FilterType::Lanczos3,
                );
                let mut resampled = RgbaImage::new(side, side);
                imageops::overlay(&mut resampled, &image, 0, 0);
                resampled.into_raw()
            };
            layers.push((
                spec,
                render::OverlayPixels {
                    haze: resample(&pixels.haze),
                    core: resample(&pixels.core),
                },
            ));
        }
        for index in 0..=STEPS + HOLD_FRAMES {
            let step = index.min(STEPS);
            let e = step as f64 / STEPS as f64;
            let held = index.checked_sub(STEPS + 1);
            let phase = held.unwrap_or(0) as f64 / HOLD_FRAMES as f64 * std::f64::consts::TAU;
            let mut frame = RgbaImage::new(side, side);
            for (spec, pixels) in &layers {
                let mut layer =
                    RgbaImage::from_raw(side, side, pixels.composite(side, ppu, phase)).unwrap();
                for (x, y, pixel) in layer.enumerate_pixels_mut() {
                    let opacity = if let (Some(from), Some(to)) = (spec.from, spec.to) {
                        let dx = to[0] - from[0];
                        let dy = to[1] - from[1];
                        let len = dx.hypot(dy);
                        let qx = x as f64 / ppu + meta.origin[0] - from[0];
                        let qy = y as f64 / ppu + meta.origin[1] - from[1];
                        let along = (qx * dx + qy * dy) / len;
                        let mask = if step == STEPS {
                            1.0
                        } else {
                            ((e * len + crate::overlay::FADE_UNITS - along)
                                / crate::overlay::FADE_UNITS)
                                .clamp(0.0, 1.0)
                        };
                        mask * (e * 3.0).min(1.0)
                    } else {
                        e
                    };
                    pixel[3] = (pixel[3] as f64 * opacity).round() as u8;
                }
                imageops::overlay(&mut frame, &layer, 0, 0);
            }
            imageops::overlay(&mut frame, &body, body_offset, body_offset);
            let filename = match held {
                Some(phase) => format!("{bank}-hold-{phase:02}.png"),
                None => format!("{bank}-{step:02}.png"),
            };
            frame.save(out.join(filename)).map_err(|e| e.to_string())?;
        }
    }
    // Review sheet: four stages against each background before live loading.
    let mut preview = RgbaImage::new(side * 4, side * 2);
    for (row, bank, bg) in [
        (0, "dark", [18, 18, 22, 255]),
        (1, "light", [245, 245, 245, 255]),
    ] {
        for (column, step) in [0, 4, 8, 16].iter().enumerate() {
            let mut tile = RgbaImage::from_pixel(side, side, image::Rgba(bg));
            let frame = image::open(out.join(format!("{bank}-{step:02}.png")))
                .map_err(|e| e.to_string())?
                .to_rgba8();
            imageops::overlay(&mut tile, &frame, 0, 0);
            imageops::overlay(
                &mut preview,
                &tile,
                column as i64 * side as i64,
                row * side as i64,
            );
        }
    }
    preview
        .save(out.join("preview.png"))
        .map_err(|e| e.to_string())?;
    std::fs::write(
        out.join("frames.txt"),
        format!(
            "KYBER_NATIVE_V1 {side} {side} {hx} {hy} {scale} {}\nKYBER_HALO_LOOP_V1 {HOLD_FRAMES} {HOLD_INTERVAL_MS}\n",
            STEPS + 1,
        ),
    )
    .map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    #[test]
    fn invalid_native_export_does_not_create_files() {
        assert!(super::export(std::path::Path::new("/unused"), "bad", 48, 2).is_err());
    }

    #[test]
    fn native_frames_keep_padding_hotspot_and_idle_consistent() {
        let stamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let dir =
            std::env::temp_dir().join(format!("kyber-native-test-{}-{stamp}", std::process::id()));
        super::export(&dir, "#f51705", 48, 2).unwrap();
        let manifest = std::fs::read_to_string(dir.join("frames.txt")).unwrap();
        assert_eq!(
            manifest,
            "KYBER_NATIVE_V1 206 206 72 72 2 17\nKYBER_HALO_LOOP_V1 24 80\n"
        );
        let idle = image::open(dir.join("dark-00.png")).unwrap().to_rgba8();
        assert_eq!(
            idle,
            image::open(dir.join("light-00.png")).unwrap().to_rgba8()
        );
        let idle_alpha: u64 = idle.pixels().map(|p| p[3] as u64).sum();
        for bank in ["dark", "light"] {
            for step in 0..=16 {
                let img = image::open(dir.join(format!("{bank}-{step:02}.png")))
                    .unwrap()
                    .to_rgba8();
                assert_eq!(img.dimensions(), (206, 206));
                let edge_alpha = img
                    .enumerate_pixels()
                    .filter(|(x, y, _)| *x == 0 || *y == 0 || *x == 205 || *y == 205)
                    .map(|(_, _, p)| p[3])
                    .max()
                    .unwrap();
                assert!(edge_alpha <= 1, "{bank}-{step} clips at {edge_alpha}");
                if step == 16 {
                    assert!(img.pixels().map(|p| p[3] as u64).sum::<u64>() > idle_alpha * 2);
                }
            }
            let full = image::open(dir.join(format!("{bank}-16.png")))
                .unwrap()
                .to_rgba8();
            let mut held = Vec::new();
            for phase in 0..super::HOLD_FRAMES {
                let img = image::open(dir.join(format!("{bank}-hold-{phase:02}.png")))
                    .unwrap()
                    .to_rgba8();
                assert_eq!(img.dimensions(), full.dimensions());
                for (x, y, p) in img.enumerate_pixels() {
                    if x == 0 || y == 0 || x == 205 || y == 205 {
                        assert!(p[3] <= 1, "{bank} held-{phase} clips");
                    }
                    let body = idle.get_pixel(x, y);
                    if body[3] == 255 {
                        assert_eq!(p, body, "body moves during shimmer");
                    }
                    let center = full.get_pixel(x, y);
                    if center.0 == [255; 4] {
                        assert!(p.0[..3].iter().all(|v| *v >= 254));
                        assert_eq!(p[3], 255);
                    }
                }
                held.push(img);
            }
            assert_eq!(held[0], full, "ignition must join the held loop seamlessly");
            assert!(
                held.iter().skip(1).any(|frame| *frame != full),
                "halo never shimmers"
            );
            let delta = |a: &image::RgbaImage, b: &image::RgbaImage| -> u64 {
                a.as_raw()
                    .iter()
                    .zip(b.as_raw())
                    .map(|(x, y)| x.abs_diff(*y) as u64)
                    .sum()
            };
            let max_adjacent = held.windows(2).map(|w| delta(&w[0], &w[1])).max().unwrap();
            assert!(
                delta(held.last().unwrap(), &held[0]) <= max_adjacent * 3 / 2,
                "visible jump at shimmer loop seam"
            );
        }
        std::fs::remove_dir_all(dir).unwrap();
    }
}
