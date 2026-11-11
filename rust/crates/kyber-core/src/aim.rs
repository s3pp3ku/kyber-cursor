//! Worms-cursor style aim: the blade turns to face its direction of travel
//! and keeps that angle when the pointer stops.
//!
//! Port of `otlet.worms-cursor/watch.py` (class `Aim`). The maths is pure so
//! it unit-tests without a compositor: a heading in degrees where 0° is
//! screen-right (+x) and 90° is screen-down (+y), exactly the angle
//! `atan2(dy, dx)` gives on a monitor.
//!
//! The geometry helpers are transforms, not duplicated constants: the glyph
//! is authored at [`crate::geometry::axis_angle`] and the aim transform
//! rotates it from there onto any heading.

use crate::geometry;

/// Accumulated pointer travel before the aim direction is updated.
pub const AIM_DIST: f64 = 8.0;
/// How much a new sample bends the smoothed direction (0..1).
pub const AIM_SMOOTH: f64 = 0.5;
/// A new heading must differ this much before the blade turns at all.
pub const HYST_DEG: f64 = 3.0;
/// Fastest turn, degrees per second.
pub const TURN_DPS: f64 = 720.0;
/// Frames of stillness before accumulated travel is discarded.
pub const IDLE_RESET: u32 = 60;

/// Settings file holding `on` / `off`.
pub const STATE_FILE: &str = "aim-mode";
/// Settings file holding the mirror switch (`on` / `off`).
pub const MIRROR_FILE: &str = "aim-mirror";
/// Settings file holding the turn switch (`on` / `off`). Off keeps the blade
/// in the stock theme's pose, tip up-left like a regular pointer, while aim
/// still draws the cursor and its glow together as one picture.
pub const TURN_FILE: &str = "aim-turn";

/// The two switches, read from the settings folder like every other Kyber
/// setting. Missing files mean aim off and mirroring on.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AimSettings {
    pub enabled: bool,
    pub mirror: bool,
    pub turn: bool,
}

impl Default for AimSettings {
    fn default() -> Self {
        Self {
            enabled: false,
            mirror: true,
            turn: true,
        }
    }
}

impl AimSettings {
    pub fn load(dir: &std::path::Path) -> Self {
        let read = |name: &str| {
            std::fs::read_to_string(dir.join(name))
                .unwrap_or_default()
                .trim()
                .to_lowercase()
        };
        let d = Self::default();
        Self {
            enabled: read(STATE_FILE) == "on",
            mirror: match read(MIRROR_FILE).as_str() {
                "off" => false,
                "on" => true,
                _ => d.mirror,
            },
            turn: read(TURN_FILE) != "off",
        }
    }
}

/// Signed shortest difference `a - b` in degrees, in (-180, 180].
pub fn circ_diff(a: f64, b: f64) -> f64 {
    let d = a - b;
    d - 360.0 * ((d + 180.0) / 360.0).floor()
}

/// Smoothed heading of the pointer's travel. Feed it every frame.
#[derive(Debug, Clone)]
pub struct Aim {
    prev: Option<(f64, f64)>,
    acc: (f64, f64),
    dir: (f64, f64),
    target: Option<f64>,
    disp: f64,
    idle: u32,
    t: Option<f64>,
}

impl Default for Aim {
    fn default() -> Self {
        Self::new()
    }
}

impl Aim {
    pub fn new() -> Self {
        Self {
            prev: None,
            acc: (0.0, 0.0),
            // Heading 0: the sawtooth starts pointing right, like the Python.
            dir: (1.0, 0.0),
            target: None,
            disp: 0.0,
            idle: 0,
            t: None,
        }
    }

    /// The heading the blade is drawn at, in degrees.
    pub fn heading(&self) -> f64 {
        self.disp
    }

    /// The heading it is still turning towards, if any.
    pub fn target(&self) -> Option<f64> {
        self.target
    }

