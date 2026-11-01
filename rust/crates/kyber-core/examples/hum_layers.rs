//! A layered sound-design workbench. No recordings or installed runtime changes.
//! Each source has its own excitation, filtering, pitch motion and envelope.
use kyber_core::{config::SmallRng, sounds};
use serde_json::json;
use std::{
    f64::consts::{PI, TAU},
    path::PathBuf,
};

const SECONDS: usize = 12;
const RATE: f64 = sounds::SR as f64;
// Keep revision 2's calibration for focused comparisons of individual sources.
const STEM_GAIN: f32 = f32::from_bits(0x3f85_8080);

struct Lowpass {
    value: f64,
}
impl Lowpass {
    fn new() -> Self {
        Self { value: 0.0 }
    }
    fn tick(&mut self, input: f64, hz: f64) -> f64 {
        self.value += (1.0 - (-TAU * hz / RATE).exp()) * (input - self.value);
        self.value
    }
}

/// Smooth independent random motion; rates are in Hz, independent of sample rate.
struct Drift {
    value: f64,
    target: f64,
    remaining: usize,
    rng: SmallRng,
}
impl Drift {
    fn new(seed: u64) -> Self {
        Self {
            value: 0.0,
            target: 0.0,
            remaining: 0,
            rng: SmallRng::seed(seed),
        }
    }
    fn tick(&mut self, hz: f64) -> f64 {
        if self.remaining == 0 {
            self.target = self.rng.range(-1.0, 1.0);
            self.remaining = (RATE * self.rng.range(0.18, 0.75)) as usize;
        }
        self.remaining -= 1;
        self.value += (self.target - self.value) * (1.0 - (-TAU * hz / RATE).exp());
        self.value
    }
}

/// A driven resonant band, rather than a clean sine oscillator.
struct Band {
    low: f64,
    band: f64,
}
impl Band {
    fn new() -> Self {
        Self {
            low: 0.0,
            band: 0.0,
        }
    }
    fn tick(&mut self, input: f64, hz: f64, q: f64) -> f64 {
        let f = 2.0 * (PI * hz / RATE).sin();
        self.low += f * self.band;
        let high = input - self.low - self.band / q;
        self.band += f * high;
        self.band
    }
}

fn motor(frames: usize) -> Vec<f32> {
    let mut pitch = Drift::new(0x1101);
    let mut load = Drift::new(0x1102);
    let mut grain_rng = SmallRng::seed(0x1103);
    let mut grain = Lowpass::new();
    let mut phase = 0.0;
    let mut whirr_phase = 0.0;
    let mut case = Band::new();
    (0..frames)
        .map(|i| {
            let t = i as f64 / RATE;
            phase += TAU * (92.3 + 0.19 * pitch.tick(0.55) + 0.045 * (TAU * 0.71 * t).sin()) / RATE;
            whirr_phase += TAU * (184.9 + 0.13 * (TAU * 0.23 * t).sin()) / RATE;
            let torque = 0.89 + 0.10 * load.tick(0.8) + 0.045 * (TAU * 0.63 * t).sin();
            let shaft = phase.sin()
                + 0.10 * (2.0 * phase + 0.8).sin()
                + 0.06 * (3.0 * phase + 2.1).sin()
                + 0.045 * whirr_phase.sin();
            let bearing = grain.tick(grain_rng.range(-1.0, 1.0), 360.0);
            let housing = case.tick(
                bearing * (0.6 + 0.4 * (0.5 * phase).sin().abs()),
                148.0,
                1.6,
            );
            ((shaft * torque + housing * 0.07) * (1.0 + 0.025 * (phase * 0.5).sin())) as f32
        })
        .collect()
}

