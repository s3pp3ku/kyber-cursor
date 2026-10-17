//! Original sound pack, synthesized from scratch. Port of `lib/make_sounds.py`.
//!
//! Same synthesis graph (buzzing hum + swept resonant noise + snap/crack +
//! metallic ring), same durations, same filenames. The PRNG differs from
//! CPython's Mersenne Twister (`SmallRng` is used so `kyber-core` needs no
//! `rand` dependency), so bytes are not bit-identical to the Python output —
//! but they are deterministic per seed, original, and MIT like the rest of
//! the project. Nothing is sampled from anywhere.
//!
//! A speed-sensitive synthesized `swipe` is also provided for fast drags
//! while holding; it is generated at runtime, not written to disk.

use std::path::{Path, PathBuf};

use crate::config::SmallRng;

pub const SR: u32 = 44_100;
pub const PEAK: f32 = 0.85;
const TAU: f32 = 2.0_f32 * std::f32::consts::PI;

fn hum(n: usize, mut f0_fn: impl FnMut(f32) -> f32) -> Vec<f32> {
    // Port of make_sounds.hum: harmonics with vibrato + slow shimmer.
    let mut out = Vec::with_capacity(n);
    let mut phase = 0.0f32;
    for i in 0..n {
        let t = i as f32 / SR as f32;
        let f = f0_fn(t) * (1.0f32 + 0.012f32 * (TAU * 7.0f32 * t).sin());
        phase += TAU * f / SR as f32;
        let mut v = 0.0f32;
        for k in 1..=6 {
            v += (k as f32 * phase).sin() / (k as f32).powf(1.15f32);
        }
        // slow beating between the harmonics, so it shimmers a little
        v *= 1.0f32 - 0.6f32 * 0.25f32 * (1.0f32 + (TAU * 2.3f32 * t).sin());
        out.push(v * 0.45f32);
    }
    out
}

fn swept_noise(
    n: usize,
    mut fc_fn: impl FnMut(f32) -> f32,
    q: f32,
    rng: &mut SmallRng,
) -> Vec<f32> {
    // Port of make_sounds.swept_noise: white noise through a resonant
    // band-pass (Chamberlin SVF) with a moving center frequency.
    let mut low = 0.0f32;
    let mut band = 0.0f32;
    let q1 = 1.0f32 / q;
    let mut out = Vec::with_capacity(n);
    for i in 0..n {
        let t = i as f32 / SR as f32;
        let fc = fc_fn(t).min(SR as f32 / 6.0f32);
        let f = 2.0f32 * (std::f32::consts::PI * fc / SR as f32).sin();
        let x = rng.range(-1.0f64, 1.0f64) as f32;
        low += f * band;
        let high = x - low - q1 * band;
        band += f * high;
        out.push(band);
    }
    out
}

fn mix(tracks: &[Vec<f32>]) -> Vec<f32> {
    let n = tracks.iter().map(|t| t.len()).max().unwrap_or(0);
    let mut out = vec![0.0f32; n];
    for t in tracks {
        for (i, v) in t.iter().enumerate() {
            out[i] += v;
        }
    }
    out
}

fn scale(track: Vec<f32>, gain: f32) -> Vec<f32> {
    track.into_iter().map(|v| v * gain).collect()
}

fn shape(track: Vec<f32>, f: impl Fn(f32) -> f32) -> Vec<f32> {
    track
        .into_iter()
        .enumerate()
        .map(|(i, v)| v * f(i as f32 / SR as f32))
        .collect()
}

fn normalize(mut track: Vec<f32>, peak: f32) -> Vec<f32> {
    let m = track
        .iter()
        .map(|v| v.abs())
        .fold(0.0f32, f32::max)
        .max(1e-9f32);
    for v in &mut track {
        *v *= peak / m;
    }
    track
}

