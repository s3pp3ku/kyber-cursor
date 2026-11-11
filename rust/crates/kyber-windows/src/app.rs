use crate::{
    cursor::{self, Bank, Frames, Originals},
    failed,
    guardian::{Guardian, Handle},
    layer::Layer,
    wide,
};
use kyber_core::{
    config::SoundMode,
    trail::{Bounds, Trail},
};
use kyber_preview::{audio::Player, Animation};
use serde::{Deserialize, Serialize};
use std::{
    cell::RefCell,
    collections::BTreeMap,
    fs,
    mem::{size_of, zeroed},
    path::PathBuf,
    ptr::{null, null_mut},
    time::Instant,
};
use windows_sys::Win32::{
    Foundation::*,
    Graphics::Gdi::*,
    System::{
        LibraryLoader::GetModuleHandleW, RemoteDesktop::*, StationsAndDesktops::*, Threading::*,
    },
    UI::{
        Controls::Dialogs::*,
        HiDpi::*,
        Input::{KeyboardAndMouse::*, *},
        Shell::*,
        WindowsAndMessaging::*,
    },
};

const CLASS: &str = "KyberCursorDesktopV1";
const TRAY: u32 = WM_APP + 1;
const RESTORE: u32 = WM_APP + 2;
const MENU: u32 = WM_APP + 3;
const CHOOSE: u32 = WM_APP + 4;
const ID_PAUSE: u32 = 100;
const ID_TRAIL: u32 = 101;
const ID_BANK: u32 = 102;
const ID_PICK: u32 = 103;
const ID_EXIT: u32 = 104;
const ID_ALL: u32 = 110;
const ID_IGNITE: u32 = 111;
const ID_MUTE: u32 = 112;
const ID_COLOR: u32 = 200;
const ID_VOLUME: u32 = 300;

thread_local! { static APP: RefCell<Option<App>> = const { RefCell::new(None) }; }

#[derive(Clone, Serialize, Deserialize)]
#[serde(default)]
struct Settings {
    color: String,
    trail: bool,
    light: bool,
    sound: SoundMode,
    volume: u8,
}
impl Default for Settings {
    fn default() -> Self {
        Self {
            color: "#07d815".into(),
            trail: true,
            light: true,
            sound: SoundMode::All,
            volume: 56,
        }
    }
}
impl Settings {
    fn load(root: &std::path::Path) -> Self {
        let mut value: Self = fs::read(root.join("windows.json"))
            .ok()
            .and_then(|bytes| serde_json::from_slice(&bytes).ok())
            .unwrap_or_default();
        if value.color.len() != 7
            || !value.color.starts_with('#')
            || !kyber_core::color::is_valid_hex(&value.color)
        {
            value.color = Self::default().color;
        }
        value.volume = value.volume.min(100);
        value
    }
    fn save(&self, root: &std::path::Path) -> Result<(), String> {
        let bytes = serde_json::to_vec_pretty(self).map_err(|e| e.to_string())?;
        fs::write(root.join("windows.json"), bytes)
            .map_err(|e| format!("Save Windows settings: {e}"))
    }
}

struct App {
    hwnd: HWND,
    settings_root: PathBuf,
    frames_root: PathBuf,
    settings: Settings,
    palette: Vec<(String, String)>,
    frames: Frames,
    banks: BTreeMap<u32, Bank>,
    dpi: u32,
    last_frame: Option<(u32, Option<usize>, bool)>,
    originals: Originals,
    guardian: Guardian,
    layer: Layer,
    icon: HICON,
    audio: Player,
    animation: Animation,
    trail: Trail,
    start: Instant,
    buttons: u8,
    armed: bool,
    pressed_at: Option<u64>,
    sounded: bool,
    last_press: Option<u64>,
    double: bool,
    last_clash: u64,
    last_swipe: u64,
    last_position: Option<(u64, f64, f64)>,
    paused: bool,
    suspended: bool,
    locked: bool,
    menu_open: bool,
    taskbar_created: u32,
    health_at: u64,
    stopped: bool,
    error: Option<String>,
}

