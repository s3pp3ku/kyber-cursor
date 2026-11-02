//! Portable fading movement trail. Hosts supply local pointer events and a viewport;
//! no input devices, IPC, desktop APIs, or cursor body are involved here.
//!
//! The Comet and Sparks looks are adapted from Plume
//! (https://github.com/qempexe/plume), Copyright (c) 2026 Andrei C. Popa,
//! MIT License. See THIRD_PARTY_NOTICES.md at the repository root.
use std::collections::VecDeque;

use resvg::tiny_skia::{Color, LineCap, Paint, Path, PathBuilder, Pixmap, Stroke, Transform};

use crate::config::SmallRng;
pub use crate::config::{Particles, TrailLook, TrailStyle};
use crate::overlay::{OverlayMeta, TRAIL_ALPHA, TRAIL_S};

const MAX_POINTS: usize = 256;
const PAD: f64 = 12.0;
/// The thickness the path looks are drawn at, in logical px (Plume's `size`).
const SIZE: f64 = 6.0;
const MAX_SPARKS: usize = 180;
/// Sparks fall. Logical px per second (squared for gravity).
const GRAVITY: f64 = 160.0;
/// The share of sparks in hot-metal colors rather than the glow color.
const EMBER_SHARE: f64 = 0.75;
/// A hot-metal spark's color as it dies, hottest first: orange, red-orange,
/// deep red. Dark enough to read over light backgrounds too.
const EMBER_COOLING: [[f64; 3]; 3] = [
    [1.0, 0.58, 0.16],
    [0.90, 0.30, 0.06],
    [0.62, 0.09, 0.03],
];
/// How far a spark gets from the blade before it stalls, in logical px:
/// sparks cut off the edge and stop short instead of flying away.
const SPARK_REACH: (f64, f64) = (6.0, 18.0);
/// Drag on a spark, per its lifetime: by the end it has all but stopped.
const SPARK_DRAG: f64 = 5.0;

#[derive(Clone, Copy, Debug, PartialEq)]
#[repr(C)]
pub struct Bounds {
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
}

impl Default for Bounds {
    fn default() -> Self {
        Self {
            x: 0.0,
            y: 0.0,
            width: 0.0,
            height: 0.0,
        }
    }
}

#[derive(Clone, Copy)]
struct Point {
    x: f64,
    y: f64,
    time: f64,
}

/// One spark. Age comes from the real clock, so particles always
/// burn out on time; motion uses a capped step so a stalled frame never
/// teleports them.
#[derive(Clone, Copy)]
struct Particle {
    x: f64,
    y: f64,
    vx: f64,
    vy: f64,
    born: f64,
    life: f64,
    /// When the motion was last stepped.
    moved: f64,
    /// A spark that burns like real metal instead of wearing the glow color.
    ember: bool,
}

impl Particle {
    fn t(&self, now: f64) -> f64 {
        ((now - self.born) / self.life).clamp(0.0, 1.0)
    }
}

pub struct Trail {
    points: VecDeque<Point>,
    colors: [[f64; 3]; 2],
    look: TrailLook,
    particles: Vec<Particle>,
    /// Pointer travel not yet spent on a particle.
    travel: f64,
    rng: SmallRng,
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
            look: TrailLook::default(),
            particles: Vec::new(),
            travel: 0.0,
            rng: SmallRng::seed(0x5eed_5a6e),
        }
    }
}

impl Trail {
    /// Forget the path and every particle.
    pub fn clear(&mut self) {
        self.points.clear();
        self.particles.clear();
        self.travel = 0.0;
    }

    pub fn set_style(&mut self, style: TrailStyle) {
        self.look.style = style;
    }

    pub fn set_look(&mut self, look: TrailLook) {
        if look.particles != self.look.particles {
            self.particles.clear();
        }
        self.look = look;
    }

    pub fn look(&self) -> TrailLook {
        self.look
    }

    pub fn set_points(&mut self, points: &[(f64, f64, f64)]) {
        self.points = points
            .iter()
            .copied()
            .map(|(x, y, t)| Point { x, y, time: t })
            .collect();
    }

    pub fn style(&self) -> TrailStyle {
        self.look.style
    }

    pub fn set_color(&mut self, color: &str) -> bool {
        if !color.starts_with('#') || !crate::color::is_valid_hex(color) {
            return false;
        }
        self.colors = OverlayMeta::new(color).trail_rgb;
        self.clear();
        true
    }

    /// Set the two trail colors (dark, light backgrounds) directly, keeping
    /// the path, for hosts that already hold an [`OverlayMeta`].
    pub fn set_trail_rgb(&mut self, rgb: [[f64; 3]; 2]) {
        self.colors = rgb;
    }

