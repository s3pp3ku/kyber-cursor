//! Settings: glow-color source, sound/trail/volume. Port of the
//! `color_mode / pick_random / roll_color / volume_gain` logic in `bin/kyber`.
//!
//! On Linux the caller may pass the Omarchy accent in as `theme_accent`.

use serde::{Deserialize, Serialize};

use crate::color::is_valid_hex;

/// Where the glow color comes from. Port of the `bin/kyber::color_mode`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum ColorMode {
    /// Follow the passed-in theme accent.
    #[default]
    Theme,
    /// A user-picked hex color.
    Custom,
    /// A vivid random color, re-rolled on demand / theme change / boot.
    Random,
}

impl ColorMode {
    pub fn parse(s: &str) -> Option<Self> {
        match s.trim().to_lowercase().as_str() {
            "theme" | "accent" | "none" => Some(Self::Theme),
            "custom" => Some(Self::Custom),
            "random" => Some(Self::Random),
            _ => None,
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Theme => "theme",
            Self::Custom => "custom",
            Self::Random => "random",
        }
    }
}

/// The path drawn behind the pointer while the blade is lit.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum TrailStyle {
    /// No path; the particles, if any, still fly.
    None,
    /// A tail that tapers behind the pointer, brightest at its head.
    #[default]
    Comet,
    /// A smooth glowing band that follows every curve.
    Ribbon,
    /// Evenly spaced dots along the path, shrinking toward the end.
    Beads,
    /// Rings left along the path, widening as they fade.
    Echoes,
}

impl TrailStyle {
    pub const ALL: [Self; 5] = [Self::Comet, Self::Ribbon, Self::Beads, Self::Echoes, Self::None];

    pub fn parse(s: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|v| v.as_str() == s.trim().to_lowercase())
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::None => "none",
            Self::Comet => "comet",
            Self::Ribbon => "ribbon",
            Self::Beads => "beads",
            Self::Echoes => "echoes",
        }
    }
}

/// What the blade throws off as it moves.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum Particles {
    /// Sparks that fly off, fall and burn out: the lightsaber look.
    #[default]
    Sparks,
    /// Bubbles that float up, swaying.
    Bubbles,
    Off,
}

impl Particles {
    pub const ALL: [Self; 3] = [Self::Sparks, Self::Bubbles, Self::Off];

    pub fn parse(s: &str) -> Option<Self> {
        match s.trim().to_lowercase().as_str() {
            "none" => Some(Self::Off),
            v => Self::ALL.into_iter().find(|p| p.as_str() == v),
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Sparks => "sparks",
            Self::Bubbles => "bubbles",
            Self::Off => "off",
        }
    }
}

/// The whole drag-trail look. Each field is one small file in the settings
/// folder, so every host (Hyprland plugin, overlay, Windows) reads the same.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct TrailLook {
    pub style: TrailStyle,
    pub particles: Particles,
    /// How long a spark or bubble lasts, in milliseconds.
    pub spark_life: u32,
    /// Pointer travel between sparks, in logical pixels: smaller is denser.
    pub spark_gap: u32,
    /// Mix in real embers that cool from white-hot to red ("mixed"), or keep
    /// every spark in the glow color ("glow").
    pub embers: bool,
}

impl Default for TrailLook {
    fn default() -> Self {
        Self { style: TrailStyle::Comet, particles: Particles::Sparks, spark_life: 850, spark_gap: 20, embers: true }
    }
}

