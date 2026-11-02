//! Linux `kyber` command for the existing `~/.config/kyber-cursor` settings
//! layout. The shell wrapper handles preview/confirmation and delegates normal
//! commands here after the Rust runtime is installed.

use std::path::PathBuf;

use kyber_core::config::{
    pick_random, volume_gain_str, ColorMode, KyberConfig, Particles, SmallRng, SoundMode, TrailLook, TrailStyle,
};
use kyber_core::sound_pack;

fn state_dir() -> PathBuf {
    kyber_core::paths::settings_dir().unwrap_or_else(|error| {
        eprintln!("kyber: {error}");
        std::process::exit(1);
    })
}

#[cfg(target_os = "linux")]
fn home() -> PathBuf {
    kyber_core::paths::home_dir().unwrap_or_else(|error| {
        eprintln!("kyber: {error}");
        std::process::exit(1);
    })
}

fn read_file(name: &str) -> String {
    std::fs::read_to_string(state_dir().join(name))
        .unwrap_or_default()
        .trim()
        .to_string()
}

fn write_file(name: &str, value: &str) {
    let dir = state_dir();
    let _ = std::fs::create_dir_all(&dir);
    if let Err(e) = std::fs::write(dir.join(name), format!("{value}\n")) {
        eprintln!("kyber: cannot write {name}: {e}");
        std::process::exit(1);
    }
}

fn load_config() -> KyberConfig {
    KyberConfig::load(&state_dir())
}

/// Preserve the existing `bin/kyber config` contract used by the tray panel.
/// The portable settings model keeps a boolean internally, but QML compares
/// `trail` to "on"/"off" and needs `color` for its icon and tooltip.
fn widget_config(cfg: &KyberConfig, accent: Option<&str>) -> serde_json::Value {
    let aim = kyber_core::aim::AimSettings::load(&state_dir());
    serde_json::json!({
        "active": cfg.active,
        "mode": cfg.mode.as_str(),
        "color": if cfg.mode == ColorMode::Theme { "theme".to_string() } else { cfg.effective_color(accent) },
        "custom": cfg.custom,
        "random": cfg.random,
        "sound": cfg.sound,
        "trail": if cfg.trail { "on" } else { "off" },
        "aim": if aim.enabled { "on" } else { "off" },
        "aimMirror": if aim.mirror { "on" } else { "off" },
        "volume": cfg.volume,
        "style": cfg.look.style,
        "particles": cfg.look.particles,
        "sparkLife": cfg.look.spark_life,
        "sparkGap": cfg.look.spark_gap,
        "sparkColors": cfg.look.spark_colors(),
    })
}

fn usage() -> ! {
    eprintln!("usage: kyber apply | off | effects [full|minimal|basic] | color [theme|custom|random|#rrggbb] | sound [all|ignite|mute|cycle] | trail [on|off] | aim [on|off|toggle] | aim mirror [on|off|toggle] | sparks [sparks|off|life MS|gap PX|colors mixed|glow|default] | style [comet|none] | volume [0-100] | random | sounds [default|list] | config | status | doctor");
    std::process::exit(2);
}

fn cmd_color(args: &[String]) {
    let v = args.first().map(|s| s.as_str()).unwrap_or("");
    if !v.is_empty()
        && v != "theme"
        && v != "accent"
        && v != "none"
        && v != "custom"
        && v != "random"
    {
        let hex = format!("#{}", v.trim_start_matches('#')).to_lowercase();
        if hex.len() != 7 || !hex[1..].chars().all(|c| c.is_ascii_hexdigit()) {
            eprintln!("usage: kyber color theme | custom | random | #rrggbb");
            std::process::exit(2);
        }
        write_file("color", &hex);
        write_file("color-mode", "custom");
    } else if !v.is_empty() {
        match v {
            "theme" | "accent" | "none" => write_file("color-mode", "theme"),
            "custom" => {
                if !state_dir().join("color").exists() {
                    eprintln!("kyber: no custom color stored yet; run: kyber color #rrggbb");
                    std::process::exit(1);
                }
                write_file("color-mode", "custom");
            }
            _ => write_file("color-mode", "random"),
        }
    }
    let cfg = load_config();
    // No OS theming in the Rust CLI yet: theme mode resolves against the
    // Omarchy accent file only on Linux; elsewhere it falls back to blue.
    let accent = linux_accent();
    println!(
        "{} {}",
        cfg.mode.as_str(),
        cfg.effective_color(accent.as_deref())
    );
    #[cfg(target_os = "linux")]
    if !v.is_empty() && cfg.active {
        cmd_apply();
    }
}

