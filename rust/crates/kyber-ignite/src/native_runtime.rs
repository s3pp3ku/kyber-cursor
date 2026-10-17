//! Native compositor input -> Rust audio, alongside the Rust recolor watcher.
//! This path never opens evdev or creates a second glow overlay.
use crate::{
    audio::{self, Player},
    native_watch,
};
use kyber_core::{
    config::SoundMode,
    overlay::{DOUBLE_CLICK_S, SOUND_DELAY_MS},
};
use std::{
    collections::BTreeSet,
    io::{BufRead, BufReader},
    os::unix::net::UnixStream,
    path::Path,
    process::Command,
    time::{Duration, Instant},
};

#[derive(Debug, Clone, Copy, PartialEq)]
enum Event {
    Down(u32),
    Up(u32),
    Swipe(f32),
    Cancel,
}
fn parse(line: &str) -> Option<Event> {
    let message = line.trim().strip_prefix("kyberbutton>>")?;
    if message == "cancel" {
        return Some(Event::Cancel);
    }
    if let Some(speed) = message.strip_prefix("swipe,") {
        let speed: f32 = speed.parse().ok()?;
        return (speed.is_finite() && (650.0..=10_000.0).contains(&speed))
            .then_some(Event::Swipe(speed));
    }
    let (kind, button) = message.split_once(',')?;
    let button = button.parse().ok()?;
    if !(0x110..=0x112).contains(&button) {
        return None;
    }
    match kind {
        "down" => Some(Event::Down(button)),
        "up" => Some(Event::Up(button)),
        _ => None,
    }
}

#[derive(Default)]
struct Sounds {
    held: BTreeSet<u32>,
    last_press: Option<f64>,
    pending: Option<f64>,
    sounded: bool,
    hum_ready_at: Option<f64>,
    last_quick_click: bool,
}
impl Sounds {
    fn cancel(&mut self) {
        *self = Self::default();
    }
    fn event(&mut self, event: Event, now: f64, mode: SoundMode) -> Option<&'static str> {
        match event {
            Event::Cancel => self.cancel(),
            Event::Down(button) if self.held.insert(button) => {
                if self.held.len() > 1 {
                    return None;
                }
                self.hum_ready_at = None;
                let previous_was_quick = self.last_quick_click;
                self.last_quick_click = false;
                let double = self
                    .last_press
                    .is_some_and(|last| now - last < DOUBLE_CLICK_S);
                self.last_press = if double { None } else { Some(now) };
                self.pending = if double {
                    None
                } else {
                    Some(now + SOUND_DELAY_MS as f64 / 1000.0)
                };
                if double && !previous_was_quick && mode == SoundMode::All {
                    return Some("clash");
                }
            }
            Event::Up(button) if self.held.remove(&button) && self.held.is_empty() => {
                let quick_click = self.pending.take().is_some() && !self.sounded;
                let sounded = std::mem::take(&mut self.sounded);
                self.hum_ready_at = None;
                self.last_quick_click = quick_click;
                if quick_click && mode == SoundMode::All {
                    return Some("clash");
                }
                if sounded && mode == SoundMode::All {
                    return Some("off");
                }
            }
            Event::Swipe(_) => {}
            _ => {}
        }
        None
    }
    fn can_swipe(&self, now: f64, mode: SoundMode) -> bool {
        mode == SoundMode::All
            && !self.held.is_empty()
            && self.sounded
            && self.hum_ready_at.is_some_and(|at| now >= at)
    }
    fn poll(&mut self, now: f64, mode: SoundMode) -> Option<&'static str> {
        if self.pending.is_some_and(|at| now >= at) {
            self.pending = None;
            if !self.held.is_empty() && mode != SoundMode::Mute {
                self.sounded = true;
                if mode == SoundMode::All {
                    // The independent hum starts after the ignition clip's delay/fade.
                    self.hum_ready_at = Some(now + 1.25);
                }
                return Some("ignite");
            }
        }
        None
    }
}

fn play(player: &Player, kind: &str) {
    eprintln!("kyber-audio: {kind}");
    if kind == "clash" {
        player.play_clash();
    } else {
        player.play_clip(kind);
    }
}