    /// Feed a pointer position (global logical px) and a monotonic time in
    /// seconds. Returns the heading to draw this frame.
    pub fn update(&mut self, x: f64, y: f64, now: f64) -> f64 {
        let Some(prev) = self.prev else {
            self.prev = Some((x, y));
            self.t = Some(now);
            return self.disp;
        };
        let (dx, dy) = (x - prev.0, y - prev.1);
        self.prev = Some((x, y));
        let mut dt = match self.t {
            Some(t) => now - t,
            None => 0.0,
        };
        self.t = Some(now);
        if dt <= 0.0 {
            dt = 1.0 / 144.0;
        }
        if dx == 0.0 && dy == 0.0 {
            self.idle += 1;
            if self.idle >= IDLE_RESET {
                self.acc = (0.0, 0.0);
            }
        } else {
            self.idle = 0;
            self.acc.0 += dx;
            self.acc.1 += dy;
            let dist = self.acc.0.hypot(self.acc.1);
            if dist >= AIM_DIST {
                let n = (self.acc.0 / dist, self.acc.1 / dist);
                self.dir.0 = self.dir.0 * (1.0 - AIM_SMOOTH) + n.0 * AIM_SMOOTH;
                self.dir.1 = self.dir.1 * (1.0 - AIM_SMOOTH) + n.1 * AIM_SMOOTH;
                self.acc = (0.0, 0.0);
                let deg = self.dir.1.atan2(self.dir.0).to_degrees().rem_euclid(360.0);
                if self
                    .target
                    .is_none_or(|t| circ_diff(deg, t).abs() > HYST_DEG)
                {
                    let first = self.target.is_none();
                    self.target = Some(deg);
                    if first {
                        // Nothing to slew from yet: snap, like the Python.
                        self.disp = deg;
                        return self.disp;
                    }
                }
            }
        }
        if let Some(target) = self.target {
            let diff = circ_diff(target, self.disp);
            let step = TURN_DPS * dt;
            if diff.abs() <= step {
                self.disp = target;
            } else {
                self.disp += step.copysign(diff);
            }
            self.disp = self.disp.rem_euclid(360.0);
        }
        self.disp
    }
}

// ------------------------------------------------------------- transforms

/// 2×3 affine matrix, row-major like tiny-skia:
/// `x' = a·x + c·y + e`, `y' = b·x + d·y + f`.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Mat {
    pub a: f64,
    pub b: f64,
    pub c: f64,
    pub d: f64,
    pub e: f64,
    pub f: f64,
}

impl Mat {
    pub const IDENTITY: Self = Self {
        a: 1.0,
        b: 0.0,
        c: 0.0,
        d: 1.0,
        e: 0.0,
        f: 0.0,
    };

    pub fn translate(tx: f64, ty: f64) -> Self {
        Self {
            e: tx,
            f: ty,
            ..Self::IDENTITY
        }
    }

    /// Screen-space rotation: positive degrees turn clockwise on a monitor
    /// because +y points down, which is the same convention as `atan2`.
    pub fn rotate_deg(deg: f64) -> Self {
        let (s, c) = deg.to_radians().sin_cos();
        Self {
            a: c,
            b: s,
            c: -s,
            d: c,
            e: 0.0,
            f: 0.0,
        }
    }

    pub fn scale(sx: f64, sy: f64) -> Self {
        Self {
            a: sx,
            d: sy,
            ..Self::IDENTITY
        }
    }

    /// `self ∘ rhs`: `rhs` runs first.
    pub fn then(self, rhs: Self) -> Self {
        Self {
            a: self.a * rhs.a + self.c * rhs.b,
            b: self.b * rhs.a + self.d * rhs.b,
            c: self.a * rhs.c + self.c * rhs.d,
            d: self.b * rhs.c + self.d * rhs.d,
            e: self.a * rhs.e + self.c * rhs.f + self.e,
            f: self.b * rhs.e + self.d * rhs.f + self.f,
        }
    }

