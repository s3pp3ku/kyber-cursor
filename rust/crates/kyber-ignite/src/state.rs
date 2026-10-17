//! Hold-to-ignite state machine. Port of the `Listener` + `Overlay` tick
//! logic in `lib/ignite-listener`. Pure (injectable clock, no syscalls)
//! so it unit-tests without input devices or a compositor.

use std::collections::VecDeque;

use kyber_core::overlay::{
    DOUBLE_CLICK_S, HUM_BEAM, HUM_HAZE, IGNITE_S, RETRACT_S, SOUND_DELAY_MS, SURGE_S, TRAIL_S,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Phase {
    Off,
    Igniting,
    Held,
    Retracting,
}

#[derive(Debug, Clone, Copy)]
pub struct TrailPt {
    pub x: f64,
    pub y: f64,
    pub t: f64,
}

/// What the main loop must do after a button transition. Syscalls
/// (placing the overlay, playing sounds) stay in the loop; decisions here.
#[derive(Debug, PartialEq)]
pub enum DownAction {
    /// First button down: loop must place + present, then `begin_visual`.
    NeedPlace { clash: bool },
    /// Already showing: re-ignite from wherever the retract got to.
    Reignite { clash: bool },
}

pub struct Engine {
    pub phase: Phase,
    pub p: f32,
    pub t: f64,
    pub surge_until: f64,
    pub trail: VecDeque<TrailPt>,
    pub trail_on: bool,
    pub light: f32,
    pub held: u32,
    press_id: u64,
    sounded: bool,
    double: bool,
    last_press: f64,
    pending_at: Option<(f64, u64)>,
}

impl Engine {
    pub fn new() -> Self {
        Self {
            phase: Phase::Off,
            p: 0.0,
            t: 0.0,
            surge_until: 0.0,
            trail: VecDeque::new(),
            trail_on: true,
            light: 0.0,
            held: 0,
            press_id: 0,
            sounded: false,
            double: false,
            last_press: 0.0,
            pending_at: None,
        }
    }

    /// Button pressed somewhere. On `NeedPlace` the loop must place +
    /// present the overlay, then call `begin_visual`. On `Reignite` the
    /// overlay is already up (e.g. mid-retract): just keep ticking. Caller
    /// plays a clash when `clash` is true and sound mode is `all`.
    pub fn button_down(&mut self, now: f64) -> DownAction {
        self.held += 1;
        self.press_id += 1;
        self.sounded = false;
        self.double = now - self.last_press < DOUBLE_CLICK_S && self.last_press > 0.0;
        self.last_press = if self.double { 0.0 } else { now };
        self.pending_at = Some((now + SOUND_DELAY_MS as f64 / 1000.0, self.press_id));
        if self.phase == Phase::Off {
            DownAction::NeedPlace { clash: self.double }
        } else {
            // Re-ignite from wherever a retract got to.
            self.phase = Phase::Igniting;
            DownAction::Reignite { clash: self.double }
        }
    }

    /// Called after the loop placed + presented the overlay.
    pub fn begin_visual(&mut self, now: f64, trail_on: bool, light: f32) {
        self.trail_on = trail_on;
        self.light = light.clamp(0.0, 1.0);
        self.t = now;
        self.phase = Phase::Igniting;
    }

    /// All buttons released. Returns true when an ignite sound played and
    /// the caller should now play an `off` clip (mode `all` only).
    pub fn button_up(&mut self) -> bool {
        self.held = self.held.saturating_sub(1);
        if self.held == 0 && matches!(self.phase, Phase::Igniting | Phase::Held) {
            self.phase = Phase::Retracting;
        }
        let s = self.sounded;
        self.sounded = false;
        s && self.held == 0
    }

    /// Fire the delayed ignite sound if the press is still held. Returns
    /// true when the caller should play an ignite clip (and flare the
    /// blade via `surge`). `muted` = sound mode is `mute`.
    pub fn poll_ignite_sound(&mut self, now: f64, muted: bool) -> bool {
        match self.pending_at {
            Some((at, id)) if now >= at && id == self.press_id => {
                self.pending_at = None;
                if self.held > 0 && self.phase != Phase::Off && !self.double && !muted {
                    self.sounded = true;
                    self.surge_until = now + SURGE_S;
                    return true;
                }
            }
            _ => {}
        }
        false
    }

    /// Advance one frame. `cursor` is the current pointer position, if known.
    /// Returns true while there is anything to draw (loop keeps ticking).
    pub fn tick(&mut self, now: f64, cursor: Option<(f64, f64)>) -> bool {
        let dt = (now - self.t).clamp(0.0, 0.05);
        self.t = now;
        match self.phase {
            Phase::Igniting => {
                self.p = (self.p + dt as f32 / IGNITE_S as f32).min(1.0);
                if self.p >= 1.0 {
                    self.phase = Phase::Held;
                }
            }
            Phase::Retracting => {
                self.p = (self.p - dt as f32 / RETRACT_S as f32).max(0.0);
            }
            Phase::Off => return false,
            Phase::Held => {}
        }
        if let Some((x, y)) = cursor {
            if self.trail_on && matches!(self.phase, Phase::Igniting | Phase::Held) {
                let push = match self.trail.back() {
                    None => true,
                    Some(last) => (x - last.x).hypot(y - last.y) >= 1.0,
                };
                if push {
                    self.trail.push_back(TrailPt { x, y, t: now });
                }
            }
        }
        while self
            .trail
            .front()
            .map(|pt| now - pt.t > TRAIL_S)
            .unwrap_or(false)
        {
            self.trail.pop_front();
        }
        if self.phase == Phase::Retracting && self.p <= 0.0 && self.trail.is_empty() {
            self.phase = Phase::Off;
            return false;
        }
        true
    }

    /// Ease-out ignition progress: fast start, gentle arrival.
    pub fn ease(&self) -> f32 {
        1.0 - (1.0 - self.p).powi(3)
    }

    /// Brightness multipliers (haze, beams) for the live blade's waver.
    /// `flicker` in [-1, 1] (random undertone, caller-supplied).
    pub fn hum(&self, flicker: f32) -> (f32, f32) {
        if self.t < self.surge_until {
            return (1.0, 1.0);
        }
        let t = self.t;
        let n = 0.65 * (2.0 * std::f64::consts::PI * 2.1 * t).sin()
            + 0.25 * (2.0 * std::f64::consts::PI * 5.3 * t + 1.1).sin()
            + 0.10 * flicker as f64;
        let level = 0.5 + 0.5 * n.clamp(-1.0, 1.0);
        (
            1.0 - HUM_HAZE as f32 * level as f32,
            1.0 - HUM_BEAM as f32 * level as f32,
        )
    }

    pub fn is_live(&self) -> bool {
        self.phase != Phase::Off
    }
}

impl Default for Engine {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn press_release_cycle() {
        let mut e = Engine::new();
        assert_eq!(e.button_down(1.0), DownAction::NeedPlace { clash: false });
        e.begin_visual(1.0, true, 0.0);
        assert!(e.tick(1.1, Some((10.0, 10.0))));
        assert!(e.poll_ignite_sound(1.0 + 0.31, false));
        assert!(e.button_up());
        // Retracts, then goes idle once the trail expires.
        assert!(e.tick(1.2, None));
        assert!(!e.tick(1.2 + TRAIL_S + 0.1, None));
        assert_eq!(e.phase, Phase::Off);
    }

    #[test]
    fn quick_click_stays_silent() {
        let mut e = Engine::new();
        e.button_down(1.0);
        e.begin_visual(1.0, true, 0.0);
        e.button_up();
        assert!(!e.poll_ignite_sound(1.31, false));
    }

    #[test]
    fn double_click_sets_clash_not_ignite() {
        let mut e = Engine::new();
        e.button_down(1.0);
        e.begin_visual(1.0, true, 0.0);
        e.button_up();
        assert_eq!(e.button_down(1.2), DownAction::Reignite { clash: true });
        e.begin_visual(1.2, true, 0.0);
        assert!(!e.poll_ignite_sound(1.51, false));
    }

    #[test]
    fn second_button_reignites_retract() {
        let mut e = Engine::new();
        e.button_down(1.0);
        e.begin_visual(1.0, true, 0.0);
        e.button_up();
        assert_eq!(e.phase, Phase::Retracting);
        // Overlay still up: re-ignite, no re-place. (0.1 s apart also
        // counts as a double click, hence clash: true.)
        assert_eq!(e.button_down(1.1), DownAction::Reignite { clash: true });
        assert_eq!(e.phase, Phase::Igniting);
        e.button_up();
        e.button_up();
        assert_eq!(e.held, 0);
    }

    #[test]
    fn trail_prunes_and_gates_on_movement() {
        let mut e = Engine::new();
        e.button_down(0.0);
        e.begin_visual(0.0, true, 0.0);
        e.tick(0.01, Some((0.0, 0.0)));
        e.tick(0.02, Some((0.0, 0.0))); // same spot: no new point
        assert_eq!(e.trail.len(), 1);
        e.tick(0.03, Some((5.0, 0.0)));
        assert_eq!(e.trail.len(), 2);
        e.button_up();
        assert!(!e.tick(TRAIL_S + 1.0, None));
        assert!(e.trail.is_empty());
    }

    #[test]
    fn hum_surges_then_wavers() {
        let mut e = Engine::new();
        e.button_down(0.0);
        e.begin_visual(0.0, true, 0.0);
        assert!(e.poll_ignite_sound(0.31, false)); // flares the blade
        e.t = 0.31;
        assert_eq!(e.hum(0.5), (1.0, 1.0));
        e.t = 0.31 + SURGE_S + 0.01;
        let (h, b) = e.hum(0.0);
        assert!(h < 1.0 && b < 1.0 && h < b, "{h} {b}");
    }
}
