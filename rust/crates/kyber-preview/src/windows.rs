//! Native cursor presentation is confined to this process's client area.
//! No global hooks, registry writes, system cursor replacement, or overlay.
use std::{
    cell::RefCell,
    ffi::c_void,
    mem::{size_of, zeroed},
    path::{Path, PathBuf},
    ptr::{null, null_mut},
    time::Instant,
};

use kyber_core::config::SoundMode;
use kyber_core::trail::{Bounds as TrailBounds, Trail};
use kyber_preview::audio::Player;
use kyber_preview::{Animation, CursorPixels, Layout};
use windows_sys::Win32::{
    Foundation::*,
    Graphics::Gdi::*,
    System::LibraryLoader::GetModuleHandleW,
    UI::{Controls::WM_MOUSELEAVE, HiDpi::*, Input::KeyboardAndMouse::*, WindowsAndMessaging::*},
};

fn wide(text: &str) -> Vec<u16> {
    text.encode_utf16().chain(Some(0)).collect()
}
fn failed(action: &str) -> String {
    format!("{action}: {}", std::io::Error::last_os_error())
}

pub fn show_error(error: &str) {
    unsafe {
        MessageBoxW(
            null_mut(),
            wide(error).as_ptr(),
            wide("Kyber cursor trial").as_ptr(),
            MB_OK | MB_ICONERROR,
        );
    }
}

struct Bitmap(HBITMAP);
impl Drop for Bitmap {
    fn drop(&mut self) {
        unsafe {
            DeleteObject(self.0);
        }
    }
}