fn monitor_dpi(point: POINT) -> u32 {
    unsafe {
        let monitor = MonitorFromPoint(point, MONITOR_DEFAULTTONEAREST);
        let (mut x, mut y) = (96, 96);
        if GetDpiForMonitor(monitor, MDT_EFFECTIVE_DPI, &mut x, &mut y) < 0 {
            96
        } else {
            x.clamp(96, 238)
        }
    }
}
unsafe extern "system" fn collect_dpi(
    monitor: HMONITOR,
    _: HDC,
    _: *mut RECT,
    data: LPARAM,
) -> i32 {
    let values = &mut *(data as *mut Vec<u32>);
    let (mut x, mut y) = (96, 96);
    if GetDpiForMonitor(monitor, MDT_EFFECTIVE_DPI, &mut x, &mut y) >= 0 {
        values.push(x.clamp(96, 238));
    }
    1
}

impl App {
    fn new(hwnd: HWND) -> Result<Self, String> {
        let settings_root = kyber_core::paths::settings_dir()?;
        fs::create_dir_all(&settings_root).map_err(|e| e.to_string())?;
        let settings = Settings::load(&settings_root);
        let frames_root = std::env::current_exe()
            .map_err(|e| e.to_string())?
            .parent()
            .ok_or("Missing app folder")?
            .join("frames");
        let palette = kyber_preview::palette(&frames_root)?;
        let root = color_frames(&frames_root, &settings_root, &palette, &settings.color)?;
        let frames = Frames::load(&root, &settings.color)?;
        let mut values = vec![96];
        unsafe {
            EnumDisplayMonitors(
                null_mut(),
                null(),
                Some(collect_dpi),
                (&mut values as *mut Vec<u32>) as LPARAM,
            );
        }
        values.sort_unstable();
        values.dedup();
        let mut banks = BTreeMap::new();
        for dpi in values {
            banks.insert(dpi, frames.bank(dpi)?);
        }
        let mut point = POINT { x: 0, y: 0 };
        unsafe {
            GetCursorPos(&mut point);
        }
        let dpi = monitor_dpi(point);
        if let std::collections::btree_map::Entry::Vacant(entry) = banks.entry(dpi) {
            entry.insert(frames.bank(dpi)?);
        }
        let icon = banks[&dpi].icon()?;
        let layer = match Layer::new() {
            Ok(layer) => layer,
            Err(e) => {
                unsafe {
                    DestroyIcon(icon);
                }
                return Err(e);
            }
        };
        let originals = Originals::capture()?;
        let guardian = Guardian::start()?;
        let mut audio = Player::new();
        audio.set_mode(settings.sound);
        audio.set_volume(settings.volume);
        let mut trail = Trail::default();
        trail.set_color(&settings.color);
        trail.set_look(kyber_core::TrailLook::load(&settings_root));
        let taskbar_created = unsafe { RegisterWindowMessageW(wide("TaskbarCreated").as_ptr()) };
        Ok(Self {
            hwnd,
            settings_root,
            frames_root,
            settings,
            palette,
            frames,
            banks,
            dpi,
            last_frame: None,
            originals,
            guardian,
            layer,
            icon,
            audio,
            animation: Animation::default(),
            trail,
            start: Instant::now(),
            buttons: 0,
            armed: false,
            pressed_at: None,
            sounded: false,
            last_press: None,
            double: false,
            last_clash: 0,
            last_swipe: 0,
            last_position: None,
            paused: false,
            suspended: false,
            locked: false,
            menu_open: false,
            taskbar_created,
            health_at: 0,
            stopped: false,
            error: None,
        })
    }
    fn now(&self) -> u64 {
        self.start.elapsed().as_millis() as u64
    }
    fn tray(&self, action: u32) -> Result<(), String> {
        let mut data: NOTIFYICONDATAW = unsafe { zeroed() };
        data.cbSize = size_of::<NOTIFYICONDATAW>() as u32;
        data.hWnd = self.hwnd;
        data.uID = 1;
        data.uFlags = NIF_MESSAGE | NIF_ICON | NIF_TIP;
        data.uCallbackMessage = TRAY;
        data.hIcon = self.icon;
        let tip = wide(if self.paused {
            "Kyber Cursor — paused. Click for controls."
        } else {
            "Kyber Cursor — click for controls."
        });
        data.szTip[..tip.len()].copy_from_slice(&tip);
        if unsafe { Shell_NotifyIconW(action, &data) } == 0 {
            Err(failed("Show Kyber tray controls"))
        } else {
            Ok(())
        }
    }
    fn enable(&mut self) -> Result<(), String> {
        self.last_frame = None;
        self.banks[&self.dpi].install(None, self.settings.light)?;
        self.last_frame = Some((self.dpi, None, self.settings.light));
        Ok(())
    }
    fn cancel(&mut self) {
        self.animation.cancel();
        self.buttons = 0;
        self.armed = false;
        self.pressed_at = None;
        self.sounded = false;
        self.last_press = None;
        self.audio.cancel();
        self.trail.clear();
        self.layer.hide();
        self.last_position = None;
    }
    fn stop(&mut self) -> Result<(), String> {
        if self.stopped {
            return Ok(());
        }
        self.cancel();
        self.originals.restore()?;
        self.guardian.finish();
        let _ = self.tray(NIM_DELETE);
        self.stopped = true;
        Ok(())
    }
    fn input(&mut self, flags: u16) {
        if self.paused || self.suspended || self.menu_open || !self.armed {
            return;
        }
        let now = self.now();
        let before = self.buttons;
        for n in 0..5 {
            if flags & (1 << (n * 2)) != 0 {
                self.buttons |= 1 << n;
            }
            if flags & (2 << (n * 2)) != 0 {
                self.buttons &= !(1 << n);
            }
        }
        if before == 0 && self.buttons != 0 {
            self.double = self
                .last_press
                .is_some_and(|last| now.saturating_sub(last) < 400);
            self.last_press = if self.double { None } else { Some(now) };
            self.pressed_at = Some(now);
            self.sounded = false;
            self.animation.set_pressed(true, now);
            if self.double && now.saturating_sub(self.last_clash) >= 140 {
                self.audio.effect("clash");
                self.last_clash = now;
            }
        } else if before != 0 && self.buttons == 0 {
            self.release(now);
        }
    }
    fn release(&mut self, now: u64) {
        self.buttons = 0;
        self.pressed_at = None;
        self.animation.set_pressed(false, now);
        if self.sounded {
            self.audio.release();
        }
        self.sounded = false;
    }
    fn tick(&mut self) -> Result<(), String> {
        let now = self.now();
        self.audio.tick();
        if now >= self.health_at {
            self.health_at = now + 250;
            if !self.guardian.alive() {
                return Err(
                    "Cursor recovery process stopped; Kyber is closing and restoring your pointer."
                        .into(),
                );
            }
            let desktop = unsafe { OpenInputDesktop(0, 0, DESKTOP_READOBJECTS) };
            let inaccessible = desktop.is_null();
            if !desktop.is_null() {
                unsafe {
                    CloseDesktop(desktop);
                }
            }
            let suspended = self.locked || inaccessible;
            if suspended != self.suspended {
                self.suspended = suspended;
                self.cancel();
                if suspended {
                    self.originals.restore()?;
                } else if !self.paused {
                    self.enable()?;
                }
            }
        }
        if self.paused || self.suspended || self.menu_open {
            return Ok(());
        }
        let mut info: CURSORINFO = unsafe { zeroed() };
        info.cbSize = size_of::<CURSORINFO>() as u32;
        if unsafe { GetCursorInfo(&mut info) } == 0 || info.flags & CURSOR_SHOWING == 0 {
            self.cancel();
            self.last_frame = None;
            return Ok(());
        }
        // Async button state repairs a lost release (device removal, desktop
        // switch); Raw Input supplies precise presses, without intercepting them.
        let down = [VK_LBUTTON, VK_RBUTTON, VK_MBUTTON, VK_XBUTTON1, VK_XBUTTON2]
            .iter()
            .enumerate()
            .fold(0u8, |mask, (n, key)| {
                mask | if unsafe { GetAsyncKeyState(*key as i32) } < 0 {
                    1 << n
                } else {
                    0
                }
            });
        if !self.armed && down == 0 {
            self.armed = true;
        }
        if self.armed && down != self.buttons {
            // Also supports pointing devices / remote sessions that do not
            // deliver Raw Input. Only the current-state high bit is used.
            let mut flags = 0u16;
            for n in 0..5 {
                if (down ^ self.buttons) & (1 << n) != 0 {
                    flags |= if down & (1 << n) != 0 {
                        1 << (n * 2)
                    } else {
                        2 << (n * 2)
                    };
                }
            }
            self.input(flags);
        }
        let dpi = monitor_dpi(info.ptScreenPos);
        if dpi != self.dpi {
            if let std::collections::btree_map::Entry::Vacant(entry) = self.banks.entry(dpi) {
                entry.insert(self.frames.bank(dpi)?);
            }
            self.dpi = dpi;
            self.last_frame = None;
            self.trail.clear();
            self.last_position = None;
        }
        let scale = f64::from(dpi) / 96.0;
        let x = f64::from(info.ptScreenPos.x) / scale;
        let y = f64::from(info.ptScreenPos.y) / scale;
        if self.buttons != 0
            && !self.double
            && !self.sounded
            && self
                .pressed_at
                .is_some_and(|at| now.saturating_sub(at) >= 300)
        {
            self.audio.press();
            self.sounded = true;
        }
        if self.buttons != 0 && self.settings.trail {
            self.trail.sample(now as f64 / 1000.0, x, y);
        }
        if let Some((at, old_x, old_y)) = self.last_position {
            let distance = (x - old_x).hypot(y - old_y);
            let dt = now.saturating_sub(at).max(1) as f64 / 1000.0;
            if self.buttons != 0
                && self.sounded
                && now.saturating_sub(self.last_swipe) >= 240
                && distance > 3.0
                && distance < 512.0
                && distance / dt > 400.0
            {
                self.audio.effect("swipe");
                self.last_swipe = now;
            }
        }
        self.last_position = Some((now, x, y));
        let frame = self
            .animation
            .running(now)
            .then(|| self.animation.frame(now, self.frames.layout));
        let key = (dpi, frame, self.settings.light);
        if self.last_frame != Some(key) {
            self.banks[&dpi].install(frame, self.settings.light)?;
            self.last_frame = Some(key);
        }
        if self.settings.trail {
            let viewport = unsafe {
                Bounds {
                    x: f64::from(GetSystemMetrics(SM_XVIRTUALSCREEN)) / scale,
                    y: f64::from(GetSystemMetrics(SM_YVIRTUALSCREEN)) / scale,
                    width: f64::from(GetSystemMetrics(SM_CXVIRTUALSCREEN)) / scale,
                    height: f64::from(GetSystemMetrics(SM_CYVIRTUALSCREEN)) / scale,
                }
            };
            if let Some(trail) =
                self.trail
                    .render(now as f64 / 1000.0, viewport, self.settings.light, scale)
            {
                self.layer.present(trail, scale)?;
            } else {
                self.layer.hide();
            }
        }
        Ok(())
    }
    fn set_color(&mut self, color: String) -> Result<(), String> {
        self.cancel();
        let root = color_frames(
            &self.frames_root,
            &self.settings_root,
            &self.palette,
            &color,
        )?;
        let frames = Frames::load(&root, &color)?;
        let mut banks = BTreeMap::new();
        for &dpi in self.banks.keys() {
            banks.insert(dpi, frames.bank(dpi)?);
        }
        self.frames = frames;
        self.banks = banks;
        self.settings.color = color;
        self.trail.set_color(&self.settings.color);
        if !self.paused && !self.suspended {
            self.enable()?;
        }
        self.settings.save(&self.settings_root)
    }
    fn command(&mut self, command: u32) -> Result<(), String> {
        match command {
            ID_PAUSE => {
                self.paused = !self.paused;
                self.cancel();
                if self.paused {
                    self.originals.restore()?;
                } else if !self.suspended {
                    self.enable()?;
                }
                self.tray(NIM_MODIFY)?;
            }
            ID_TRAIL => {
                self.settings.trail = !self.settings.trail;
                self.trail.clear();
                self.layer.hide();
            }
            ID_BANK => {
                self.settings.light = !self.settings.light;
                self.last_frame = None;
            }
            ID_ALL | ID_IGNITE | ID_MUTE => {
                self.settings.sound = match command {
                    ID_ALL => SoundMode::All,
                    ID_IGNITE => SoundMode::Ignite,
                    _ => SoundMode::Mute,
                };
                self.audio.set_mode(self.settings.sound);
            }
            ID_EXIT => unsafe {
                PostMessageW(self.hwnd, WM_CLOSE, 0, 0);
            },
            id if (ID_COLOR..ID_COLOR + self.palette.len() as u32).contains(&id) => {
                self.set_color(self.palette[(id - ID_COLOR) as usize].1.clone())?;
            }
            id if (ID_VOLUME..=ID_VOLUME + 10).contains(&id) => {
                self.settings.volume = ((id - ID_VOLUME) * 10) as u8;
                self.audio.set_volume(self.settings.volume);
            }
            _ => {}
        }
        self.settings.save(&self.settings_root)
    }
}
impl Drop for App {
    fn drop(&mut self) {
        if let Err(error) = self.stop() {
            log(&error);
        }
        unsafe {
            DestroyIcon(self.icon);
        }
    }
}

