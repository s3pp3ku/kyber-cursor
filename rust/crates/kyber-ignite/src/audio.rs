//! Sound playback via rodio — no `pw-play` needed. Respects the
//! `sound-mode` file (`all`/`ignite`/`mute`), the cubic volume curve, and
//! the shared recorded defaults. Local user recordings can override each role.

use std::collections::BTreeMap;
use std::fs::File;
use std::io::BufReader;
use std::path::PathBuf;
use std::sync::{
    atomic::{AtomicBool, AtomicU64, Ordering},
    Arc, Mutex,
};
use std::time::Duration;

use kyber_core::config::{SmallRng, SoundMode};
use kyber_core::sound_pack::{self, ClipSource, RATE as SR};
use rodio::{buffer::SamplesBuffer, Source};

fn state_dir() -> PathBuf {
    std::env::var("XDG_CONFIG_HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|_| home().join(".config"))
        .join("kyber-cursor")
}

fn home() -> PathBuf {
    std::env::var("HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|_| PathBuf::from("/tmp"))
}

pub fn sound_mode() -> SoundMode {
    let m = std::fs::read_to_string(state_dir().join("sound-mode")).unwrap_or_default();
    SoundMode::parse(m.trim()).unwrap_or(SoundMode::All)
}

/// Slider percent → amplitude. Same cubic curve as the Python.
pub fn gain() -> f32 {
    let v: i32 = std::fs::read_to_string(state_dir().join("volume"))
        .ok()
        .and_then(|s| s.trim().parse().ok())
        .unwrap_or(55);
    ((v.clamp(0, 100) as f32) / 100.0).powi(3)
}

fn clips(kind: &str) -> Vec<ClipSource> {
    sound_pack::sources(Some(&state_dir().join("sounds")), kind)
}

fn pick<T: Clone>(rng: &mut SmallRng, files: &[T]) -> Option<T> {
    if files.is_empty() {
        return None;
    }
    Some(files[(rng.next_f64() * files.len() as f64) as usize % files.len()].clone())
}

#[cfg(test)]
fn powerups(files: Vec<PathBuf>) -> Vec<PathBuf> {
    let preferred: Vec<_> = files
        .iter()
        .filter(|p| {
            let name = p
                .file_stem()
                .unwrap_or_default()
                .to_string_lossy()
                .to_lowercase();
            name.contains("ignite") || name.contains("ignition")
        })
        .cloned()
        .collect();
    if preferred.is_empty() {
        files
    } else {
        preferred
    }
}

#[cfg(test)]
fn single_clashes(files: &[PathBuf]) -> Vec<PathBuf> {
    files
        .iter()
        .filter(|p| {
            !p.file_name()
                .unwrap_or_default()
                .to_string_lossy()
                .starts_with("2-")
        })
        .cloned()
        .collect()
}

fn pick_next<T: Clone + PartialEq>(
    rng: &mut SmallRng,
    files: &[T],
    previous: Option<&T>,
) -> Option<T> {
    let alternatives: Vec<_> = files
        .iter()
        .filter(|p| Some(*p) != previous)
        .cloned()
        .collect();
    pick(
        rng,
        if alternatives.is_empty() {
            files
        } else {
            &alternatives
        },
    )
}

struct Decoded {
    samples: Vec<f32>,
    channels: u16,
    rate: u32,
}
fn decode(source: &ClipSource) -> Result<Decoded, String> {
    if let ClipSource::Bundled(index) = source {
        return Ok(Decoded {
            samples: sound_pack::CLIPS[*index].samples(),
            channels: 1,
            rate: SR,
        });
    }
    let ClipSource::File(path) = source else {
        unreachable!()
    };
    let file = File::open(path).map_err(|e| e.to_string())?;
    let decoder = rodio::Decoder::new(BufReader::new(file)).map_err(|e| e.to_string())?;
    let channels = decoder.channels();
    let rate = decoder.sample_rate();
    let samples: Vec<_> = decoder.take_duration(Duration::from_secs(60)).collect();
    if channels == 0
        || rate == 0
        || samples.len() < channels as usize
        || samples.iter().any(|s| !s.is_finite())
    {
        return Err("Invalid decoded clip".into());
    }
    Ok(Decoded {
        samples,
        channels,
        rate,
    })
}

struct HumSource(sound_pack::RecordedHum);
impl Iterator for HumSource {
    type Item = f32;
    fn next(&mut self) -> Option<f32> {
        self.0.next()
    }
}
impl Source for HumSource {
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

fn rng() -> SmallRng {
    SmallRng::seed(
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos() as u64)
            .unwrap_or(1),
    )
}

pub struct Player {
    mixer: Option<rodio::mixer::Mixer>,
    // Kept alive: dropping the stream stops all playback.
    _stream: Option<rodio::OutputStream>,
    voices: Arc<Mutex<Vec<rodio::Sink>>>,
    epoch: Arc<AtomicU64>,
    blade_epoch: Arc<AtomicU64>,
    swipe_epoch: Arc<AtomicU64>,
    swipe_duck: Arc<AtomicBool>,
    blade_release: Mutex<Option<Arc<AtomicBool>>>,
    previous: Mutex<BTreeMap<String, ClipSource>>,
    attack: Mutex<Option<rodio::Sink>>,
}

impl Player {
    pub fn new() -> Self {
        match rodio::OutputStreamBuilder::open_default_stream() {
            Ok(stream) => {
                let mixer = stream.mixer().clone();
                Self {
                    mixer: Some(mixer),
                    _stream: Some(stream),
                    voices: Arc::default(),
                    epoch: Arc::default(),
                    blade_epoch: Arc::default(),
                    swipe_epoch: Arc::default(),
                    swipe_duck: Arc::default(),
                    blade_release: Mutex::default(),
                    previous: Mutex::default(),
                    attack: Mutex::default(),
                }
            }
            Err(e) => {
                eprintln!("kyber-ignite: no audio device ({e}); sounds off");
                Self {
                    mixer: None,
                    _stream: None,
                    voices: Arc::default(),
                    epoch: Arc::default(),
                    blade_epoch: Arc::default(),
                    swipe_epoch: Arc::default(),
                    swipe_duck: Arc::default(),
                    blade_release: Mutex::default(),
                    previous: Mutex::default(),
                    attack: Mutex::default(),
                }
            }
        }
    }

    pub fn available(&self) -> bool {
        self.mixer.is_some()
    }

    pub fn stop_all(&self) {
        self.stop_blade();
        self.epoch.fetch_add(1, Ordering::SeqCst);
        if let Ok(mut voices) = self.voices.lock() {
            for voice in voices.drain(..) {
                voice.stop();
            }
        }
    }

    pub fn stop_blade(&self) {
        if let Ok(mut attack) = self.attack.lock() {
            if let Some(sink) = attack.take() {
                sink.stop();
            }
        }
        self.blade_epoch.fetch_add(1, Ordering::SeqCst);
        self.swipe_epoch.fetch_add(1, Ordering::SeqCst);
        self.swipe_duck.store(false, Ordering::SeqCst);
        if let Ok(mut release) = self.blade_release.lock() {
            if let Some(release) = release.take() {
                release.store(true, Ordering::SeqCst);
                eprintln!("kyber-audio: held hum released");
            }
        }
    }

    fn choose(&self, kind: &str) -> Option<ClipSource> {
        let files = clips(kind);
        let mut previous = self.previous.lock().ok()?;
        let selected = pick_next(&mut rng(), &files, previous.get(kind))?;
        previous.insert(kind.to_string(), selected.clone());
        Some(selected)
    }

    fn start_blade(&self, source: ClipSource) {
        self.stop_blade();
        let Some(mixer) = &self.mixer else { return };
        let Ok(attack) = decode(&source) else { return };
        let seconds = attack.samples.len() as f64 / attack.channels as f64 / attack.rate as f64;
        let sink = rodio::Sink::connect_new(mixer);
        sink.set_volume(gain());
        sink.append(SamplesBuffer::new(
            attack.channels,
            attack.rate,
            attack.samples,
        ));
        if let Ok(mut slot) = self.attack.lock() {
            *slot = Some(sink);
        }
        let Some(hum_source) = self.choose("hum") else {
            return;
        };
        let Ok(hum) = decode(&hum_source) else { return };
        let samples = sound_pack::mono_at_pack_rate(&hum.samples, hum.channels, hum.rate);
        let release = Arc::new(AtomicBool::new(false));
        if let Ok(mut slot) = self.blade_release.lock() {
            *slot = Some(release.clone());
        }
        let held = sound_pack::RecordedHum::new(
            samples,
            (seconds - 0.15).max(0.0),
            release,
            Some(self.swipe_duck.clone()),
        );
        let sink = rodio::Sink::connect_new(mixer);
        sink.set_volume(gain());
        sink.append(HumSource(held));
        if let Ok(mut voices) = self.voices.lock() {
            voices.retain(|v| !v.empty());
            voices.push(sink);
        }
        eprintln!(
            "kyber-audio: recorded power-up + recorded hum ({}, {})",
            source.name(),
            hum_source.name()
        );
    }

    fn play_file(&self, source: ClipSource, delay_ms: u64, all_only: bool) {
        let Some(mixer) = self.mixer.clone() else {
            return;
        };
        let voices = self.voices.clone();
        let epoch = self.epoch.clone();
        let token = epoch.load(Ordering::SeqCst);
        std::thread::spawn(move || {
            if delay_ms > 0 {
                std::thread::sleep(Duration::from_millis(delay_ms));
            }
            let mode = sound_mode();
            if epoch.load(Ordering::SeqCst) != token
                || mode == SoundMode::Mute
                || (all_only && mode != SoundMode::All)
            {
                return;
            }
            let Ok(clip) = decode(&source) else { return };
            let sink = rodio::Sink::connect_new(&mixer);
            sink.set_volume(gain());
            sink.append(SamplesBuffer::new(clip.channels, clip.rate, clip.samples));
            if let Ok(mut voices) = voices.lock() {
                if epoch.load(Ordering::SeqCst) != token {
                    sink.stop();
                    return;
                }
                voices.retain(|voice| !voice.empty());
                voices.push(sink);
            }
        });
    }

    /// Random clip from `sounds/<kind>/`, if any.
    pub fn play_clip(&self, kind: &str) {
        if kind == "off" {
            self.stop_blade();
        }
        if let Some(path) = self.choose(kind) {
            if kind == "ignite" && sound_mode() == SoundMode::All {
                self.start_blade(path);
            } else {
                self.play_file(path, 0, kind != "ignite");
            }
        }
    }

    /// One recorded clash per click.
    pub fn play_clash(&self) {
        if let Some(source) = self.choose("clash") {
            self.play_file(source, 0, true);
        }
    }

    /// Play the selected recorded swipe at its original pitch and speed.
    pub fn play_swipe(&self, _speed: f32) {
        if sound_mode() != SoundMode::All || self.swipe_duck.load(Ordering::SeqCst) {
            return;
        }
        let Some(mixer) = self.mixer.clone() else {
            return;
        };
        let Some(source) = self.choose("swipe") else {
            return;
        };
        let Ok(clip) = decode(&source) else { return };
        let duration = Duration::from_secs_f64(
            clip.samples.len() as f64 / clip.channels as f64 / clip.rate as f64,
        );
        self.swipe_duck.store(true, Ordering::SeqCst);
        let swipe_token = self.swipe_epoch.fetch_add(1, Ordering::SeqCst) + 1;
        let swipe_epoch = self.swipe_epoch.clone();
        let swipe_duck = self.swipe_duck.clone();
        std::thread::spawn(move || {
            std::thread::sleep(duration);
            if swipe_epoch.load(Ordering::SeqCst) == swipe_token {
                swipe_duck.store(false, Ordering::SeqCst);
            }
        });
        let sink = rodio::Sink::connect_new(&mixer);
        sink.set_volume(gain());
        sink.append(SamplesBuffer::new(clip.channels, clip.rate, clip.samples));
        if let Ok(mut voices) = self.voices.lock() {
            voices.retain(|voice| !voice.empty());
            voices.push(sink);
        }
    }
}

/// Local audio review using the live one-shot decoder and independent hum source.
/// Reads the user's pack; does not open an audio device or change desktop settings.
pub fn export_review(out: &std::path::Path) -> Result<(), String> {
    let ignites = clips("ignite");
    let offs = clips("off");
    if ignites.is_empty() || offs.is_empty() {
        return Err("Local ignite and off clips are required".into());
    }
    std::fs::create_dir_all(out).map_err(|e| e.to_string())?;
    fn mono(samples: &[f32], channels: u16, rate: u32, seconds: f64) -> f32 {
        let ch = channels as usize;
        let position = seconds * rate as f64;
        let frame = position as usize;
        let fraction = position.fract() as f32;
        let get = |frame: usize| -> f32 {
            (0..ch)
                .map(|c| samples.get(frame * ch + c).copied().unwrap_or(0.0))
                .sum::<f32>()
                / ch as f32
        };
        get(frame) * (1.0 - fraction) + get(frame + 1) * fraction
    }
    let mut provenance = Vec::new();
    for (i, ignite) in ignites.iter().enumerate() {
        let clip = decode(ignite)?;
        let hum_sources = clips("hum");
        let hum = decode(&hum_sources[i % hum_sources.len()])?;
        let held: Vec<_> = sound_pack::RecordedHum::new(
            sound_pack::mono_at_pack_rate(&hum.samples, hum.channels, hum.rate),
            (clip.samples.len() as f64 / clip.channels as f64 / clip.rate as f64 - 0.15).max(0.0),
            Arc::new(AtomicBool::new(false)),
            None,
        )
        .take((sounds_rate() as f64 * 4.1) as usize)
        .collect();
        for (j, off) in offs.iter().enumerate() {
            let off_clip = decode(off)?;
            let mut cycle = Vec::with_capacity((sounds_rate() as f64 * 6.0) as usize);
            for sample in 0..(sounds_rate() as f64 * 6.0) as usize {
                let t = sample as f64 / sounds_rate() as f64;
                let mut value = 0.0;
                if t >= 0.10 {
                    value += mono(&clip.samples, clip.channels, clip.rate, t - 0.10);
                }
                if (0.10..4.17).contains(&t) {
                    let release = if t < 4.15 {
                        1.0
                    } else {
                        (0.5 + 0.5 * (std::f64::consts::PI * (t - 4.15) / 0.02).cos()) as f32
                    };
                    value += mono(&held, 1, sounds_rate(), t - 0.10) * release;
                }
                if t >= 4.15 {
                    let elapsed = t - 4.15;
                    let edge = (elapsed / 0.002).min(1.0) as f32;
                    value +=
                        mono(&off_clip.samples, off_clip.channels, off_clip.rate, elapsed) * edge;
                }
                cycle.push(value);
            }
            let peak = cycle.iter().fold(0.0f32, |max, s| max.max(s.abs()));
            if peak > 0.85 {
                for sample in &mut cycle {
                    *sample *= 0.85 / peak;
                }
            }
            let name = format!("cycle-{i}-{j}.wav");
            kyber_core::sounds::save_wav(&out.join(&name), &cycle).map_err(|e| e.to_string())?;
            provenance.push(serde_json::json!({"file":name,"ignite":ignite.name(),
                "off":off.name(),"hum":"downloaded recorded hum; shared crossfade loop"}));
        }
    }
    std::fs::write(
        out.join("sources.json"),
        serde_json::to_vec_pretty(&provenance).map_err(|e| e.to_string())?,
    )
    .map_err(|e| e.to_string())?;
    eprintln!(
        "Exported {} local power-up / held-hum / power-down variations",
        provenance.len()
    );
    Ok(())
}

fn sounds_rate() -> u32 {
    kyber_core::sounds::SR
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn powerups_exclude_swings_but_allow_custom_names_as_fallback() {
        let files = ["whoosh.mp3", "lightsaber-ignite.mp3", "sithignition.mp3"]
            .map(PathBuf::from)
            .to_vec();
        assert_eq!(
            powerups(files),
            ["lightsaber-ignite.mp3", "sithignition.mp3"].map(PathBuf::from)
        );
        let custom = vec![PathBuf::from("my-recording.wav")];
        assert_eq!(powerups(custom.clone()), custom);
    }
    #[test]
    fn one_click_selects_a_single_hit_clash_clip() {
        let files = ["clash-1.wav", "2-clash-1.wav", "clash-2.wav"].map(PathBuf::from);
        assert_eq!(
            single_clashes(&files),
            [PathBuf::from("clash-1.wav"), PathBuf::from("clash-2.wav")]
        );
        assert!(single_clashes(&[PathBuf::from("2-clash-1.wav")]).is_empty());
    }
    #[test]
    fn variations_do_not_repeat_when_an_alternative_exists() {
        let files = ["a.wav", "b.wav"].map(PathBuf::from);
        let mut rng = SmallRng::seed(1);
        let mut last = files[0].clone();
        for _ in 0..30 {
            let next = pick_next(&mut rng, &files, Some(&last)).unwrap();
            assert_ne!(next, last);
            last = next;
        }
        assert_eq!(
            pick_next(&mut rng, &files[..1], Some(&files[0])),
            Some(files[0].clone())
        );
        assert_eq!(pick_next::<PathBuf>(&mut rng, &[], None), None);
    }
    #[test]
    fn cubic_gain_matches_python() {
        assert!((gain_for(100) - 1.0).abs() < 1e-6);
        assert_eq!(gain_for(0), 0.0);
        assert!(gain_for(18) < 0.01);
    }

    fn gain_for(percent: i32) -> f32 {
        ((percent.clamp(0, 100) as f32) / 100.0).powi(3)
    }
}