fn electrical(frames: usize) -> Vec<f32> {
    let mut pitch = Drift::new(0x2201);
    let mut width = Drift::new(0x2202);
    let mut energy = Drift::new(0x2203);
    let mut rng = SmallRng::seed(0x2204);
    let mut phase = 0.0;
    let mut phase_b = 0.8;
    let mut dc = Lowpass::new();
    let mut hiss = Lowpass::new();
    let mut bank = [Band::new(), Band::new(), Band::new()];
    let mut highpass = [Lowpass::new(), Lowpass::new()];
    (0..frames)
        .map(|i| {
            let t = i as f64 / RATE;
            phase += TAU * (92.65 + 0.35 * pitch.tick(0.36)) / RATE;
            phase_b += TAU * (92.12 + 0.11 * (TAU * 0.31 * t + 0.6).sin()) / RATE;
            let asymmetry = 0.14 * width.tick(0.6) + 0.07 * (TAU * 0.27 * t).sin();
            // Two softly switched, independently drifting current sources. Their
            // higher harmonics beat at different rates; no repeated attack clip.
            let a = (4.3 * (phase.sin() + asymmetry)).tanh();
            let b = (3.6 * (phase_b.sin() - 0.65 * asymmetry)).tanh();
            let noise = hiss.tick(rng.range(-1.0, 1.0), 1700.0);
            let driven = a * 0.66 + b * 0.34 + noise * 0.10 * (0.25 + phase.sin().abs());
            let ac = driven - dc.tick(driven, 110.0);
            let rasp = ac - highpass[0].tick(ac, 170.0);
            let rasp = rasp - highpass[1].tick(rasp, 170.0);
            // Separate resonances change strength independently, making timbre
            // evolve instead of putting one volume wobble over a fixed spectrum.
            let third = bank[0].tick(rasp, 277.0 + 1.4 * (TAU * 0.17 * t).sin(), 2.8)
                * (0.50 + 0.40 * (TAU * 0.43 * t + 0.4).sin());
            let fourth = bank[1].tick(rasp, 369.5 + 1.2 * (TAU * 0.29 * t).sin(), 2.1)
                * (0.58 + 0.28 * (TAU * 0.23 * t + 2.0).sin());
            let fifth = bank[2].tick(rasp, 463.0 + 1.8 * (TAU * 0.13 * t).sin(), 2.5)
                * (0.50 + 0.38 * (TAU * 0.61 * t + 3.2).sin());
            ((0.18 * rasp + 0.46 * third + 0.32 * fourth + 0.24 * fifth)
                * (0.9 + 0.16 * energy.tick(0.7))) as f32
        })
        .collect()
}

fn undertone(frames: usize) -> Vec<f32> {
    let mut drift = Drift::new(0x3301);
    let mut load = Drift::new(0x3302);
    let mut rng = SmallRng::seed(0x3303);
    let mut phase = 1.7;
    let mut band = Band::new();
    let mut low = Lowpass::new();
    (0..frames)
        .map(|i| {
            let t = i as f64 / RATE;
            phase += TAU * (37.7 + 0.12 * drift.tick(0.25)) / RATE;
            let wheel = (2.0 * phase + 0.45 * phase.sin()).sin();
            let flutter = 0.82 + 0.12 * (TAU * 1.37 * t).sin() + 0.14 * load.tick(0.48);
            let rubbed = band.tick(
                rng.range(-1.0, 1.0),
                190.0 + 35.0 * (TAU * 0.37 * t).sin(),
                1.7,
            );
            let uneven = (1.8 * (wheel + 0.2 * phase.sin())).tanh()
                + 0.1 * (3.0 * phase + 0.7).sin()
                + 0.13 * rubbed * (0.5 + 0.5 * phase.cos().abs());
            low.tick(uneven * flutter, 330.0) as f32
        })
        .collect()
}

fn electrical_crackle(frames: usize) -> Vec<f32> {
    let mut movement = Drift::new(0x4401);
    let mut level = Drift::new(0x4402);
    let mut rng = SmallRng::seed(0x4403);
    let mut bands = [Band::new(), Band::new()];
    let mut hum_low = Lowpass::new();
    let mut hum_dc = Lowpass::new();
    let mut highcut = Lowpass::new();
    let mut dc = Lowpass::new();
    let mut phase = 0.3;
    let mut phase_b = 1.1;
    let mut charge = 0.0;
    let mut discharge_decay = 0.0;
    (0..frames)
        .map(|i| {
            let t = i as f64 / RATE;
            let motion = movement.tick(0.45);
            let energy = level.tick(0.65);
            // A sustained saturated current gives the texture a humming body.
            phase += TAU * (91.8 + 0.18 * motion) / RATE;
            phase_b += TAU * (183.3 + 0.13 * (TAU * 0.41 * t).sin()) / RATE;
            let current = (2.2
                * (phase.sin() + 0.19 * phase_b.sin() + 0.07 * (3.0 * phase + 0.8).sin()))
            .tanh();
            let hum = hum_low.tick(current, 720.0);
            let hum = hum - hum_dc.tick(hum, 58.0);
            // Dense short discharges excite low/mid resonances. Each decays
            // independently of the slower motion and is gated by the current.
            let density = 110.0 + 190.0 * (0.5 + 0.5 * (TAU * 0.73 * t + energy).sin());
            if rng.next_f64() < density / RATE {
                charge += rng.range(-1.0, 1.0);
                discharge_decay = (-1.0 / (RATE * rng.range(0.0012, 0.0045))).exp();
            }
            charge *= discharge_decay;
            let gate = 0.30 + 0.70 * phase.sin().abs().powi(3);
            let discharge = charge.tanh() * gate;
            let low_arc = bands[0].tick(discharge, 310.0 + 35.0 * motion, 1.9);
            let mid_arc = bands[1].tick(discharge, 690.0 + 65.0 * energy, 1.5);
            let arcing = (3.1 * low_arc + 1.0 * mid_arc).tanh();
            // Only a trace of broadband grain remains within the gated texture.
            let grain = 0.008 * rng.range(-1.0, 1.0) * gate;
            let body = (0.48 * hum + 0.52 * arcing + grain) * (0.86 + 0.16 * energy);
            let coloured = highcut.tick(body, 1000.0);
            (coloured - dc.tick(coloured, 65.0)) as f32
        })
        .collect()
}