fn color_frames(
    root: &std::path::Path,
    settings: &std::path::Path,
    palette: &[(String, String)],
    color: &str,
) -> Result<PathBuf, String> {
    if let Some((name, _)) = palette
        .iter()
        .find(|(_, hex)| hex.eq_ignore_ascii_case(color))
    {
        return Ok(root.join(name));
    }
    // Versioned cache: the held-state glow change alters rendered frames, so a
    // v1 cache must not be reused for custom colors.
    let directory = settings.join("cache/windows-frames-v2").join(&color[1..]);
    if !directory.join("frames.txt").exists() {
        kyber_core::native::export(&directory, color, 48, 2)?;
    }
    Ok(directory)
}

pub fn show_error(error: &str) {
    log(error);
    unsafe {
        MessageBoxW(
            null_mut(),
            wide(error).as_ptr(),
            wide("Kyber Cursor").as_ptr(),
            MB_OK | MB_ICONERROR,
        );
    }
}
fn log(message: &str) {
    if let Ok(root) = kyber_core::paths::settings_dir() {
        let _ = fs::create_dir_all(&root);
        // A single bounded status file; never records clicks or application names.
        let _ = fs::write(root.join("windows-status.txt"), message);
    }
}

fn with_app(action: impl FnOnce(&mut App) -> Result<(), String>) {
    APP.with(|cell| {
        if let Ok(mut slot) = cell.try_borrow_mut() {
            if let Some(app) = slot.as_mut() {
                if let Err(error) = action(app) {
                    log(&error);
                    app.error = Some(error);
                    unsafe {
                        PostMessageW(app.hwnd, WM_CLOSE, 0, 0);
                    }
                }
            }
        }
    });
}

