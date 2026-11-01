//! Shared, testable preparation for the window-scoped native cursor trial.
use std::path::{Component, Path};

use image::{imageops, RgbaImage};

#[cfg(target_os = "windows")]
pub mod audio;

pub const IGNITE_MS: u64 = 150;
pub const RELEASE_MS: u64 = 110;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Layout {
    pub width: u32,
    pub height: u32,
    pub hotspot: [u32; 2],
    pub scale: u32,
    pub steps: usize,
    pub held: usize,
    pub hold_ms: u64,
}

impl Layout {
    pub fn parse(text: &str) -> Result<Self, String> {
        let mut lines = text.lines();
        let first: Vec<_> = lines.next().unwrap_or("").split_whitespace().collect();
        let ["KYBER_NATIVE_V1", w, h, x, y, scale, steps] = first.as_slice() else {
            return Err("Unrecognized cursor frame manifest".into());
        };
        let number = |value: &str| value.parse::<u32>().map_err(|e| e.to_string());
        let mut layout = Self {
            width: number(w)?,
            height: number(h)?,
            hotspot: [number(x)?, number(y)?],
            scale: number(scale)?,
            steps: number(steps)? as usize,
            held: 0,
            hold_ms: 80,
        };
        if let Some(line) = lines.next() {
            let fields: Vec<_> = line.split_whitespace().collect();
            let ["KYBER_HALO_LOOP_V1", held, interval] = fields.as_slice() else {
                return Err("Unrecognized held animation manifest".into());
            };
            layout.held = number(held)? as usize;
            layout.hold_ms = u64::from(number(interval)?);
            if !(1..=64).contains(&layout.held) || !(10..=1000).contains(&layout.hold_ms) {
                return Err("Held animation is outside the trial limits".into());
            }
        }
        if lines.any(|line| !line.trim().is_empty())
            || !(1..=256).contains(&layout.width)
            || !(1..=256).contains(&layout.height)
            || layout.hotspot[0] >= layout.width
            || layout.hotspot[1] >= layout.height
            || !(1..=4).contains(&layout.scale)
            || !(2..=64).contains(&layout.steps)
        {
            return Err("Cursor layout is outside the trial limits".into());
        }
        Ok(layout)
    }

    pub fn file_names(self, bank: &str) -> Vec<String> {
        (0..self.steps)
            .map(|n| format!("{bank}-{n:02}.png"))
            .chain((0..self.held).map(|n| format!("{bank}-hold-{n:02}.png")))
            .collect()
    }

    pub fn dimensions(self, dpi: u32) -> Result<([u32; 2], [u32; 2]), String> {
        if !(48..=384).contains(&dpi) {
            return Err("Unsupported display DPI".into());
        }
        let divisor = u64::from(self.scale) * 96;
        let scaled =
            |value: u32| ((u64::from(value) * u64::from(dpi) + divisor / 2) / divisor) as u32;
        let size = [scaled(self.width), scaled(self.height)];
        if size.iter().any(|n| !(1..=256).contains(n)) {
            return Err("This display scale exceeds the trial's 256-pixel cursor limit".into());
        }
        // Round the hotspot by the actual rounded image dimensions.
        let hotspot = [
            ((u64::from(self.hotspot[0]) * u64::from(size[0]) + u64::from(self.width) / 2)
                / u64::from(self.width)) as u32,
            ((u64::from(self.hotspot[1]) * u64::from(size[1]) + u64::from(self.height) / 2)
                / u64::from(self.height)) as u32,
        ];
        Ok((size, hotspot))
    }
}

pub fn palette(root: &Path) -> Result<Vec<(String, String)>, String> {
    let text = std::fs::read_to_string(root.join("colors.json"))
        .map_err(|e| format!("Cannot read colors.json: {e}"))?;
    let colors: Vec<(String, String)> = serde_json::from_str(&text).map_err(|e| e.to_string())?;
    if colors.is_empty()
        || colors.len() > 128
        || colors.iter().any(|(name, hex)| {
            let mut parts = Path::new(name).components();
            !matches!(parts.next(), Some(Component::Normal(_)))
                || parts.next().is_some()
                || name.contains(['/', '\\', ':'])
                || !kyber_core::color::is_valid_hex(hex)
                || !hex.starts_with('#')
        })
    {
        return Err("Invalid color palette".into());
    }
    Ok(colors)
}

pub fn load_frame(path: &Path, layout: Layout) -> Result<RgbaImage, String> {
    let error = |e| format!("{}: {e}", path.display());
    let dimensions = image::image_dimensions(path).map_err(|e| error(e.to_string()))?;
    if dimensions != (layout.width, layout.height) {
        return Err(error("Unexpected frame dimensions".into()));
    }
    image::open(path)
        .map(|i| i.into_rgba8())
        .map_err(|e| error(e.to_string()))
}