impl TrailLook {
    pub const SPARK_LIFE_MS: (u32, u32) = (150, 1500);
    pub const SPARK_GAP_PX: (u32, u32) = (2, 40);
    pub const FILES: [&'static str; 5] = ["trail-style", "trail-particles", "spark-life", "spark-gap", "spark-colors"];

    /// Read from a settings folder; missing or bad files keep the default.
    pub fn load(dir: &std::path::Path) -> Self {
        let read = |name: &str| std::fs::read_to_string(dir.join(name)).unwrap_or_default();
        let d = Self::default();
        let num = |name: &str, (lo, hi): (u32, u32), fallback: u32| {
            read(name).trim().parse::<u32>().map_or(fallback, |v| v.clamp(lo, hi))
        };
        Self {
            style: TrailStyle::parse(&read("trail-style")).unwrap_or(d.style),
            particles: Particles::parse(&read("trail-particles")).unwrap_or(d.particles),
            spark_life: num("spark-life", Self::SPARK_LIFE_MS, d.spark_life),
            spark_gap: num("spark-gap", Self::SPARK_GAP_PX, d.spark_gap),
            embers: match read("spark-colors").trim() {
                "glow" => false,
                "mixed" => true,
                _ => d.embers,
            },
        }
    }

    /// The file contents, in the order of [`Self::FILES`].
    pub fn file_values(&self) -> [String; 5] {
        [
            self.style.as_str().into(),
            self.particles.as_str().into(),
            self.spark_life.to_string(),
            self.spark_gap.to_string(),
            self.spark_colors().into(),
        ]
    }

    /// "mixed" (glow color plus embers) or "glow" (glow color only).
    pub fn spark_colors(&self) -> &'static str {
        if self.embers { "mixed" } else { "glow" }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum SoundMode {
    #[default]
    All,
    Ignite,
    Mute,
}

impl SoundMode {
    pub fn parse(s: &str) -> Option<Self> {
        match s.trim().to_lowercase().as_str() {
            "all" => Some(Self::All),
            "ignite" => Some(Self::Ignite),
            "mute" => Some(Self::Mute),
            _ => None,
        }
    }

    pub fn cycle(self) -> Self {
        match self {
            Self::All => Self::Ignite,
            Self::Ignite => Self::Mute,
            Self::Mute => Self::All,
        }
    }
}

impl std::fmt::Display for TrailStyle {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

impl std::fmt::Display for SoundMode {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Self::All => "all",
            Self::Ignite => "ignite",
            Self::Mute => "mute",
        })
    }
}

/// Portable settings model. The CLI adapts its JSON to the existing tray
/// contract (`trail` as "on"/"off", plus the effective `color` field).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KyberConfig {
    #[serde(default)]
    pub active: bool,
    #[serde(default)]
    pub mode: ColorMode,
    #[serde(default)]
    pub custom: String,
    #[serde(default)]
    pub random: String,
    #[serde(default)]
    pub sound: SoundMode,
    #[serde(default = "default_trail")]
    pub trail: bool,
    #[serde(default)]
    pub look: TrailLook,
    #[serde(default = "default_volume")]
    pub volume: u8,
}

fn default_trail() -> bool {
    true
}

fn default_volume() -> u8 {
    55
}

impl Default for KyberConfig {
    fn default() -> Self {
        Self {
            active: false,
            mode: ColorMode::Theme,
            custom: String::new(),
            random: String::new(),
            sound: SoundMode::All,
            trail: true,
            look: TrailLook::default(),
            volume: 55,
        }
    }
}

impl KyberConfig {
    /// Read the whole settings folder. Missing or unreadable files keep the
    /// defaults, so a half-written settings folder never breaks a runtime.
    pub fn load(dir: &std::path::Path) -> Self {
        let read = |name: &str| {
            std::fs::read_to_string(dir.join(name))
                .unwrap_or_default()
                .trim()
                .to_string()
        };
        let mode = ColorMode::parse(&read("color-mode")).unwrap_or_else(|| {
            if dir.join("color").exists() {
                ColorMode::Custom
            } else {
                ColorMode::Theme
            }
        });
        Self {
            active: dir.join("active").exists(),
            mode,
            custom: read("color"),
            random: read("random-color"),
            sound: SoundMode::parse(&read("sound-mode")).unwrap_or(SoundMode::All),
            trail: read("trail-mode") != "off",
            look: TrailLook::load(dir),
            volume: read("volume").parse().unwrap_or(55),
        }
    }

