//! Accent/glow color math. Port of `build.py::_rgb/lighten/darken/luminance`
//! plus the dark-accent lift used for the overlay haze.

/// Target relative luminance so deep orange/red/purple glow about as
/// strongly as green. See `build.py::GLOW_LUMA_TARGET`.
pub const GLOW_LUMA_TARGET: f64 = 0.40;
/// Extra haze strength for dark glows (`build.py::DARK_K`).
pub const DARK_K: f64 = 0.40;

/// `#rrggbb` -> (r, g, b). Returns None unless the format matches exactly.
pub fn parse_hex(h: &str) -> Option<[u8; 3]> {
    let h = h.strip_prefix('#')?;
    if h.len() != 6 {
        return None;
    }
    let r = u8::from_str_radix(&h[0..2], 16).ok()?;
    let g = u8::from_str_radix(&h[2..4], 16).ok()?;
    let b = u8::from_str_radix(&h[4..6], 16).ok()?;
    Some([r, g, b])
}

pub fn is_valid_hex(h: &str) -> bool {
    let h = h.strip_prefix('#').unwrap_or(h);
    h.len() == 6 && h.chars().all(|c| c.is_ascii_hexdigit())
}

pub fn to_hex(rgb: [u8; 3]) -> String {
    format!("#{:02x}{:02x}{:02x}", rgb[0], rgb[1], rgb[2])
}

/// Mix towards white by `k` (0..1). Port of `build.py::lighten`.
pub fn lighten(hex: &str, k: f64) -> String {
    let [r, g, b] = parse_hex(hex).unwrap_or([0x7a, 0xa2, 0xf7]);
    let m = |c: u8| (c as f64 + (255.0 - c as f64) * k).round() as u8;
    to_hex([m(r), m(g), m(b)])
}

/// Scale towards black by `k` (0..1). Port of `build.py::darken`.
pub fn darken(hex: &str, k: f64) -> String {
    let [r, g, b] = parse_hex(hex).unwrap_or([0x7a, 0xa2, 0xf7]);
    let m = |c: u8| (c as f64 * (1.0 - k)).round() as u8;
    to_hex([m(r), m(g), m(b)])
}

fn lin_channel(c: u8) -> f64 {
    let v = c as f64 / 255.0;
    if v <= 0.04045 {
        v / 12.92
    } else {
        ((v + 0.055) / 1.055).powf(2.4)
    }
}

/// WCAG relative luminance, 0..1. Port of `build.py::luminance`.
pub fn luminance(hex: &str) -> f64 {
    let [r, g, b] = parse_hex(hex).unwrap_or([0xe8, 0xec, 0xf2]);
    0.2126 * lin_channel(r) + 0.7152 * lin_channel(g) + 0.0722 * lin_channel(b)
}

/// How much to boost a dark glow's haze so every accent glows about
/// equally. Port of the `lift` computation in `build.py::write_overlay`.
pub fn glow_lift(glow_hex: &str) -> f64 {
    let luma = luminance(glow_hex).max(0.01);
    (GLOW_LUMA_TARGET / luma).sqrt().clamp(1.0, 1.6)
}

/// Haze tint nudged towards white for dark glows. Port of `build.py::write_overlay`.
pub fn haze_for_glow(glow_hex: &str) -> String {
    let luma = luminance(glow_hex);
    let k = (0.0f64).max(1.0 - luma / GLOW_LUMA_TARGET) * 0.45;
    lighten(glow_hex, k.min(0.35))
}

/// Beam tint with a modest luminance floor. Bright cyan/green remain
/// unchanged; weak red/blue hues move slightly toward white but keep hue.
pub fn beam_color(glow_hex: &str) -> String {
    let luma = luminance(glow_hex);
    let k = (0.0f64).max(1.0 - luma / GLOW_LUMA_TARGET) * 0.46;
    lighten(glow_hex, k.min(0.36))
}

/// Width multiplier for weak hues. Bright teal/green remain at 1.0 while
/// low-luminance red/blue gain up to 45 percent.
pub fn beam_width_scale(glow_hex: &str) -> f64 {
    let luma = luminance(glow_hex);
    1.0 + ((0.0f64).max(1.0 - luma / GLOW_LUMA_TARGET) * 0.58).min(0.45)
}

/// Extra body for saturated, visually dim colors; neutral black stays unchanged.
pub fn vivid_beam_boost(hex: &str) -> f64 {
    let rgb = parse_hex(hex).unwrap_or([0; 3]);
    if *rgb.iter().max().unwrap() <= 32 {
        return 0.0;
    }
    let chroma = (*rgb.iter().max().unwrap() - *rgb.iter().min().unwrap()) as f64 / 255.0;
    (chroma / 0.40).min(1.0) * (1.0 - luminance(hex) / GLOW_LUMA_TARGET).max(0.0)
}

/// Blue needs a strong core, not the luminance compensation's oversized haze.
/// Keep neutral near-black and non-blue reference colors unchanged.
pub fn blue_haze_weight(hex: &str) -> f64 {
    let [r, g, b] = parse_hex(hex).unwrap_or([0; 3]);
    ((b as f64 - r.max(g) as f64) / 128.0).clamp(0.0, 1.0)
        * ((b as f64 - 32.0) / 96.0).clamp(0.0, 1.0)
}