unsafe extern "system" fn proc(hwnd: HWND, message: u32, w: WPARAM, l: LPARAM) -> LRESULT {
    match message {
        WM_TIMER => with_app(App::tick),
        WM_INPUT => {
            let mut raw: RAWINPUT = zeroed();
            let mut bytes = size_of::<RAWINPUT>() as u32;
            if GetRawInputData(
                l as HRAWINPUT,
                RID_INPUT,
                (&mut raw as *mut RAWINPUT).cast(),
                &mut bytes,
                size_of::<RAWINPUTHEADER>() as u32,
            ) != u32::MAX
                && raw.header.dwType == RIM_TYPEMOUSE
            {
                let flags = raw.data.mouse.Anonymous.Anonymous.usButtonFlags;
                if flags != 0 {
                    with_app(|app| {
                        app.input(flags);
                        Ok(())
                    });
                }
            }
            return DefWindowProcW(hwnd, message, w, l);
        }
        WM_INPUT_DEVICE_CHANGE => with_app(|app| {
            app.cancel();
            app.last_frame = None;
            Ok(())
        }),
        WM_WTSSESSION_CHANGE => with_app(|app| {
            // Lock/disconnect/logoff all suspend; connect/unlock resumes on the
            // next timer only after an accessible input desktop is available.
            match w as u32 {
                2 | 4 | 6 | 7 => app.locked = true,
                1 | 3 | 5 | 8 => app.locked = false,
                _ => {}
            }
            app.health_at = 0;
            Ok(())
        }),
        WM_DISPLAYCHANGE => with_app(|app| {
            app.cancel();
            app.last_frame = None;
            Ok(())
        }),
        WM_SETTINGCHANGE if w as u32 == SPI_SETCURSORS => with_app(|app| {
            // Windows has loaded a new scheme. Save it before applying Kyber.
            app.originals = Originals::capture()?;
            app.last_frame = None;
            Ok(())
        }),
        TRAY => {
            if matches!(l as u32, WM_LBUTTONUP | WM_RBUTTONUP | WM_CONTEXTMENU) {
                PostMessageW(hwnd, MENU, 0, 0);
            }
        }
        MENU => menu(hwnd),
        CHOOSE => choose_color(hwnd),
        WM_COMMAND => {
            let command = (w & 0xffff) as u32;
            if command == ID_PICK {
                PostMessageW(hwnd, CHOOSE, 0, 0);
            } else {
                with_app(|app| app.command(command));
            }
        }
        WM_QUERYENDSESSION => {
            with_app(|app| {
                app.cancel();
                app.menu_open = true;
                app.originals.restore()
            });
            return 1;
        }
        WM_ENDSESSION => {
            if w != 0 {
                with_app(App::stop);
                PostQuitMessage(0);
            } else {
                with_app(|app| {
                    app.menu_open = false;
                    app.last_frame = None;
                    Ok(())
                });
            }
        }
        WM_CLOSE | RESTORE => {
            with_app(App::stop);
            DestroyWindow(hwnd);
            return 0;
        }
        WM_DESTROY => {
            KillTimer(hwnd, 1);
            WTSUnRegisterSessionNotification(hwnd);
            PostQuitMessage(0);
            return 0;
        }
        _ => {
            with_app(|app| {
                if message == app.taskbar_created {
                    app.tray(NIM_ADD)?;
                }
                Ok(())
            });
            return DefWindowProcW(hwnd, message, w, l);
        }
    }
    0
}