struct Cursor(HCURSOR);
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
    fn new(pixels: &CursorPixels) -> Result<Self, String> {
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

    fn check_metadata(&self, pixels: &CursorPixels) -> Result<(), String> {
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

struct Bank {
    layout: Layout,
    light: Vec<Cursor>,
    dark: Vec<Cursor>,
}
impl Bank {
    fn load(root: &Path, dpi: u32, check: bool) -> Result<Self, String> {
        let text = std::fs::read_to_string(root.join("frames.txt"))
            .map_err(|e| format!("{}: {e}", root.display()))?;
        let layout = Layout::parse(&text)?;
        let load = |name| -> Result<Vec<Cursor>, String> {
            layout
                .file_names(name)
                .into_iter()
                .map(|file| {
                    let image = kyber_preview::load_frame(&root.join(file), layout)?;
                    let pixels = CursorPixels::new(&image, layout, dpi)?;
                    let cursor = Cursor::new(&pixels)?;
                    if check {
                        cursor.check_metadata(&pixels)?;
                    }
                    Ok(cursor)
                })
                .collect()
        };
        Ok(Self {
            layout,
            light: load("light")?,
            dark: load("dark")?,
        })
    }
}

struct App {
    root: PathBuf,
    colors: Vec<(String, String)>,
    selected: usize,
    bank: Bank,
    dpi: u32,
    background: u8,
    variant: u8,
    animation: Animation,
    audio: Player,
    trail: Trail,
    pressed: bool,
    started: Instant,
    tracking: bool,
    arrow: HCURSOR,
}
impl App {
    fn now(&self) -> u64 {
        self.started.elapsed().as_millis() as u64
    }
    fn cancel(&mut self, hwnd: HWND) {
        self.animation.cancel();
        self.audio.cancel();
        self.pressed = false;
        self.trail.clear();
        unsafe {
            KillTimer(hwnd, 1);
        }
        self.restore();
    }
    fn restore(&self) {
        unsafe {
            let current = GetCursor();
            if self
                .bank
                .light
                .iter()
                .chain(&self.bank.dark)
                .any(|cursor| cursor.0 == current)
            {
                SetCursor(self.arrow);
            }
        }
    }
    fn light_at(&self, x: i32, width: i32) -> bool {
        match self.variant {
            1 => true,
            2 => false,
            _ => self.background == 1 || (self.background == 0 && x < width / 2),
        }
    }
    fn position(&self, hwnd: HWND) -> Option<(POINT, RECT)> {
        unsafe {
            let mut point: POINT = zeroed();
            let mut rect: RECT = zeroed();
            if GetCursorPos(&mut point) == 0 || WindowFromPoint(point) != hwnd {
                return None;
            }
            if ScreenToClient(hwnd, &mut point) == 0 || GetClientRect(hwnd, &mut rect) == 0 {
                return None;
            }
            (point.x >= 0 && point.y >= 0 && point.x < rect.right && point.y < rect.bottom)
                .then_some((point, rect))
        }
    }
    fn cursor(&self, hwnd: HWND) {
        if let Some((point, rect)) = self.position(hwnd) {
            let frames = if self.light_at(point.x, rect.right) {
                &self.bank.light
            } else {
                &self.bank.dark
            };
            let index = self.animation.frame(self.now(), self.bank.layout);
            unsafe {
                SetCursor(frames[index].0);
            }
        }
    }
    fn sample_trail(&mut self, hwnd: HWND) {
        if self.pressed {
            if let Some((point, _)) = self.position(hwnd) {
                self.trail
                    .sample(self.now() as f64 / 1000.0, point.x as f64, point.y as f64);
            }
        }
    }
    fn trail_live(&mut self) -> bool {
        self.trail.bounds(self.now() as f64 / 1000.0).is_some()
    }
    fn title(&self, hwnd: HWND) {
        let title = format!(
            "Kyber Windows cursor trial — {} — {}% display scale",
            self.colors[self.selected].1,
            self.dpi * 100 / 96
        );
        unsafe {
            SetWindowTextW(hwnd, wide(&title).as_ptr());
            InvalidateRect(hwnd, null(), 0);
        }
    }
    fn color(&mut self, hwnd: HWND, next: usize) -> Result<(), String> {
        self.cancel(hwnd);
        let bank = Bank::load(&self.root.join(&self.colors[next].0), self.dpi, false)?;
        self.bank = bank;
        self.selected = next;
        self.trail.set_color(&self.colors[next].1);
        self.title(hwnd);
        self.cursor(hwnd);
        Ok(())
    }
    fn paint(&mut self, hwnd: HWND) {
        unsafe {
            let mut ps: PAINTSTRUCT = zeroed();
            let dc = BeginPaint(hwnd, &mut ps);
            let mut rect: RECT = zeroed();
            GetClientRect(hwnd, &mut rect);
            let paint_background = |rect: RECT, light: bool| {
                let color = if light { 0x00ffffff } else { 0x00161212 };
                let brush = CreateSolidBrush(color);
                FillRect(dc, &rect, brush);
                DeleteObject(brush);
            };
            if self.background == 0 {
                paint_background(
                    RECT {
                        right: rect.right / 2,
                        ..rect
                    },
                    true,
                );
                paint_background(
                    RECT {
                        left: rect.right / 2,
                        ..rect
                    },
                    false,
                );
            } else {
                paint_background(rect, self.background == 1);
            }
            if let Some(image) = self.trail.render(
                self.now() as f64 / 1000.0,
                TrailBounds {
                    x: 0.0,
                    y: 0.0,
                    width: rect.right.max(0) as f64,
                    height: rect.bottom.max(0) as f64,
                },
                self.position(hwnd)
                    .is_some_and(|(point, bounds)| self.light_at(point.x, bounds.right)),
                1.0,
            ) {
                let width = image.pixels.width() as i32;
                let height = image.pixels.height() as i32;
                let header = BITMAPV5HEADER {
                    bV5Size: size_of::<BITMAPV5HEADER>() as u32,
                    bV5Width: width,
                    bV5Height: -height,
                    bV5Planes: 1,
                    bV5BitCount: 32,
                    bV5Compression: BI_BITFIELDS,
                    bV5RedMask: 0x00ff0000,
                    bV5GreenMask: 0x0000ff00,
                    bV5BlueMask: 0x000000ff,
                    bV5AlphaMask: 0xff000000,
                    ..zeroed()
                };
                let memory_dc = CreateCompatibleDC(dc);
                if !memory_dc.is_null() {
                    let mut bits: *mut c_void = null_mut();
                    let bitmap = CreateDIBSection(
                        memory_dc,
                        (&header as *const BITMAPV5HEADER).cast(),
                        DIB_RGB_COLORS,
                        &mut bits,
                        null_mut(),
                        0,
                    );
                    if !bitmap.is_null() && !bits.is_null() {
                        let mut bgra = Vec::with_capacity(image.pixels.data().len());
                        for pixel in image.pixels.data().chunks_exact(4) {
                            bgra.extend_from_slice(&[pixel[2], pixel[1], pixel[0], pixel[3]]);
                        }
                        std::ptr::copy_nonoverlapping(bgra.as_ptr(), bits.cast(), bgra.len());
                        let previous = SelectObject(memory_dc, bitmap);
                        let blend = BLENDFUNCTION {
                            BlendOp: AC_SRC_OVER as u8,
                            BlendFlags: 0,
                            SourceConstantAlpha: 255,
                            AlphaFormat: AC_SRC_ALPHA as u8,
                        };
                        AlphaBlend(
                            dc,
                            image.bounds.x.round() as i32,
                            image.bounds.y.round() as i32,
                            image.bounds.width.round() as i32,
                            image.bounds.height.round() as i32,
                            memory_dc,
                            0,
                            0,
                            width,
                            height,
                            blend,
                        );
                        if !previous.is_null() {
                            SelectObject(memory_dc, previous);
                        }
                    }
                    if !bitmap.is_null() {
                        DeleteObject(bitmap);
                    }
                    DeleteDC(memory_dc);
                }
            }
            let paint_panel = |rect: RECT, light: bool| {
                SetTextColor(dc, if light { 0x00222222 } else { 0x00eeeeee });
                SetBkMode(dc, TRANSPARENT as i32);
                let font = GetStockObject(DEFAULT_GUI_FONT);
                let previous = SelectObject(dc, font);
                let sound = match (self.audio.available(), self.audio.mode()) {
                    (false, _) => "unavailable (no default output device)",
                    (true, SoundMode::All) if self.audio.has_custom_clips() => {
                        "local recordings + recorded hum"
                    }
                    (true, SoundMode::All) => "bundled recordings + recorded hum",
                    (true, SoundMode::Ignite) => "power-up only",
                    (true, SoundMode::Mute) => "muted",
                };
                let text = format!("{}\n\nHold the LEFT mouse button to ignite and draw a fading trail.\nMove the pointer across the window.\n\nSpace / arrows: change color\nB: white, dark, or split background\nV: matching, light, or dark glow variant\nS: cycle sound mode\nEscape: close\n\nColor: {}\nGlow variant: {}\nSound: {}\n\nThe custom pointer and trail stay in this window. Sounds use the shared private recording pack.",
                    if light {"WHITE BACKGROUND"} else {"DARK BACKGROUND"}, self.colors[self.selected].1,
                    ["match background","light on both backgrounds","dark on both backgrounds"][self.variant as usize], sound);
                let mut area = RECT {
                    left: rect.left + 16,
                    top: rect.top + 16,
                    right: rect.right - 16,
                    bottom: rect.bottom - 16,
                };
                let text = wide(&text);
                DrawTextW(
                    dc,
                    text.as_ptr(),
                    (text.len() - 1) as i32,
                    &mut area,
                    DT_LEFT | DT_WORDBREAK | DT_NOPREFIX,
                );
                SelectObject(dc, previous);
            };
            if self.background == 0 {
                paint_panel(
                    RECT {
                        right: rect.right / 2,
                        ..rect
                    },
                    true,
                );
                paint_panel(
                    RECT {
                        left: rect.right / 2,
                        ..rect
                    },
                    false,
                );
            } else {
                paint_panel(rect, self.background == 1);
            }
            EndPaint(hwnd, &ps);
        }
    }
}

unsafe extern "system" fn procedure(
    hwnd: HWND,
    message: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    // The owning Box lives in run() until the window has been destroyed. A
    // RefCell prevents reentrant Win32 calls from creating aliased &mut App.
    if message == WM_NCCREATE {
        let create = &*(lparam as *const CREATESTRUCTW);
        SetWindowLongPtrW(hwnd, GWLP_USERDATA, create.lpCreateParams as isize);
        return 1;
    }
    if message == WM_NCDESTROY {
        SetWindowLongPtrW(hwnd, GWLP_USERDATA, 0);
        return DefWindowProcW(hwnd, message, wparam, lparam);
    }
    if message == WM_CLOSE || (message == WM_KEYDOWN && wparam == VK_ESCAPE as usize) {
        DestroyWindow(hwnd);
        return 0;
    }
    if message == WM_DESTROY {
        KillTimer(hwnd, 1);
        PostQuitMessage(0);
        return 0;
    }
    let pointer = GetWindowLongPtrW(hwnd, GWLP_USERDATA) as *const RefCell<App>;
    if pointer.is_null() {
        return DefWindowProcW(hwnd, message, wparam, lparam);
    }
    let Ok(mut app) = (*pointer).try_borrow_mut() else {
        return DefWindowProcW(hwnd, message, wparam, lparam);
    };
    match message {
        WM_SETCURSOR if (lparam as u32 & 0xffff) == HTCLIENT => {
            app.cursor(hwnd);
            return 1;
        }
        WM_MOUSEMOVE => {
            if !app.tracking {
                let mut track = TRACKMOUSEEVENT {
                    cbSize: size_of::<TRACKMOUSEEVENT>() as u32,
                    dwFlags: TME_LEAVE,
                    hwndTrack: hwnd,
                    dwHoverTime: 0,
                };
                app.tracking = TrackMouseEvent(&mut track) != 0;
            }
            app.cursor(hwnd);
            app.sample_trail(hwnd);
            InvalidateRect(hwnd, null(), 0);
            return 0;
        }
        WM_LBUTTONDOWN => {
            SetCapture(hwnd);
            app.pressed = true;
            app.trail.clear();
            app.sample_trail(hwnd);
            let now = app.now();
            app.animation.set_pressed(true, now);
            app.audio.press();
            if SetTimer(hwnd, 1, 16, None) == 0 {
                app.cancel(hwnd);
                show_error(&failed("Start cursor animation timer"));
            }
            app.cursor(hwnd);
            InvalidateRect(hwnd, null(), 0);
            return 0;
        }
        WM_LBUTTONUP => {
            if GetCapture() == hwnd {
                ReleaseCapture();
            }
            app.pressed = false;
            let now = app.now();
            app.animation.set_pressed(false, now);
            app.audio.release();
            app.cursor(hwnd);
            InvalidateRect(hwnd, null(), 0);
            return 0;
        }
        WM_TIMER if wparam == 1 => {
            if app.position(hwnd).is_none() || GetForegroundWindow() != hwnd {
                app.cancel(hwnd);
            } else {
                app.cursor(hwnd);
                InvalidateRect(hwnd, null(), 0);
                if !app.animation.running(app.now()) && !app.trail_live() {
                    KillTimer(hwnd, 1);
                }
            }
            return 0;
        }
        WM_MOUSELEAVE => {
            app.tracking = false;
            app.cancel(hwnd);
            return 0;
        }
        WM_KILLFOCUS | WM_CANCELMODE => {
            app.cancel(hwnd);
            return 0;
        }
        WM_ACTIVATE if (wparam & 0xffff) == WA_INACTIVE as usize => {
            app.cancel(hwnd);
            return 0;
        }
        WM_KEYDOWN => {
            if wparam == VK_SPACE as usize
                || wparam == VK_RIGHT as usize
                || wparam == VK_LEFT as usize
            {
                let count = app.colors.len();
                let next = (app.selected
                    + if wparam == VK_LEFT as usize {
                        count - 1
                    } else {
                        1
                    })
                    % count;
                if let Err(error) = app.color(hwnd, next) {
                    show_error(&error);
                }
            } else if wparam == 0x42 {
                app.cancel(hwnd);
                app.background = (app.background + 1) % 3;
                app.title(hwnd);
                app.cursor(hwnd);
            } else if wparam == 0x56 {
                app.variant = (app.variant + 1) % 3;
                app.title(hwnd);
                app.cursor(hwnd);
            } else if wparam == 0x53 {
                app.audio.cycle_mode();
                app.title(hwnd);
                InvalidateRect(hwnd, null(), 0);
            }
            return 0;
        }
        WM_DPICHANGED => {
            app.cancel(hwnd);
            let dpi = (wparam & 0xffff) as u32;
            match Bank::load(&app.root.join(&app.colors[app.selected].0), dpi, false) {
                Ok(bank) => {
                    app.bank = bank;
                    app.dpi = dpi;
                    app.title(hwnd);
                }
                Err(error) => {
                    show_error(&error);
                    DestroyWindow(hwnd);
                    return 0;
                }
            }
            let rect = &*(lparam as *const RECT);
            SetWindowPos(
                hwnd,
                null_mut(),
                rect.left,
                rect.top,
                rect.right - rect.left,
                rect.bottom - rect.top,
                SWP_NOZORDER | SWP_NOACTIVATE,
            );
            return 0;
        }
        WM_PAINT => {
            app.paint(hwnd);
            return 0;
        }
        WM_ERASEBKGND => return 1,
        WM_SIZE => {
            InvalidateRect(hwnd, null(), 0);
            return 0;
        }
        _ => {}
    }
    DefWindowProcW(hwnd, message, wparam, lparam)
}

pub fn run() -> Result<(), String> {
    let executable = std::env::current_exe().map_err(|e| e.to_string())?;
    let mut root = executable
        .parent()
        .ok_or("Cannot find the executable folder")?
        .join("frames");
    let mut report = executable.with_file_name("self-check.json");
    let mut check = false;
    let mut arguments = std::env::args_os().skip(1);
    while let Some(argument) = arguments.next() {
        if argument == "--frames" {
            root = arguments.next().ok_or("--frames needs a folder")?.into();
        } else if argument == "--report" {
            report = arguments.next().ok_or("--report needs a file")?.into();
        } else if argument == "--self-check" {
            check = true;
        } else {
            return Err(format!("Unknown argument: {}", argument.to_string_lossy()));
        }
    }
    let colors = kyber_preview::palette(&root)?;
    if check {
        let mut cursors = 0;
        for dpi in [96, 192] {
            for (name, _) in &colors {
                let bank = Bank::load(&root.join(name), dpi, true)?;
                cursors += bank.light.len() + bank.dark.len();
            }
        }
        let result = serde_json::json!({"platform":"Windows","colors":colors.len(),"dpi":[96,192],"cursors_created_and_verified":cursors,
            "scope":"Native cursor creation, dimensions and hotspots only; interactive movement/input still need user testing","audio_playback_tested":false});
        std::fs::write(
            report,
            serde_json::to_string_pretty(&result).map_err(|e| e.to_string())?,
        )
        .map_err(|e| e.to_string())?;
        return Ok(());
    }
    unsafe {
        if SetProcessDpiAwarenessContext(DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2) == 0
            && GetLastError() != ERROR_ACCESS_DENIED
        {
            return Err(failed("Enable per-monitor display scaling"));
        }
        let instance = GetModuleHandleW(null());
        if instance.is_null() {
            return Err(failed("Find application instance"));
        }
        let arrow = LoadCursorW(null_mut(), IDC_ARROW);
        if arrow.is_null() {
            return Err(failed("Load normal pointer"));
        }
        let class_name = wide("KyberWindowScopedCursorTrial");
        let class = WNDCLASSW {
            lpfnWndProc: Some(procedure),
            hInstance: instance,
            hCursor: arrow,
            lpszClassName: class_name.as_ptr(),
            ..zeroed()
        };
        if RegisterClassW(&class) == 0 {
            return Err(failed("Register test window"));
        }
        let bank = Bank::load(&root.join(&colors[0].0), 96, false)?;
        let mut trail = Trail::default();
        trail.set_color(&colors[0].1);
        let app = Box::new(RefCell::new(App {
            root,
            colors,
            selected: 0,
            bank,
            dpi: 96,
            background: 0,
            variant: 0,
            animation: Animation::default(),
            audio: Player::new(),
            trail,
            pressed: false,
            started: Instant::now(),
            tracking: false,
            arrow,
        }));
        let hwnd = CreateWindowExW(
            0,
            class_name.as_ptr(),
            wide("Kyber Windows cursor trial").as_ptr(),
            WS_OVERLAPPEDWINDOW,
            CW_USEDEFAULT,
            CW_USEDEFAULT,
            1000,
            700,
            null_mut(),
            null_mut(),
            instance,
            (&*app as *const RefCell<App>).cast_mut().cast(),
        );
        if hwnd.is_null() {
            return Err(failed("Create test window"));
        }
        let dpi = GetDpiForWindow(hwnd);
        if dpi != 96 {
            let mut state = app.borrow_mut();
            let bank = match Bank::load(&state.root.join(&state.colors[0].0), dpi, false) {
                Ok(bank) => bank,
                Err(error) => {
                    drop(state);
                    DestroyWindow(hwnd);
                    return Err(error);
                }
            };
            state.bank = bank;
            state.dpi = dpi;
        }
        app.borrow().title(hwnd);
        ShowWindow(hwnd, SW_SHOW);
        UpdateWindow(hwnd);
        let mut message: MSG = zeroed();
        let result = loop {
            let status = GetMessageW(&mut message, null_mut(), 0, 0);
            if status == -1 {
                break Err(failed("Read window messages"));
            }
            if status == 0 {
                break Ok(());
            }
            TranslateMessage(&message);
            DispatchMessageW(&message);
        };
        // Detach the window state before dropping its cursor handles, including
        // the message-loop error path. Never destroy an active custom cursor.
        if IsWindow(hwnd) != 0 {
            DestroyWindow(hwnd);
        }
        app.borrow().restore();
        drop(app);
        UnregisterClassW(class_name.as_ptr(), instance);
        result
    }
}
