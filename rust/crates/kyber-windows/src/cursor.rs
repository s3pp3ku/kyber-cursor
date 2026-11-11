//! Owned GDI cursors. SetSystemCursor only receives disposable copies.
use crate::failed;
use image::RgbaImage;
use kyber_core::{geometry, render};
use kyber_preview::{CursorPixels, Layout};
use std::{
    ffi::c_void,
    mem::{size_of, zeroed},
    path::Path,
    ptr::null_mut,
};
use windows_sys::Win32::{Graphics::Gdi::*, UI::WindowsAndMessaging::*};
struct Bitmap(HBITMAP);
impl Drop for Bitmap {
    fn drop(&mut self) {
        unsafe {
            DeleteObject(self.0);
        }
    }
}

pub struct Cursor(pub HCURSOR);
impl Drop for Cursor {
    fn drop(&mut self) {
        unsafe {
            DestroyCursor(self.0);
        }
    }
}

#[repr(C)]
struct MaskInfo {
    header: BITMAPINFOHEADER,
    colors: [RGBQUAD; 2],
}

impl Cursor {
    pub fn copy(&self) -> Result<Self, String> {
        let handle = unsafe { CopyImage(self.0, IMAGE_CURSOR, 0, 0, 0) };
        if handle.is_null() {
            Err(failed("Copy cursor"))
        } else {
            Ok(Self(handle))
        }
    }

    pub fn install(&self, id: u32) -> Result<(), String> {
        let copy = self.copy()?;
        let handle = copy.0;
        // Windows consumes this copy, never our reusable cached frame.
        std::mem::forget(copy);
        if unsafe { SetSystemCursor(handle, id) } == 0 {
            Err(failed("Set Windows cursor"))
        } else {
            Ok(())
        }
    }

    pub fn new(pixels: &CursorPixels) -> Result<Self, String> {
        // The GDI color bitmap is top-down premultiplied BGRA, with explicit
        // alpha masks. The separate monochrome AND mask is transparent at A=0.
        unsafe {
            let header = BITMAPV5HEADER {
                bV5Size: size_of::<BITMAPV5HEADER>() as u32,
                bV5Width: pixels.size[0] as i32,
                bV5Height: -(pixels.size[1] as i32),
                bV5Planes: 1,
                bV5BitCount: 32,
                bV5Compression: BI_BITFIELDS,
                bV5RedMask: 0x00ff0000,
                bV5GreenMask: 0x0000ff00,
                bV5BlueMask: 0x000000ff,
                bV5AlphaMask: 0xff000000,
                ..zeroed()
            };
            let mut bits: *mut c_void = null_mut();
            let color = CreateDIBSection(
                null_mut(),
                (&header as *const BITMAPV5HEADER).cast(),
                DIB_RGB_COLORS,
                &mut bits,
                null_mut(),
                0,
            );
            if color.is_null() {
                return Err(failed("Create cursor color bitmap"));
            }
            let color = Bitmap(color);
            if bits.is_null() {
                return Err("Cursor color bitmap has no pixels".into());
            }
            std::ptr::copy_nonoverlapping(pixels.bgra.as_ptr(), bits.cast(), pixels.bgra.len());
            let mask_info = MaskInfo {
                header: BITMAPINFOHEADER {
                    biSize: size_of::<BITMAPINFOHEADER>() as u32,
                    biWidth: pixels.size[0] as i32,
                    biHeight: -(pixels.size[1] as i32),
                    biPlanes: 1,
                    biBitCount: 1,
                    biCompression: BI_RGB,
                    ..zeroed()
                },
                colors: [
                    zeroed(),
                    RGBQUAD {
                        rgbBlue: 255,
                        rgbGreen: 255,
                        rgbRed: 255,
                        rgbReserved: 0,
                    },
                ],
            };
            let mut mask_bits: *mut c_void = null_mut();
            let mask = CreateDIBSection(
                null_mut(),
                (&mask_info as *const MaskInfo).cast(),
                DIB_RGB_COLORS,
                &mut mask_bits,
                null_mut(),
                0,
            );
            if mask.is_null() {
                return Err(failed("Create cursor transparency mask"));
            }
            let mask = Bitmap(mask);
            if mask_bits.is_null() {
                return Err("Cursor mask has no pixels".into());
            }
            std::ptr::copy_nonoverlapping(
                pixels.mask.as_ptr(),
                mask_bits.cast(),
                pixels.mask.len(),
            );
            let info = ICONINFO {
                fIcon: 0,
                xHotspot: pixels.hotspot[0],
                yHotspot: pixels.hotspot[1],
                hbmMask: mask.0,
                hbmColor: color.0,
            };
            let cursor = CreateIconIndirect(&info);
            if cursor.is_null() {
                return Err(failed("Create native cursor"));
            }
            Ok(Self(cursor))
        }
    }