    /// Time of the newest path point.
    pub fn newest_time(&self) -> Option<f64> {
        self.points.back().map(|p| p.time)
    }

    /// Start a new path without killing particles still in the air, so a
    /// fresh press doesn't snuff the last swing's sparks.
    pub fn break_path(&mut self) {
        self.points.clear();
        self.travel = 0.0;
    }

    pub fn sample(&mut self, now: f64, x: f64, y: f64) {
        if ![now, x, y].iter().all(|v| v.is_finite()) || x.abs().max(y.abs()) > 1e7 {
            return;
        }
        self.prune(now);
        if let Some(last) = self.points.back().copied() {
            let distance = (x - last.x).hypot(y - last.y);
            // A clock reset or pointer warp must never draw a stripe across screens.
            if now < last.time || distance > 512.0 {
                self.break_path();
            } else if distance < 2.0 || (now - last.time < 1.0 / 240.0 && distance < 8.0) {
                return;
            } else {
                self.emit(now, last, x, y, distance);
            }
        }
        self.points.push_back(Point { x, y, time: now });
        while self.points.len() > MAX_POINTS {
            self.points.pop_front();
        }
    }

    /// Throw particles along the segment just travelled, one per `spark_gap`
    /// of movement, spread along the segment so fast swings don't clump.
    fn emit(&mut self, now: f64, from: Point, x: f64, y: f64, distance: f64) {
        let (max, gap) = match self.look.particles {
            Particles::Off => return,
            Particles::Sparks => (MAX_SPARKS, f64::from(self.look.spark_gap.max(1))),
        };
        let life = f64::from(self.look.spark_life) / 1000.0;
        self.travel += distance;
        while self.travel >= gap {
            self.travel -= gap;
            if self.particles.len() >= max {
                continue;
            }
            let along = (1.0 - self.travel / distance).clamp(0.0, 1.0);
            let (px, py) = (from.x + (x - from.x) * along, from.y + (y - from.y) * along);
            let r = &mut self.rng;
            let angle = r.range(0.0, std::f64::consts::TAU);
            let life = life * r.range(0.6, 1.0);
            // With drag k a spark travels speed / k before stalling.
            let speed = r.range(SPARK_REACH.0, SPARK_REACH.1) * SPARK_DRAG / life;
            let particle = Particle {
                x: px,
                y: py,
                vx: angle.cos() * speed,
                vy: angle.sin() * speed,
                born: now,
                moved: now,
                life,
                ember: r.range(0.0, 1.0) < EMBER_SHARE,
            };
            self.particles.push(particle);
        }
    }

