//! Aim runtime for the native overlay: the heading, the baked cursor body,
//! and the compositor pointer it borrows while the mode is on.
//!
//! The pointer is the one thing here that can hurt the user: if this process
//! hides it and then dies without restoring it, the session has no cursor.
//! Everything below therefore hides it only after the overlay is already up,
//! and [`PointerGuard`] restores it on every way out this process can take.

use std::sync::OnceLock;
use std::time::{Duration, Instant};

use kyber_core::aim::{self, Aim, AimSettings};
use kyber_core::config::{theme_accent, KyberConfig};
use kyber_core::render::AimCursor;

use crate::cursor;

/// Owns the heading and the baked body for the daemon's aim mode.
pub struct AimRuntime {
    pub settings: AimSettings,
    heading: f64,
    mirror_flip: bool,
    state: Aim,
    cursor: Option<AimCursor>,
    /// (accent, baked pixels) the cached body was made for.
    key: Option<(String, u32)>,
    /// The compositor's pointer is hidden and we are the cursor.
    pub armed: bool,
    warned: bool,
    /// The color the body was last resolved for, and when it was read.
    accent: String,
    accent_checked: Instant,
    /// When the compositor was last asked whether the pointer is still hidden.
    hide_checked: Instant,
}

impl AimRuntime {
    pub fn new() -> Self {
        let settings = kyber_core::paths::settings_dir()
            .map(|dir| AimSettings::load(&dir))
            .unwrap_or_default();
        Self {
            settings,
            heading: 0.0,
            mirror_flip: false,
            state: Aim::new(),
            cursor: None,
            key: None,
            armed: false,
            warned: false,
            accent: String::new(),
            // Force a read on the first frame.
            accent_checked: Instant::now() - Duration::from_secs(2),
            hide_checked: Instant::now(),
        }
    }

    /// Re-read the two switches. Two small files, safe to call a few times a
    /// second; returns whether anything changed.
    pub fn reload(&mut self) -> bool {
        let Ok(dir) = kyber_core::paths::settings_dir() else {
            return false;
        };
        let next = AimSettings::load(&dir);
        let changed = next != self.settings;
        self.settings = next;
        changed
    }

    /// Feed the pointer; returns this frame's heading in degrees.
    pub fn tick(&mut self, pos: (f64, f64), now: f64) -> f64 {
        let travel = self.state.update(pos.0, pos.1, now);
        // Turn off: hold the stock pose. The tracker still runs, so turning
        // back on starts from the real direction of travel.
        self.heading = if self.settings.turn { travel } else { aim::blade_heading() };
        self.mirror_flip = aim::wants_mirror(self.heading);
        self.heading
    }

    pub fn heading(&self) -> f64 {
        self.heading
    }

    /// Mirror the artwork this frame: the switch must be on *and* this
    /// heading must be one where the unmirrored lit edge faces away.
    pub fn mirror(&self) -> bool {
        self.settings.mirror && self.mirror_flip
    }

    pub fn cursor(&self) -> Option<&AimCursor> {
        self.cursor.as_ref()
    }

    /// Bake (or reuse) the cursor body at `px` device pixels in the color the
    /// theme currently resolves to. Rebuilds only when either changes.
    pub fn ensure_cursor(&mut self, px: u32) -> bool {
        let accent = self.accent_now();
        if self.key.as_ref().is_some_and(|(a, p)| *a == accent && *p == px) {
            return self.cursor.is_some();
        }
        match AimCursor::build(&accent, px) {
            Ok(cursor) => {
                self.cursor = Some(cursor);
                self.key = Some((accent, px));
                true
            }
            Err(error) => {
                eprintln!("kyber-ignite: aim cannot bake the cursor: {error}");
                self.cursor = None;
                self.key = None;
                false
            }
        }
    }

    /// The theme's cursor color, read once a second rather than once a frame.
    fn accent_now(&mut self) -> String {
        if self.accent.is_empty() || self.accent_checked.elapsed() >= Duration::from_secs(1) {
            self.accent_checked = Instant::now();
            self.accent = current_accent();
        }
        self.accent.clone()
    }

    /// Hide the compositor's pointer, once per arm. Only call this after the
    /// replacement is actually on screen.
    pub fn hide_pointer(&mut self) -> bool {
        if self.armed {
            return true;
        }
        if cursor::set_invisible(true) {
            self.armed = true;
            // Everything the pointer did while aim was off is ancient
            // history: start the heading fresh from where it is now, instead
            // of turning on a stale jump the moment aim comes back.
            self.state = Aim::new();
            self.heading = 0.0;
            self.mirror_flip = false;
            true
        } else {
            if !self.warned {
                eprintln!(
                    "kyber-ignite: hyprctl could not hide the pointer; aim stays off \
                     rather than leaving the desktop without a cursor"
                );
                self.warned = true;
            }
            false
        }
    }

    /// Hide the pointer again if something else gave it back. Other tools
    /// own this same switch: the screensavers hide it on start and show it on
    /// exit, which would leave the compositor's pointer drawn next to ours.
    /// Asks at most once a second.
    pub fn keep_pointer_hidden(&mut self) {
        if !self.armed || self.hide_checked.elapsed() < Duration::from_secs(1) {
            return;
        }
        self.hide_checked = Instant::now();
        if cursor::pointer_hidden() == Some(false) {
            eprintln!("kyber-ignite: the pointer was un-hidden by something else; hiding it again");
            cursor::set_invisible(true);
        }
    }