    pub fn check_metadata(&self, pixels: &CursorPixels) -> Result<(), String> {
        unsafe {
            let mut info: ICONINFO = zeroed();
            if GetIconInfo(self.0, &mut info) == 0 {
                return Err(failed("Read native cursor metadata"));
            }
            let color = Bitmap(info.hbmColor);
            let _mask = Bitmap(info.hbmMask);
            let mut bitmap: BITMAP = zeroed();
            if color.0.is_null()
                || GetObjectW(
                    color.0,
                    size_of::<BITMAP>() as i32,
                    (&mut bitmap as *mut BITMAP).cast(),
                ) == 0
            {
                return Err(failed("Read cursor bitmap size"));
            }
            if info.fIcon != 0
                || [info.xHotspot, info.yHotspot] != pixels.hotspot
                || [
                    bitmap.bmWidth.unsigned_abs(),
                    bitmap.bmHeight.unsigned_abs(),
                ] != pixels.size
            {
                return Err("Windows changed the cursor dimensions or hotspot".into());
            }
            Ok(())
        }
    }
}

pub const ROLES: &[(u32, &str)] = &[
    (OCR_NORMAL, "default"),
    (OCR_IBEAM, "text"),
    (OCR_WAIT, "wait"),
    (OCR_CROSS, "crosshair"),
    (OCR_UP, "default"),
    (OCR_SIZENWSE, "nwse-resize"),
    (OCR_SIZENESW, "nesw-resize"),
    (OCR_SIZEWE, "ew-resize"),
    (OCR_SIZENS, "ns-resize"),
    (OCR_SIZEALL, "move"),
    (OCR_NO, "not-allowed"),
    (OCR_HAND, "pointer"),
    (OCR_APPSTARTING, "progress"),
];

pub fn restore_scheme() -> Result<(), String> {
    if unsafe { SystemParametersInfoW(SPI_SETCURSORS, 0, null_mut(), 0) } == 0 {
        Err(failed("Restore your Windows cursor scheme"))
    } else {
        Ok(())
    }
}

pub struct Originals(Vec<(u32, Cursor)>);
impl Originals {
    pub fn capture() -> Result<Self, String> {
        let mut cursors = Vec::new();
        for &(id, _) in ROLES {
            let handle = unsafe { LoadCursorW(null_mut(), id as usize as *const u16) };
            if handle.is_null() {
                return Err(failed("Save your original Windows cursor"));
            }
            let copied = unsafe { CopyImage(handle, IMAGE_CURSOR, 0, 0, 0) };
            if copied.is_null() {
                return Err(failed("Copy your original Windows cursor"));
            }
            cursors.push((id, Cursor(copied)));
        }
        Ok(Self(cursors))
    }
    pub fn restore(&self) -> Result<(), String> {
        // Reload the persisted scheme, including animated busy cursors. Copies
        // are a last-resort fallback, since copying can flatten an animation.
        if restore_scheme().is_ok() {
            return Ok(());
        }
        let mut error = None;
        for (id, cursor) in &self.0 {
            if let Err(e) = cursor.install(*id) {
                error = Some(e);
            }
        }
        if let Some(e) = error {
            Err(e)
        } else {
            Ok(())
        }
    }
}

