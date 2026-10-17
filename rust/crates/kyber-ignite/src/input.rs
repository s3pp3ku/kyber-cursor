//! Global mouse-button watch via evdev. Port of the `pointer_devices` /
//! `rescan` / `on_input` logic in `lib/ignite-listener`.
//!
//! Needs read access to `/dev/input/event*` (the `input` group). Devices
//! are opened read-only and never grabbed, so clicks reach the compositor
//! and apps exactly as before — this only observes button state.

use std::path::{Path, PathBuf};
use std::sync::mpsc::Sender;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ButtonEvent {
    Down,
    Up,
}

// BTN_LEFT / BTN_RIGHT / BTN_MIDDLE (linux/input-event-codes.h).
const BUTTONS: [u16; 3] = [0x110, 0x111, 0x112];

fn is_pointer(path: &Path) -> bool {
    let dev = match evdev::Device::open(path) {
        Ok(d) => d,
        Err(_) => return false,
    };
    dev.supported_keys()
        .map(|keys| keys.contains(evdev::KeyCode::BTN_LEFT))
        .unwrap_or(false)
}

/// Paths of readable pointer devices, sorted for stable logs.
pub fn pointer_devices() -> Vec<PathBuf> {
    let mut out = Vec::new();
    let Ok(rd) = std::fs::read_dir("/dev/input") else {
        return out;
    };
    let mut names: Vec<_> = rd
        .flatten()
        .map(|e| e.path())
        .filter(|p| {
            p.file_name()
                .and_then(|n| n.to_str())
                .map(|n| n.starts_with("event"))
                .unwrap_or(false)
        })
        .collect();
    names.sort();
    for p in names {
        if is_pointer(&p) {
            out.push(p);
        }
    }
    out
}

/// Watch one device on its own thread until it errors (unplug) or the
/// process exits. Echo events (value 2) are ignored, like the Python.
pub fn spawn_watcher(path: PathBuf, tx: Sender<ButtonEvent>) -> std::thread::JoinHandle<()> {
    std::thread::spawn(move || {
        let mut dev = match evdev::Device::open(&path) {
            Ok(d) => d,
            Err(_) => return,
        };
        let mut held = [false; 3];
        eprintln!("kyber-ignite: watching {}", path.display());
        loop {
            let events = match dev.fetch_events() {
                Ok(it) => it.collect::<Vec<_>>(),
                Err(_) => break, // unplugged; the rescan picks up replacements
            };
            for ev in events {
                if ev.event_type() != evdev::EventType::KEY || ev.value() == 2 {
                    continue;
                }
                let Some(index) = BUTTONS.iter().position(|code| *code == ev.code()) else {
                    continue;
                };
                let down = ev.value() == 1;
                if held[index] == down {
                    continue;
                }
                held[index] = down;
                let _ = tx.send(if down {
                    ButtonEvent::Down
                } else {
                    ButtonEvent::Up
                });
            }
        }
        // A disconnected device cannot deliver its final button releases.
        for down in held {
            if down {
                let _ = tx.send(ButtonEvent::Up);
            }
        }
        eprintln!("kyber-ignite: disconnected {}", path.display());
    })
}

pub fn reap_watchers(
    watched: &mut std::collections::HashMap<PathBuf, std::thread::JoinHandle<()>>,
) {
    watched.retain(|_, thread| !thread.is_finished());
}

#[cfg(test)]
mod tests {
    #[test]
    fn disconnected_device_path_can_be_watched_again() {
        let path = std::path::PathBuf::from("/dev/input/event17");
        let thread = std::thread::spawn(|| {});
        while !thread.is_finished() {
            std::thread::yield_now();
        }
        let mut watched = std::collections::HashMap::from([(path.clone(), thread)]);
        super::reap_watchers(&mut watched);
        assert!(!watched.contains_key(&path));
    }
}