/// White-hot core tint for dark backgrounds. The halo keeps the selected
/// hue, while this inner tint reaches a readable luminance floor.
pub fn blade_core_color(glow_hex: &str) -> String {
    if luminance(glow_hex) >= 0.52 {
        return glow_hex.to_ascii_lowercase();
    }
    let (mut lo, mut hi) = (0.0, 0.80);
    for _ in 0..12 {
        let mid = (lo + hi) / 2.0;
        if luminance(&lighten(glow_hex, mid)) < 0.52 {
            lo = mid;
        } else {
            hi = mid;
        }
    }
    lighten(glow_hex, hi)
}

/// Keep the glow tint visible without making dim accents paint dark smoke or
/// pale accents disappear on white. Only the emitted light is tone-adjusted;
/// the stored selection and idle cursor keep their original color.
pub fn glow_tint(hex: &str, light_background: bool) -> String {
    let (floor, ceiling) = if light_background {
        (0.22, 0.50)
    } else {
        (0.28, 0.72)
    };
    let luma = luminance(hex);
    if (floor..=ceiling).contains(&luma) {
        return hex.to_lowercase();
    }
    let (mut lo, mut hi) = (0.0, 1.0);
    let target = if luma < floor { floor } else { ceiling };
    for _ in 0..12 {
        let mid = (lo + hi) / 2.0;
        let candidate = if luma < floor {
            lighten(hex, mid)
        } else {
            darken(hex, mid)
        };
        if (luminance(&candidate) < target) == (luma < floor) {
            lo = mid;
        } else {
            hi = mid;
        }
    }
    if luma < floor {
        lighten(hex, hi)
    } else {
        darken(hex, hi)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn emitted_tints_bound_dim_and_pale_colors_without_reversing_hue() {
        for color in [
            "#000000", "#ffffff", "#156878", "#181818", "#f5f4ee", "#0316ff", "#ff0000", "#00ff00",
            "#808080",
        ] {
            for (light, lo, hi) in [(false, 0.28, 0.72), (true, 0.22, 0.50)] {
                let tint = glow_tint(color, light);
                let luma = luminance(&tint);
                assert!(
                    (lo - 0.006..=hi + 0.006).contains(&luma),
                    "{color}: {tint}, {luma}"
                );
                let original = parse_hex(color).unwrap();
                let toned = parse_hex(&tint).unwrap();
                for i in 0..3 {
                    for j in 0..3 {
                        if original[i] >= original[j] {
                            assert!(toned[i] >= toned[j], "{color} hue reversed");
                        }
                    }
                }
            }
        }
    }

    #[test]
    fn blue_haze_tuning_preserves_reference_colors() {
        assert_eq!(blue_haze_weight("#001aff"), 1.0);
        for color in ["#000010", "#000000", "#ff0700", "#00cccc", "#ffd700"] {
            assert_eq!(blue_haze_weight(color), 0.0, "{color}");
        }
        assert!((0.0..1.0).contains(&blue_haze_weight("#7aa2f7")));
    }

    #[test]
    fn hex_roundtrip() {
        assert_eq!(parse_hex("#7aa2f7"), Some([0x7a, 0xa2, 0xf7]));
        assert_eq!(parse_hex("nope"), None);
        assert!(is_valid_hex("#ff7a00"));
        assert!(!is_valid_hex("#xyz"));
    }

    #[test]
    fn lighten_darken_match_python() {
        // Values verified against the Python implementation.
        assert_eq!(lighten("#000000", 0.5), "#808080");
        assert_eq!(darken("#ffffff", 0.5), "#808080");
        assert_eq!(lighten("#ff0000", 0.45), "#ff7373");
    }

    #[test]
    fn luminance_ordering() {
        assert!(luminance("#000000") < 0.01);
        assert!(luminance("#ffffff") > 0.9);
        // Green reads brighter than pure blue, matching the comment in build.py.
        assert!(luminance("#00ff00") > luminance("#0000ff"));
    }

    #[test]
    fn dark_glow_gets_lift_bright_does_not() {
        assert!(glow_lift("#5a1a00") > 1.2);
        assert_eq!(glow_lift("#7CFC00"), 1.0);
    }

    #[test]
    fn beam_color_lifts_weak_hues_but_preserves_bright_teal() {
        assert_eq!(beam_color("#00e5ff"), "#00e5ff");
        assert_ne!(beam_color("#ff0000"), "#ff0000");
        assert_eq!(beam_color("#001066"), lighten("#001066", 0.36));
        assert_eq!(beam_width_scale("#00e5ff"), 1.0);
        assert!(beam_width_scale("#ff0700") > 1.1);
        assert_eq!(beam_width_scale("#001aff"), 1.45);
        assert!(luminance(&blade_core_color("#000000")) >= 0.51);
        assert_eq!(blade_core_color("#00e5ff"), "#00e5ff");
    }

    #[test]
    fn vivid_boost_targets_red_blue_and_preserves_black_teal() {
        assert!(vivid_beam_boost("#ff0700") > 0.4);
        assert!(vivid_beam_boost("#0009f5") > 0.7);
        assert_eq!(vivid_beam_boost("#000010"), 0.0);
        assert_eq!(vivid_beam_boost("#000000"), 0.0);
        assert_eq!(vivid_beam_boost("#00e5ff"), 0.0);
    }
}
