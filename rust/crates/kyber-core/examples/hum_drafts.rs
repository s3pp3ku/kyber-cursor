//! Fresh hum auditions from a broad tonal brief. No reference waveform input.
//! These are design drafts; the installed HoldingHum oscillator is untouched.
use kyber_core::{config::SmallRng, sounds};
use serde_json::json;
use std::{f64::consts::TAU, path::PathBuf};

struct Design {
    name: &'static str,
    label: &'static str,
    primary_hz: f64,
    body_hz: f64,
    primary: [f64; 10],
    body: [f64; 8],
}

fn render(design: &Design, seconds: usize) -> (Vec<f32>, f64) {
    let rate = sounds::SR as f64;
    let mut rng = SmallRng::seed(0x4b59_4245_5248_554d);
    // New phases and noise: no extracted phases, samples, or looped recordings.
    let phases: Vec<_> = (0..18).map(|_| rng.next_f64() * TAU).collect();
    let mut low_noise = 0.0;
    let mut dc = 0.0;
    let frames = seconds * sounds::SR as usize;
    let mut samples = Vec::with_capacity(frames);
    for frame in 0..frames {
        let t = frame as f64 / rate;
        // Barely perceptible slow drift, rather than the live version's
        // 7 Hz vibrato and second detuned copy of the entire harmonic stack.
        let phase = TAU * design.primary_hz * t
            + design.primary_hz * 0.00012 / 0.41 * (1.0 - (TAU * 0.41 * t).cos());
        let body_phase = TAU * design.body_hz * t
            + design.body_hz * 0.00015 / 0.29 * (1.0 - (TAU * 0.29 * t).cos());
        let primary: f64 = design
            .primary
            .iter()
            .enumerate()
            .map(|(i, amplitude)| amplitude * ((i + 1) as f64 * phase + phases[i]).sin())
            .sum();
        let body: f64 = design
            .body
            .iter()
            .enumerate()
            .map(|(i, amplitude)| amplitude * ((i + 1) as f64 * body_phase + phases[10 + i]).sin())
            .sum();
        low_noise += (rng.range(-1.0, 1.0) - low_noise) * 0.075;
        dc += (low_noise - dc) * 0.003;
        let breath = 1.0 + 0.04 * (TAU * 0.57 * t).sin() + 0.025 * (TAU * 0.19 * t).sin();
        let fade_in = (t / 0.07).min(1.0);
        let fade_out = ((seconds as f64 - 1.0 / rate - t) / 0.10).clamp(0.0, 1.0);
        let edge = (0.5 - 0.5 * (std::f64::consts::PI * fade_in).cos())
            * (0.5 - 0.5 * (std::f64::consts::PI * fade_out).cos());
        samples.push(((primary + body + 0.025 * (low_noise - dc)) * breath * edge) as f32);
    }
    // Match sustained average level across drafts; never clip to get there.
    let trim = (rate * 0.2) as usize;
    let held = &samples[trim..samples.len() - trim];
    let rms = (held.iter().map(|v| (*v as f64).powi(2)).sum::<f64>() / held.len() as f64).sqrt();
    let peak = samples.iter().map(|v| v.abs() as f64).fold(0.0, f64::max);
    let gain = (0.22 / rms).min(0.85 / peak);
    for sample in &mut samples {
        *sample *= gain as f32;
    }
    (samples, gain)
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let out = PathBuf::from(
        std::env::args_os()
            .nth(1)
            .ok_or("usage: hum_drafts NEW_OUTPUT_DIRECTORY")?,
    );
    std::fs::create_dir(&out)?;
    let designs = [
        Design {
            name: "01-layered",
            label: "1 · Layered",
            primary_hz: 92.0,
            body_hz: 37.5,
            primary: [1.0, 0.06, 0.42, 0.28, 0.24, 0.025, 0.02, 0.02, 0.025, 0.02],
            body: [0.04, 0.33, 0.06, 0.035, 0.035, 0.02, 0.016, 0.012],
        },
        Design {
            name: "02-rounded",
            label: "2 · Rounded",
            primary_hz: 90.0,
            body_hz: 36.5,
            primary: [1.0, 0.45, 0.21, 0.22, 0.09, 0.03, 0.025, 0.015, 0.01, 0.01],
            body: [0.025, 0.23, 0.04, 0.035, 0.025, 0.02, 0.012, 0.01],
        },
        Design {
            name: "03-balanced",
            label: "3 · Balanced",
            primary_hz: 91.0,
            body_hz: 37.0,
            primary: [
                1.0, 0.25, 0.31, 0.25, 0.16, 0.025, 0.02, 0.018, 0.018, 0.015,
            ],
            body: [0.03, 0.28, 0.05, 0.035, 0.03, 0.02, 0.014, 0.01],
        },
    ];
    let mut manifest = Vec::new();
    for design in designs {
        let (samples, gain) = render(&design, 9);
        let filename = format!("{}.wav", design.name);
        sounds::save_wav(&out.join(&filename), &samples)?;
        manifest.push(json!({
            "file": filename, "label": design.label, "primary_hz": design.primary_hz,
            "body_hz": design.body_hz, "primary_harmonics": design.primary,
            "body_harmonics": design.body, "gain": gain,
            "seconds": 9, "sample_rate": sounds::SR,
            "source": "New additive oscillators and seeded filtered noise; no reference audio input",
        }));
    }
    std::fs::write(
        out.join("designs.json"),
        serde_json::to_vec_pretty(&manifest)?,
    )?;
    println!(
        "Exported three fresh nine-second hum drafts to {}",
        out.display()
    );
    Ok(())
}
