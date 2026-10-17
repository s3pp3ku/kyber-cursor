//! Portable fading movement trail. Hosts supply local pointer events and a viewport;
//! no input devices, IPC, desktop APIs, or cursor body are involved here.
use std::collections::VecDeque;

use resvg::tiny_skia::{Color, LineCap, Paint, PathBuilder, Pixmap, Stroke, Transform};

use crate::overlay::{OverlayMeta, TRAIL_ALPHA, TRAIL_S};

const MAX_POINTS: usize = 256;
const PAD: f64 = 12.0;

#[derive(Clone, Copy, Debug, Default)]
#[repr(C)]
pub struct Bounds {
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
}

#[derive(Clone, Copy)]
struct Point {
    x: f64,
    y: f64,
    time: f64,
}

pub struct Trail {
    points: VecDeque<Point>,
    colors: [[f64; 3]; 2],
}

pub struct TrailImage {
    pub bounds: Bounds,
    pub pixels: Pixmap,
}

impl Default for Trail {
    fn default() -> Self {
        Self {
            points: VecDeque::new(),
            colors: OverlayMeta::new("#0088ff").trail_rgb,
        }
    }
}

impl Trail {
    pub fn clear(&mut self) {
        self.points.clear();
    }

    pub fn set_color(&mut self, color: &str) -> bool {
        if !color.starts_with('#') || !crate::color::is_valid_hex(color) {
            return false;
        }
        self.colors = OverlayMeta::new(color).trail_rgb;
        self.clear();
        true
    }

    pub fn sample(&mut self, now: f64, x: f64, y: f64) {
        if ![now, x, y].iter().all(|v| v.is_finite()) || x.abs().max(y.abs()) > 1e7 {
            return;
        }
        self.prune(now);
        if let Some(last) = self.points.back() {
            let distance = (x - last.x).hypot(y - last.y);
            // A clock reset or pointer warp must never draw a stripe across screens.
            if now < last.time || distance > 512.0 {
                self.clear();
            } else if distance < 2.0 || (now - last.time < 1.0 / 240.0 && distance < 8.0) {
                return;
            }
        }
        self.points.push_back(Point { x, y, time: now });
        while self.points.len() > MAX_POINTS {
            self.points.pop_front();
        }
    }

    fn prune(&mut self, now: f64) {
        while self.points.front().is_some_and(|p| now - p.time >= TRAIL_S) {
            self.points.pop_front();
        }
    }

    pub fn point_count(&self) -> usize {
        self.points.len()
    }

    pub fn bounds(&mut self, now: f64) -> Option<Bounds> {
        if !now.is_finite() {
            return None;
        }
        self.prune(now);
        let first = self.points.front()?;
        let (mut x0, mut y0, mut x1, mut y1) = (first.x, first.y, first.x, first.y);
        for p in &self.points {
            x0 = x0.min(p.x);
            y0 = y0.min(p.y);
            x1 = x1.max(p.x);
            y1 = y1.max(p.y);
        }
        Some(Bounds {
            x: (x0 - PAD).floor(),
            y: (y0 - PAD).floor(),
            width: (x1 + PAD).ceil() - (x0 - PAD).floor(),
            height: (y1 + PAD).ceil() - (y0 - PAD).floor(),
        })
    }

