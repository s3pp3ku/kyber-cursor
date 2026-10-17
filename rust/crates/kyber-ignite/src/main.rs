//! `kyber-ignite`: hold-to-ignite overlay daemon.
//!
//! Watches mouse buttons system-wide (evdev), draws the theme's glow art on
//! a click-through layer-shell overlay while one is held, and plays sounds
//! via rodio. No Python, GTK, grim-sidecar... (grim still samples the
//! background patch; see `shot.rs`).

mod art;
mod audio;
mod cursor;
mod dmabuf;
mod input;
mod native_runtime;
mod native_watch;
mod shell;
mod shot;
mod state;

fn main() {
    let args: Vec<String> = std::env::args().collect();
    if args.get(1).map(String::as_str) == Some("--export-audio-review") {
        let result = args
            .get(2)
            .ok_or("supply an output directory".to_string())
            .and_then(|out| audio::export_review(std::path::Path::new(out)));
        if let Err(error) = result {
            eprintln!("{error}");
            std::process::exit(1);
        }
        return;
    }
    if args.get(1).map(String::as_str) == Some("--prepare-native") {
        let result = args
            .get(2)
            .ok_or("usage: kyber-ignite --prepare-native DIRECTORY".to_string())
            .and_then(|out| native_watch::prepare(std::path::Path::new(out)));
        if let Err(error) = result {
            eprintln!("{error}");
            std::process::exit(1);
        }
        return;
    }
    if args.get(1).map(String::as_str) == Some("--native-runtime") {
        let result = match (args.get(2), args.get(3)) {
            (Some(frames), Some(plugin)) => {
                native_runtime::run(std::path::Path::new(frames), std::path::Path::new(plugin))
            }
            _ => Err("usage: kyber-ignite --native-runtime FRAMES PLUGIN".into()),
        };
        if let Err(error) = result {
            eprintln!("kyber-native: {error}");
            std::process::exit(1);
        }
        return;
    }
    if let Some(i) = args.iter().position(|a| a == "--watch-native") {
        let result = args
            .get(i + 1)
            .ok_or_else(|| "usage: kyber-ignite --watch-native DIRECTORY".to_string())
            .and_then(|out| native_watch::run(std::path::Path::new(out)));
        if let Err(error) = result {
            eprintln!("{error}");
            std::process::exit(1);
        }
        return;
    }
    if let Some(i) = args.iter().position(|a| a == "--export-native") {
        let result = match (args.get(i + 1), args.get(i + 2)) {
            (Some(out), Some(color)) => {
                kyber_core::native::export(std::path::Path::new(out), color, 48, 2)
            }
            _ => Err("usage: kyber-ignite --export-native DIRECTORY '#rrggbb'".into()),
        };
        if let Err(error) = result {
            eprintln!("{error}");
            std::process::exit(1);
        }
        return;
    }
    if args.iter().any(|a| a == "--describe") {
        println!("backend: linux");
        println!("input: evdev /dev/input (input group, read-only, never grabbed)");
        println!(
            "overlay: wlr-layer-shell fullscreen transparent, empty input region, dmabuf upload"
        );
        println!("cursor: Hyprland IPC socket");
        println!("background: grim 20x20 patch (portal in step 4)");
        println!("audio: rodio (no pw-play)");
        return;
    }
    if let Some(i) = args.iter().position(|a| a == "--demo") {
        let seconds: f64 = args.get(i + 1).and_then(|s| s.parse().ok()).unwrap_or(3.0);
        std::process::exit(shell::run_demo(seconds));
    }
    if std::env::var("WAYLAND_DISPLAY").is_err() && std::env::var("WAYLAND_SOCKET").is_err() {
        // Hyprland always sets WAYLAND_DISPLAY; without it there is no overlay.
        eprintln!("kyber-ignite: no Wayland display found");
        std::process::exit(1);
    }
    std::process::exit(shell::run_daemon());
}
