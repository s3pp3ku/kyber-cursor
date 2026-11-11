//! Background luminance around the cursor. Port of
//! `ignite-listener::background_luma`, widened: a square around the tip with
//! the blade body masked out, reduced to its median luminance.
//!
//! Implemented with `grim` (exact geometry, already proven by the Python).
//! xdg-portal screenshots need per-DE origin handling to crop correctly;
//! that's step 4 — the call shape (`Option<f32>`, dark fallback) already
//! matches what a portal backend will return.

use std::process::Command;

fn luminance_of_rgb(r: f32, g: f32, b: f32) -> f32 {
    fn lin(v: f32) -> f32 {
        let v = v / 255.0;
        if v <= 0.04045 {
            v / 12.92
        } else {
            ((v + 0.055) / 1.055).powf(2.4)
        }
    }
    0.2126 * lin(r) + 0.7152 * lin(g) + 0.0722 * lin(b)
}

#[cfg(test)]
pub fn luminance_of_ppm(ppm: &[u8]) -> Option<f32> {
    // P6 <w> <h> 255 <data> — same split as the Python.
    let mut parts = ppm.splitn(4, |b| *b == b'\n');
    let (magic, dims, _max) = (parts.next()?, parts.next()?, parts.next()?);
    let data = parts.next()?;
    if magic != b"P6" || data.len() % 3 != 0 || data.is_empty() {
        return None;
    }
    let _ = dims;
    let n = data.len() / 3;
    let (mut r, mut g, mut b) = (0u64, 0u64, 0u64);
    for px in data.chunks_exact(3) {
        r += px[0] as u64;
        g += px[1] as u64;
        b += px[2] as u64;
    }
    Some(luminance_of_rgb(
        r as f32 / n as f32,
        g as f32 / n as f32,
        b as f32 / n as f32,
    ))
}

/// Logical px of screen looked at around the tip, each way, and how far from
/// the blade's centre line its own pixels reach.
const REACH: f64 = 40.0;
const BODY_LEN: f64 = 60.0;
const BODY_HALF: f64 = 18.0;

/// Background luminance around the tip at (`x`, `y`).
///
/// Kyber's own overlay is on screen when this is taken (the aim body always
/// is), and the old 20×20 patch next to the pointer landed right on the dark
/// blade, so every page read as dark. This looks at a wider square, leaves
/// out the strip where the body lies (tip toward the pommel, down-right),
/// and takes the median of the rest, so a turned blade or a leftover glow
/// still can't outvote the page.
pub fn background_luma(x: f64, y: f64) -> Option<f32> {
    let size = (REACH * 2.0) as i32;
    let (ox, oy) = (x as i32 - REACH as i32, y as i32 - REACH as i32);
    let out = Command::new("grim")
        .args(["-g", &format!("{ox},{oy} {size}x{size}"), "-t", "ppm", "-s", "0.25", "-"])
        .output()
        .ok()?;
    if !out.status.success() {
        return None;
    }
    let (w, h, data) = ppm_pixels(&out.stdout)?;
    let (tx, ty) = (x - ox as f64, y - oy as f64);
    let dir = kyber_core::aim::blade_heading().to_radians() + std::f64::consts::PI;
    masked_median(w, h, data, size as f64, (tx, ty), (dir.cos(), dir.sin()))
}

/// Width, height and RGB bytes of a binary PPM.
fn ppm_pixels(ppm: &[u8]) -> Option<(usize, usize, &[u8])> {
    let mut parts = ppm.splitn(4, |b| *b == b'\n');
    let (magic, dims, _max) = (parts.next()?, parts.next()?, parts.next()?);
    let data = parts.next()?;
    let dims = std::str::from_utf8(dims).ok()?;
    let (w, h) = dims.trim().split_once(' ')?;
    let (w, h): (usize, usize) = (w.parse().ok()?, h.parse().ok()?);
    (magic == b"P6" && w > 0 && h > 0 && data.len() >= w * h * 3).then_some((w, h, data))
}

/// Median luminance of the pixels outside the blade strip. `span` is the
/// patch's size in logical px, `tip` the tip within it, `dir` tip → pommel.
fn masked_median(
    w: usize,
    h: usize,
    data: &[u8],
    span: f64,
    tip: (f64, f64),
    dir: (f64, f64),
) -> Option<f32> {
    let mut lumas = Vec::with_capacity(w * h);
    for j in 0..h {
        for i in 0..w {
            let px = (i as f64 + 0.5) * span / w as f64 - tip.0;
            let py = (j as f64 + 0.5) * span / h as f64 - tip.1;
            let along = (px * dir.0 + py * dir.1).clamp(0.0, BODY_LEN);
            let (dx, dy) = (px - along * dir.0, py - along * dir.1);
            if dx * dx + dy * dy < BODY_HALF * BODY_HALF {
                continue;
            }
            let k = (j * w + i) * 3;
            lumas.push(luminance_of_rgb(
                data[k] as f32,
                data[k + 1] as f32,
                data[k + 2] as f32,
            ));
        }
    }
    if lumas.is_empty() {
        return None;
    }
    lumas.sort_by(f32::total_cmp);
    Some(lumas[lumas.len() / 2])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn white_is_bright_black_is_dark() {
        let white = [b"P6\n2 2\n255\n".to_vec(), vec![255u8; 12]].concat();
        let black = [b"P6\n2 2\n255\n".to_vec(), vec![0u8; 12]].concat();
        assert!(luminance_of_ppm(&white).unwrap() > 0.9);
        assert!(luminance_of_ppm(&black).unwrap() < 0.01);
    }

    #[test]
    fn the_blade_does_not_count_as_background() {
        // White page with a black blade drawn down-right from the tip, as
        // the aim body sits: the old patch read this as dark.
        let (w, h, span) = (32usize, 32usize, 80.0);
        let tip = (40.0, 40.0);
        let a = kyber_core::aim::blade_heading().to_radians() + std::f64::consts::PI;
        let dir = (a.cos(), a.sin());
        let mut data = vec![255u8; w * h * 3];
        for j in 0..h {
            for i in 0..w {
                let px = (i as f64 + 0.5) * span / w as f64 - tip.0;
                let py = (j as f64 + 0.5) * span / h as f64 - tip.1;
                let along = (px * dir.0 + py * dir.1).clamp(0.0, 44.0);
                let (dx, dy) = (px - along * dir.0, py - along * dir.1);
                if dx * dx + dy * dy < 12.0 * 12.0 {
                    data[(j * w + i) * 3..][..3].fill(0);
                }
            }
        }
        let l = masked_median(w, h, &data, span, tip, dir).unwrap();
        assert!(l > 0.9, "{l}");
    }

    #[test]
    fn reads_ppm_size() {
        let img = [b"P6\n2 1\n255\n".to_vec(), vec![9u8; 6]].concat();
        assert_eq!(ppm_pixels(&img).map(|(w, h, _)| (w, h)), Some((2, 1)));
        assert_eq!(ppm_pixels(b"P6\n2 1\n255\n\x01"), None);
    }

    #[test]
    fn garbage_is_none() {
        assert_eq!(luminance_of_ppm(b"nope"), None);
        assert_eq!(luminance_of_ppm(b"P6\n1 1\n255\n\x01\x02"), None);
    }
}
