//! Original comparison-video ignition: motor growl, resonant sweep, electric hum.
//! Synthesized entirely in Rust, without recordings or external samples.
use kyber_core::{config::SmallRng, sounds};
use std::{f64::consts::TAU, path::PathBuf};

fn ignition(seed: u64, pitch: f64) -> Vec<f32> {
    let rate = sounds::SR as f64;
    let duration = 1.45;
    let mut rng = SmallRng::seed(seed);
    let mut phase = 0.0;
    let mut sub_phase = 0.0;
    let mut detuned = 0.0;
    let mut sweep_phase = 0.0;
    let mut low_noise = 0.0;
    let mut band = 0.0;
    let mut low = 0.0;
    let mut track = Vec::with_capacity((duration * rate) as usize);
    for i in 0..(duration * rate) as usize {
        let t = i as f64 / rate;
        let start = 1.0 - (-t / 0.026).exp();
        let fundamental = pitch * (0.56 + 0.44 * start + 0.32 * (-t / 0.20).exp());
        let wobble = 1.0 + 0.006 * (TAU * 5.4 * t).sin();
        phase = (phase + TAU * fundamental * wobble / rate) % TAU;
        sub_phase = (sub_phase + TAU * fundamental * 0.5 * wobble / rate) % TAU;
        detuned = (detuned + TAU * (fundamental * 1.013 + 0.7) / rate) % TAU;
        let mut motor = 0.0;
        for harmonic in 1..=18 {
            let h = harmonic as f64;
            // Midrange partials carry the growl on phone speakers too.
            let formant = 0.70 + 0.85 * (-((h * fundamental - 570.0) / 340.0).powi(2)).exp();
            motor += formant * (h * phase + 0.30 * (detuned * 2.0).sin()).sin() / h.powf(1.22);
        }
        let hum = 0.44 * motor + 0.19 * detuned.sin() + 0.14 * sub_phase.sin();
        // Short, breath-like opening; textured noise sits below the motor.
        let noise = rng.range(-1.0, 1.0);
        low_noise += 0.12 * (noise - low_noise);
        let opening = (t / 0.018).min(1.0) * (-t / 0.145).exp();
        let center = 450.0 + 1800.0 * (1.0 - (-t / 0.055).exp()) * (-t / 0.24).exp();
        let coefficient = 2.0 * (std::f64::consts::PI * center / rate).sin();
        low += coefficient * band;
        let high = noise - low - band / 1.9;
        band += coefficient * high;
        let breath = 0.22 * band * opening;
        sweep_phase = (sweep_phase + TAU * (125.0 + 530.0 * (-t / 0.10).exp()) / rate) % TAU;
        let growl = 0.28 * (sweep_phase + 1.4 * phase.sin()).sin() * (-t / 0.22).exp();
        let snap = 0.26 * low_noise * (-t / 0.010).exp();
        let envelope = (t / 0.004).min(1.0)
            * ((duration - t) / 0.11).clamp(0.0, 1.0)
            * (0.74 + 0.26 * (-t / 0.30).exp());
        let sample = ((hum + breath + growl + snap) * 1.45).tanh() * envelope;
        track.push(sample as f32);
    }
    let peak = track.iter().fold(0.0f32, |m, v| m.max(v.abs()));
    for sample in &mut track {
        *sample *= sounds::PEAK / peak;
    }
    track
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let directory = PathBuf::from(
        std::env::args_os()
            .nth(1)
            .ok_or("supply an output directory")?,
    );
    for (index, (seed, pitch)) in [(111, 103.0), (223, 110.0), (337, 97.0)]
        .into_iter()
        .enumerate()
    {
        let track = ignition(seed, pitch);
        assert!(track.iter().all(|sample| sample.is_finite()));
        sounds::save_wav(&directory.join(format!("ignite-{}.wav", index + 1)), &track)?;
    }
    Ok(())
}