    /// Premultiplied RGBA, clipped to a monitor's global logical viewport.
    /// Large strokes reduce raster resolution rather than allocating without bound.
    pub fn render(&mut self, now: f64, viewport: Bounds, light: bool) -> Option<TrailImage> {
        let bounds = self.bounds(now)?;
        if self.points.len() < 2
            || ![viewport.x, viewport.y, viewport.width, viewport.height]
                .iter()
                .all(|v| v.is_finite())
            || viewport.width <= 0.0
            || viewport.height <= 0.0
        {
            return None;
        }
        let x = bounds.x.max(viewport.x).floor();
        let y = bounds.y.max(viewport.y).floor();
        let width = (bounds.x + bounds.width)
            .min(viewport.x + viewport.width)
            .ceil()
            - x;
        let height = (bounds.y + bounds.height)
            .min(viewport.y + viewport.height)
            .ceil()
            - y;
        if width <= 0.0 || height <= 0.0 {
            return None;
        }
        let factor = 1.0_f64
            .min(2048.0 / width)
            .min(2048.0 / height)
            .min((1_048_576.0 / (width * height)).sqrt());
        let mut pixels = Pixmap::new(
            (width * factor).ceil() as u32,
            (height * factor).ceil() as u32,
        )?;
        let rgb = self.colors[usize::from(light)];
        // Two soft outer strokes and a narrower center; age fades both width and
        // opacity, with no white line or extra cursor head inside the trail.
        for (a, b) in self.points.iter().zip(self.points.iter().skip(1)) {
            let life = (1.0 - (now - a.time) / TRAIL_S).clamp(0.0, 1.0) as f32;
            let mut path = PathBuilder::new();
            path.move_to(((a.x - x) * factor) as f32, ((a.y - y) * factor) as f32);
            path.line_to(((b.x - x) * factor) as f32, ((b.y - y) * factor) as f32);
            let path = path.finish()?;
            for (width_scale, strength) in [(2.2, 0.04), (1.4, 0.08), (1.0, 0.22), (0.45, 0.40)] {
                let alpha = TRAIL_ALPHA as f32 * strength * life.powf(1.5);
                let mut paint = Paint::default();
                paint.set_color(Color::from_rgba(
                    rgb[0] as f32,
                    rgb[1] as f32,
                    rgb[2] as f32,
                    alpha,
                )?);
                let stroke = Stroke {
                    width: 5.6 * width_scale * (0.2 + 0.8 * life) * factor as f32,
                    line_cap: LineCap::Round,
                    ..Default::default()
                };
                pixels.stroke_path(&path, &paint, &stroke, Transform::identity(), None);
            }
        }
        Some(TrailImage {
            bounds: Bounds {
                x,
                y,
                width,
                height,
            },
            pixels,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    const VIEW: Bounds = Bounds {
        x: 0.0,
        y: 0.0,
        width: 1000.0,
        height: 800.0,
    };
    fn trace() -> Trail {
        let mut trail = Trail::default();
        for n in 0..40 {
            trail.sample(n as f64 / 100.0, 100.0 + n as f64 * 3.0, 200.0);
        }
        trail
    }
    #[test]
    fn tail_fades_after_release_then_expires_without_more_motion() {
        let mut trail = trace();
        let power = |image: TrailImage| {
            image
                .pixels
                .data()
                .iter()
                .skip(3)
                .step_by(4)
                .map(|a| *a as u64)
                .sum::<u64>()
        };
        let young = power(trail.render(0.40, VIEW, false).unwrap());
        let old = power(trail.render(0.75, VIEW, false).unwrap());
        assert!(young > old * 3 && old > 0);
        assert!(trail.render(1.01, VIEW, false).is_none());
        assert_eq!(trail.point_count(), 0);
    }
    #[test]
    fn stationary_pointer_and_warps_do_not_leave_dots_or_screen_wide_lines() {
        let mut trail = Trail::default();
        for n in 0..100 {
            trail.sample(n as f64 / 100.0, 100.0, 100.0);
        }
        assert!(trail.render(1.0, VIEW, false).is_none());
        trail.sample(1.01, 900.0, 100.0);
        assert_eq!(trail.point_count(), 1);
        trail.sample(f64::NAN, 0.0, 0.0);
        assert_eq!(trail.point_count(), 1);
    }
    #[test]
    fn monitor_clipping_negative_coordinates_and_clear_are_supported() {
        let mut trail = trace();
        assert!(trail
            .render(0.4, Bounds { x: -1000.0, ..VIEW }, false)
            .is_none());
        let image = trail
            .render(
                0.4,
                Bounds {
                    x: 150.0,
                    width: 100.0,
                    ..VIEW
                },
                true,
            )
            .unwrap();
        assert!(image.bounds.x >= 150.0 && image.bounds.x + image.bounds.width <= 250.0);
        assert_eq!(image.pixels.width() as f64, image.bounds.width);
        assert!(image
            .pixels
            .data()
            .chunks_exact(4)
            .all(|p| p[..3].iter().all(|c| *c <= p[3])));
        trail.clear();
        assert!(trail.bounds(0.5).is_none());
        trail.sample(0.5, -100.0, -100.0);
        trail.sample(0.6, -80.0, -80.0);
        let negative = trail
            .render(
                0.6,
                Bounds {
                    x: -200.0,
                    y: -200.0,
                    width: 200.0,
                    height: 200.0,
                },
                false,
            )
            .unwrap();
        assert!(negative.bounds.x < 0.0 && negative.bounds.y < 0.0);
    }
    #[test]
    fn history_and_texture_allocations_are_bounded() {
        let mut trail = Trail::default();
        for n in 0..5000 {
            trail.sample(
                n as f64 / 10000.0,
                (n % 100) as f64 * 3.0,
                (n / 100) as f64 * 3.0,
            );
        }
        assert!(trail.point_count() <= MAX_POINTS);
        let image = trail.render(0.5, VIEW, false).unwrap();
        assert!(image.pixels.width() <= 2048 && image.pixels.height() <= 2048);
        trail.clear();
        // A very large multi-monitor trace must downsample instead of allocating
        // a desktop-sized RGBA buffer inside the compositor.
        for n in 0..250 {
            trail.sample(n as f64 / 1000.0, n as f64 * 200.0, n as f64 * 200.0);
        }
        let huge = trail
            .render(
                0.25,
                Bounds {
                    width: 100_000.0,
                    height: 100_000.0,
                    ..VIEW
                },
                false,
            )
            .unwrap();
        assert!(huge.pixels.width() <= 2048 && huge.pixels.height() <= 2048);
        assert!(huge.pixels.data().len() < 4_300_000);
    }
}
