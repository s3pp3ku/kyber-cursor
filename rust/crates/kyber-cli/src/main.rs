//! `kyber` command (step 1): picker-first settings that interop with the
//! existing `~/.config/kyber-cursor` file layout, so the Rust CLI can run
//! alongside the current bash `bin/kyber` without fighting it.
//!
//! Native daily mode also owns Linux apply/off, sound/volume controls and doctor.
//! Setup/uninstall and extra sound-file management retain their existing wrapper.

use std::path::PathBuf;

use kyber_core::config::{
    pick_random, volume_gain_str, ColorMode, KyberConfig, SmallRng, SoundMode,
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
    let mode = ColorMode::parse(&read_file("color-mode")).unwrap_or_else(|| {
        if state_dir().join("color").exists() {
            ColorMode::Custom
        } else {
            ColorMode::Theme
        }
    });
    KyberConfig {
        active: state_dir().join("active").exists(),
        mode,
        custom: read_file("color"),
        random: read_file("random-color"),
        sound: SoundMode::parse(&read_file("sound-mode")).unwrap_or(SoundMode::All),
        trail: read_file("trail-mode") != "off",
        volume: read_file("volume").parse().unwrap_or(55),
    }
}

/// Preserve the existing `bin/kyber config` contract used by the tray panel.
/// The portable settings model keeps a boolean internally, but QML compares
/// `trail` to "on"/"off" and needs `color` for its icon and tooltip.
fn widget_config(cfg: &KyberConfig, accent: Option<&str>) -> serde_json::Value {
    serde_json::json!({
        "active": cfg.active,
        "mode": cfg.mode.as_str(),
        "color": if cfg.mode == ColorMode::Theme { "theme".to_string() } else { cfg.effective_color(accent) },
        "custom": cfg.custom,
        "random": cfg.random,
        "sound": cfg.sound,
        "trail": if cfg.trail { "on" } else { "off" },
        "volume": cfg.volume,
    })
}

fn usage() -> ! {
    eprintln!("usage: kyber apply | off | color [theme|custom|random|#rrggbb] | sound [all|ignite|mute|cycle] | trail [on|off] | volume [0-100] | random | sounds [default|list] | config | status | doctor");
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
    for rel in [
        ".local/state/omarchy/current/theme/colors.toml",
        ".config/omarchy/current/theme/colors.toml",
    ] {
        let p = home().join(rel);
        if let Ok(text) = std::fs::read_to_string(&p) {
            for line in text.lines() {
                let line = line.trim();
                if let Some(v) = line.strip_prefix("accent") {
                    let v = v
                        .trim()
                        .trim_start_matches(['=', ' ', '\t', '"'])
                        .trim_end_matches('"')
                        .trim();
                    let v = v.trim_matches('"').trim();
                    if v.len() == 7 && v.starts_with('#') {
                        return Some(v.to_lowercase());
                    }
                }
            }
        }
    }
    None
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
    if enabled && state_dir().join("native-runtime").exists() {
        #[cfg(target_os = "linux")]
        if !native_trails_available() {
            eprintln!("The running native cursor bridge needs the trail update first.");
            std::process::exit(3);
        }
    }
    write_file("trail-mode", if enabled { "on" } else { "off" });
    println!("{}", if enabled { "on" } else { "off" });
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
    let service = std::process::Command::new("systemctl")
        .args(["--user", "is-active", "kyber-ignite.service"])
        .output();
    let native = std::process::Command::new("hyprctl")
        .args(["kybernative", "status"])
        .output();
    let service_ok = service.is_ok_and(|o| o.status.success());
    let native_ok = native.is_ok_and(|o| {
        o.status.success() && String::from_utf8_lossy(&o.stdout).contains("protocol=2")
    });
    println!(
        "Rust native runtime: {}",
        if service_ok { "running" } else { "not running" }
    );
    println!(
        "Native cursor bridge: {}",
        if native_ok { "loaded" } else { "not loaded" }
    );
    println!("Input: compositor button events (no input-group permission needed)");
    let cfg = load_config();
    println!("Sound: {:?}, volume {}%", cfg.sound, cfg.volume);
    if native_trails_available() {
        println!("Movement trails: {}", if cfg.trail { "on" } else { "off" });
    } else {
        println!("Movement trails: native bridge update needed");
    }
    if service_ok && native_ok {
        println!("Everything checks out.");
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
    let _ = std::process::Command::new("hyprctl")
        .args(["kybernative", "stop"])
        .output();
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
                let n = std::fs::read_dir(&d).map(|r| r.count()).unwrap_or(0);
                println!("{kind}: {n} clip(s)");
            }
        }
        _ => {
            eprintln!("usage: kyber sounds default | list");
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
        "trail" => cmd_trail(rest),
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
