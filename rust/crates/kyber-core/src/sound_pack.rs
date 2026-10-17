//! The selected recording pack. No platform audio dependency.
//! Code is MIT; the recordings have their own provenance in assets/sounds/.
use std::{
    path::{Path, PathBuf},
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
};

pub const RATE: u32 = 44100;
pub const PACK_ID: &str = "kyber-selected-clips-v1";
pub const MANIFEST: &str = include_str!("../../../../assets/sounds/manifest.json");
pub const NOTICE: &str = include_str!("../../../../assets/sounds/README.md");
pub const ROLES: [&str; 5] = ["ignite", "off", "clash", "swipe", "hum"];

pub struct Clip {
    pub role: &'static str,
    pub name: &'static str,
    pub wav: &'static [u8],
}

macro_rules! clip {
    ($role:literal, $name:literal) => {
        Clip {
            role: $role,
            name: concat!($name, ".wav"),
            wav: include_bytes!(concat!(
                "../../../../assets/sounds/",
                $role,
                "/",
                $name,
                ".wav"
            )),
        }
    };
}

pub static CLIPS: &[Clip] = &[
    clip!("ignite", "lightsaber-ignite"),
    clip!("ignite", "lightsaber-ignite-burst"),
    clip!("ignite", "sith-lightsaber-1"),
    clip!("off", "lightsaber-off"),
    clip!("off", "sith-deactivate-1"),
    clip!("clash", "lightsaber-clash-02"),
    clip!("clash", "lightsaber-clash-04"),
    clip!("swipe", "lightsaber-whoosh-01"),
    clip!("swipe", "lightsaber-whoosh-04"),
    clip!("hum", "lightsaber-hum"),
    clip!("hum", "lightsaber-hummm"),
];

