//! C bridge ownership: one handle on the compositor thread; rendered bytes stay
//! valid until the next render or destroy. All animation/raster work is Rust.
use kyber_core::trail::{Bounds, Trail, TrailImage, TrailLook};
use std::ffi::{c_char, CStr};

#[derive(Default)]
pub struct Handle {
    trail: Trail,
    image: Option<TrailImage>,
}

#[derive(Default)]
#[repr(C)]
pub struct Raster {
    pub data: *const u8,
    pub width: u32,
    pub height: u32,
    pub bounds: Bounds,
}

#[no_mangle]
pub extern "C" fn kyber_trail_create() -> *mut Handle {
    Box::into_raw(Box::new(Handle::default()))
}

/// # Safety
/// `handle` must be null or the unique live handle returned by create, used only
/// on its owning thread. It must not be used again after this call.
#[no_mangle]
pub unsafe extern "C" fn kyber_trail_destroy(handle: *mut Handle) {
    if !handle.is_null() {
        drop(Box::from_raw(handle));
    }
}

/// # Safety
/// `handle` must be null or a live handle on its owning thread.
#[no_mangle]
pub unsafe extern "C" fn kyber_trail_clear(handle: *mut Handle) {
    if let Some(h) = handle.as_mut() {
        h.trail.clear();
    }
}

/// Start a new path; sparks already in the air keep flying.
///
/// # Safety
/// `handle` must be null or a live handle on its owning thread.
#[no_mangle]
pub unsafe extern "C" fn kyber_trail_break(handle: *mut Handle) {
    if let Some(h) = handle.as_mut() {
        h.trail.break_path();
    }
}

/// # Safety
/// `handle` must be null or a live handle on its owning thread. `color` must
/// point to a valid nul-terminated UTF-8 string for the duration of the call.
#[no_mangle]
pub unsafe extern "C" fn kyber_trail_color(handle: *mut Handle, color: *const c_char) -> bool {
    if color.is_null() {
        return false;
    }
    match (handle.as_mut(), CStr::from_ptr(color).to_str()) {
        (Some(h), Ok(color)) => h.trail.set_color(color),
        _ => false,
    }
}

/// # Safety
/// `handle` must be null or a live handle on its owning thread.
#[no_mangle]
pub unsafe extern "C" fn kyber_trail_sample(handle: *mut Handle, now: f64, x: f64, y: f64) {
    if let Some(h) = handle.as_mut() {
        h.trail.sample(now, x, y);
    }
}

/// # Safety
/// `handle` must be null or a live handle on its owning thread.
#[no_mangle]
pub unsafe extern "C" fn kyber_trail_bounds(handle: *mut Handle, now: f64) -> Bounds {
    handle
        .as_mut()
        .and_then(|h| h.trail.bounds(now))
        .unwrap_or_default()
}

/// Re-read the trail look (style, sparks, spark life and spacing) from the
/// Kyber settings folder. Cheap; hosts may call it on their settings poll.
/// Returns true when the look changed.
///
/// # Safety
/// `handle` must be null or a live handle on its owning thread.
#[no_mangle]
pub unsafe extern "C" fn kyber_trail_reload(handle: *mut Handle) -> bool {
    let (Some(h), Ok(dir)) = (handle.as_mut(), kyber_core::paths::settings_dir()) else {
        return false;
    };
    let look = TrailLook::load(&dir);
    if look == h.trail.look() {
        return false;
    }
    h.trail.set_look(look);
    true
}

/// # Safety
/// `handle` must be null or a live handle on its owning thread.
#[no_mangle]
pub unsafe extern "C" fn kyber_trail_particles(handle: *const Handle) -> usize {
    handle.as_ref().map_or(0, |h| h.trail.particle_count())
}

/// # Safety
/// `handle` must be null or a live handle on its owning thread.
#[no_mangle]
pub unsafe extern "C" fn kyber_trail_points(handle: *const Handle) -> usize {
    handle.as_ref().map_or(0, |h| h.trail.point_count())
}

/// # Safety
/// `handle` must be null or a live handle on its owning thread. The returned
/// buffer is borrowed until the next render/destroy; copy/upload it immediately.
#[no_mangle]
pub unsafe extern "C" fn kyber_trail_render(
    handle: *mut Handle,
    now: f64,
    view: Bounds,
    light: bool,
    scale: f64,
) -> Raster {
    let Some(h) = handle.as_mut() else {
        return Raster::default();
    };
    // `scale` is the monitor's pixels per logical px, so the trail is drawn
    // sharp on HiDPI screens instead of being stretched up.
    h.image = h.trail.render(now, view, light, scale);
    h.image
        .as_ref()
        .map_or_else(Raster::default, |image| Raster {
            data: image.pixels.data().as_ptr(),
            width: image.pixels.width(),
            height: image.pixels.height(),
            bounds: image.bounds,
        })
}