    /// Give the pointer back. Safe to call when it was never taken.
    pub fn release_pointer(&mut self) {
        if !self.armed {
            return;
        }
        if cursor::set_invisible(false) {
            self.armed = false;
        } else {
            eprintln!(
                "kyber-ignite: could not restore the pointer; run \
                 `hyprctl eval 'hl.config({{ cursor = {{ invisible = false }} }})'`"
            );
        }
    }
}

impl Default for AimRuntime {
    fn default() -> Self {
        Self::new()
    }
}

/// The cursor color the theme was built with (custom/random/theme accent).
fn current_accent() -> String {
    let accent = theme_accent();
    match kyber_core::paths::settings_dir() {
        Ok(dir) => KyberConfig::load(&dir).effective_color(accent.as_deref()),
        Err(_) => accent.unwrap_or_else(|| "#7aa2f7".to_string()),
    }
}

/// Restores the compositor's pointer on the way out: normal return, a panic,
/// SIGTERM or SIGINT.
///
/// SIGKILL, a power loss or a debugger killing the process cannot be caught:
/// the pointer then stays hidden until `cursor:invisible` is reset (restart
/// Hyprland, or run the one-line hyprctl eval printed by `kyber doctor`).
pub struct PointerGuard;

impl PointerGuard {
    pub fn install() -> Self {
        spawn_signal_restorer();
        Self
    }
}

impl Drop for PointerGuard {
    fn drop(&mut self) {
        // Restore unconditionally: `kyber-ignite` may be the only thing that
        // ever hid the pointer in this session.
        cursor::set_invisible(false);
    }
}

/// A dedicated thread that waits for SIGTERM/SIGINT and restores the pointer
/// before exiting. Signals are delivered to the *thread* that has them
/// unblocked, so blocking them process-wide first is what makes `sigwait`
/// reliable here.
fn spawn_signal_restorer() {
    static INSTALLED: OnceLock<()> = OnceLock::new();
    if INSTALLED.set(()).is_err() {
        return;
    }
    let mut set: libc::sigset_t = unsafe { std::mem::zeroed() };
    // SAFETY: `set` is a valid, zero-initialized sigset_t for these calls.
    unsafe {
        libc::sigemptyset(&mut set);
        libc::sigaddset(&mut set, libc::SIGTERM);
        libc::sigaddset(&mut set, libc::SIGINT);
        libc::pthread_sigmask(libc::SIG_BLOCK, &set, std::ptr::null_mut());
    }
    std::thread::spawn(move || {
        let mut signal: libc::c_int = 0;
        // SAFETY: `set` outlives this thread and `signal` is a valid out ptr.
        let rc = unsafe { libc::sigwait(&set, &mut signal) };
        if rc == 0 {
            eprintln!("kyber-ignite: signal {signal}; restoring the pointer");
        }
        cursor::set_invisible(false);
        std::process::exit(0);
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn settings_default_to_aim_off_with_mirroring_on() {
        let dir = std::env::temp_dir().join(format!("kyber-aim-settings-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let defaults = AimSettings::load(&dir);
        assert!(!defaults.enabled);
        assert!(defaults.mirror);
        std::fs::write(dir.join(aim::STATE_FILE), "on\n").unwrap();
        std::fs::write(dir.join(aim::MIRROR_FILE), "off\n").unwrap();
        let set = AimSettings::load(&dir);
        assert!(set.enabled && !set.mirror);
        assert!(set.turn, "turning is on unless switched off");
        std::fs::write(dir.join(aim::TURN_FILE), "off\n").unwrap();
        assert!(!AimSettings::load(&dir).turn);
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn mirroring_follows_the_heading_and_the_switch() {
        let mut runtime = AimRuntime::new();
        runtime.settings = AimSettings { enabled: true, mirror: true, turn: true };
        // The theme's own heading: nothing to mirror.
        runtime.heading = aim::blade_heading();
        runtime.mirror_flip = aim::wants_mirror(runtime.heading);
        assert!(!runtime.mirror());
        // Travelling right, the unmirrored lit edge would face away: mirror.
        runtime.heading = 0.0;
        runtime.mirror_flip = aim::wants_mirror(0.0);
        assert!(runtime.mirror());
        // ...unless the switch is off.
        runtime.settings.mirror = false;
        assert!(!runtime.mirror());
    }

    #[test]
    fn turn_off_holds_the_stock_pose() {
        let mut runtime = AimRuntime::new();
        runtime.settings = AimSettings { enabled: true, mirror: true, turn: false };
        // Sweep right, then down: a turning blade would follow both.
        for i in 0..60 {
            let h = runtime.tick((i as f64 * 20.0, 0.0), i as f64 / 60.0);
            assert_eq!(h, aim::blade_heading());
            assert!(!runtime.mirror());
        }
        for i in 0..60 {
            let h = runtime.tick((1200.0, i as f64 * 20.0), 1.0 + i as f64 / 60.0);
            assert_eq!(h, aim::blade_heading());
        }
    }
}
