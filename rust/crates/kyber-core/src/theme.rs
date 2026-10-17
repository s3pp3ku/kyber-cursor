//! Theme build and install. Port of `build.py::_role_blob/_write_theme_files`
//! plus the caching/atomic-install logic of `bin/kyber::cmd_apply/install_theme`.
//!
//! Rendering is pure Rust; `apply`/`install_theme` write Xcursor themes under
//! `~/.local/share/icons` on Linux.

use std::path::Path;
#[cfg(target_os = "linux")]
use std::path::PathBuf;

use crate::geometry::{self, SIZES, THEME};
use crate::overlay::OverlayMeta;
use crate::render::{rasterize_role, render_overlay_layers};
use crate::xcursor::{self, Frame};

/// Build recipe marker. Bumped automatically with the crate version, so a
/// new kyber-core always rebuilds instead of reusing stale art.
pub const RECIPE: &str = concat!(env!("CARGO_PKG_VERSION"), "-compact-shimmer-1");

/// Xcursor alias names per role. Must match `build.py::ALIASES`.
pub fn aliases(role: &str) -> &'static [&'static str] {
    match role {
        "default" => &["left_ptr", "default", "top_left_arrow", "arrow"],
        "pointer" => &["hand2", "pointer", "link", "hand1"],
        "help" => &["question_arrow", "help", "whats_this"],
        "progress" => &["left_ptr_watch", "progress", "half-busy"],
        "not-allowed" => &["crossed_circle", "not-allowed", "no-drop", "forbidden"],
        "copy" => &["copy", "dnd-copy"],
        "alias" => &["alias", "dnd-link"],
        "context" => &["context-menu"],
        "zoom-in" => &["zoom-in"],
        "zoom-out" => &["zoom-out"],
        "wait" => &["watch", "wait"],
        "text" => &["xterm", "text", "ibeam", "vertical-text"],
        "crosshair" => &["crosshair", "cross", "tcross"],
        "cell" => &["cell", "plus"],
        "move" => &["fleur", "move", "all-scroll", "size_all", "dnd-move"],
        "ns-resize" => &[
            "sb_v_double_arrow",
            "ns-resize",
            "size_ver",
            "n-resize",
            "s-resize",
            "top_side",
            "bottom_side",
            "row-resize",
            "v_double_arrow",
        ],
        "ew-resize" => &[
            "sb_h_double_arrow",
            "ew-resize",
            "size_hor",
            "e-resize",
            "w-resize",
            "left_side",
            "right_side",
            "col-resize",
            "h_double_arrow",
        ],
        "nwse-resize" => &[
            "bd_double_arrow",
            "nwse-resize",
            "size_fdiag",
            "nw-resize",
            "se-resize",
            "top_left_corner",
            "bottom_right_corner",
        ],
        "nesw-resize" => &[
            "fd_double_arrow",
            "nesw-resize",
            "size_bdiag",
            "ne-resize",
            "sw-resize",
            "top_right_corner",
            "bottom_left_corner",
        ],
        "grab" => &["openhand", "grab"],
        "grabbing" => &["closedhand", "grabbing", "dnd-none"],
        _ => &["left_ptr"],
    }
}

/// Python `round()` (banker's rounding: halves go to even), for
/// hotspot parity with `build.py`. Rust's `f64::round` goes half away
/// from zero, which differs by 1px on centered hotspots at some sizes.
fn py_round(v: f64) -> u32 {
    let f = v.floor();
    let d = v - f;
    if d < 0.5 {
        f as u32
    } else if d > 0.5 {
        (f + 1.0) as u32
    } else {
        let i = f as u32;
        if i % 2 == 0 {
            i
        } else {
            i + 1
        }
    }
}

fn role_blob(role: &str, accent: &str, glow: &str, lit: bool) -> Vec<u8> {
    let kind = geometry::role_kind(role);
    let hot = geometry::hotspot(role);
    let mut frames = Vec::new();
    for size in SIZES {
        let px = (size as f64 * geometry::SCALE).round() as u32;
        let rgba = rasterize_role(kind, accent, glow, lit, px);
        let hx = py_round((hot.0 - geometry::VIEW[0]) / geometry::VIEW[2] * px as f64);
        let hy = py_round((hot.1 - geometry::VIEW[1]) / geometry::VIEW[3] * px as f64);
        frames.push(Frame {
            nominal: size,
            hot_x: hx,
            hot_y: hy,
            width: px,
            height: px,
            rgba,
        });
    }
    xcursor::encode(&frames)
}