    pub fn apply(self, p: (f64, f64)) -> (f64, f64) {
        (
            self.a * p.0 + self.c * p.1 + self.e,
            self.b * p.0 + self.d * p.1 + self.f,
        )
    }

    /// tiny-skia's `Transform::from_row` order.
    pub fn row(self) -> [f32; 6] {
        [
            self.a as f32,
            self.b as f32,
            self.c as f32,
            self.d as f32,
            self.e as f32,
            self.f as f32,
        ]
    }
}

/// The unmirrored glyph's lit edge, as a unit normal on the pointer axis.
///
/// The lit strip runs along the tip→`head_l` edge, so its offset direction is
/// `-axis_n()`. This is the single description of "which side the light is
/// on"; the mirror rule and its tests both read it.
pub fn lit_edge_normal() -> (f64, f64) {
    let (nx, ny) = geometry::axis_n();
    (-nx, -ny)
}

/// The direction the authored glyph's blade points.
///
/// The theme's axis runs tip → pommel (`geometry::axis_u`), so the sharp end
/// faces the other way: at the native 62° axis the blade points up-left, which
/// is exactly how the stock pointer looks. Aim is the direction of *travel*,
/// so the artwork is turned onto `heading - blade_heading()`.
pub fn blade_heading() -> f64 {
    geometry::axis_angle().to_degrees() + 180.0
}

/// Where the lit edge ends up for a pure rotation onto `heading_deg`.
pub fn lit_edge_normal_at(heading_deg: f64) -> (f64, f64) {
    let turn = heading_deg - blade_heading();
    let (nx, ny) = lit_edge_normal();
    let (s, c) = turn.to_radians().sin_cos();
    (nx * c - ny * s, nx * s + ny * c)
}

/// Does this heading need the artwork mirrored across the pointer axis?
///
/// A pure rotation swings the lit strip round with the blade; past the
/// vertical it faces up the screen, away from the viewer, so the glyph reads
/// as lit from the wrong side. Mirroring across the axis (which is a
/// reflection inside the blade's own frame, so it turns nothing) puts the
/// strip back on the near side, as in the stock theme at its native 62°.
pub fn wants_mirror(heading_deg: f64) -> bool {
    lit_edge_normal_at(heading_deg).1 < 0.0
}