pub struct Frames {
    pub layout: Layout,
    color: String,
    light: Vec<RgbaImage>,
    dark: Vec<RgbaImage>,
    idle: Vec<(RgbaImage, Layout)>,
    tray_icon: RgbaImage,
}
impl Frames {
    pub fn load(root: &Path, color: &str) -> Result<Self, String> {
        let layout = Layout::parse(
            &std::fs::read_to_string(root.join("frames.txt")).map_err(|e| e.to_string())?,
        )?;
        let load = |bank| {
            layout
                .file_names(bank)
                .iter()
                .map(|file| kyber_preview::load_frame(&root.join(file), layout))
                .collect::<Result<Vec<_>, _>>()
        };
        let light = load("light")?;
        let dark = load("dark")?;
        let body_px = (48.0 * geometry::SCALE).round() as u32;
        let ppu = body_px as f64 / geometry::VIEW[2];
        let offset = (kyber_core::overlay::OVERLAY_MARGIN * ppu).round() as i64;
        let mut idle = Vec::new();
        for &(_, role) in ROLES {
            let body = RgbaImage::from_raw(
                body_px,
                body_px,
                render::rasterize_role(geometry::role_kind(role), color, color, false, body_px),
            )
            .ok_or("Invalid role pixels")?;
            let mut image = RgbaImage::new(layout.width, layout.height);
            image::imageops::overlay(&mut image, &body, offset, offset);
            let (hx, hy) = geometry::hotspot(role);
            let hotspot = [
                (offset as f64 + (hx - geometry::VIEW[0]) * ppu).round() as u32,
                (offset as f64 + (hy - geometry::VIEW[1]) * ppu).round() as u32,
            ];
            idle.push((image, Layout { hotspot, ..layout }));
        }
        let tray_icon = make_tray_icon(color)?;
        Ok(Self {
            layout,
            color: color.to_owned(),
            light,
            dark,
            idle,
            tray_icon,
        })
    }
    pub fn bank(&self, dpi: u32) -> Result<Bank, String> {
        // The accepted padded cursor reaches 256px at 238 DPI. Keep it intact
        // on larger display scales instead of cropping the halo or failing.
        let dpi = dpi.clamp(96, 238);
        let make = |image: &RgbaImage, layout, blade: bool| {
            let mut pixels = CursorPixels::new(image, layout, dpi)?;
            if blade {
                sharpen_blade_at_size(&mut pixels, layout, dpi, &self.color);
            }
            let cursor = Cursor::new(&pixels)?;
            cursor.check_metadata(&pixels)?;
            Ok(cursor)
        };
        let light = self
            .light
            .iter()
            .enumerate()
            .map(|(index, im)| make(im, self.layout, index + 1 >= self.layout.steps))
            .collect::<Result<_, String>>()?;
        let dark = self
            .dark
            .iter()
            .enumerate()
            .map(|(index, im)| make(im, self.layout, index + 1 >= self.layout.steps))
            .collect::<Result<_, String>>()?;
        let idle = self
            .idle
            .iter()
            .map(|(im, l)| make(im, *l, false))
            .collect::<Result<_, String>>()?;
        Ok(Bank {
            light,
            dark,
            idle,
            tray_icon: self.tray_icon.clone(),
        })
    }
}

/// Add a narrow, crisp colored blade and white hot core after Windows has
/// resampled the animation to the actual system-cursor size. The existing
/// glow and aura remain as rendered in the frame bank.
fn sharpen_blade_at_size(pixels: &mut CursorPixels, layout: Layout, dpi: u32, color: &str) {
    let [r, g, b] = kyber_core::color::parse_hex(&kyber_core::color::beam_color(color))
        .unwrap_or([80, 255, 100]);
    let glow = [r, g, b];
    let (from, to) = geometry::edge_line(-0.6, 0.03, 0.80);
    let origin = [
        geometry::VIEW[0] - kyber_core::overlay::OVERLAY_MARGIN,
        geometry::VIEW[1] - kyber_core::overlay::OVERLAY_MARGIN,
    ];
    let px_per_unit = (48.0 * geometry::SCALE / geometry::VIEW[2]) * f64::from(dpi)
        / f64::from(layout.scale * 96);
    let a = (
        (from.0 - origin[0]) * px_per_unit,
        (from.1 - origin[1]) * px_per_unit,
    );
    let b = (
        (to.0 - origin[0]) * px_per_unit,
        (to.1 - origin[1]) * px_per_unit,
    );
    let segment = (b.0 - a.0, b.1 - a.1);
    let length_sq = segment.0 * segment.0 + segment.1 * segment.1;

    for y in 0..pixels.size[1] {
        for x in 0..pixels.size[0] {
            let relative = (f64::from(x) + 0.5 - a.0, f64::from(y) + 0.5 - a.1);
            let along =
                ((relative.0 * segment.0 + relative.1 * segment.1) / length_sq).clamp(0.0, 1.0);
            let distance = (relative.0 - along * segment.0).hypot(relative.1 - along * segment.1);
            // Windows renders these cursors at roughly 32px on a 100% display.
            // Give the beam a few solid pixels so the color and white core survive
            // scaling and video capture instead of collapsing into a soft edge.
            let color_alpha = ((2.0 - distance).clamp(0.0, 1.0) * 0.98) as f32;
            let white_alpha = ((1.2 - distance).clamp(0.0, 1.0) * 1.0) as f32;
            if color_alpha > 0.0 {
                blend_bgra(pixels, x, y, glow, color_alpha);
            }
            if white_alpha > 0.0 {
                blend_bgra(pixels, x, y, [255; 3], white_alpha);
            }
        }
    }
}