#[cfg(not(target_os = "linux"))]
fn linux_accent() -> Option<String> {
    None
}

#[cfg(target_os = "linux")]
fn linux_accent() -> Option<String> {
    kyber_core::config::theme_accent()
}

fn cmd_volume(args: &[String]) {
    if let Some(v) = args.first() {
        let n: i32 = v.parse().unwrap_or(-1);
        if !(0..=100).contains(&n) {
            eprintln!("usage: kyber volume 0-100");
            std::process::exit(2);
        }
        write_file("volume", &n.to_string());
        println!("{n}");
    } else {
        println!("{}", read_file("volume").parse::<i32>().unwrap_or(55));
    }
}

fn cmd_sound(args: &[String]) {
    let mode = match args.first().map(String::as_str).unwrap_or("cycle") {
        "cycle" => load_config().sound.cycle(),
        value => SoundMode::parse(value).unwrap_or_else(|| usage()),
    };
    let value = match mode {
        SoundMode::All => "all",
        SoundMode::Ignite => "ignite",
        SoundMode::Mute => "mute",
    };
    write_file("sound-mode", value);
    println!("{value}");
}

fn cmd_trail(args: &[String]) {
    let enabled = match args.first().map(String::as_str).unwrap_or("toggle") {
        "on" => true,
        "off" => false,
        "toggle" => !load_config().trail,
        _ => usage(),
    };
    let runtime = read_file("native-runtime");
    if enabled && !runtime.is_empty() && runtime != "overlay" {
        #[cfg(target_os = "linux")]
        if !native_trails_available() {
            eprintln!("The running native cursor bridge needs the trail update first.");
            std::process::exit(3);
        }
    }
    write_file("trail-mode", if enabled { "on" } else { "off" });
    println!("{}", if enabled { "on" } else { "off" });
}

/// Aim only exists in the native overlay runtime: the XCursor theme is a
/// static bitmap, so nothing in the legacy path can turn the blade. Refusing
/// here (and leaving the switch off) is honest; a silently-on switch that
/// does nothing would be worse.
fn native_overlay_runtime() -> bool {
    read_file("native-runtime") == "overlay"
}

fn aim_mirror_on() -> bool {
    match read_file(kyber_core::aim::MIRROR_FILE).as_str() {
        "off" => false,
        "on" => true,
        // Mirroring is on unless it is switched off.
        _ => true,
    }
}

fn toggle_value(arg: Option<&str>, currently: bool) -> bool {
    match arg {
        Some("on") => true,
        Some("off") => false,
        Some("toggle") | None => !currently,
        _ => {
            eprintln!("usage: kyber aim [on|off|toggle] | kyber aim mirror [on|off|toggle]");
            std::process::exit(2);
        }
    }
}

/// `kyber aim on|off|toggle` and `kyber aim mirror on|off|toggle`.
fn cmd_aim(args: &[String]) {
    match args.first().map(String::as_str) {
        Some("mirror") => {
            let on = toggle_value(args.get(1).map(String::as_str), aim_mirror_on());
            write_file(kyber_core::aim::MIRROR_FILE, if on { "on" } else { "off" });
            println!("{}", if on { "on" } else { "off" });
        }
        arg => {
            let on = toggle_value(arg, read_file(kyber_core::aim::STATE_FILE) == "on");
            if on && !native_overlay_runtime() {
                write_file(kyber_core::aim::STATE_FILE, "off");
                eprintln!(
                    "kyber: aim stays off. It needs the native overlay runtime \
                     (kyber-ignite), which can draw the cursor turned at any angle; \
                     this install uses the static XCursor theme, where the pointer \
                     cannot rotate."
                );
                std::process::exit(3);
            }
            write_file(kyber_core::aim::STATE_FILE, if on { "on" } else { "off" });
            println!("{}", if on { "on" } else { "off" });
        }
    }
}

