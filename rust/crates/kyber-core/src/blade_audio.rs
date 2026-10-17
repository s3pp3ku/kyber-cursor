//! Original continuous blade hum synthesis. Recorded attacks never enter it.
use std::f64::consts::TAU;

pub struct HoldingHum {
    base: f64,
    rate: u32,
    frame: u64,
    phase: f64,
    detuned_phase: f64,
    texture_state: u64,
    pitch_wander: f64,
}
impl HoldingHum {
    pub fn new(base: f64, rate: u32) -> Self {
        Self::new_seeded(base, rate, base.to_bits() ^ rate as u64)
    }

    pub fn new_seeded(base: f64, rate: u32, seed: u64) -> Self {
        assert!(base.is_finite() && (40.0..=160.0).contains(&base));
        assert!(rate >= 8000);
        Self {
            base,
            rate,
            frame: 0,
            phase: 0.0,
            detuned_phase: 0.0,
            texture_state: if seed == 0 {
                0x9e37_79b9_7f4a_7c15
            } else {
                seed
            },
            pitch_wander: 0.0,
        }
    }
}
impl Iterator for HoldingHum {
    type Item = f32;
    fn next(&mut self) -> Option<f32> {
        let t = self.frame as f64 / self.rate as f64;
        // Smooth, non-repeating pitch drift keeps the blade from sounding like
        // a fixed alarm tone. Each held press gets a fresh, very small noise
        // texture; the filtered wander changes slowly and never clicks.
        self.texture_state ^= self.texture_state << 13;
        self.texture_state ^= self.texture_state >> 7;
        self.texture_state ^= self.texture_state << 17;
        let noise = ((self.texture_state >> 11) as f64 / ((1_u64 << 53) as f64)) * 2.0 - 1.0;
        self.pitch_wander += (noise - self.pitch_wander) * 0.00005;
        let slow_drift = 0.004 * (TAU * 0.37 * t).sin() + 0.0025 * (TAU * 0.13 * t).sin();
        let frequency = self.base
            * (1.0 + 0.012 * (TAU * 7.0 * t).sin() + slow_drift + 3.0 * self.pitch_wander);
        self.phase += TAU * frequency / self.rate as f64;
        self.detuned_phase += TAU * frequency * 1.012 / self.rate as f64;
        let mut sample = 0.0;
        for harmonic in 1..=6 {
            let weight = (harmonic as f64).powf(1.15);
            sample += 0.72 * (harmonic as f64 * self.phase).sin() / weight;
            sample += 0.28 * (harmonic as f64 * self.detuned_phase).sin() / weight;
        }
        let texture = (0.92 + 0.045 * (TAU * 2.3 * t).sin() + 0.035 * (TAU * 1.71 * t).sin())
            * (1.0 + 0.7 * self.pitch_wander);
        self.frame += 1;
        Some((sample * texture * 0.45) as f32)
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn long_holds_have_no_attack_spikes_or_loop_boundary() {
        for pitch in [78.0, 86.0, 94.0] {
            let a: Vec<_> = HoldingHum::new(pitch, 44100).take(44100 * 12).collect();
            assert!(a.iter().all(|s| s.is_finite() && s.abs() < 1.0));
            assert!(a.windows(2).all(|p| (p[1] - p[0]).abs() < 0.06));
            let levels: Vec<_> = a
                .chunks_exact(4410)
                .map(|w| (w.iter().map(|s| s * s).sum::<f32>() / w.len() as f32).sqrt())
                .collect();
            let minimum = levels.iter().copied().fold(f32::INFINITY, f32::min);
            let maximum = levels.iter().copied().fold(0.0, f32::max);
            assert!(maximum / minimum < 1.6, "unstable hum {maximum}/{minimum}");
        }
    }
    #[test]
    fn pitch_variation_is_fixed_for_each_hold() {
        let a: Vec<_> = HoldingHum::new(78.0, 44100).take(1000).collect();
        let b: Vec<_> = HoldingHum::new(86.0, 44100).take(1000).collect();
        assert_ne!(a, b);
        assert_eq!(
            a,
            HoldingHum::new(78.0, 44100).take(1000).collect::<Vec<_>>()
        );
    }
}