pub struct CursorPixels {
    pub size: [u32; 2],
    pub hotspot: [u32; 2],
    pub bgra: Vec<u8>,
    pub mask_stride: usize,
    pub mask: Vec<u8>,
}

impl CursorPixels {
    pub fn new(image: &RgbaImage, layout: Layout, dpi: u32) -> Result<Self, String> {
        if image.dimensions() != (layout.width, layout.height) {
            return Err("Unexpected frame dimensions".into());
        }
        let (size, hotspot) = layout.dimensions(dpi)?;
        // Resample premultiplied color, not straight RGBA: hidden RGB in a
        // transparent pixel must not darken or discolor the feathered edge.
        // Float intermediates avoid quantizing the faint halo before resizing.
        let premultiplied = image::Rgba32FImage::from_fn(image.width(), image.height(), |x, y| {
            let [r, g, b, a] = image.get_pixel(x, y).0;
            let alpha = f32::from(a) / 255.0;
            image::Rgba([
                f32::from(r) / 255.0 * alpha,
                f32::from(g) / 255.0 * alpha,
                f32::from(b) / 255.0 * alpha,
                alpha,
            ])
        });
        let image = imageops::resize(
            &premultiplied,
            size[0],
            size[1],
            imageops::FilterType::Lanczos3,
        );
        let mask_stride = size[0].div_ceil(32) as usize * 4;
        let mut mask = vec![0; mask_stride * size[1] as usize];
        let mut bgra = Vec::with_capacity(image.as_raw().len());
        for (x, y, pixel) in image.enumerate_pixels() {
            let [r, g, b, a] = pixel.0;
            let alpha = a.clamp(0.0, 1.0);
            let channel = |c: f32| (c.clamp(0.0, alpha) * 255.0).round() as u8;
            let alpha = (alpha * 255.0).round() as u8;
            bgra.extend_from_slice(&[channel(b), channel(g), channel(r), alpha]);
            if alpha == 0 {
                mask[y as usize * mask_stride + x as usize / 8] |= 0x80 >> (x % 8);
            }
        }
        Ok(Self {
            size,
            hotspot,
            bgra,
            mask_stride,
            mask,
        })
    }
}

/// Timestamp-driven animation. Reversals retain their current ignition level;
/// cancellation (leaving/deactivation) returns directly to idle.
#[derive(Default)]
pub struct Animation {
    pressed: bool,
    changed: u64,
    from: f64,
}