/// One switch for the whole experience, like the sound modes:
///   full     trail + sparks + every sound (the stock experience)
///   minimal  trail, no sparks, only the ignite sounds
///   basic    just the glow: no trail, no sounds
/// Each still lives in its own file, so any of them can be changed after.
fn cmd_effects(args: &[String]) {
    let (trail, particles, sound) = match args.first().map(String::as_str).unwrap_or("full") {
        "full" => ("on", Some(Particles::Sparks), "all"),
        "minimal" => ("on", Some(Particles::Off), "ignite"),
        "basic" => ("off", None, "mute"),
        _ => {
            eprintln!("usage: kyber effects full | minimal | basic");
            std::process::exit(2);
        }
    };
    cmd_trail(&[trail.to_string()]);
    if let Some(p) = particles {
        write_file("trail-particles", p.as_str());
    }
    cmd_sound(&[sound.to_string()]);
}

/// The path drawn behind the blade. Comet is the default look; the others
/// are extras for people who want to play.
fn cmd_trail_style(args: &[String]) {
    let current = load_config().look.style;
    let style = match args.first().map(String::as_str).unwrap_or("cycle") {
        "cycle" => {
            let all = TrailStyle::ALL;
            all[(all.iter().position(|s| *s == current).unwrap_or(0) + 1) % all.len()]
        }
        v => TrailStyle::parse(v).unwrap_or_else(|| {
            eprintln!("usage: kyber style comet | none | cycle");
            std::process::exit(2);
        }),
    };
    write_file("trail-style", style.as_str());
    println!("{style}");
}

/// What the blade throws off as it moves, and how long and how close
/// together. Sparks, 850 ms, every 20 px is the default look.
fn cmd_sparks(args: &[String]) {
    let usage = || -> ! {
        let (l, g) = (TrailLook::SPARK_LIFE_MS, TrailLook::SPARK_GAP_PX);
        eprintln!(
            "usage: kyber sparks sparks | off | cycle | life {}-{} (ms) | gap {}-{} (px, smaller is closer together) | colors mixed|glow | default",
            l.0, l.1, g.0, g.1
        );
        std::process::exit(2);
    };
    let look = load_config().look;
    let number = |range: (u32, u32)| -> u32 {
        match args.get(1).and_then(|v| v.parse::<u32>().ok()) {
            Some(v) => v.clamp(range.0, range.1),
            None => usage(),
        }
    };
    match args.first().map(String::as_str).unwrap_or("cycle") {
        "life" => {
            let v = number(TrailLook::SPARK_LIFE_MS);
            write_file("spark-life", &v.to_string());
            println!("{v}");
        }
        "gap" => {
            let v = number(TrailLook::SPARK_GAP_PX);
            write_file("spark-gap", &v.to_string());
            println!("{v}");
        }
        "colors" => match args.get(1).map(String::as_str) {
            Some(v @ ("mixed" | "glow")) => {
                write_file("spark-colors", v);
                println!("{v}");
            }
            _ => usage(),
        },
        "default" => {
            for name in TrailLook::FILES {
                let _ = std::fs::remove_file(state_dir().join(name));
            }
            let d = TrailLook::default();
            println!("{} {} {}ms {}px", d.style, d.particles.as_str(), d.spark_life, d.spark_gap);
        }
        "cycle" => {
            let all = Particles::ALL;
            let next = all[(all.iter().position(|p| *p == look.particles).unwrap_or(0) + 1) % all.len()];
            write_file("trail-particles", next.as_str());
            println!("{}", next.as_str());
        }
        v => {
            let Some(p) = Particles::parse(v) else { usage() };
            write_file("trail-particles", p.as_str());
            println!("{}", p.as_str());
        }
    }
}

#[cfg(target_os = "linux")]
fn native_trails_available() -> bool {
    std::process::Command::new("hyprctl")
        .args(["kybernative", "status"])
        .output()
        .is_ok_and(|o| {
            o.status.success() && String::from_utf8_lossy(&o.stdout).contains("trails=1")
        })
}