    /// Resolve the glow color, using the Omarchy accent when available.
    pub fn effective_color(&self, theme_accent: Option<&str>) -> String {
        match self.mode {
            ColorMode::Custom if is_valid_hex_full(&self.custom) => self.custom.to_lowercase(),
            ColorMode::Random if is_valid_hex_full(&self.random) => self.random.to_lowercase(),
            _ => theme_accent
                .filter(|a| is_valid_hex_full(a))
                .map(|a| a.to_lowercase())
                .unwrap_or_else(|| "#7aa2f7".to_string()),
        }
    }

    pub fn to_json_line(&self) -> String {
        serde_json::to_string(self).unwrap_or_else(|_| "{}".to_string())
    }
}

fn is_valid_hex_full(h: &str) -> bool {
    h.len() == 7 && h.starts_with('#') && is_valid_hex(&h[1..])
}

/// The Omarchy theme's accent color, if this session has one. Linux only;
/// every other platform resolves `ColorMode::Theme` to the fallback blue.
#[cfg(target_os = "linux")]
pub fn theme_accent() -> Option<String> {
    let home = crate::paths::home_dir().ok()?;
    for rel in [
        ".local/state/omarchy/current/theme/colors.toml",
        ".config/omarchy/current/theme/colors.toml",
    ] {
        if let Ok(text) = std::fs::read_to_string(home.join(rel)) {
            for line in text.lines() {
                let line = line.trim();
                if let Some(value) = line.strip_prefix("accent") {
                    let value = value
                        .trim()
                        .trim_start_matches(['=', ' ', '\t', '"'])
                        .trim_end_matches('"')
                        .trim();
                    if is_valid_hex_full(value) {
                        return Some(value.to_lowercase());
                    }
                }
            }
        }
    }
    None
}

#[cfg(not(target_os = "linux"))]
pub fn theme_accent() -> Option<String> {
    None
}

/// Slider percent -> amplitude. Cubic, like system volume sliders.
/// Port of `ignite-listener::volume_gain` / `bin/kyber` preview gain.
pub fn volume_gain(percent: i32) -> f64 {
    let p = percent.clamp(0, 100) as f64 / 100.0;
    p.powi(3)
}

/// Format the gain exactly like the Python (`'%.4f'`).
pub fn volume_gain_str(percent: i32) -> String {
    format!("{:.4}", volume_gain(percent))
}

// --- random vivid color ----------------------------------------

// Port of `bin/kyber::pick_random`: HSV with high saturation/value and a
// minimum hue distance from the previous color so re-rolls look clearly
// different.

/// Minimal PRNG (xorshift64*) so `kyber-core` needs no `rand` dependency
/// and re-rolls are reproducible in tests.
#[derive(Debug, Clone)]
pub struct SmallRng(u64);

impl SmallRng {
    pub fn seed(seed: u64) -> Self {
        Self(seed | 1)
    }

    fn next_u64(&mut self) -> u64 {
        let mut x = self.0;
        x ^= x >> 12;
        x ^= x << 25;
        x ^= x >> 27;
        self.0 = x;
        x.wrapping_mul(0x2545_F491_4F6C_DD1D)
    }

    pub fn next_f64(&mut self) -> f64 {
        // 53-bit float in [0, 1).
        ((self.next_u64() >> 11) as f64) / ((1u64 << 53) as f64)
    }

    pub fn range(&mut self, lo: f64, hi: f64) -> f64 {
        lo + self.next_f64() * (hi - lo)
    }
}