/// Build both themes (`Omarchy-Kyber` + `Omarchy-Kyber-Ignited`) into
/// `out/`. Roles render in parallel on all cores. Also writes the overlay
/// PNGs + `overlay.json` into the Ignited theme and a `preview.png`
/// contact sheet next to it (handy for eyeballing diffs).
pub fn build_all(accent: &str, glow: &str, out: &Path) -> std::io::Result<()> {
    let accent = accent.to_lowercase();
    std::thread::scope(|s| {
        let mut handles = Vec::new();
        for lit in [false, true] {
            for role in geometry::ROLES {
                // `lit`/`role` are per-iteration loop variables: move copies
                // into the thread, clone the accent string (cheap, 42x).
                let accent = accent.clone();
                handles.push(s.spawn(move || (lit, role, role_blob(role, &accent, glow, lit))));
            }
        }
        let mut idle: Vec<(&str, Vec<u8>)> = Vec::new();
        let mut ignited: Vec<(&str, Vec<u8>)> = Vec::new();
        for h in handles {
            let (lit, role, blob) = h.join().expect("role thread");
            if lit {
                ignited.push((role, blob));
            } else {
                idle.push((role, blob));
            }
        }
        write_theme(THEME, "Omarchy Kyber", idle, out)?;
        write_theme(
            &format!("{THEME}-Ignited"),
            "Omarchy Kyber Ignited",
            ignited,
            out,
        )?;
        Ok::<(), std::io::Error>(())
    })?;
    // Overlay art for the ignite daemon.
    let ign_dir = out.join(format!("{THEME}-Ignited"));
    for (name, png) in render_overlay_layers(glow) {
        std::fs::write(ign_dir.join(name), png)?;
    }
    std::fs::write(
        ign_dir.join("overlay.json"),
        OverlayMeta::new(glow).to_json() + "\n",
    )?;
    Ok(())
}

fn write_theme(
    name: &str,
    label: &str,
    blobs: Vec<(&str, Vec<u8>)>,
    out: &Path,
) -> std::io::Result<()> {
    let theme = out.join(name);
    let cursors = theme.join("cursors");
    std::fs::create_dir_all(&cursors)?;
    std::fs::write(
        theme.join("index.theme"),
        format!("[Icon Theme]\nName={label}\nComment=Omarchy accent-adaptive Kyber cursor theme\nInherits=Adwaita\n"),
    )?;
    for (role, blob) in blobs {
        let names = aliases(role);
        std::fs::write(cursors.join(names[0]), blob)?;
        #[cfg(target_os = "linux")]
        for alias in &names[1..] {
            let dest = cursors.join(alias);
            let _ = std::fs::remove_file(&dest);
            std::os::unix::fs::symlink(names[0], &dest)?;
        }
        #[cfg(not(unix))]
        for alias in &names[1..] {
            let dest = cursors.join(alias);
            let _ = std::fs::remove_file(&dest);
            std::fs::hard_link(cursors.join(names[0]), &dest)?;
        }
    }
    Ok(())
}

// --- install (Unix/Linux only) ------------------------------------------

/// Recursive copy that preserves symlinks (the `cursors/` aliases).
#[cfg(target_os = "linux")]
fn copy_tree(src: &Path, dst: &Path) -> std::io::Result<()> {
    std::fs::create_dir_all(dst)?;
    for entry in std::fs::read_dir(src)? {
        let entry = entry?;
        let (from, to) = (entry.path(), dst.join(entry.file_name()));
        let ft = entry.file_type()?;
        if ft.is_symlink() {
            let target = std::fs::read_link(&from)?;
            std::os::unix::fs::symlink(target, to)?;
        } else if ft.is_dir() {
            copy_tree(&from, &to)?;
        } else {
            std::fs::copy(&from, &to)?;
        }
    }
    Ok(())
}

/// Install one built theme dir under `$ICONS` atomically (copy to a
/// throwaway dir, then repoint a symlink with one rename), stamping the
/// accent + recipe for the build cache. Port of `bin/kyber::install_theme`.
#[cfg(target_os = "linux")]
fn install_theme(
    name: &str,
    src: &Path,
    accent: &str,
    icons: &Path,
    builds: &Path,
) -> std::io::Result<()> {
    let stamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    let target = builds.join(format!("{name}.{}.{stamp}", std::process::id()));
    copy_tree(src, &target)?;
    std::fs::write(target.join(".accent"), accent.to_lowercase())?;
    std::fs::write(target.join(".recipe"), RECIPE)?;
    let link_path = icons.join(name);
    if std::fs::symlink_metadata(&link_path)
        .map(|m| !m.file_type().is_symlink())
        .unwrap_or(false)
    {
        let prev = builds.join(format!("{name}.previous"));
        let _ = std::fs::remove_dir_all(&prev);
        std::fs::rename(&link_path, &prev)?;
    }
    let tmp_link = icons.join(format!(".{name}.link.{}", std::process::id()));
    let _ = std::fs::remove_file(&tmp_link);
    std::os::unix::fs::symlink(&target, &tmp_link)?;
    std::fs::rename(&tmp_link, &link_path)?;
    // Prune older builds of this theme.
    if let Ok(rd) = std::fs::read_dir(builds) {
        for entry in rd.flatten() {
            let p = entry.path();
            if p.is_dir()
                && p.file_name()
                    .and_then(|n| n.to_str())
                    .map(|n| n.starts_with(&format!("{name}.")))
                    .unwrap_or(false)
                && p != target
            {
                let _ = std::fs::remove_dir_all(&p);
            }
        }
    }
    Ok(())
}

