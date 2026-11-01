//! Render the aim cursor at eight headings, once with mirroring off and once
//! with it on, plus a contact sheet for each.
//!
//!   cargo run -p kyber-preview --example aim_headings -- OUT_DIR [ACCENT]
//!
//! Uses the same `kyber_core::render::AimCursor` the daemon draws with, so
//! what these frames show is what `kyber aim on` puts on screen — only bigger
//! (4 device px per viewBox unit instead of the ~1.6 of a 24px cursor at 1.6
//! display scale).
use std::path::{Path, PathBuf};

use image::{imageops, RgbaImage};
use kyber_core::render::AimCursor;
use kyber_core::tiny_skia;

const HEADINGS: [u32; 8] = [0, 45, 90, 135, 180, 225, 270, 315];
const TILE: u32 = 480;
/// Preview pixels per viewBox unit (the daemon uses ~1.6).
const PPU: f64 = 4.0;
const SHEET_TILE: u32 = 240;

fn main() -> Result<(), String> {
    let mut args = std::env::args_os().skip(1);
    let out = PathBuf::from(
        args.next()
            .ok_or("usage: aim_headings OUT_DIR [ACCENT]")?,
    );
    let accent = args
        .next()
        .map(|a| a.to_string_lossy().to_string())
        .unwrap_or_else(|| "#7aa2f7".to_string());
    render(&out, &accent)
}

fn render(out: &Path, accent: &str) -> Result<(), String> {
    std::fs::create_dir_all(out).map_err(|e| e.to_string())?;
    let baked = (kyber_core::geometry::VIEW[2] * PPU).round() as u32;
    let art = AimCursor::build(accent, baked)?;
    let mut rows: Vec<RgbaImage> = Vec::new();
    for (mode, mirror) in [("off", false), ("on", true)] {
        let dir = out.join(mode);
        std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
        let mut frames = Vec::new();
        for heading in HEADINGS {
            let mut tile = tiny_skia::Pixmap::new(TILE, TILE).ok_or("tile pixmap")?;
            // An opaque backdrop: the body is drawn with a real "over" blend,
            // exactly as it is over a desktop.
            tile.fill(tiny_skia::Color::from_rgba8(18, 18, 24, 255));
            // A faint floor marker so the tile's centre is readable.
            art.draw(
                &mut tile,
                heading as f64,
                mirror,
                (TILE as f64 / 2.0, TILE as f64 / 2.0),
                1.0,
            );
            let png = kyber_core::render::to_png(tile.data(), TILE);
            let file = dir.join(format!("heading-{heading:03}.png"));
            std::fs::write(&file, &png).map_err(|e| e.to_string())?;
            frames.push(image::load_from_memory(&png).map_err(|e| e.to_string())?.to_rgba8());
            println!("{}", file.display());
        }
        // Contact sheet: the eight headings in one image, left to right.
        let mut sheet = RgbaImage::new(SHEET_TILE * HEADINGS.len() as u32, SHEET_TILE);
        for (i, frame) in frames.iter().enumerate() {
            let small = imageops::resize(
                frame,
                SHEET_TILE,
                SHEET_TILE,
                imageops::FilterType::Lanczos3,
            );
            imageops::overlay(&mut sheet, &small, i as i64 * SHEET_TILE as i64, 0);
        }
        let path = dir.join("contact-sheet.png");
        sheet.save(&path).map_err(|e| e.to_string())?;
        println!("{}", path.display());
        rows.push(sheet);
    }
    // One image with both rows: mirror off on top, mirror on below.
    let mut both = RgbaImage::new(
        SHEET_TILE * HEADINGS.len() as u32,
        SHEET_TILE * rows.len() as u32,
    );
    for (i, row) in rows.iter().enumerate() {
        imageops::overlay(&mut both, row, 0, i as i64 * SHEET_TILE as i64);
    }
    let path = out.join("contact-sheets.png");
    both.save(&path).map_err(|e| e.to_string())?;
    println!("{}", path.display());
    Ok(())
}