    /// Age and move the particles up to `now`.
    fn step(&mut self, now: f64) {
        self.particles.retain(|p| now - p.born < p.life && now >= p.born);
        let sparks = self.look.particles == Particles::Sparks;
        let gravity = if sparks { GRAVITY } else { 0.0 };
        for p in &mut self.particles {
            let dt = (now - p.moved).clamp(0.0, 0.1);
            p.moved = now.max(p.moved);
            if sparks {
                let drag = (-SPARK_DRAG / p.life * dt).exp();
                p.vx *= drag;
                p.vy *= drag;
            }
            p.vy += gravity * dt;
            p.x += p.vx * dt;
            p.y += p.vy * dt;
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

    pub fn particle_count(&self) -> usize {
        self.particles.len()
    }

    /// How far a spark's drawing reaches from its center: the streak trails
    /// back along the velocity.
    fn reach(&self, p: &Particle) -> (f64, f64, f64, f64) {
        let (tx, ty) = (p.x - p.vx * 0.045, p.y - p.vy * 0.045);
        (p.x.min(tx), p.y.min(ty), p.x.max(tx), p.y.max(ty))
    }

    pub fn bounds(&mut self, now: f64) -> Option<Bounds> {
        if !now.is_finite() {
            return None;
        }
        self.prune(now);
        self.step(now);
        let mut boxes = self
            .points
            .iter()
            .map(|p| (p.x, p.y, p.x, p.y))
            .chain(self.particles.iter().map(|p| self.reach(p)));
        let (mut x0, mut y0, mut x1, mut y1) = boxes.next()?;
        for (a, b, c, d) in boxes {
            x0 = x0.min(a);
            y0 = y0.min(b);
            x1 = x1.max(c);
            y1 = y1.max(d);
        }
        Some(Bounds {
            x: (x0 - PAD).floor(),
            y: (y0 - PAD).floor(),
            width: (x1 + PAD).ceil() - (x0 - PAD).floor(),
            height: (y1 + PAD).ceil() - (y0 - PAD).floor(),
        })
    }

    /// Premultiplied RGBA, clipped to a monitor's global logical viewport.
    /// `factor` is pixels-per-logical-unit; pass the display buffer scale so the
    /// trail lands at the same pixel density as the frame.
    pub fn render(&mut self, now: f64, viewport: Bounds, light: bool, factor: f64) -> Option<TrailImage> {
        let bounds = self.bounds(now)?;
        let draw_path = self.points.len() >= 2 && self.look.style != TrailStyle::None;
        if (!draw_path && self.particles.is_empty())
            || ![viewport.x, viewport.y, viewport.width, viewport.height, factor]
                .iter()
                .all(|v| v.is_finite())
            || viewport.width <= 0.0
            || viewport.height <= 0.0
            || factor <= 0.0
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
        let factor = factor.min(2048.0 / width).min(2048.0 / height).min((1_048_576.0 / (width * height)).sqrt());
        let mut pixels = Pixmap::new((width * factor).ceil() as u32, (height * factor).ceil() as u32)?;
        // Every look wears the glow color, so the trail always matches the blade.
        let rgb = self.colors[usize::from(light)];
        let pen = Pen { rgb, x, y, factor };

        if draw_path {
            match self.look.style {
                TrailStyle::None => {}
                TrailStyle::Comet => self.render_comet(now, &mut pixels, &pen),
            }
        }
        match self.look.particles {
            Particles::Off => {}
            Particles::Sparks => self.render_sparks(now, &mut pixels, &pen),
        }

        Some(TrailImage {
            bounds: Bounds { x, y, width, height },
            pixels,
        })
    }

    /// How old a path point is, 0 at the pointer .. 1 when it fades out
    /// (Plume's `t`).
    fn age(now: f64, p: &Point) -> f64 {
        ((now - p.time) / TRAIL_S).clamp(0.0, 1.0)
    }

    fn render_comet(&self, now: f64, pixels: &mut Pixmap, pen: &Pen) {
        for (a, b) in self.points.iter().zip(self.points.iter().skip(1)) {
            let mut path = PathBuilder::new();
            let life = (1.0 - Self::age(now, a)) as f32;
            path.move_to(pen.px(a.x), pen.py(a.y));
            path.line_to(pen.px(b.x), pen.py(b.y));
            let Some(path) = path.finish() else { continue };
            for (width_scale, strength) in [(2.2, 0.04), (1.4, 0.08), (1.0, 0.22), (0.45, 0.40)] {
                let alpha = strength * life.powf(1.5);
                pen.stroke(pixels, &path, alpha as f64, 5.6 * width_scale * (0.2 + 0.8 * life as f64));
            }
        }
    }

    fn render_sparks(&self, now: f64, pixels: &mut Pixmap, pen: &Pen) {
        // Hard cutting sparks: thin streaks along each spark's flight that
        // shrink as drag stalls it, with only a slight glow under them.
        // Most burn from orange to deep red; the rest wear the glow color.
        for s in &self.particles {
            let t = s.t(now);
            let pen = &if s.ember && self.look.embers { Pen { rgb: ember_color(t), ..*pen } } else { *pen };
            let mut path = PathBuilder::new();
            path.move_to(pen.px(s.x), pen.py(s.y));
            path.line_to(pen.px(s.x - s.vx * 0.05), pen.py(s.y - s.vy * 0.05));
            let Some(path) = path.finish() else { continue };
            let width = (SIZE * 0.2 * (1.0 - t * 0.5)).max(0.8);
            pen.stroke(pixels, &path, (1.0 - t) * 0.15, width * 2.2);
            pen.stroke(pixels, &path, 1.0 - t, width);
        }
    }
}

/// Where a hot-metal spark is on its way from orange to deep red, `t` 0..1 of its life.
fn ember_color(t: f64) -> [f64; 3] {
    let pos = t.clamp(0.0, 1.0) * (EMBER_COOLING.len() as f64 - 1.0);
    let i = (pos as usize).min(EMBER_COOLING.len() - 2);
    let f = pos - i as f64;
    let (a, b) = (EMBER_COOLING[i], EMBER_COOLING[i + 1]);
    [0, 1, 2].map(|c| a[c] + (b[c] - a[c]) * f)
}

/// The glow color and the mapping from global logical px to the image.
#[derive(Clone, Copy)]
struct Pen {
    rgb: [f64; 3],
    x: f64,
    y: f64,
    factor: f64,
}

impl Pen {
    fn px(&self, gx: f64) -> f32 {
        ((gx - self.x) * self.factor) as f32
    }

    fn py(&self, gy: f64) -> f32 {
        ((gy - self.y) * self.factor) as f32
    }

    /// `strength` 0..1 of the trail's full opacity.
    fn paint(&self, strength: f64) -> Option<Paint<'static>> {
        let alpha = (TRAIL_ALPHA * strength).clamp(0.0, 1.0) as f32;
        let color = Color::from_rgba(self.rgb[0] as f32, self.rgb[1] as f32, self.rgb[2] as f32, alpha)?;
        let mut paint = Paint::default();
        paint.set_color(color);
        paint.anti_alias = true;
        Some(paint)
    }

    /// `width` in logical px.
    fn stroke(&self, pixels: &mut Pixmap, path: &Path, strength: f64, width: f64) {
        let Some(paint) = self.paint(strength) else { return };
        let stroke = Stroke {
            width: (width * self.factor) as f32,
            line_cap: LineCap::Round,
            ..Default::default()
        };
        pixels.stroke_path(path, &paint, &stroke, Transform::identity(), None);
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
    fn comet_scales_away_with_style_and_color() {
        let mut trail = trace();
        trail.set_style(TrailStyle::Comet);
        assert_eq!(trail.style(), TrailStyle::Comet);
        let image = trail.render(0.40, VIEW, false, 1.0).unwrap();
        assert!(image.pixels.width() > 0 && image.pixels.height() > 0);
    }

    #[test]
    fn none_style_is_empty() {
        let mut trail = Trail::default();
        trail.set_look(TrailLook { style: TrailStyle::None, particles: Particles::Off, ..TrailLook::default() });
        for n in 0..40 {
            trail.sample(n as f64 / 100.0, 100.0 + n as f64 * 3.0, 200.0);
        }
        assert!(trail.render(0.40, VIEW, false, 1.0).is_none());
    }

    #[test]
    fn sparks_fly_with_the_path_fall_and_burn_out() {
        let mut trail = trace();
        assert_eq!(trail.look(), TrailLook::default(), "sparks are the default look");
        // 117 px of travel at one spark per 20 px
        assert!((5..=7).contains(&trail.particle_count()), "{}", trail.particle_count());
        trail.bounds(0.40);
        let before: Vec<f64> = trail.particles.iter().filter(|p| p.born + p.life > 0.45).map(|p| p.vy).collect();
        assert!(!before.is_empty());
        trail.bounds(0.45);
        for (p, vy) in trail.particles.iter().zip(before) {
            let drag = (-SPARK_DRAG / p.life * 0.05).exp();
            assert!((p.vy - vy * drag - GRAVITY * 0.05).abs() < 1e-9, "drag slows sparks, gravity pulls them down");
        }
        assert!(trail.render(0.40, VIEW, false, 2.0).is_some());
        // the path has faded by 0.39 + 0.60 s; the last spark by 0.39 + 0.85 s
        assert!(trail.bounds(1.3).is_none());
        assert_eq!(trail.particle_count(), 0);
    }

    #[test]
    fn some_sparks_are_embers_that_cool_toward_red() {
        let mut trail = Trail::default();
        for n in 0..60 {
            trail.sample(n as f64 / 100.0, 100.0 + n as f64 * 4.0, 200.0);
        }
        let embers = trail.particles.iter().filter(|p| p.ember).count();
        assert!(embers > 0 && embers < trail.particle_count(), "a mix: {embers} of {}", trail.particle_count());
        assert_eq!(ember_color(0.0), EMBER_COOLING[0]);
        assert_eq!(ember_color(1.0), EMBER_COOLING[2]);
        let green = |t| ember_color(t)[1];
        assert!(green(0.2) > green(0.5) && green(0.5) > green(0.9), "cools as it ages");
    }

    #[test]
    fn sparks_stop_short_of_flying_away() {
        let mut trail = Trail::default();
        for n in 0..30 {
            trail.sample(n as f64 / 100.0, 100.0 + n as f64 * 4.0, 200.0);
        }
        // each spark's random lifetime tells it apart as others burn out
        let start: std::collections::HashMap<u64, (f64, f64)> =
            trail.particles.iter().map(|p| (p.life.to_bits(), (p.x, p.y))).collect();
        assert_eq!(start.len(), trail.particle_count());
        let mut reach = 0.0f64;
        for frame in 1..=150 {
            trail.step(0.29 + frame as f64 / 100.0);
            for p in &trail.particles {
                if let Some(&(x, y)) = start.get(&p.life.to_bits()) {
                    reach = reach.max((p.x - x).hypot(p.y - y));
                }
            }
        }
        assert!(reach < SPARK_REACH.1 + 15.0, "a spark got {reach:.1} px from the blade");
    }

    #[test]
    fn spark_gap_sets_how_close_together_they_are() {
        let count = |gap| {
            let mut trail = Trail::default();
            trail.set_look(TrailLook { spark_gap: gap, ..TrailLook::default() });
            for n in 0..40 {
                trail.sample(n as f64 / 100.0, 100.0 + n as f64 * 3.0, 200.0);
            }
            trail.particle_count()
        };
        assert!(count(3) > 2 * count(8));
    }

    #[test]
    fn spark_life_sets_how_long_they_last() {
        let alive_at = |life, now| {
            let mut trail = Trail::default();
            trail.set_look(TrailLook { spark_life: life, style: TrailStyle::None, ..TrailLook::default() });
            trail.sample(0.0, 100.0, 100.0);
            trail.sample(0.01, 140.0, 100.0);
            trail.bounds(now);
            trail.particle_count()
        };
        assert_eq!(alive_at(200, 0.3), 0);
        assert!(alive_at(1200, 0.3) > 0);
    }

    #[test]
    fn sparks_wear_the_glow_color() {
        let mut trail = Trail::default();
        assert!(trail.set_color("#ff0000"));
        trail.set_look(TrailLook { style: TrailStyle::None, ..TrailLook::default() });
        trail.sample(0.0, 100.0, 100.0);
        trail.sample(0.01, 160.0, 100.0);
        let image = trail.render(0.02, VIEW, false, 1.0).unwrap();
        let (mut red, mut other) = (0u32, 0u32);
        for p in image.pixels.data().chunks_exact(4).filter(|p| p[3] > 0) {
            red += u32::from(p[0]);
            other += u32::from(p[1]) + u32::from(p[2]);
        }
        assert!(red > 0 && red > other, "red {red} vs green+blue {other}");
    }

    #[test]
    fn stationary_pointer_and_warps_do_not_leave_dots_or_screen_wide_lines() {
        let mut trail = Trail::default();
        for n in 0..100 {
            trail.sample(n as f64 / 100.0, 100.0, 100.0);
        }
        assert!(trail.render(1.0, VIEW, false, 1.0).is_none());
        assert_eq!(trail.particle_count(), 0, "a still pointer throws no sparks");
        trail.sample(1.01, 900.0, 100.0);
        assert_eq!(trail.point_count(), 1);
        trail.sample(f64::NAN, 0.0, 0.0);
        assert_eq!(trail.point_count(), 1);
    }

    #[test]
    fn monitor_clipping_negative_coordinates_and_clear_are_supported() {
        let mut trail = trace();
        assert!(trail.render(0.4, Bounds { x: -1000.0, ..VIEW }, false, 1.0).is_none());
        let image = trail
            .render(0.4, Bounds { x: 150.0, width: 100.0, ..VIEW }, true, 1.0)
            .unwrap();
        assert!(image.bounds.x >= 150.0 && image.bounds.x + image.bounds.width <= 250.0);
        assert_eq!(image.pixels.width() as f64, image.bounds.width);
        trail.clear();
        assert!(trail.bounds(0.5).is_none());
        trail.sample(0.5, -100.0, -100.0);
        trail.sample(0.6, -80.0, -80.0);
        let negative = trail
            .render(0.6, Bounds { x: -200.0, y: -200.0, width: 200.0, height: 200.0 }, false, 1.0)
            .unwrap();
        assert!(negative.bounds.x < 0.0 && negative.bounds.y < 0.0);
    }

    #[test]
    fn history_and_texture_allocations_are_bounded() {
        let mut trail = Trail::default();
        for n in 0..5000 {
            trail.sample(n as f64 / 10000.0, (n % 100) as f64 * 3.0, (n / 100) as f64 * 3.0);
        }
        assert!(trail.point_count() <= MAX_POINTS);
        let image = trail.render(0.5, VIEW, false, 1.0).unwrap();
        assert!(image.pixels.width() <= 2048 && image.pixels.height() <= 2048);
        trail.clear();
        for n in 0..250 {
            trail.sample(n as f64 / 1000.0, n as f64 * 200.0, n as f64 * 200.0);
        }
        let huge = trail
            .render(0.25, Bounds { width: 100_000.0, height: 100_000.0, ..VIEW }, false, 1.0)
            .unwrap();
        assert!(huge.pixels.width() <= 2048 && huge.pixels.height() <= 2048);
        assert!(huge.pixels.data().len() < 4_300_000);
    }
}