/// Glyph transform: turn the authored artwork so its blade points along
/// `heading_deg`, mirror it across the pointer axis if asked, and put the tip
/// (the hotspot) exactly at `cursor_px`.
///
/// `tip_px` is the tip's position inside the source bitmap, `cursor_px` the
/// hotspot in destination pixels. Both are in the same pixel scale, so the
/// transform is rotation-only: it never resamples beyond the turn itself.
pub fn glyph_transform(
    heading_deg: f64,
    mirror: bool,
    tip_px: (f64, f64),
    cursor_px: (f64, f64),
) -> Mat {
    let to_origin = Mat::translate(-tip_px.0, -tip_px.1);
    let unrotate = Mat::rotate_deg(-geometry::axis_angle().to_degrees());
    let flip = Mat::scale(1.0, if mirror { -1.0 } else { 1.0 });
    // `unrotate` lands the glyph in a blade-aligned frame where the sharp end
    // points along -x (the axis runs tip → pommel), so turning it onto the
    // heading is a turn of heading - 180 about the tip.
    let turn = Mat::rotate_deg(heading_deg - 180.0);
    let place = Mat::translate(cursor_px.0, cursor_px.1);
    place.then(turn).then(flip).then(unrotate).then(to_origin)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn close(a: f64, b: f64) -> bool {
        circ_diff(a, b).abs() < 1e-6
    }

    #[test]
    fn starts_at_rest_and_turns_on_movement() {
        let mut aim = Aim::new();
        assert_eq!(aim.update(0.0, 0.0, 0.0), 0.0);
        // 20 px right: one frame is enough to snap to the first heading.
        let deg = aim.update(20.0, 0.0, 0.016);
        assert!(close(deg, 0.0), "{deg}");
    }

    #[test]
    fn holds_the_angle_when_the_pointer_stops() {
        let mut aim = Aim::new();
        aim.update(0.0, 0.0, 0.0);
        aim.update(20.0, 0.0, 0.016);
        let deg = aim.update(20.0, 20.0, 0.032);
        assert!(deg > 8.0 && deg < 20.0, "{deg}");
        let held = (0..8).fold(deg, |_, i| aim.update(20.0, 20.0, 0.08 + i as f64 * 0.05));
        assert!(close(held, 45.0), "{held}");
        let frozen = aim.update(20.0, 20.0, 1.0);
        assert!((frozen - held).abs() < 0.01, "{frozen}");
        // Long stillness never resets the heading, only the travel sum.
        let frozen = (0..IDLE_RESET + 5).fold(frozen, |_, i| {
            aim.update(20.0, 20.0, 1.1 + i as f64 * 0.016)
        });
        assert!((frozen - held).abs() < 0.01, "{frozen}");
    }

    #[test]
    fn small_heading_changes_are_ignored_by_the_hysteresis() {
        let mut aim = Aim::new();
        aim.prev = Some((0.0, 0.0));
        aim.t = Some(0.0);
        aim.target = Some(0.0);
        // A 2° travel sample smooths to ~1°: under HYST_DEG, no retarget.
        let ang = 2f64.to_radians();
        aim.update(ang.cos() * 20.0, ang.sin() * 20.0, 0.016);
        assert_eq!(aim.target(), Some(0.0), "2° must not retarget");
        // A 20° sample is well past it, so the blade does turn.
        let mut aim = Aim::new();
        aim.prev = Some((0.0, 0.0));
        aim.t = Some(0.0);
        aim.target = Some(0.0);
        let ang = 20f64.to_radians();
        aim.update(ang.cos() * 20.0, ang.sin() * 20.0, 0.016);
        assert!(aim.target().is_some_and(|d| d > 9.0), "{:?}", aim.target());
    }

    #[test]
    fn turns_at_720_degrees_per_second() {
        let mut aim = Aim::new();
        aim.update(0.0, 0.0, 0.0);
        aim.update(20.0, 0.0, 0.016);
        assert!(close(aim.heading(), 0.0));
        // Point the travel straight down, then measure one 50 ms frame.
        aim.target = Some(90.0);
        let deg = aim.update(20.0, 0.0, 0.016 + 0.05);
        assert!((deg - 36.0).abs() < 1e-6, "{deg}");
        // ...and the next one: a constant 720°/s, never a snap.
        let deg = aim.update(20.0, 0.0, 0.016 + 0.10);
        assert!((deg - 72.0).abs() < 1e-6, "{deg}");
        // A whole quarter turn from rest would take 125 ms.
        let mut aim = Aim::new();
        aim.prev = Some((0.0, 0.0));
        aim.t = Some(0.0);
        aim.target = Some(90.0);
        let deg = aim.update(0.0, 0.0, 0.125);
        assert!((deg - 90.0).abs() < 1e-6, "{deg}");
    }

    #[test]
    fn turns_towards_350_the_short_way_from_10() {
        let mut aim = Aim::new();
        aim.prev = Some((0.0, 0.0));
        aim.t = Some(0.0);
        aim.disp = 10.0;
        aim.target = Some(350.0);
        // 720°/s × 1 ms = 0.72°: from 10 it must go down, not up to 190.
        let deg = aim.update(0.0, 0.0, 0.001);
        assert!(deg < 10.0 && deg > 9.0, "{deg}");
        // The whole 20° way round takes ~28 ms, not 340° worth.
        let deg = aim.update(0.0, 0.0, 0.030);
        assert!(close(deg, 350.0), "{deg}");
    }

    #[test]
    fn a_350_degree_travel_sample_keeps_its_short_heading() {
        let mut aim = Aim::new();
        aim.prev = Some((0.0, 0.0));
        aim.t = Some(0.0);
        aim.disp = 10.0;
        aim.target = Some(10.0);
        let ang = 350f64.to_radians();
        let deg = aim.update(ang.cos() * 20.0, ang.sin() * 20.0, 0.016);
        // The smoothed vector lands just above 0°, never the long way round.
        assert!(!(10.0..=350.0).contains(&deg), "{deg}");
        assert!(circ_diff(deg, 10.0).abs() <= 12.0, "{deg}");
    }

    #[test]
    fn travel_accumulates_across_small_steps() {
        let mut aim = Aim::new();
        aim.update(0.0, 0.0, 0.0);
        // 4 px per frame is below AIM_DIST, so nothing happens until the sum
        // crosses 8 px.
        let deg = aim.update(4.0, 0.0, 0.016);
        assert!(close(deg, 0.0) && aim.target().is_none(), "{deg}");
        let deg = aim.update(8.0, 0.0, 0.032);
        assert!(close(deg, 0.0) && aim.target().is_some(), "{deg}");
    }

    #[test]
    fn transform_puts_the_tip_on_the_cursor_and_the_axis_on_the_heading() {
        let tip = (8.0 - geometry::VIEW[0], 5.0 - geometry::VIEW[1]);
        let tip_px = (tip.0 * 2.0, tip.1 * 2.0);
        let cursor = (400.0, 300.0);
        for heading in [0.0, 45.0, 90.0, 135.0, 180.0, 225.0, 270.0, 315.0, 62.0] {
            let m = glyph_transform(heading, false, tip_px, cursor);
            let placed = m.apply(tip_px);
            assert!(
                (placed.0 - cursor.0).abs() < 1e-9 && (placed.1 - cursor.1).abs() < 1e-9,
                "heading {heading}: tip at {placed:?}"
            );
            // The authored axis runs tip → pommel, so it must land opposite
            // the heading: the blade (the sharp end) leads the travel.
            let (ux, uy) = geometry::axis_u();
            let ahead = (tip_px.0 + ux * 10.0 * 2.0, tip_px.1 + uy * 10.0 * 2.0);
            let placed = m.apply(ahead);
            let dir = (placed.0 - cursor.0, placed.1 - cursor.1);
            let got = dir.1.atan2(dir.0).to_degrees();
            assert!(
                close(got, heading + 180.0),
                "heading {heading}: axis at {got}"
            );
        }
    }

    #[test]
    fn mirroring_reflects_across_the_axis_only() {
        let tip = (0.0, 0.0);
        let cursor = (0.0, 0.0);
        let plain = glyph_transform(30.0, false, tip, cursor);
        let mirrored = glyph_transform(30.0, true, tip, cursor);
        let (nx, ny) = lit_edge_normal();
        let a = plain.apply((nx * 10.0, ny * 10.0));
        let b = mirrored.apply((nx * 10.0, ny * 10.0));
        // Same distance from the tip, opposite side of the axis.
        assert!((a.0.hypot(a.1) - b.0.hypot(b.1)).abs() < 1e-9);
        let heading = 30f64.to_radians();
        let axis = (heading.cos(), heading.sin());
        let across = (-axis.1, axis.0);
        assert!((a.0 * axis.0 + a.1 * axis.1 - (b.0 * axis.0 + b.1 * axis.1)).abs() < 1e-9);
        assert!((a.0 * across.0 + a.1 * across.1 + b.0 * across.0 + b.1 * across.1).abs() < 1e-9);
    }

    #[test]
    fn the_stock_heading_is_never_mirrored() {
        let native = blade_heading();
        assert!(
            !wants_mirror(native),
            "the theme's own angle must stay as drawn"
        );
        // ...and the mirror rule always lands the lit strip on the near side.
        for step in 0..360 {
            let heading = step as f64;
            let mirror = wants_mirror(heading);
            let (_, ny) = lit_edge_normal_at(heading);
            let y = if mirror { -ny } else { ny };
            assert!(y >= -1e-9, "heading {heading} leaves the lit edge away");
        }
    }
}