fn fade_edges(mut track: Vec<f32>, fade_in: f32, fade_out: f32) -> Vec<f32> {
    let n = track.len();
    let a = (SR as f32 * fade_in) as usize;
    let b = (SR as f32 * fade_out) as usize;
    for (i, v) in track.iter_mut().enumerate().take(a.min(n)) {
        *v *= i as f32 / a as f32;
    }
    for (i, v) in track.iter_mut().rev().enumerate().take(b.min(n)) {
        *v *= i as f32 / b as f32;
    }
    track
}

/// Blade powering on: snap + rising whoosh + settling hum. 1.3 s.
pub fn ignite(seed: u64, base: f32, length: f32) -> Vec<f32> {
    let mut rng = SmallRng::seed(seed);
    let n = (SR as f32 * length) as usize;
    let glide = |t: f32| base * (0.55f32 + 0.45f32 * (1.0f32 - (-t / 0.18f32).exp()));
    let h = hum(n, glide);
    let h = shape(h, |t| {
        (t / 0.05f32).min(1.0f32)
            * (if t < length - 0.3f32 {
                1.0f32
            } else {
                (-(t - (length - 0.3f32)) / 0.10f32).exp()
            })
    });
    let w = swept_noise(
        n,
        |t| if t < 0.5f32 { 260.0f32 * (t * 5.2f32).exp() } else { 3500.0f32 },
        4.0f32,
        &mut rng,
    );
    let w = shape(w, |t| {
        (t / 0.06f32).min(1.0f32) * (-(0.0f32.max(t - 0.5f32)) / 0.25f32).exp() * 0.55f32
    });
    let snap_n = (SR as f32 * 0.05f32) as usize;
    let snap: Vec<f32> = (0..snap_n)
        .map(|i| rng.range(-1.0f64, 1.0f64) as f32 * (-(i as f32 / SR as f32) / 0.012f32).exp())
        .collect();
    // Pad snap to n so mix() lines up.
    let mut snap_full = vec![0.0f32; n];
    for (i, v) in snap.into_iter().enumerate() {
        snap_full[i] = v;
    }
    normalize(fade_edges(mix(&[h, w, scale(snap_full, 0.7f32)]), 0.004f32, 0.03f32), PEAK)
}

/// Blade powering off: falling hum + whoosh. 0.8 s.
pub fn off(seed: u64, base: f32, length: f32) -> Vec<f32> {
    let mut rng = SmallRng::seed(seed);
    let n = (SR as f32 * length) as usize;
    let fall = |t: f32| base * (0.45f32 + 0.55f32 * (-t / 0.2f32).exp());
    let h = hum(n, fall);
    let h = shape(h, |t| (-t / 0.22f32).exp());
    let w = swept_noise(n, |t| 2600.0f32 * (-t * 4.5f32).exp() + 160.0f32, 4.0f32, &mut rng);
    let w = shape(w, |t| (-t / 0.2f32).exp() * 0.5f32);
    normalize(fade_edges(mix(&[h, w]), 0.004f32, 0.08f32), PEAK)
}

/// One strike: noise crack + metallic ring + swelling hum. 0.7 s.
pub fn clash(seed: u64, length: f32) -> Vec<f32> {
    let mut rng = SmallRng::seed(seed);
    let n = (SR as f32 * length) as usize;
    let crack: Vec<f32> = (0..n)
        .map(|i| rng.range(-1.0f64, 1.0f64) as f32 * (-(i as f32 / SR as f32) / 0.045f32).exp())
        .collect();
    let partials = [
        rng.range(1100.0f64, 1500.0f64) as f32,
        rng.range(1800.0f64, 2300.0f64) as f32,
        rng.range(2700.0f64, 3400.0f64) as f32,
        rng.range(4100.0f64, 4900.0f64) as f32,
    ];
    let mut ring = vec![0.0f32; n];
    for (k, &f) in partials.iter().enumerate() {
        let tau = 0.22f32 - k as f32 * 0.04f32;
        let amp = 0.5f32 / (k as f32 + 1.0f32);
        let ph = rng.range(0.0f64, TAU as f64) as f32;
        for (i, v) in ring.iter_mut().enumerate() {
            let t = i as f32 / SR as f32;
            *v += amp * (TAU * f * t + ph).sin() * (-t / tau).exp();
        }
    }
    let h = hum(n, |t| 96.0f32 * (1.0f32 + 0.15f32 * (-t / 0.1f32).exp()));
    let h = shape(h, |t| 0.8f32 * (-t / 0.28f32).exp());
    normalize(
        fade_edges(mix(&[scale(crack, 1.0f32), scale(ring, 0.8f32), h]), 0.004f32, 0.03f32),
        PEAK,
    )
}