impl Clip {
    /// Prepared pack files have the canonical 44-byte PCM16 mono WAV header.
    pub fn samples(&self) -> Vec<f32> {
        assert_eq!(&self.wav[..4], b"RIFF");
        assert_eq!(&self.wav[36..40], b"data");
        self.wav[44..]
            .chunks_exact(2)
            .map(|v| i16::from_le_bytes([v[0], v[1]]) as f32 / 32768.0)
            .collect()
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ClipSource {
    File(PathBuf),
    Bundled(usize),
}

impl ClipSource {
    pub fn name(&self) -> String {
        match self {
            Self::File(path) => path
                .file_name()
                .unwrap_or_default()
                .to_string_lossy()
                .into_owned(),
            Self::Bundled(index) => CLIPS[*index].name.to_string(),
        }
    }
}

/// The old generated filenames must not silently reintroduce rejected sounds.
/// Files remain on disk; arbitrary user-named recordings still override defaults.
pub fn legacy_generated(name: &str) -> bool {
    matches!(
        name.to_ascii_lowercase().as_str(),
        "ignite-1.wav"
            | "ignite-2.wav"
            | "ignite-3.wav"
            | "off-1.wav"
            | "off-2.wav"
            | "clash-1.wav"
            | "clash-2.wav"
            | "clash-3.wav"
            | "2-clash-1.wav"
            | "2-clash-2.wav"
    )
}

pub fn sources(directory: Option<&Path>, role: &str) -> Vec<ClipSource> {
    let mut files: Vec<_> = directory
        .and_then(|dir| std::fs::read_dir(dir.join(role)).ok())
        .into_iter()
        .flatten()
        .filter_map(Result::ok)
        .map(|e| e.path())
        .filter(|path| {
            path.is_file()
                && !legacy_generated(&path.file_name().unwrap_or_default().to_string_lossy())
        })
        .filter(|path| {
            matches!(
                path.extension()
                    .and_then(|v| v.to_str())
                    .map(str::to_ascii_lowercase)
                    .as_deref(),
                Some("wav" | "mp3" | "ogg" | "oga" | "flac")
            )
        })
        .collect();
    files.sort();
    if role == "ignite" {
        let preferred: Vec<_> = files
            .iter()
            .filter(|path| {
                let name = path
                    .file_stem()
                    .unwrap_or_default()
                    .to_string_lossy()
                    .to_ascii_lowercase();
                name.contains("ignite")
                    || name.contains("ignition")
                    || name.contains("lightsaber-1")
            })
            .cloned()
            .collect();
        if !preferred.is_empty() {
            files = preferred;
        }
    }
    if !files.is_empty() {
        return files.into_iter().map(ClipSource::File).collect();
    }
    CLIPS
        .iter()
        .enumerate()
        .filter(|(_, clip)| clip.role == role)
        .map(|(index, _)| ClipSource::Bundled(index))
        .collect()
}

/// Export the same recordings for installers and portable sound-only packages.
/// Existing files are left alone; generated defaults are not regenerated.
pub fn export(directory: &Path) -> std::io::Result<()> {
    for clip in CLIPS {
        let folder = directory.join(clip.role);
        std::fs::create_dir_all(&folder)?;
        let path = folder.join(clip.name);
        if !path.exists() {
            std::fs::write(path, clip.wav)?;
        }
    }
    std::fs::write(directory.join("manifest.json"), MANIFEST)?;
    std::fs::write(directory.join("AUDIO-NOTICE.md"), NOTICE)
}

pub fn mono_at_pack_rate(samples: &[f32], channels: u16, rate: u32) -> Vec<f32> {
    let channels = channels as usize;
    if channels == 0 || rate == 0 {
        return Vec::new();
    }
    let frames = samples.len() / channels;
    let output_frames = (frames as f64 * RATE as f64 / rate as f64) as usize;
    let at = |frame: usize| -> f32 {
        let start = frame.min(frames.saturating_sub(1)) * channels;
        samples
            .get(start..start + channels)
            .map(|v| v.iter().sum::<f32>() / channels as f32)
            .unwrap_or(0.0)
    };
    (0..output_frames)
        .map(|i| {
            let position = i as f64 * rate as f64 / RATE as f64;
            let frame = position as usize;
            let fraction = position.fract() as f32;
            at(frame) * (1.0 - fraction) + at(frame + 1) * fraction
        })
        .collect()
}

/// The same recorded hold, loop boundary, fade-in, release and swipe duck on
/// every backend. Pitch is never varied. No clip is regenerated or re-synthesized.
pub struct RecordedHum {
    samples: Vec<f32>,
    cursor: usize,
    overlap: usize,
    delay: usize,
    elapsed: usize,
    release_position: Option<usize>,
    released: Arc<AtomicBool>,
    duck: Option<Arc<AtomicBool>>,
    duck_gain: f32,
}

impl RecordedHum {
    pub fn new(
        mut samples: Vec<f32>,
        delay_seconds: f64,
        released: Arc<AtomicBool>,
        duck: Option<Arc<AtomicBool>>,
    ) -> Self {
        // Avoid the clips' start/end transients and existing edge fades.
        let edge = (RATE as usize * 3 / 10).min(samples.len() / 10);
        samples = samples[edge..samples.len() - edge].to_vec();
        if samples.len() < 4 {
            samples = vec![0.0; 4];
        }
        let overlap = (RATE as usize * 8 / 100).min(samples.len() / 4).max(2);
        Self {
            samples,
            cursor: 0,
            overlap,
            delay: (delay_seconds.max(0.0) * RATE as f64) as usize,
            elapsed: 0,
            release_position: None,
            released,
            duck,
            duck_gain: 1.0,
        }
    }
}

impl Iterator for RecordedHum {
    type Item = f32;
    fn next(&mut self) -> Option<f32> {
        if self.released.load(Ordering::Relaxed) && self.release_position.is_none() {
            self.release_position = Some(0);
        }
        let release_gain = if let Some(position) = &mut self.release_position {
            if *position >= (RATE / 50) as usize {
                return None;
            }
            let gain =
                0.5 + 0.5 * (std::f32::consts::PI * *position as f32 / (RATE / 50) as f32).cos();
            *position += 1;
            gain
        } else {
            1.0
        };
        if self.delay > 0 {
            self.delay -= 1;
            return Some(0.0);
        }
        let loop_end = self.samples.len();
        let blend_start = loop_end - self.overlap;
        let value = if self.cursor >= blend_start {
            let offset = self.cursor - blend_start;
            let fraction = offset as f32 / (self.overlap - 1) as f32;
            self.samples[self.cursor] * (1.0 - fraction) + self.samples[offset] * fraction
        } else {
            self.samples[self.cursor]
        };
        self.cursor += 1;
        if self.cursor >= loop_end {
            self.cursor = self.overlap;
        }
        let fade_in = (self.elapsed as f32 / (RATE as f32 * 0.15)).min(1.0);
        self.elapsed += 1;
        let target = if self
            .duck
            .as_ref()
            .is_some_and(|flag| flag.load(Ordering::Relaxed))
        {
            0.08
        } else {
            1.0
        };
        self.duck_gain += (target - self.duck_gain) * 0.0015;
        Some(value * fade_in * release_gain * self.duck_gain)
    }
}
