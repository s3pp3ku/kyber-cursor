use kyber_core::{blade_audio::HoldingHum, sounds::SR};
use rodio::Source;
use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc,
};
use std::time::Duration;

/// Independent hum. Power-up/down recordings play once elsewhere.
pub struct BladeSource {
    hum: HoldingHum,
    released: Arc<AtomicBool>,
    swipe_duck: Option<Arc<AtomicBool>>,
    duck_gain: f32,
    position: usize,
    release_position: Option<usize>,
}
impl BladeSource {
    pub fn new(pitch: f64, released: Arc<AtomicBool>) -> Self {
        Self::with_swipe_duck(pitch, released, None)
    }

    #[allow(dead_code)] // The standalone video review example does not play swipes.
    pub fn new_with_swipe_duck(
        pitch: f64,
        released: Arc<AtomicBool>,
        swipe_duck: Arc<AtomicBool>,
    ) -> Self {
        Self::with_swipe_duck(pitch, released, Some(swipe_duck))
    }

    fn with_swipe_duck(
        pitch: f64,
        released: Arc<AtomicBool>,
        swipe_duck: Option<Arc<AtomicBool>>,
    ) -> Self {
        let seed = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|elapsed| elapsed.as_nanos() as u64)
            .unwrap_or(1)
            ^ pitch.to_bits();
        Self {
            hum: HoldingHum::new_seeded(pitch, SR, seed),
            released,
            swipe_duck,
            duck_gain: 1.0,
            position: 0,
            release_position: None,
        }
    }
}
impl Iterator for BladeSource {
    type Item = f32;
    fn next(&mut self) -> Option<f32> {
        let fade_frames = (SR / 50) as usize;
        if self.released.load(Ordering::Relaxed) && self.release_position.is_none() {
            self.release_position = Some(0);
        }
        let release_gain = if let Some(position) = &mut self.release_position {
            if *position >= fade_frames {
                return None;
            }
            let gain =
                0.5 + 0.5 * (std::f32::consts::PI * *position as f32 / fade_frames as f32).cos();
            *position += 1;
            gain
        } else {
            1.0
        };
        // Let the power-up clip finish its rising sweep before the steady
        // blade hum fades in, matching the Python clip's 1.3 s ignition.
        let delay = SR as usize;
        let fade_in = (self.position.saturating_sub(delay) as f32 / (SR as f32 * 0.30)).min(1.0);
        self.position += 1;
        if self.position <= delay {
            return Some(0.0);
        }
        let duck_target = if self
            .swipe_duck
            .as_ref()
            .is_some_and(|duck| duck.load(Ordering::Relaxed))
        {
            0.08
        } else {
            1.0
        };
        self.duck_gain += (duck_target - self.duck_gain) * 0.0015;
        // HoldingHum already applies the 0.45 mix gain from make_sounds.py.
        // A second 0.45 here made the live Rust hum noticeably too quiet.
        Some(self.hum.next()? * fade_in * release_gain * self.duck_gain)
    }
}
impl Source for BladeSource {
    fn current_span_len(&self) -> Option<usize> {
        None
    }
    fn channels(&self) -> u16 {
        1
    }
    fn sample_rate(&self) -> u32 {
        SR
    }
    fn total_duration(&self) -> Option<Duration> {
        None
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn hum_starts_gently_and_stops_twenty_ms_after_release() {
        let release = Arc::new(AtomicBool::new(false));
        let mut source = BladeSource::new(86.0, release.clone());
        assert!(source.by_ref().take(SR as usize).all(|v| v == 0.0));
        let held: Vec<_> = source.by_ref().take(SR as usize * 8).collect();
        assert!(held.iter().any(|s| s.abs() > 0.05));
        assert!(held.windows(2).all(|p| (p[1] - p[0]).abs() < 0.03));
        release.store(true, Ordering::Relaxed);
        let tail: Vec<_> = source.collect();
        assert_eq!(tail.len(), (SR / 50) as usize);
        assert!(tail.last().unwrap().abs() < 0.00001);
    }
    #[test]
    fn release_during_initial_delay_cannot_leave_a_hum_running() {
        let source = BladeSource::new(78.0, Arc::new(AtomicBool::new(true)));
        let samples: Vec<_> = source.collect();
        assert_eq!(samples.len(), (SR / 50) as usize);
        assert!(samples.iter().all(|s| *s == 0.0));
    }

    #[test]
    fn swipe_ducks_the_hum_then_restores_it_smoothly() {
        let released = Arc::new(AtomicBool::new(false));
        let ducked = Arc::new(AtomicBool::new(false));
        let mut source = BladeSource::new_with_swipe_duck(86.0, released, ducked.clone());
        source.by_ref().take(SR as usize + 1).for_each(drop);
        ducked.store(true, Ordering::Relaxed);
        source.by_ref().take(4_000).for_each(drop);
        assert!(source.duck_gain < 0.1);
        ducked.store(false, Ordering::Relaxed);
        source.by_ref().take(4_000).for_each(drop);
        assert!(source.duck_gain > 0.99);
    }
}