impl Animation {
    pub fn level(&self, now: u64) -> f64 {
        let elapsed = now.saturating_sub(self.changed) as f64;
        if self.pressed {
            (self.from + elapsed / IGNITE_MS as f64).min(1.0)
        } else {
            (self.from - elapsed / RELEASE_MS as f64).max(0.0)
        }
    }
    pub fn set_pressed(&mut self, pressed: bool, now: u64) {
        if pressed != self.pressed {
            self.from = self.level(now);
            self.changed = now;
            self.pressed = pressed;
        }
    }
    pub fn cancel(&mut self) {
        *self = Self::default();
    }
    pub fn running(&self, now: u64) -> bool {
        self.pressed || self.level(now) > 0.0
    }
    pub fn frame(&self, now: u64, layout: Layout) -> usize {
        let level = self.level(now);
        if self.pressed && level >= 1.0 && layout.held > 0 {
            let full_at = self.changed + ((1.0 - self.from) * IGNITE_MS as f64).ceil() as u64;
            layout.steps + (now.saturating_sub(full_at) / layout.hold_ms) as usize % layout.held
        } else {
            (level * (layout.steps - 1) as f64).round() as usize
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn layout() -> Layout {
        Layout::parse("KYBER_NATIVE_V1 206 206 72 72 2 17\nKYBER_HALO_LOOP_V1 24 80\n").unwrap()
    }

    #[test]
    fn manifest_rejects_invalid_hotspots_and_animation_limits() {
        for bad in [
            "KYBER_NATIVE_V1 206 206 206 72 2 17",
            "KYBER_NATIVE_V1 0 206 72 72 2 17",
            "KYBER_NATIVE_V1 206 206 72 72 0 17",
            "KYBER_NATIVE_V1 206 206 72 72 2 1",
            "KYBER_NATIVE_V1 206 206 72 72 2 17\nKYBER_HALO_LOOP_V1 999 80",
            "KYBER_NATIVE_V1 206 206 72 72 2 17\nunknown",
        ] {
            assert!(Layout::parse(bad).is_err(), "accepted {bad}");
        }
        assert_eq!(layout().file_names("light").len(), 41);
        assert_eq!(
            Layout::parse("KYBER_NATIVE_V1 206 206 72 72 2 17\n")
                .unwrap()
                .held,
            0
        );
    }

    #[test]
    fn dpi_scaling_preserves_the_reference_size_and_hotspot() {
        assert_eq!(layout().dimensions(96).unwrap(), ([103, 103], [36, 36]));
        assert_eq!(layout().dimensions(120).unwrap(), ([129, 129], [45, 45]));
        assert_eq!(layout().dimensions(192).unwrap(), ([206, 206], [72, 72]));
        assert!(layout().dimensions(384).is_err());
    }

    #[test]
    fn windows_pixels_use_premultiplied_bgra_and_transparent_and_mask() {
        let l = Layout {
            width: 3,
            height: 1,
            hotspot: [1, 0],
            scale: 1,
            ..layout()
        };
        let image = RgbaImage::from_raw(
            3,
            1,
            vec![200, 100, 50, 128, 255, 60, 20, 0, 20, 40, 60, 255],
        )
        .unwrap();
        let pixels = CursorPixels::new(&image, l, 96).unwrap();
        assert_eq!(pixels.bgra, [25, 50, 100, 128, 0, 0, 0, 0, 60, 40, 20, 255]);
        assert_eq!(pixels.mask_stride, 4);
        assert_eq!(pixels.mask, [0x40, 0, 0, 0]);
    }

    #[test]
    fn transparent_rgb_cannot_discolor_the_resized_halo() {
        let l = Layout {
            width: 2,
            height: 2,
            hotspot: [0, 0],
            scale: 2,
            ..layout()
        };
        let mut image = RgbaImage::from_pixel(2, 2, image::Rgba([0, 255, 0, 0]));
        image.put_pixel(0, 0, image::Rgba([255, 0, 0, 255]));
        image.put_pixel(0, 1, image::Rgba([255, 0, 0, 255]));
        let pixels = CursorPixels::new(&image, l, 96).unwrap();
        assert_eq!(pixels.size, [1, 1]);
        assert_eq!(pixels.bgra[0], 0);
        assert_eq!(pixels.bgra[1], 0, "transparent green bleeds into the halo");
        assert_eq!(
            pixels.bgra[2], pixels.bgra[3],
            "opaque red darkens at the edge"
        );
        assert!((127..=128).contains(&pixels.bgra[3]));
    }

    #[test]
    fn transparency_mask_rows_are_dword_aligned() {
        let l = Layout {
            width: 33,
            height: 2,
            hotspot: [0, 0],
            scale: 1,
            ..layout()
        };
        let mut image = RgbaImage::new(33, 2);
        image.put_pixel(32, 1, image::Rgba([255; 4]));
        let pixels = CursorPixels::new(&image, l, 96).unwrap();
        assert_eq!(pixels.mask_stride, 8);
        assert_eq!(
            pixels.mask,
            [255, 255, 255, 255, 128, 0, 0, 0, 255, 255, 255, 255, 0, 0, 0, 0]
        );
    }

    #[test]
    fn repeated_holds_wrap_shimmer_and_release_fully() {
        let l = layout();
        let mut a = Animation::default();
        for start in [0, 4000, 8000] {
            a.set_pressed(true, start);
            assert_eq!(a.frame(start, l), 0);
            assert_eq!(a.frame(start + 75, l), 8);
            assert_eq!(a.frame(start + 150, l), 17);
            assert_eq!(a.frame(start + 150 + 23 * 80, l), 40);
            assert_eq!(a.frame(start + 150 + 24 * 80, l), 17);
            a.set_pressed(false, start + 2500);
            assert_eq!(a.frame(start + 2500, l), 16);
            assert!(a.running(start + 2550));
            assert_eq!(a.frame(start + 2610, l), 0);
            assert!(!a.running(start + 2610));
        }
    }

    #[test]
    fn rapid_reversal_is_continuous_and_cancel_clears_a_stuck_hold() {
        let l = layout();
        let mut a = Animation::default();
        a.set_pressed(true, 0);
        a.set_pressed(true, 50);
        assert_eq!(a.level(75), 0.5);
        a.set_pressed(false, 75);
        assert_eq!(a.level(75), 0.5);
        a.set_pressed(true, 97);
        assert!((a.level(97) - 0.3).abs() < 1e-10);
        a.cancel();
        assert_eq!(a.frame(10000, l), 0);
        assert!(!a.running(10000));
    }
}