fn blend_bgra(pixels: &mut CursorPixels, x: u32, y: u32, rgb: [u8; 3], alpha: f32) {
    let offset = ((y * pixels.size[0] + x) * 4) as usize;
    let inverse = 1.0 - alpha;
    let source = [rgb[2], rgb[1], rgb[0]];
    for (channel, value) in source.into_iter().enumerate() {
        pixels.bgra[offset + channel] = (f32::from(value) * alpha
            + f32::from(pixels.bgra[offset + channel]) * inverse)
            .round() as u8;
    }
    let old_alpha = f32::from(pixels.bgra[offset + 3]) / 255.0;
    let new_alpha = alpha + old_alpha * inverse;
    pixels.bgra[offset + 3] = (new_alpha * 255.0).round() as u8;
    if new_alpha > 0.0 {
        let mask = y as usize * pixels.mask_stride + x as usize / 8;
        pixels.mask[mask] &= !(0x80 >> (x % 8));
    }
}

fn make_tray_icon(color: &str) -> Result<RgbaImage, String> {
    let source_size = 96;
    let source = RgbaImage::from_raw(
        source_size,
        source_size,
        render::rasterize_role("pointer", color, color, true, source_size),
    )
    .ok_or("Invalid tray icon pixels")?;
    let mut bounds = [source_size, source_size, 0, 0];
    for (x, y, pixel) in source.enumerate_pixels() {
        if pixel.0[3] > 8 {
            bounds[0] = bounds[0].min(x);
            bounds[1] = bounds[1].min(y);
            bounds[2] = bounds[2].max(x);
            bounds[3] = bounds[3].max(y);
        }
    }
    if bounds[2] < bounds[0] || bounds[3] < bounds[1] {
        return Err("Tray icon has no visible pixels".into());
    }
    let margin = 5;
    let left = bounds[0].saturating_sub(margin);
    let top = bounds[1].saturating_sub(margin);
    let right = (bounds[2] + margin).min(source_size - 1);
    let bottom = (bounds[3] + margin).min(source_size - 1);
    let crop = image::imageops::crop_imm(&source, left, top, right - left + 1, bottom - top + 1)
        .to_image();
    let scale = 48.0 / crop.width().max(crop.height()) as f32;
    let width = (crop.width() as f32 * scale).round() as u32;
    let height = (crop.height() as f32 * scale).round() as u32;
    let crop = image::imageops::resize(&crop, width, height, image::imageops::FilterType::Lanczos3);
    let mut icon = RgbaImage::new(48, 48);
    image::imageops::overlay(
        &mut icon,
        &crop,
        i64::from((48 - width) / 2),
        i64::from((48 - height) / 2),
    );
    Ok(icon)
}

pub struct Bank {
    light: Vec<Cursor>,
    dark: Vec<Cursor>,
    idle: Vec<Cursor>,
    tray_icon: RgbaImage,
}
impl Bank {
    pub fn install(&self, frame: Option<usize>, light: bool) -> Result<(), String> {
        for (n, &(id, _)) in ROLES.iter().enumerate() {
            let cursor = match frame {
                Some(index) => {
                    if light {
                        &self.light[index]
                    } else {
                        &self.dark[index]
                    }
                }
                None => &self.idle[n],
            };
            cursor.install(id)?;
        }
        Ok(())
    }
    pub fn icon(&self) -> Result<HICON, String> {
        let layout = Layout {
            width: 48,
            height: 48,
            hotspot: [24, 24],
            scale: 1,
            steps: 2,
            held: 0,
            hold_ms: 80,
        };
        let pixels = CursorPixels::new(&self.tray_icon, layout, 96)?;
        let cursor = Cursor::new(&pixels)?;
        // ICONINFO owns the temporary returned bitmaps; CreateIconIndirect copies them.
        unsafe {
            let mut info: ICONINFO = zeroed();
            if GetIconInfo(cursor.0, &mut info) == 0 {
                return Err(failed("Create tray icon"));
            }
            let _color = Bitmap(info.hbmColor);
            let _mask = Bitmap(info.hbmMask);
            info.fIcon = 1;
            let icon = CreateIconIndirect(&info);
            if icon.is_null() {
                Err(failed("Create tray icon"))
            } else {
                Ok(icon)
            }
        }
    }
}