pub fn run(frames: &Path, plugin: &Path) -> Result<(), String> {
    let runtime = std::env::var_os("XDG_RUNTIME_DIR").ok_or("Missing XDG_RUNTIME_DIR")?;
    let signature = std::env::var_os("HYPRLAND_INSTANCE_SIGNATURE")
        .ok_or("Missing HYPRLAND_INSTANCE_SIGNATURE")?;
    let socket = std::path::PathBuf::from(runtime)
        .join("hypr")
        .join(signature)
        .join(".socket2.sock");
    let stream = UnixStream::connect(socket)
        .map_err(|e| format!("Cannot subscribe to compositor events: {e}"))?;
    stream
        .set_read_timeout(Some(Duration::from_millis(20)))
        .map_err(|e| e.to_string())?;
    native_watch::prepare(frames)?;
    let player = Player::new();
    if !player.available() {
        return Err("No audio output device is available".into());
    }
    let current = Command::new("hyprctl")
        .args(["kybernative", "status"])
        .output()
        .map_err(|e| e.to_string())?;
    let status = String::from_utf8_lossy(&current.stdout);
    if !status.contains("protocol=2") {
        if status.contains("generation=") {
            return Err("Unload the old cursor trial before starting the native runtime".into());
        }
        let loaded = Command::new("hyprctl")
            .args(["plugin", "load"])
            .arg(plugin)
            .output()
            .map_err(|e| e.to_string())?;
        if !loaded.status.success() {
            return Err(format!(
                "Native adapter refused to load: {}{}",
                String::from_utf8_lossy(&loaded.stdout),
                String::from_utf8_lossy(&loaded.stderr)
            ));
        }
    }
    let config = kyber_core::paths::settings_dir()?;
    let bank =
        std::fs::read_to_string(config.join("native-bank")).unwrap_or_else(|_| "light".into());
    let bank = if bank.trim() == "dark" {
        "dark"
    } else {
        "light"
    };
    let selected = Command::new("hyprctl")
        .args(["kybernative", bank])
        .output()
        .map_err(|e| e.to_string())?;
    if !selected.status.success() {
        return Err("Could not select the native background bank".into());
    }
    let (tx, rx) = std::sync::mpsc::channel();
    let directory = frames.to_owned();
    std::thread::spawn(move || {
        let _ = tx.send(native_watch::run(&directory));
    });
    eprintln!("kyber-native: ready; Rust audio connected; bank={bank}");
    let mut reader = BufReader::new(stream);
    let started = Instant::now();
    let mut state = Sounds::default();
    let mut mode = audio::sound_mode();
    let mut line = String::new();
    loop {
        if let Ok(result) = rx.try_recv() {
            return result;
        }
        let next_mode = audio::sound_mode();
        let enabled = config.join("active").exists();
        if next_mode != mode || !enabled {
            state.cancel();
            player.stop_all();
            mode = next_mode;
        }
        match reader.read_line(&mut line) {
            Ok(0) => {
                player.stop_all();
                return Err("Compositor event stream closed".into());
            }
            Ok(_) => {
                if let Some(event) = parse(&line) {
                    if event == Event::Cancel {
                        state.cancel();
                        player.stop_all();
                    } else if let Event::Swipe(speed) = event {
                        if enabled && state.can_swipe(started.elapsed().as_secs_f64(), mode) {
                            player.play_swipe(speed);
                        }
                    } else if enabled && mode != SoundMode::Mute {
                        if let Some(kind) =
                            state.event(event, started.elapsed().as_secs_f64(), mode)
                        {
                            play(&player, kind);
                        }
                        if state.held.is_empty() {
                            player.stop_blade();
                        }
                    }
                }
                line.clear();
            }
            Err(e)
                if matches!(
                    e.kind(),
                    std::io::ErrorKind::WouldBlock | std::io::ErrorKind::TimedOut
                ) => {}
            Err(e) => {
                player.stop_all();
                return Err(e.to_string());
            }
        }
        if line.len() > 65536 {
            return Err("Oversized compositor event".into());
        }
        if enabled {
            if let Some(kind) = state.poll(started.elapsed().as_secs_f64(), mode) {
                play(&player, kind);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn parser_ignores_unrelated_input_and_accepts_only_pointer_buttons() {
        assert_eq!(parse("kyberbutton>>down,272\n"), Some(Event::Down(272)));
        assert_eq!(parse("kyberbutton>>cancel\n"), Some(Event::Cancel));
        assert_eq!(
            parse("kyberbutton>>swipe,1800\n"),
            Some(Event::Swipe(1800.0))
        );
        for line in [
            "activewindow>>app,title",
            "kyberbutton>>down,30",
            "kyberbutton>>up,no",
            "kyberbutton>>down,272,foo",
            "kyberbutton>>swipe,500",
            "kyberbutton>>swipe,inf",
        ] {
            assert_eq!(parse(line), None);
        }
    }
    #[test]
    fn swipe_requires_a_held_blade_with_the_hum_audible() {
        let mut s = Sounds::default();
        assert!(!s.can_swipe(0.0, SoundMode::All));
        s.event(Event::Down(272), 1.0, SoundMode::All);
        assert!(!s.can_swipe(1.2, SoundMode::All));
        assert_eq!(s.poll(1.4, SoundMode::All), Some("ignite"));
        assert!(!s.can_swipe(2.64, SoundMode::All));
        assert!(s.can_swipe(2.65, SoundMode::All));
        assert!(!s.can_swipe(2.65, SoundMode::Ignite));
        assert_eq!(s.event(Event::Up(272), 2.7, SoundMode::All), Some("off"));
        assert!(!s.can_swipe(2.8, SoundMode::All));
    }
    #[test]
    fn hold_plays_ignite_then_off_and_quick_tap_clashes() {
        let mut s = Sounds::default();
        let m = SoundMode::All;
        assert_eq!(s.event(Event::Down(272), 1.0, m), None);
        assert_eq!(s.poll(1.1, m), None);
        assert_eq!(s.poll(1.4, m), Some("ignite"));
        assert_eq!(s.event(Event::Up(272), 1.5, m), Some("off"));
        s.event(Event::Down(272), 2.0, m);
        assert_eq!(s.event(Event::Up(272), 2.05, m), Some("clash"));
        assert_eq!(s.poll(3.0, m), None);
    }
    #[test]
    fn intermediate_release_keeps_the_final_off_sound() {
        let mut s = Sounds::default();
        let m = SoundMode::All;
        s.event(Event::Down(272), 1.0, m);
        assert_eq!(s.poll(1.4, m), Some("ignite"));
        s.event(Event::Down(273), 1.5, m);
        assert_eq!(s.event(Event::Up(272), 1.6, m), None);
        assert_eq!(s.event(Event::Up(273), 1.7, m), Some("off"));
    }
    #[test]
    fn cancel_mute_duplicates_and_ignite_only_are_respected() {
        let mut s = Sounds::default();
        let m = SoundMode::Ignite;
        s.event(Event::Down(272), 1.0, m);
        s.event(Event::Down(272), 1.01, m);
        assert_eq!(s.poll(1.4, m), Some("ignite"));
        assert_eq!(s.event(Event::Up(272), 1.5, m), None);
        s.event(Event::Down(272), 2.0, m);
        s.event(Event::Cancel, 2.1, m);
        assert_eq!(s.poll(2.5, m), None);
        s.event(Event::Down(272), 3.0, SoundMode::Mute);
        assert_eq!(s.poll(3.5, SoundMode::Mute), None);
    }
    #[test]
    fn double_click_plays_clash_without_a_delayed_ignite() {
        let mut s = Sounds::default();
        let m = SoundMode::All;
        s.event(Event::Down(272), 1.0, m);
        assert_eq!(s.event(Event::Up(272), 1.05, m), Some("clash"));
        assert_eq!(s.event(Event::Down(272), 1.2, m), None);
        assert_eq!(s.poll(1.8, m), None);
    }
    #[test]
    fn second_held_button_does_not_restart_power_up() {
        let mut s = Sounds::default();
        s.event(Event::Down(272), 1.0, SoundMode::All);
        assert_eq!(s.poll(1.4, SoundMode::All), Some("ignite"));
        s.event(Event::Down(273), 1.5, SoundMode::All);
        assert_eq!(s.poll(1.9, SoundMode::All), None);
        assert_eq!(s.event(Event::Up(273), 2.0, SoundMode::All), None);
        assert_eq!(s.event(Event::Up(272), 2.1, SoundMode::All), Some("off"));
    }
}