unsafe fn item(menu: HMENU, id: u32, label: &str, checked: bool) {
    AppendMenuW(
        menu,
        MF_STRING | if checked { MF_CHECKED } else { 0 },
        id as usize,
        wide(label).as_ptr(),
    );
}
unsafe fn submenu(menu: HMENU, child: HMENU, label: &str) {
    AppendMenuW(menu, MF_POPUP, child as usize, wide(label).as_ptr());
}
unsafe fn menu(hwnd: HWND) {
    let snapshot = APP.with(|cell| {
        cell.borrow().as_ref().map(|a| {
            (
                a.settings.clone(),
                a.palette.clone(),
                a.paused,
                a.audio.available(),
            )
        })
    });
    let Some((settings, palette, paused, audio)) = snapshot else {
        return;
    };
    let menu = CreatePopupMenu();
    if menu.is_null() {
        return;
    }
    with_app(|app| {
        app.menu_open = true;
        app.cancel();
        if !app.paused && !app.suspended {
            app.enable()?;
        }
        Ok(())
    });
    item(
        menu,
        ID_PAUSE,
        if paused {
            "Enable Kyber"
        } else {
            "Pause — normal Windows pointer"
        },
        false,
    );
    let colors = CreatePopupMenu();
    for (n, (name, hex)) in palette.iter().enumerate() {
        item(
            colors,
            ID_COLOR + n as u32,
            name,
            settings.color.eq_ignore_ascii_case(hex),
        );
    }
    item(colors, ID_PICK, "Choose another color…", false);
    submenu(menu, colors, "Blade color");
    item(menu, ID_TRAIL, "Fading trail", settings.trail);
    item(
        menu,
        ID_BANK,
        "Gentler glow for light backgrounds",
        settings.light,
    );
    let sounds = CreatePopupMenu();
    item(
        sounds,
        ID_ALL,
        "All sounds",
        settings.sound == SoundMode::All,
    );
    item(
        sounds,
        ID_IGNITE,
        "Power-up only",
        settings.sound == SoundMode::Ignite,
    );
    item(sounds, ID_MUTE, "Mute", settings.sound == SoundMode::Mute);
    submenu(
        menu,
        sounds,
        if audio {
            "Sounds"
        } else {
            "Sounds (no audio device available)"
        },
    );
    let volume = CreatePopupMenu();
    for n in 0..=10 {
        item(
            volume,
            ID_VOLUME + n,
            &format!("{}%", n * 10),
            settings.volume == (n * 10) as u8,
        );
    }
    submenu(menu, volume, &format!("Volume: {}%", settings.volume));
    AppendMenuW(menu, MF_SEPARATOR, 0, null());
    item(menu, ID_EXIT, "Exit and restore Windows pointer", false);
    let mut point: POINT = zeroed();
    GetCursorPos(&mut point);
    SetForegroundWindow(hwnd);
    let command = TrackPopupMenu(
        menu,
        TPM_RETURNCMD | TPM_NONOTIFY | TPM_RIGHTBUTTON,
        point.x,
        point.y,
        0,
        hwnd,
        null(),
    );
    DestroyMenu(menu);
    PostMessageW(hwnd, WM_NULL, 0, 0);
    with_app(|app| {
        app.menu_open = false;
        app.cancel();
        app.last_frame = None;
        Ok(())
    });
    if command != 0 {
        PostMessageW(hwnd, WM_COMMAND, command as WPARAM, 0);
    }
}
unsafe fn choose_color(hwnd: HWND) {
    let current = APP
        .with(|cell| cell.borrow().as_ref().map(|a| a.settings.color.clone()))
        .unwrap_or_else(|| "#07d815".into());
    let rgb = u32::from_str_radix(&current[1..], 16).unwrap_or(0x07d815);
    let mut custom = [0u32; 16];
    let mut picker = CHOOSECOLORW {
        lStructSize: size_of::<CHOOSECOLORW>() as u32,
        hwndOwner: hwnd,
        rgbResult: (rgb >> 16) | (rgb & 0xff00) | ((rgb & 0xff) << 16),
        lpCustColors: custom.as_mut_ptr(),
        Flags: CC_FULLOPEN | CC_RGBINIT,
        ..zeroed()
    };
    with_app(|app| {
        app.menu_open = true;
        app.cancel();
        if !app.paused && !app.suspended {
            app.enable()?;
        }
        Ok(())
    });
    let accepted = ChooseColorW(&mut picker) != 0;
    with_app(|app| {
        app.menu_open = false;
        if accepted {
            let c = picker.rgbResult;
            app.set_color(format!(
                "#{:02x}{:02x}{:02x}",
                c & 255,
                (c >> 8) & 255,
                (c >> 16) & 255
            ))?;
        }
        app.cancel();
        app.last_frame = None;
        Ok(())
    });
}

