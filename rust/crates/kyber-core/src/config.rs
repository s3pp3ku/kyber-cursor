//! Settings: glow-color source, sound/trail/volume. Port of the
//! `color_mode / pick_random / roll_color / volume_gain` logic in `bin/kyber`.
//!
//! On Linux the caller may pass the Omarchy accent in as `theme_accent`.

use serde::{Deserialize, Serialize};

use crate::color::is_valid_hex;

/// Where the glow color comes from. Port of `bin/kyber::color_mode`.
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
            volume: 55,
        }
    }
}

impl KyberConfig {
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

// --- random vivid color -----------------------------------------------
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