/// Two strikes with a gap. A `2-*` file already holds both and plays once.
pub fn double_clash(seed_a: u64, seed_b: u64, gap: f32) -> Vec<f32> {
    let a = clash(seed_a, 0.6f32);
    let b = clash(seed_b, 0.7f32);
    let off_samples = (SR as f32 * gap) as usize;
    let mut out = vec![0.0f32; off_samples + b.len()];
    for (i, v) in a.into_iter().enumerate() {
        out[i] += v;
    }
    for (i, v) in b.into_iter().enumerate() {
        out[off_samples + i] += v * 0.95f32;
    }
    normalize(fade_edges(out, 0.004f32, 0.03f32), PEAK)
}

/// Original short blade slash, varied by cursor speed and seed. Faster swipes
/// are brighter and tighter; filtered noise gives each pass a fresh texture.
pub fn swipe(seed: u64, speed: f32) -> Vec<f32> {
    let mut rng = SmallRng::seed(seed);
    let force = ((speed - 650.0f32) / 3500.0f32).clamp(0.0f32, 1.0f32);
    let length = (0.29f32 - 0.09f32 * force + rng.range(-0.025f64, 0.025f64) as f32)
        .clamp(0.16f32, 0.34f32);
    let n = (SR as f32 * length) as usize;
    let low = 260.0f32 + rng.range(0.0f64, 220.0f64) as f32;
    let peak = 1700.0f32 + force * 1500.0f32 + rng.range(-250.0f64, 250.0f64) as f32;
    let phase = rng.range(0.0f64, TAU as f64) as f32;
    let noise = swept_noise(
        n,
        |t| {
            let u = (t / length).clamp(0.0f32, 1.0f32);
            low + (peak - low) * (std::f32::consts::PI * u).sin().abs().powf(0.72f32)
                + 90.0f32 * (TAU * 5.0f32 * t + phase).sin()
        },
        2.6f32,
        &mut rng,
    );
    let noise = shape(noise, |t| {
        let u = (t / length).clamp(0.0f32, 1.0f32);
        (t / 0.006f32).min(1.0f32) * (1.0f32 - u).powf(1.25f32)
            * (0.75f32 + 0.25f32 * (phase + u * 8.0f32).sin())
    });
    let body = hum(n, |t| 150.0f32 - 45.0f32 * (t / length).clamp(0.0f32, 1.0f32));
    let body = shape(body, |t| 0.18f32 * (t / 0.008f32).min(1.0f32) * (-(t / length) * 4.0f32).exp());
    normalize(fade_edges(mix(&[noise, body]), 0.002f32, 0.025f32), PEAK)
}

/// Write mono 16-bit WAV. Standard library only.
pub fn save_wav(path: &Path, track: &[f32]) -> std::io::Result<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let mut buf = Vec::with_capacity(44 + track.len() * 2);
    let data_bytes = (track.len() * 2) as u32;
    buf.extend_from_slice(b"RIFF");
    buf.extend_from_slice(&(36 + data_bytes).to_le_bytes());
    buf.extend_from_slice(b"WAVEfmt ");
    buf.extend_from_slice(&16u32.to_le_bytes());
    buf.extend_from_slice(&1u16.to_le_bytes()); // PCM
    buf.extend_from_slice(&1u16.to_le_bytes()); // mono
    buf.extend_from_slice(&SR.to_le_bytes());
    buf.extend_from_slice(&(SR * 2).to_le_bytes()); // byte rate
    buf.extend_from_slice(&2u16.to_le_bytes()); // block align
    buf.extend_from_slice(&16u16.to_le_bytes()); // bits
    buf.extend_from_slice(b"data");
    buf.extend_from_slice(&data_bytes.to_le_bytes());
    for v in track {
        let s = (v.clamp(-1.0f32, 1.0f32) * 32767.0f32) as i16;
        buf.extend_from_slice(&s.to_le_bytes());
    }
    std::fs::write(path, buf)
}