fn restore_running() -> Result<(), String> {
    unsafe {
        let hwnd = FindWindowW(wide(CLASS).as_ptr(), null());
        if !hwnd.is_null() {
            let mut result = 0;
            if SendMessageTimeoutW(
                hwnd,
                RESTORE,
                0,
                0,
                SMTO_ABORTIFHUNG | SMTO_BLOCK,
                5000,
                &mut result,
            ) == 0
            {
                return Err("Kyber is not responding. End kyber-cursor.exe in Task Manager, then run Restore Windows Cursor again.".into());
            }
        }
    }
    cursor::restore_scheme()
}

pub fn run() -> Result<(), String> {
    if std::env::args_os().any(|a| a == "--restore") {
        return restore_running();
    }
    if std::env::args_os().len() != 1 {
        return Err("Start kyber-cursor.exe without arguments, or use --restore.".into());
    }
    unsafe {
        SetProcessDpiAwarenessContext(DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2);
        let mutex = CreateMutexW(null(), 0, wide("Local\\KyberCursorDesktopV1").as_ptr());
        if mutex.is_null() {
            return Err(failed("Create Kyber instance lock"));
        }
        let already = GetLastError() == ERROR_ALREADY_EXISTS;
        let _mutex = Handle(mutex);
        if already {
            let hwnd = FindWindowW(wide(CLASS).as_ptr(), null());
            if !hwnd.is_null() {
                PostMessageW(hwnd, MENU, 0, 0);
            }
            return Ok(());
        }
        let instance = GetModuleHandleW(null());
        let class = wide(CLASS);
        let wc = WNDCLASSW {
            lpfnWndProc: Some(proc),
            hInstance: instance,
            lpszClassName: class.as_ptr(),
            ..zeroed()
        };
        if RegisterClassW(&wc) == 0 {
            return Err(failed("Register Kyber controller"));
        }
        let hwnd = CreateWindowExW(
            WS_EX_TOOLWINDOW | WS_EX_NOACTIVATE,
            class.as_ptr(),
            wide("Kyber Cursor").as_ptr(),
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
            return Err(failed("Create Kyber controller"));
        }
        let mut app = match App::new(hwnd) {
            Ok(app) => app,
            Err(e) => {
                DestroyWindow(hwnd);
                return Err(e);
            }
        };
        // Only mouse Raw Input is requested; never suppress legacy input or
        // install a keyboard/mouse hook in another application.
        let raw = RAWINPUTDEVICE {
            usUsagePage: 1,
            usUsage: 2,
            dwFlags: RIDEV_INPUTSINK | RIDEV_DEVNOTIFY,
            hwndTarget: hwnd,
        };
        if RegisterRawInputDevices(&raw, 1, size_of::<RAWINPUTDEVICE>() as u32) == 0 {
            return Err(failed("Listen for mouse buttons"));
        }
        if WTSRegisterSessionNotification(hwnd, NOTIFY_FOR_THIS_SESSION) == 0 {
            return Err(failed("Listen for Windows lock/unlock"));
        }
        app.tray(NIM_ADD)?;
        app.enable()?;
        app.settings.save(&app.settings_root)?;
        let audio = app.audio.available();
        APP.with(|cell| *cell.borrow_mut() = Some(app));
        if SetTimer(hwnd, 1, 8, None) == 0 {
            with_app(App::stop);
            return Err(failed("Start Kyber animation timer"));
        }
        log(if audio {
            "Kyber desktop running. Native Windows listening and visual approval pending."
        } else {
            "Kyber desktop running without audio: no usable output device. Restart Kyber after connecting one."
        });
        let mut message: MSG = zeroed();
        let mut message_error = None;
        loop {
            let result = GetMessageW(&mut message, null_mut(), 0, 0);
            if result == -1 {
                message_error = Some(failed("Read Windows messages"));
                break;
            }
            if result == 0 {
                break;
            }
            TranslateMessage(&message);
            DispatchMessageW(&message);
        }
        let mut app = APP
            .with(|cell| cell.borrow_mut().take())
            .ok_or("Missing Kyber state")?;
        app.stop()?;
        if let Some(error) = app.error.take().or(message_error) {
            return Err(error);
        }
        Ok(())
    }
}
