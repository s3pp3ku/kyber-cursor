//! Background luminance around the cursor. Port of
//! `ignite-listener::background_luma`: a 20×20 patch next to the pointer,
//! averaged to one number, taken before the overlay appears.
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

pub fn background_luma(x: f64, y: f64) -> Option<f32> {
    let out = Command::new("grim")
        .args([
            "-g",
            &format!("{},{} 20x20", x as i32 + 2, y as i32 + 2),
            "-t",
            "ppm",
            "-s",
            "0.1",
            "-",
        ])
        .output()
        .ok()?;
    if !out.status.success() {
        return None;
    }
    luminance_of_ppm(&out.stdout)
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
    fn garbage_is_none() {
        assert_eq!(luminance_of_ppm(b"nope"), None);
        assert_eq!(luminance_of_ppm(b"P6\n1 1\n255\n\x01\x02"), None);
    }
}