fn finish(mut source: Vec<f32>, target_rms: f64, resonance: f64) -> Vec<f32> {
    let dry = source.clone();
    let delays = [(0.0073, 0.50), (0.0187, 0.28), (0.0311, 0.16)];
    // Short mono housing reflections add physical colour, with no stereo
    // widening or long room tail standing in for layered source character.
    let mut damping = Lowpass::new();
    for i in 0..source.len() {
        let reflected: f64 = delays
            .iter()
            .map(|(seconds, weight)| {
                i.checked_sub((seconds * RATE) as usize)
                    .map_or(0.0, |at| dry[at] as f64 * weight)
            })
            .sum();
        source[i] += (resonance * damping.tick(reflected, 900.0)) as f32;
    }
    let trim = (RATE * 0.3) as usize;
    let body = &source[trim..source.len() - trim];
    let rms = (body.iter().map(|v| (*v as f64).powi(2)).sum::<f64>() / body.len() as f64).sqrt();
    let gain = target_rms / rms.max(1e-12);
    let length = source.len();
    for (i, sample) in source.iter_mut().enumerate() {
        let fade_in = (i as f64 / (RATE * 0.10)).min(1.0);
        let fade_out = ((length - 1 - i) as f64 / (RATE * 0.16)).min(1.0);
        let edge = (0.5 - 0.5 * (PI * fade_in).cos()) * (0.5 - 0.5 * (PI * fade_out).cos());
        *sample *= (gain * edge) as f32;
    }
    source
}

fn stats(samples: &[f32]) -> (f64, f64) {
    let trim = (RATE * 0.3) as usize;
    let body = &samples[trim..samples.len() - trim];
    let rms = (body.iter().map(|v| (*v as f64).powi(2)).sum::<f64>() / body.len() as f64).sqrt();
    let peak = samples.iter().map(|v| v.abs() as f64).fold(0.0, f64::max);
    (rms, peak)
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let out = PathBuf::from(
        std::env::args_os()
            .nth(1)
            .ok_or("usage: hum_layers NEW_OUTPUT_DIRECTORY")?,
    );
    std::fs::create_dir(&out)?;
    let frames = SECONDS * sounds::SR as usize;
    let mut stems = vec![
        ("motor", "Low motor", finish(motor(frames), 0.17, 0.13)),
        (
            "electrical",
            "Electrical rasp",
            finish(electrical(frames), 0.105, 0.08),
        ),
        (
            "undertone",
            "Lower throb",
            finish(undertone(frames), 0.065, 0.20),
        ),
        (
            "crackle",
            "Electrical crackle",
            finish(electrical_crackle(frames), 0.024, 0.12),
        ),
    ];
    let mut mix: Vec<f32> = (0..frames)
        .map(|i| stems.iter().map(|(_, _, s)| s[i]).sum())
        .collect();
    for (_, _, samples) in &mut stems {
        samples.iter_mut().for_each(|v| *v *= STEM_GAIN);
    }
    mix.iter_mut().for_each(|v| *v *= STEM_GAIN);
    let mut layers = Vec::new();
    for (id, name, samples) in &stems {
        let file = format!("{id}.wav");
        sounds::save_wav(&out.join(&file), samples)?;
        let (rms, peak) = stats(samples);
        layers.push(
            json!({"id":id,"name":name,"file":file,"rms":rms,"peak":peak,"default_gain":1.0}),
        );
    }
    sounds::save_wav(&out.join("full-hum.wav"), &mix)?;
    let (rms, peak) = stats(&mix);
    std::fs::write(
        out.join("layers.json"),
        serde_json::to_vec_pretty(&json!({
            "version":3,"sample_rate":sounds::SR,"channels":1,"seconds":SECONDS,
            "layers":layers,"mix":{"file":"full-hum.wav","rms":rms,"peak":peak},
            "source":"Four independent original procedural sources; no input recordings",
            "seed_policy":"Fixed independent seeds per source for reproducible auditions",
            "level_calibration":"Revision 2 source gains preserved for focused comparison",
            "runtime_changed":false
        }))?,
    )?;
    println!(
        "Exported four independent sources and their full hum to {}",
        out.display()
    );
    Ok(())
}
