//! Rising electrical activation texture, audition only.
//! The packager adds a pitch rise made from the approved complete hold blend.
//! No contact impulse, strike body, descending sweep or reference recording.
use kyber_core::{config::SmallRng, sounds};
use serde_json::json;
use std::{
    f64::consts::{PI, TAU},
    path::PathBuf,
};

const RATE: f64 = sounds::SR as f64;
const LENGTH: f64 = 1.8;
const RISE: f64 = 0.90;
const ACTIVE: f64 = 1.15;

struct Band {
    low: f64,
    band: f64,
}
impl Band {
    fn tick(&mut self, input: f64, hz: f64) -> f64 {
        let f = 2.0 * (PI * hz / RATE).sin();
        self.low += f * self.band;
        let high = input - self.low - self.band / 1.6;
        self.band += f * high;
        self.band
    }
}

struct Lowpass(f64);
impl Lowpass {
    fn tick(&mut self, input: f64, hz: f64) -> f64 {
        self.0 += (1.0 - (-TAU * hz / RATE).exp()) * (input - self.0);
        self.0
    }
}

fn smooth(value: f64) -> f64 {
    let v = value.clamp(0.0, 1.0);
    v * v * (3.0 - 2.0 * v)
}

fn render() -> (Vec<f32>, f64) {
    let mut rng = SmallRng::seed(0x6201);
    let mut phase = 0.0;
    let mut second_phase = 0.7;
    let mut band = Band { low: 0.0, band: 0.0 };
    let mut lowcut = Lowpass(0.0);
    let mut highcut = Lowpass(0.0);
    let mut samples = Vec::with_capacity((RATE * LENGTH) as usize);
    for frame in 0..(RATE * LENGTH) as usize {
        let t = frame as f64 / RATE;
        let progress = smooth(t / RISE);
        // All pitch and formant trajectories rise and then stay level.
        // At the top, this current sits near harmonic 3 of the held motor.
        let frequency = 108.0_f64 * (276.3_f64 / 108.0).powf(progress);
        phase += TAU * frequency / RATE;
        second_phase += TAU * frequency * 1.007 / RATE;
        let current = (2.7 * (phase.sin() + 0.12 * (2.0 * phase).sin())).tanh();
        let other = (2.1 * second_phase.sin()).tanh();
        // Quiet grain follows the current. It has no separate impulse or hit.
        let grain = rng.range(-1.0, 1.0) * (0.25 + 0.75 * phase.sin().powi(2));
        let resonance = band.tick(0.75 * current + 0.25 * other + 0.08 * grain,
                                  320.0 * (1550.0_f64 / 320.0).powf(progress));
        let load = 0.94 + 0.035 * (TAU * 4.7 * t).sin()
            + 0.025 * (TAU * 13.1 * t + 0.4).sin();
        let value = (0.50 * current + 0.28 * other + 0.12 * resonance + 0.025 * grain) * load;
        let coloured = highcut.tick(value, 1100.0 + 2400.0 * progress);
        let ac = coloured - lowcut.tick(coloured, 35.0);
        // Energy grows through the ascent. The texture then dissolves into
        // the held sound; it never drops in pitch or produces an impact tail.
        let swell = smooth(t / 0.12) * (0.25 + 0.75 * progress);
        let handoff = 1.0 - smooth((t - RISE) / (ACTIVE - RISE));
        samples.push((ac * swell * handoff) as f32);
    }
    let active = &samples[..(ACTIVE * RATE) as usize];
    let rms = (active.iter().map(|v| (*v as f64).powi(2)).sum::<f64>()
        / active.len() as f64).sqrt();
    let peak = active.iter().map(|v| v.abs() as f64).fold(0.0, f64::max);
    // Leave the approved hum at its original level when both are combined.
    let gain = (0.065 / rms.max(1e-12)).min(0.19 / peak.max(1e-12));
    samples.iter_mut().for_each(|v| *v *= gain as f32);
    (samples, gain)
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let out = PathBuf::from(std::env::args_os().nth(1)
        .ok_or("usage: ignition_drafts NEW_OUTPUT_DIRECTORY")?);
    std::fs::create_dir(&out)?;
    let (samples, gain) = render();
    sounds::save_wav(&out.join("rising-current-texture.wav"), &samples)?;
    let manifest = json!([{
        "id": "rising-current", "label": "Rising current",
        "description": "The complete layered hum climbs from a low pitch into its normal pitch over 0.9 seconds, with a growing electrical current alongside it.",
        "file": "rising-current-texture.wav", "file_seconds": LENGTH,
        "active_seconds": ACTIVE, "rise_seconds": RISE,
        "hum_pitch_start_ratio": 0.38, "hum_pitch_end_ratio": 1.0,
        "current_pitch_hz": [108.0, 276.3], "resonance_hz": [320.0, 1550.0],
        "synthesis_gain": gain,
        "sources": ["approved complete hum, rising in pitch", "two loaded rising currents", "rising electrical resonance", "quiet current-gated grain"],
        "seed": 0x6201,
    }]);
    std::fs::write(out.join("ignitions.json"), serde_json::to_vec_pretty(&manifest)?)?;
    println!("Exported one rising activation texture to {}", out.display());
    Ok(())
}