/// Is `dir` a cached build of `accent` with the current recipe?
#[cfg(target_os = "linux")]
fn cache_hit(dir: &Path, accent: &str) -> bool {
    let acc = std::fs::read_to_string(dir.join(".accent")).unwrap_or_default();
    let rec = std::fs::read_to_string(dir.join(".recipe")).unwrap_or_default();
    acc.trim() == accent.to_lowercase() && rec.trim() == RECIPE
}

/// Full apply on Linux: reuse a cached build when the accent and recipe
/// match, otherwise build to a temp dir and install atomically. `glow`
/// defaults to the accent (no fixed `IGNITE_COLOR` yet — `build.py` parity).
#[cfg(target_os = "linux")]
pub fn apply(accent: &str, icons: &Path) -> std::io::Result<PathBuf> {
    let builds = icons.join(".kyber-builds");
    std::fs::create_dir_all(&builds)?;
    std::fs::create_dir_all(icons)?;
    if let Ok(rd) = std::fs::read_dir(&builds) {
        for entry in rd.flatten() {
            let p = entry.path();
            let name = p.file_name().and_then(|n| n.to_str()).unwrap_or("");
            if name.starts_with(&format!("{THEME}."))
                && cache_hit(&p, accent)
                && icons.join(THEME).is_symlink()
            {
                return Ok(p);
            }
        }
    }
    let tmp = std::env::temp_dir().join(format!("kyber-build-{}-{}", std::process::id(), {
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0)
    }));
    build_all(accent, accent, &tmp)?;
    for name in [THEME.to_string(), format!("{THEME}-Ignited")] {
        install_theme(&name, &tmp.join(&name), accent, icons, &builds)?;
    }
    let _ = std::fs::remove_dir_all(&tmp);
    Ok(builds)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn py_round_matches_python() {
        // round-half-even: 38.5 -> 38, 39.5 -> 40, 25.5 -> 26.
        assert_eq!(py_round(38.5), 38);
        assert_eq!(py_round(39.5), 40);
        assert_eq!(py_round(25.5), 26);
        assert_eq!(py_round(19.0), 19);
        assert_eq!(py_round(19.4), 19);
        assert_eq!(py_round(19.6), 20);
    }

    #[test]
    fn aliases_cover_every_role() {
        for role in geometry::ROLES {
            assert!(!aliases(role).is_empty(), "{role}");
        }
    }

    // Full build (42 roles × 4 sizes + 8 overlay layers with wide blurs):
    // minutes in debug, ~a minute in release. Run explicitly:
    // `cargo test --release -- --ignored build_all_writes_two_themes_plus_overlay`.
    #[test]
    #[ignore]
    fn build_all_writes_two_themes_plus_overlay() {
        let dir = std::env::temp_dir().join(format!("kyber-theme-test-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        build_all("#7aa2f7", "#7aa2f7", &dir).expect("build");
        for theme in [THEME, &format!("{THEME}-Ignited")] {
            let cursors = dir.join(theme).join("cursors");
            assert!(dir.join(theme).join("index.theme").is_file(), "{theme}");
            // First alias of every role exists and is a valid Xcursor file.
            for role in geometry::ROLES {
                let p = cursors.join(aliases(role)[0]);
                let bytes = std::fs::read(&p).unwrap();
                assert_eq!(&bytes[0..4], b"Xcur", "{role}");
            }
        }
        let meta =
            std::fs::read_to_string(dir.join(format!("{THEME}-Ignited")).join("overlay.json"))
                .unwrap();
        let v: serde_json::Value = serde_json::from_str(&meta).unwrap();
        assert_eq!(v["sizes"], serde_json::json!([24, 32, 48, 64]));
        let _ = std::fs::remove_dir_all(&dir);
    }
}
