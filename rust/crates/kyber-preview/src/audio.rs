//! Windows playback of the shared recorded default pack or local overrides.
use kyber_core::{
    config::{SmallRng, SoundMode},
    sound_pack::{self, ClipSource, RATE},
};
use rodio::{
    buffer::SamplesBuffer, mixer::Mixer, Decoder, OutputStream, OutputStreamBuilder, Sink, Source,
};
use std::{
    collections::BTreeMap,
    fs::File,
    io::BufReader,
    path::PathBuf,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
    time::Duration,
};

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
        RATE
    }
    fn total_duration(&self) -> Option<Duration> {
        None
    }
}

struct Decoded {
    samples: Vec<f32>,
    channels: u16,
    rate: u32,
}
fn decode(source: &ClipSource) -> Option<Decoded> {
    if let ClipSource::Bundled(index) = source {
        return Some(Decoded {
            samples: sound_pack::CLIPS[*index].samples(),
            channels: 1,
            rate: RATE,
        });
    }
    let ClipSource::File(path) = source else {
        unreachable!()
    };
    let decoder = Decoder::new(BufReader::new(File::open(path).ok()?)).ok()?;
    let channels = decoder.channels();
    let rate = decoder.sample_rate();
    let samples: Vec<_> = decoder.take_duration(Duration::from_secs(60)).collect();
    if channels == 0
        || rate == 0
        || samples.len() < channels as usize
        || samples.iter().any(|s| !s.is_finite())
    {
        return None;
    }
    Some(Decoded {
        samples,
        channels,
        rate,
    })
}

pub struct Player {
    _stream: Option<OutputStream>,
    mixer: Option<Mixer>,
    mode: SoundMode,
    one_shots: Vec<Sink>,
    attack: Option<Sink>,
    hum: Option<(Sink, Arc<AtomicBool>)>,
    sounds_dir: Option<PathBuf>,
    previous: BTreeMap<String, ClipSource>,
    seed: SmallRng,
    gain: f32,
    swipes: Vec<Sink>,
    duck: Arc<AtomicBool>,
}

impl Default for Player {
    fn default() -> Self {
        Self::new()
    }
}

impl Player {
    pub fn new() -> Self {
        let settings = kyber_core::paths::settings_dir().ok();
        let volume = settings
            .as_ref()
            .and_then(|p| std::fs::read_to_string(p.join("volume")).ok())
            .and_then(|v| v.trim().parse::<i32>().ok())
            .unwrap_or(55)
            .clamp(0, 100);
        let (stream, mixer) = match OutputStreamBuilder::open_default_stream() {
            Ok(stream) => {
                let mixer = stream.mixer().clone();
                (Some(stream), Some(mixer))
            }
            Err(error) => {
                eprintln!("Kyber preview: audio output is unavailable: {error}");
                (None, None)
            }
        };
        Self {
            _stream: stream,
            mixer,
            mode: SoundMode::All,
            one_shots: Vec::new(),
            attack: None,
            hum: None,
            sounds_dir: settings.map(|p| p.join("sounds")),
            previous: BTreeMap::new(),
            seed: SmallRng::seed(0x4b59_4245_5257_494e),
            gain: (volume as f32 / 100.0).powi(3),
            swipes: Vec::new(),
            duck: Arc::new(AtomicBool::new(false)),
        }
    }

    pub fn mode(&self) -> SoundMode {
        self.mode
    }
    pub fn available(&self) -> bool {
        self.mixer.is_some()
    }
    pub fn has_custom_clips(&self) -> bool {
        ["ignite", "off", "hum"].iter().any(|role| {
            sound_pack::sources(self.sounds_dir.as_deref(), role)
                .iter()
                .any(|source| matches!(source, ClipSource::File(_)))
        })
    }

    pub fn cycle_mode(&mut self) {
        if !self.available() {
            return;
        }
        self.mode = match self.mode {
            SoundMode::All => SoundMode::Ignite,
            SoundMode::Ignite => SoundMode::Mute,
            SoundMode::Mute => SoundMode::All,
        };
        if self.mode != SoundMode::All {
            self.release_hum();
        }
        if self.mode == SoundMode::Mute {
            self.cancel();
        }
    }

    pub fn set_mode(&mut self, mode: SoundMode) {
        if self.mode != mode {
            self.cancel();
            self.mode = mode;
        }
    }

    pub fn set_volume(&mut self, percent: u8) {
        self.gain = (percent.min(100) as f32 / 100.0).powi(3);
        for sink in self.one_shots.iter().chain(self.attack.iter()) {
            sink.set_volume(self.gain);
        }
        for sink in &self.swipes {
            sink.set_volume(self.swipe_gain());
        }
        if let Some((sink, _)) = &self.hum {
            sink.set_volume(self.gain);
        }
    }