/// One clip to synthesize: sound folder, filename, samples.
type ClipJob = (&'static str, String, Vec<f32>);

/// Generate the full default pack into `out/{ignite,off,clash}/`. Returns
/// the list of files written. Same names as `make_sounds.py`.
pub fn build_pack(out: &Path) -> std::io::Result<Vec<PathBuf>> {
    let mut written = Vec::new();
    let jobs: Vec<ClipJob> = vec![
        ("ignite", "ignite-1.wav".into(), ignite(11, 78.0f32, 1.3f32)),
        ("ignite", "ignite-2.wav".into(), ignite(23, 86.0f32, 1.3f32)),
        ("ignite", "ignite-3.wav".into(), ignite(37, 94.0f32, 1.3f32)),
        ("off", "off-1.wav".into(), off(41, 80.0f32, 0.8f32)),
        ("off", "off-2.wav".into(), off(53, 90.0f32, 0.8f32)),
        ("clash", "clash-1.wav".into(), clash(61, 0.7f32)),
        ("clash", "clash-2.wav".into(), clash(73, 0.7f32)),
        ("clash", "clash-3.wav".into(), clash(89, 0.7f32)),
        ("clash", "2-clash-1.wav".into(), double_clash(97, 101, 0.14f32)),
        (
            "clash",
            "2-clash-2.wav".into(),
            double_clash(103, 107, 0.18f32),
        ),
    ];
    for (kind, name, track) in jobs {
        let path = out.join(kind).join(&name);
        save_wav(&path, &track)?;
        written.push(path);
    }
    Ok(written)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn clips_have_expected_lengths() {
        assert_eq!(ignite(11, 78.0f32, 1.3f32).len(), (SR as f32 * 1.3f32) as usize);
        assert_eq!(off(41, 80.0f32, 0.8f32).len(), (SR as f32 * 0.8f32) as usize);
        assert_eq!(clash(61, 0.7f32).len(), (SR as f32 * 0.7f32) as usize);
    }

    #[test]
    fn deterministic_and_normalized() {
        let a = ignite(11, 78.0f32, 1.3f32);
        let b = ignite(11, 78.0f32, 1.3f32);
        assert_eq!(a, b);
        let peak = a.iter().map(|v| v.abs()).fold(0.0f32, f32::max);
        assert!((peak - PEAK).abs() < 0.01f32, "peak {peak}");
        assert!(a.iter().all(|v| v.is_finite()));
        // Different seeds give different clips.
        assert_ne!(a, ignite(23, 86.0f32, 1.3f32));
    }

    #[test]
    fn swipe_is_finite_varied_and_speed_sensitive() {
        let regular = swipe(17, 650.0f32);
        let fast = swipe(17, 5_000.0f32);
        assert_eq!(regular, swipe(17, 650.0f32));
        assert_ne!(regular, swipe(18, 650.0f32));
        assert_ne!(regular.len(), fast.len());
        assert!(regular.iter().chain(&fast).all(|v| v.is_finite()));
        for track in [&regular, &fast] {
            let peak = track.iter().map(|v| v.abs()).fold(0.0f32, f32::max);
            assert!((peak - PEAK).abs() < 0.01f32, "peak {peak}");
        }
    }

    #[test]
    fn wav_header_roundtrip() {
        let dir = std::env::temp_dir().join(format!("kyber-snd-test-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let files = build_pack(&dir).expect("build pack");
        assert_eq!(files.len(), 10);
        for f in &files {
            let bytes = std::fs::read(f).unwrap();
            assert!(bytes.starts_with(b"RIFF"));
            assert_eq!(&bytes[8..12], b"WAVE");
        }
        let _ = std::fs::remove_dir_all(&dir);
    }
}