#[cfg(target_os = "linux")]
fn cmd_doctor() {
    let runtime_mode = read_file("native-runtime");
    let uses_native_bridge = !runtime_mode.is_empty() && runtime_mode != "overlay";
    let in_input_group = std::process::Command::new("id")
        .arg("-nG")
        .output()
        .is_ok_and(|output| {
            output.status.success()
                && String::from_utf8_lossy(&output.stdout)
                    .split_whitespace()
                    .any(|group| group == "input")
        });
    let click_glow_off_expected = !uses_native_bridge && !in_input_group;
    let service = std::process::Command::new("systemctl")
        .args(["--user", "is-active", "kyber-ignite.service"])
        .output();
    let service_ok = service.is_ok_and(|o| o.status.success());
    let native_ok = if uses_native_bridge {
        std::process::Command::new("hyprctl")
            .args(["kybernative", "status"])
            .output()
            .is_ok_and(|o| {
                o.status.success() && String::from_utf8_lossy(&o.stdout).contains("protocol=2")
            })
    } else {
        true
    };
    println!("Rust runtime: {}", if service_ok { "running" } else { "not running" });
    if uses_native_bridge {
        println!(
            "Native cursor bridge: {}",
            if native_ok { "loaded" } else { "not loaded" }
        );
        println!("Input: compositor button events (no input-group permission needed)");
    } else {
        println!("Input: read-only pointer events from /dev/input");
        if click_glow_off_expected {
            println!("Click glow: optional and off (this session is not in the input group)");
        } else {
            println!("Input permission: active through the input group");
        }
    }
    let cfg = load_config();
    println!("Sound: {:?}, volume {}%", cfg.sound, cfg.volume);
    if !uses_native_bridge || native_trails_available() {
        println!("Movement trails: {}", if cfg.trail { "on" } else { "off" });
    } else {
        println!("Movement trails: native bridge update needed");
    }
    if (service_ok || click_glow_off_expected) && native_ok {
        if click_glow_off_expected {
            println!("Everything checks out. (The optional click glow is off.)");
        } else {
            println!("Everything checks out.");
        }
    } else {
        eprintln!("Native runtime needs attention; check kyber-ignite.service.");
        std::process::exit(1);
    }
}

#[cfg(not(target_os = "linux"))]
fn cmd_doctor() {
    eprintln!("Native runtime doctor is currently Linux-only.");
    std::process::exit(3);
}

fn cmd_random() {
    let prev = read_file("random-color");
    let seed = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos() as u64)
        .unwrap_or(1);
    let next = pick_random(&mut SmallRng::seed(seed), &prev);
    write_file("random-color", &next);
    write_file("color-mode", "random");
    println!("{next}");
}

#[cfg(target_os = "linux")]
fn cursor_size() -> String {
    for var in ["HYPRCURSOR_SIZE", "XCURSOR_SIZE"] {
        if let Ok(v) = std::env::var(var) {
            if v.chars().all(|c| c.is_ascii_digit()) && !v.is_empty() {
                return v;
            }
        }
    }
    "24".to_string()
}

/// Bounce through another theme first: Hyprland skips reloading when asked
/// for the name it already has. Port of `bin/kyber::set_cursor`.
#[cfg(target_os = "linux")]
fn set_cursor(theme: &str) {
    let size = cursor_size();
    let bounce = if theme == "Adwaita" {
        "Omarchy-Kyber"
    } else {
        "Adwaita"
    };
    if std::process::Command::new("hyprctl")
        .arg("setcursor")
        .arg(bounce)
        .arg(&size)
        .output()
        .is_ok()
    {
        let mut ok = false;
        for _ in 0..3 {
            if std::process::Command::new("hyprctl")
                .arg("setcursor")
                .arg(theme)
                .arg(&size)
                .output()
                .map(|o| o.status.success())
                .unwrap_or(false)
            {
                ok = true;
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(400));
        }
        if !ok {
            eprintln!("Hyprland did not answer; the cursor will update on your next theme switch or re-login.");
        }
    }
    let _ = std::process::Command::new("gsettings")
        .args(["set", "org.gnome.desktop.interface", "cursor-theme", theme])
        .output();
}