fn hsv_to_rgb(h: f64, s: f64, v: f64) -> (u8, u8, u8) {
    let h = h.fract().clamp(0.0, 1.0);
    let i = (h * 6.0).floor() as i32;
    let f = h * 6.0 - i as f64;
    let p = v * (1.0 - s);
    let q = v * (1.0 - f * s);
    let t = v * (1.0 - (1.0 - f) * s);
    let (r, g, b) = match i.rem_euclid(6) {
        0 => (v, t, p),
        1 => (q, v, p),
        2 => (p, v, t),
        3 => (p, q, v),
        4 => (t, p, v),
        _ => (v, p, q),
    };
    (
        (r * 255.0).round() as u8,
        (g * 255.0).round() as u8,
        (b * 255.0).round() as u8,
    )
}

fn hue_of(hex: &str) -> Option<f64> {
    let h = hex.strip_prefix('#')?;
    if h.len() != 6 {
        return None;
    }
    let r = u8::from_str_radix(&h[0..2], 16).ok()? as f64 / 255.0;
    let g = u8::from_str_radix(&h[2..4], 16).ok()? as f64 / 255.0;
    let b = u8::from_str_radix(&h[4..6], 16).ok()? as f64 / 255.0;
    let max = r.max(g).max(b);
    let min = r.min(g).min(b);
    if max == min {
        return Some(0.0);
    }
    let d = max - min;
    let hue = if max == r {
        ((g - b) / d) % 6.0
    } else if max == g {
        (b - r) / d + 2.0
    } else {
        (r - g) / d + 4.0
    };
    Some((hue / 6.0).rem_euclid(1.0))
}

/// Pick a vivid random glow color, clearly hue-shifted from `prev_hex`.
pub fn pick_random(rng: &mut SmallRng, prev_hex: &str) -> String {
    let prev_hue = hue_of(prev_hex);
    loop {
        let h = rng.next_f64();
        let distinct = match prev_hue {
            None => true,
            Some(ph) => {
                let d = (h - ph).abs();
                d.min(1.0 - d) >= 0.17
            }
        };
        if distinct {
            let s = rng.range(0.65, 0.95);
            let v = rng.range(0.85, 1.0);
            let (r, g, b) = hsv_to_rgb(h, s, v);
            return format!("#{r:02x}{g:02x}{b:02x}");
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn volume_curve_is_cubic() {
        assert!((volume_gain(100) - 1.0).abs() < 1e-9);
        assert!((volume_gain(0)).abs() < 1e-9);
        assert_eq!(volume_gain_str(55), format!("{:.4}", 0.55f64.powi(3)));
        // 18% linear would still sound loud; cubic keeps it quiet.
        assert!(volume_gain(18) < 0.01);
    }

    #[test]
    fn effective_color_prefers_picker() {
        let cfg = KyberConfig {
            mode: ColorMode::Custom,
            custom: "#FF7A00".into(),
            ..KyberConfig::default()
        };
        assert_eq!(cfg.effective_color(Some("#00ff00")), "#ff7a00");
    }

    #[test]
    fn effective_color_falls_back_to_theme_then_blue() {
        let cfg = KyberConfig::default();
        assert_eq!(cfg.effective_color(Some("#00ff00")), "#00ff00");
        assert_eq!(cfg.effective_color(None), "#7aa2f7");
    }

    #[test]
    fn random_is_vivid_and_distinct() {
        let mut rng = SmallRng::seed(42);
        let prev = "#ff0000";
        for _ in 0..20 {
            let next = pick_random(&mut rng, prev);
            assert!(is_valid_hex_full(&next));
            let (ph, nh) = (hue_of(prev).unwrap(), hue_of(&next).unwrap());
            let d = (ph - nh).abs().min(1.0 - (ph - nh).abs());
            assert!(d >= 0.17 - 1e-9, "{next} too close to {prev}");
        }
    }

    #[test]
    fn sound_mode_cycles() {
        assert_eq!(SoundMode::All.cycle(), SoundMode::Ignite);
        assert_eq!(SoundMode::Ignite.cycle(), SoundMode::Mute);
        assert_eq!(SoundMode::Mute.cycle(), SoundMode::All);
    }
}
