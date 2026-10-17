//! Cursor position and monitors via Hyprland IPC. Port of the `hypr` /
//! `cursor_pos` / `Overlay::place` helpers in `lib/ignite-listener`.
//!
//! This is the Omarchy path. Other compositors (Sway, GNOME, KDE, X11)
//! get their own module in step 4 behind these same structs.

use std::io::{Read, Write};
use std::os::unix::net::UnixStream;
use std::time::Duration;

use kyber_core::geometry;

#[derive(Clone)]
pub struct Monitor {
    pub name: String,
    pub x: f64,
    pub y: f64,
    /// Logical size (physical / scale), like the Python.
    pub w: f64,
    pub h: f64,
    pub scale: f64,
}

fn socket_path() -> std::path::PathBuf {
    let runtime =
        std::env::var("XDG_RUNTIME_DIR").unwrap_or_else(|_| format!("/run/user/{}", nix_uid()));
    let sig = std::env::var("HYPRLAND_INSTANCE_SIGNATURE").unwrap_or_default();
    std::path::Path::new(&runtime)
        .join("hypr")
        .join(sig)
        .join(".socket.sock")
}

fn nix_uid() -> u32 {
    // No libc dep for one number: parse `id -u` is overkill; the env var is
    // always set inside a systemd user session. Fall back to 1000.
    1000
}

fn hypr(request: &str) -> std::io::Result<String> {
    let mut sock = UnixStream::connect(socket_path())?;
    sock.set_read_timeout(Some(Duration::from_millis(500)))?;
    sock.write_all(request.as_bytes())?;
    let mut out = Vec::new();
    sock.read_to_end(&mut out)?;
    String::from_utf8(out).map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))
}

pub fn cursor_pos() -> std::io::Result<(f64, f64)> {
    let reply = hypr("cursorpos")?;
    let (x, y) = reply
        .trim()
        .split_once(',')
        .ok_or_else(|| std::io::Error::new(std::io::ErrorKind::InvalidData, "cursorpos"))?;
    Ok((
        x.trim()
            .parse()
            .map_err(|_| std::io::Error::new(std::io::ErrorKind::InvalidData, "x"))?,
        y.trim()
            .parse()
            .map_err(|_| std::io::Error::new(std::io::ErrorKind::InvalidData, "y"))?,
    ))
}

pub fn monitors() -> std::io::Result<Vec<Monitor>> {
    let reply = hypr("j/monitors")?;
    let v: serde_json::Value = serde_json::from_str(&reply)
        .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?;
    let mut out = Vec::new();
    if let Some(list) = v.as_array() {
        for m in list {
            out.push(Monitor {
                name: m
                    .get("name")
                    .and_then(|n| n.as_str())
                    .unwrap_or("")
                    .to_string(),
                x: m.get("x").and_then(|n| n.as_f64()).unwrap_or(0.0),
                y: m.get("y").and_then(|n| n.as_f64()).unwrap_or(0.0),
                w: m.get("width").and_then(|n| n.as_f64()).unwrap_or(0.0)
                    / m.get("scale").and_then(|n| n.as_f64()).unwrap_or(1.0),
                h: m.get("height").and_then(|n| n.as_f64()).unwrap_or(0.0)
                    / m.get("scale").and_then(|n| n.as_f64()).unwrap_or(1.0),
                scale: m.get("scale").and_then(|n| n.as_f64()).unwrap_or(1.0),
            });
        }
    }
    Ok(out)
}

pub fn monitor_at(monitors: &[Monitor], x: f64, y: f64) -> Option<usize> {
    monitors
        .iter()
        .position(|m| m.x <= x && x < m.x + m.w && m.y <= y && y < m.y + m.h)
}

pub fn cursor_size() -> u32 {
    for var in ["HYPRCURSOR_SIZE", "XCURSOR_SIZE"] {
        if let Ok(v) = std::env::var(var) {
            if let Ok(n) = v.parse::<u32>() {
                return n;
            }
        }
    }
    24
}

/// Logical px per overlay viewBox unit, as the compositor shows the cursor.
/// Port of `ignite-listener::displayed_units_to_logical`: Hyprland loads
/// Xcursors at size × ceil(scale) and shows that bitmap at buffer scale
/// ceil(scale); the theme renders each nominal size at nominal × SCALE px
/// over `view_w` units. `SIZE_FUDGE` (`CURSOR_OVERLAY_SCALE`) fine-tunes.
pub fn units_to_logical(sizes: &[u32], monitor_scale: f64) -> f64 {
    let load_scale = monitor_scale.ceil() as u32;
    let want = cursor_size() * load_scale.max(1);
    let nominal = sizes
        .iter()
        .min_by_key(|s| s.abs_diff(want))
        .copied()
        .unwrap_or(24);
    let bitmap_px = (nominal as f64 * geometry::SCALE).round();
    let fudge: f64 = std::env::var("CURSOR_OVERLAY_SCALE")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(1.0);
    bitmap_px / load_scale.max(1) as f64 / geometry::VIEW[2] * fudge
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn logical_units_match_python() {
        // 24px cursor, 1.0 scale: 38.4→38px bitmap over 48 units.
        assert!((units_to_logical(&[24, 32, 48, 64], 1.0) - 38.0 / 48.0).abs() < 1e-9);
        // 1.6 scale (user's machine): ceil→2, want 48, bitmap 77px, /2.
        let f = units_to_logical(&[24, 32, 48, 64], 1.6);
        assert!((f - 77.0 / 2.0 / 48.0).abs() < 1e-9, "{f}");
    }
}