    pub fn tick(&mut self) {
        self.one_shots.retain(|sink| !sink.empty());
        self.swipes.retain(|sink| !sink.empty());
        self.duck.store(!self.swipes.is_empty(), Ordering::Relaxed);
    }

    pub fn effect(&mut self, role: &str) {
        self.tick();
        if self.mode != SoundMode::All || !matches!(role, "swipe" | "clash") {
            return;
        }
        // Allow two short whooshes to overlap so quick direction changes
        // respond, while keeping rapid pointer movement from washing them out.
        if role == "swipe" && self.swipes.len() >= 2 {
            return;
        }
        if let Some(clip) = self.choose(role) {
            if let Some(sink) = self.sink() {
                sink.append(SamplesBuffer::new(clip.channels, clip.rate, clip.samples));
                if role == "swipe" {
                    sink.set_volume(self.swipe_gain());
                    self.duck.store(true, Ordering::Relaxed);
                    self.swipes.push(sink);
                } else {
                    self.one_shots.push(sink);
                }
            }
        }
    }

    fn swipe_gain(&self) -> f32 {
        (self.gain * 2.4).min(1.0)
    }

    fn choose(&mut self, role: &str) -> Option<Decoded> {
        let sources = sound_pack::sources(self.sounds_dir.as_deref(), role);
        let choices: Vec<_> = sources
            .iter()
            .filter(|source| Some(*source) != self.previous.get(role))
            .collect();
        let choices = if choices.is_empty() {
            sources.iter().collect::<Vec<_>>()
        } else {
            choices
        };
        if choices.is_empty() {
            return None;
        }
        let source =
            choices[(self.seed.next_f64() * choices.len() as f64) as usize % choices.len()];
        self.previous.insert(role.to_string(), source.clone());
        decode(source)
    }

    fn sink(&self) -> Option<Sink> {
        let sink = Sink::connect_new(self.mixer.as_ref()?);
        sink.set_volume(self.gain);
        Some(sink)
    }

    pub fn press(&mut self) {
        self.release_hum();
        if let Some(sink) = self.attack.take() {
            sink.stop();
        }
        if self.mode == SoundMode::Mute {
            return;
        }
        let mut delay = 0.0;
        if let Some(clip) = self.choose("ignite") {
            delay = (clip.samples.len() as f64 / clip.channels as f64 / clip.rate as f64 - 0.15)
                .max(0.0);
            if let Some(sink) = self.sink() {
                sink.append(SamplesBuffer::new(clip.channels, clip.rate, clip.samples));
                self.attack = Some(sink);
            }
        }
        if self.mode == SoundMode::All {
            if let Some(clip) = self.choose("hum") {
                let release = Arc::new(AtomicBool::new(false));
                let held = sound_pack::RecordedHum::new(
                    sound_pack::mono_at_pack_rate(&clip.samples, clip.channels, clip.rate),
                    delay,
                    release.clone(),
                    Some(self.duck.clone()),
                );
                if let Some(sink) = self.sink() {
                    sink.append(HumSource(held));
                    self.hum = Some((sink, release));
                }
            }
        }
    }

    pub fn release(&mut self) {
        self.release_hum();
        if let Some(sink) = self.attack.take() {
            sink.stop();
        }
        if self.mode == SoundMode::All {
            if let Some(clip) = self.choose("off") {
                if let Some(sink) = self.sink() {
                    self.one_shots.retain(|voice| !voice.empty());
                    sink.append(SamplesBuffer::new(clip.channels, clip.rate, clip.samples));
                    self.one_shots.push(sink);
                }
            }
        }
    }

    pub fn cancel(&mut self) {
        for sink in self.swipes.drain(..) {
            sink.stop();
        }
        self.duck.store(false, Ordering::Relaxed);
        if let Some((sink, _)) = self.hum.take() {
            sink.stop();
        }
        if let Some(sink) = self.attack.take() {
            sink.stop();
        }
        for sink in self.one_shots.drain(..) {
            sink.stop();
        }
    }

    fn release_hum(&mut self) {
        // Keep the sink alive until the shared 20 ms release has finished.
        if let Some((sink, flag)) = self.hum.take() {
            flag.store(true, Ordering::Relaxed);
            self.one_shots.retain(|voice| !voice.empty());
            self.one_shots.push(sink);
        }
    }
}

impl Drop for Player {
    fn drop(&mut self) {
        self.cancel();
    }
}