/// What a random roll belongs to: this theme in this boot. A new theme or a
/// new boot means a new random color; a shell reload does not. Port of
/// `bin/kyber::roll_context`.
#[cfg(target_os = "linux")]
fn roll_context() -> String {
    let theme = [
        ".local/state/omarchy/current/theme.name",
        ".config/omarchy/current/theme.name",
    ]
    .iter()
    .filter_map(|rel| std::fs::read_to_string(home().join(rel)).ok())
    .next()
    .and_then(|t| t.lines().next().map(|s| s.trim().to_string()))
    .unwrap_or_default();
    let boot = std::fs::read_to_string("/proc/sys/kernel/random/boot_id").unwrap_or_default();
    format!("{}|{}", theme.trim(), boot.trim())
}

#[cfg(target_os = "linux")]
fn cmd_apply() {
    use kyber_core::config::ColorMode;
    // Random mode: re-roll on theme/boot change, like bin/kyber.
    let cfg = load_config();
    if cfg.mode == ColorMode::Random {
        let ctx = roll_context();
        if read_file("random-color").is_empty() || read_file("roll-context") != ctx {
            cmd_random();
            write_file("roll-context", &ctx);
        }
    }
    let cfg = load_config();
    let accent = cfg.effective_color(linux_accent().as_deref());
    let icons = home().join(".local/share/icons");
    match kyber_core::theme::apply(&accent, &icons) {
        Ok(_) => {
            write_file("active", "");
            // `active` is a flag file; keep it empty like bin/kyber's `: >`.
            set_cursor("Omarchy-Kyber");
            println!(
                "Kyber applied (accent {}, size {}).",
                accent.to_lowercase(),
                cursor_size()
            );
        }
        Err(e) => {
            eprintln!("kyber: build/install failed: {e}");
            std::process::exit(1);
        }
    }
}

#[cfg(not(target_os = "linux"))]
fn cmd_apply() {
    eprintln!("kyber: native cursor apply is currently supported on Linux/Hyprland only.");
    std::process::exit(3);
}

#[cfg(target_os = "linux")]
fn cmd_off() {
    let _ = std::fs::remove_file(state_dir().join("active"));
    if read_file("native-runtime") != "overlay" {
        let _ = std::process::Command::new("hyprctl")
            .args(["kybernative", "stop"])
            .output();
    }
    set_cursor("Adwaita");
    println!("Kyber off: back to the Adwaita cursor.");
}

#[cfg(not(target_os = "linux"))]
fn cmd_off() {
    eprintln!("kyber: native cursor control is currently supported on Linux/Hyprland only.");
    std::process::exit(3);
}

