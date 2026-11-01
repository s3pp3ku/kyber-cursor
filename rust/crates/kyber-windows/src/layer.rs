//! Only the fading trail uses a window. Windows moves the real cursor itself.
use crate::{failed, wide};
use kyber_core::trail::TrailImage;
use std::{
    ffi::c_void,
    mem::{size_of, zeroed},
    ptr::{null, null_mut},
};
use windows_sys::Win32::{
    Foundation::*, Graphics::Gdi::*, System::LibraryLoader::GetModuleHandleW,
    UI::WindowsAndMessaging::*,
};

pub struct Layer {
    pub hwnd: HWND,
    visible: bool,
}
impl Layer {
    pub fn new() -> Result<Self, String> {
        unsafe {
            let class = wide("KyberCursorTrailV1");
            let instance = GetModuleHandleW(null());
            let wc = WNDCLASSW {
                lpfnWndProc: Some(proc),
                hInstance: instance,
                lpszClassName: class.as_ptr(),
                ..zeroed()
            };
            if RegisterClassW(&wc) == 0 {
                return Err(failed("Register trail window"));
            }
            let hwnd = CreateWindowExW(
                WS_EX_LAYERED
                    | WS_EX_TRANSPARENT
                    | WS_EX_NOACTIVATE
                    | WS_EX_TOOLWINDOW
                    | WS_EX_TOPMOST,
                class.as_ptr(),
                wide("Kyber fading trail").as_ptr(),
                WS_POPUP,
                0,
                0,
                1,
                1,
                null_mut(),
                null_mut(),
                instance,
                null(),
            );
            if hwnd.is_null() {
                Err(failed("Create trail window"))
            } else {
                Ok(Self {
                    hwnd,
                    visible: false,
                })
            }
        }
    }
    pub fn hide(&mut self) {
        if self.visible {
            unsafe {
                ShowWindow(self.hwnd, SW_HIDE);
            }
            self.visible = false;
        }
    }
    pub fn present(&mut self, trail: TrailImage, scale: f64) -> Result<(), String> {
        let width = (trail.bounds.width * scale).ceil().max(1.0) as u32;
        let height = (trail.bounds.height * scale).ceil().max(1.0) as u32;
        // Bound the actual destination allocation as well as the core raster.
        if width > 8192 || height > 8192 || u64::from(width) * u64::from(height) > 8_388_608 {
            self.hide();
            return Ok(());
        }
        let image = image::RgbaImage::from_raw(
            trail.pixels.width(),
            trail.pixels.height(),
            trail.pixels.take(),
        )
        .ok_or("Invalid trail pixels")?;
        let image =
            image::imageops::resize(&image, width, height, image::imageops::FilterType::Triangle);
        unsafe {
            let screen = GetDC(null_mut());
            if screen.is_null() {
                return Err(failed("Get trail drawing context"));
            }
            let dc = CreateCompatibleDC(screen);
            if dc.is_null() {
                ReleaseDC(null_mut(), screen);
                return Err(failed("Create trail drawing context"));
            }
            let info = BITMAPINFO {
                bmiHeader: BITMAPINFOHEADER {
                    biSize: size_of::<BITMAPINFOHEADER>() as u32,
                    biWidth: width as i32,
                    biHeight: -(height as i32),
                    biPlanes: 1,
                    biBitCount: 32,
                    biCompression: BI_RGB,
                    ..zeroed()
                },
                ..zeroed()
            };
            let mut bits: *mut c_void = null_mut();
            let bitmap = CreateDIBSection(screen, &info, DIB_RGB_COLORS, &mut bits, null_mut(), 0);
            if bitmap.is_null() || bits.is_null() {
                if !bitmap.is_null() {
                    DeleteObject(bitmap);
                }
                DeleteDC(dc);
                ReleaseDC(null_mut(), screen);
                return Err(failed("Create trail bitmap"));
            }
            let out = std::slice::from_raw_parts_mut(
                bits.cast::<u8>(),
                width as usize * height as usize * 4,
            );
            for (rgba, bgra) in image.as_raw().chunks_exact(4).zip(out.chunks_exact_mut(4)) {
                bgra.copy_from_slice(&[rgba[2], rgba[1], rgba[0], rgba[3]]);
            }
            let old = SelectObject(dc, bitmap);
            let destination = POINT {
                x: (trail.bounds.x * scale).floor() as i32,
                y: (trail.bounds.y * scale).floor() as i32,
            };
            let size = SIZE {
                cx: width as i32,
                cy: height as i32,
            };
            let source = POINT { x: 0, y: 0 };
            let blend = BLENDFUNCTION {
                BlendOp: AC_SRC_OVER as u8,
                BlendFlags: 0,
                SourceConstantAlpha: 255,
                AlphaFormat: AC_SRC_ALPHA as u8,
            };
            let ok = UpdateLayeredWindow(
                self.hwnd,
                screen,
                &destination,
                &size,
                dc,
                &source,
                0,
                &blend,
                ULW_ALPHA,
            );
            SelectObject(dc, old);
            DeleteObject(bitmap);
            DeleteDC(dc);
            ReleaseDC(null_mut(), screen);
            if ok == 0 {
                return Err(failed("Display fading trail"));
            }
            if !self.visible {
                SetWindowPos(
                    self.hwnd,
                    HWND_TOPMOST,
                    0,
                    0,
                    0,
                    0,
                    SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE | SWP_SHOWWINDOW,
                );
                self.visible = true;
            }
        }
        Ok(())
    }
}
impl Drop for Layer {
    fn drop(&mut self) {
        unsafe {
            DestroyWindow(self.hwnd);
        }
    }
}
unsafe extern "system" fn proc(hwnd: HWND, message: u32, w: WPARAM, l: LPARAM) -> LRESULT {
    match message {
        WM_NCHITTEST => HTTRANSPARENT as LRESULT,
        WM_MOUSEACTIVATE => MA_NOACTIVATE as LRESULT,
        WM_ERASEBKGND => 1,
        _ => DefWindowProcW(hwnd, message, w, l),
    }
}