fn cmd_sounds(args: &[String]) {
    let sub = args.first().map(|s| s.as_str()).unwrap_or("list");
    let dir = state_dir().join("sounds");
    match sub {
        "default" => match sound_pack::export(&dir) {
            Ok(_) => {
                // Export the bundled recordings, never the rejected generated pack.
                cmd_sounds(&["list".to_string()]);
            }
            Err(e) => {
                eprintln!("kyber: cannot write sounds: {e}");
                std::process::exit(1);
            }
        },
        "list" => {
            for kind in sound_pack::ROLES {
                let d = dir.join(kind);
                let mut files: Vec<_> = std::fs::read_dir(&d)
                    .into_iter()
                    .flatten()
                    .flatten()
                    .map(|entry| entry.path())
                    .filter(|path| path.is_file())
                    .collect();
                files.sort();
                println!("{kind}: {} clip(s)", files.len());
                for path in files {
                    println!("  {}", path.file_name().unwrap_or_default().to_string_lossy());
                }
            }
        }
        "add" => {
            let Some(source) = args.get(1).map(std::path::Path::new) else {
                eprintln!("usage: kyber sounds add FILE ignite|off|clash|swipe|hum");
                std::process::exit(2);
            };
            let Some(role) = args.get(2).map(String::as_str) else {
                eprintln!("usage: kyber sounds add FILE ignite|off|clash|swipe|hum");
                std::process::exit(2);
            };
            let extension_ok = source
                .extension()
                .and_then(|value| value.to_str())
                .is_some_and(|value| matches!(value.to_ascii_lowercase().as_str(), "wav" | "mp3" | "ogg" | "oga" | "flac"));
            if !sound_pack::ROLES.contains(&role) || !source.is_file() || !extension_ok {
                eprintln!("use an existing WAV, MP3, Ogg or FLAC file and a valid sound role");
                std::process::exit(2);
            }
            let Some(name) = source.file_name() else {
                eprintln!("sound file must have a filename");
                std::process::exit(2);
            };
            let destination_dir = dir.join(role);
            if let Err(error) = std::fs::create_dir_all(&destination_dir) {
                eprintln!("kyber: cannot create sound folder: {error}");
                std::process::exit(1);
            }
            let destination = destination_dir.join(name);
            if destination.exists() {
                eprintln!("kyber: {} already exists; no file replaced", destination.display());
                std::process::exit(1);
            }
            if let Err(error) = std::fs::copy(source, &destination) {
                eprintln!("kyber: cannot add sound: {error}");
                std::process::exit(1);
            }
            println!("Added {} to sounds/{role}.", name.to_string_lossy());
        }
        "preview" => {
            let gain = (load_config().volume as f64 / 100.0).powi(3);
            let mut played = false;
            for role in sound_pack::ROLES {
                let mut files: Vec<_> = std::fs::read_dir(dir.join(role))
                    .into_iter()
                    .flatten()
                    .flatten()
                    .map(|entry| entry.path())
                    .filter(|path| path.is_file())
                    .collect();
                files.sort();
                if let Some(path) = files.first() {
                    println!("Playing {role}: {}", path.file_name().unwrap_or_default().to_string_lossy());
                    let status = std::process::Command::new("pw-play")
                        .arg("--volume")
                        .arg(format!("{gain:.4}"))
                        .arg(path)
                        .status();
                    if !status.is_ok_and(|status| status.success()) {
                        eprintln!("kyber: could not play {role}; check PipeWire and pw-play");
                        std::process::exit(1);
                    }
                    played = true;
                } else {
                    println!("{role}: no local clips");
                }
            }
            if !played {
                eprintln!("No local clips found. Run: kyber sounds default");
                std::process::exit(1);
            }
        }
        _ => {
            eprintln!("usage: kyber sounds default | add FILE ROLE | list | preview");
            std::process::exit(2);
        }
    }
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let (cmd, rest) = match args.split_first() {
        Some((c, r)) => (c.as_str(), r),
        None => usage(),
    };
    match cmd {
        "color" => cmd_color(rest),
        "volume" => cmd_volume(rest),
        "sound" => cmd_sound(rest),
        "style" => cmd_trail_style(rest),
        "sparks" => cmd_sparks(rest),
        "effects" => cmd_effects(rest),
        "trail" => cmd_trail(rest),
        "aim" => cmd_aim(rest),
        "doctor" => cmd_doctor(),
        "random" => {
            if !rest.is_empty() {
                usage();
            }
            cmd_random();
            #[cfg(target_os = "linux")]
            if load_config().active {
                cmd_apply();
            }
        }
        "sounds" => cmd_sounds(rest),
        "config" => {
            let cfg = load_config();
            println!("{}", widget_config(&cfg, linux_accent().as_deref()));
        }
        "status" => {
            let cfg = load_config();
            println!("active:  {}", if cfg.active { "yes" } else { "no" });
            println!(
                "color:   {} ({})",
                cfg.mode.as_str(),
                cfg.effective_color(linux_accent().as_deref())
            );
            println!("sound:   {}", format!("{:?}", cfg.sound).to_lowercase());
            println!("trail:   {}", if cfg.trail { "on" } else { "off" });
            let aim = kyber_core::aim::AimSettings::load(&state_dir());
            println!(
                "aim:     {} (mirror {})",
                if aim.enabled { "on" } else { "off" },
                if aim.mirror { "on" } else { "off" }
            );
            println!(
                "look:    {} + {} ({} ms, every {} px)",
                cfg.look.style, cfg.look.particles.as_str(), cfg.look.spark_life, cfg.look.spark_gap
            );
            println!(
                "volume:  {} (gain {})",
                cfg.volume,
                volume_gain_str(cfg.volume as i32)
            );
        }
        "apply" => {
            if !rest.is_empty() {
                usage();
            }
            cmd_apply();
        }
        "off" => {
            if !rest.is_empty() {
                usage();
            }
            cmd_off();
        }
        "setup" => {
            eprintln!("kyber: '{cmd}' is still handled by bin/kyber until the platform backends land (after step 2).");
            std::process::exit(3);
        }
        _ => usage(),
    }
}
