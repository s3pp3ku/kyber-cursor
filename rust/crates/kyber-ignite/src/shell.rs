//! Layer-shell overlay window, tiny-skia frame renderer, and the daemon
//! main loop. Replaces the GTK4 overlay + `grim` + `pw-play` half of
//! `lib/ignite-listener`; the evdev half lives in `input.rs`, the timing
//! logic in `state.rs`.
//!
//! The window is fullscreen-per-monitor, transparent, with an empty input
//! region (clicks pass through), like the Python's OVERLAY layer.

use std::sync::mpsc::RecvTimeoutError;
use std::time::{Duration, Instant};

use kyber_core::config::SmallRng;
use kyber_core::overlay::{LIGHT_HI, LIGHT_LO};
use kyber_core::render::AimCursor;
use kyber_core::trail::{Bounds, Trail, TrailImage, TrailLook};

use crate::aim::AimRuntime;
use crate::art::{selected_art, Art, LayerKind};
use crate::audio::{self, Player};
use crate::cursor;
use crate::input::{self, ButtonEvent};
use crate::shot;
use crate::state::{DownAction, Engine, TrailPt};

// ---------------------------------------------------------------- wayland

#[cfg(target_os = "linux")]
mod wl {
    use super::*;
    use sctk::compositor::{CompositorHandler, CompositorState};
    use sctk::output::{OutputHandler, OutputState};
    use sctk::reexports::protocols::wp::linux_dmabuf::zv1::client::{
        zwp_linux_buffer_params_v1, zwp_linux_dmabuf_v1,
    };
    use sctk::registry::{ProvidesRegistryState, RegistryState};
    use sctk::reexports::protocols::xdg::shell::client::xdg_positioner::{
        Anchor as PopupAnchor, Gravity as PopupGravity,
    };
    use sctk::shell::wlr_layer::{
        Anchor, KeyboardInteractivity, Layer, LayerShell, LayerShellHandler, LayerSurface,
        LayerSurfaceConfigure,
    };
    use sctk::shell::xdg::popup::{Popup, PopupConfigure, PopupHandler};
    use sctk::shell::xdg::window::{Window, WindowConfigure, WindowHandler};
    use sctk::shell::xdg::{XdgPositioner, XdgShell};
    use sctk::shm::slot::{Buffer as ShmBuffer, SlotPool};
    use sctk::shm::{Shm, ShmHandler};
    use std::os::fd::AsRawFd;
    use wayland_client::protocol::{
        wl_buffer::WlBuffer, wl_output, wl_region::WlRegion, wl_surface::WlSurface,
    };
    use wayland_client::{Connection, Dispatch, EventQueue, Proxy, QueueHandle};

    use crate::dmabuf::{DmaAlloc, DRM_FORMAT_ARGB8888};

    /// Release-tracking tag for imported buffers.
    #[derive(Clone, Debug)]
    struct BufTag(usize);

    struct DmaSlot {
        alloc: DmaAlloc,
        buffer: Option<WlBuffer>,
        busy: bool,
    }

    pub struct App {
        registry_state: RegistryState,
        compositor: CompositorState,
        layer_shell: LayerShell,
        outputs: OutputState,
        dmabuf: zwp_linux_dmabuf_v1::ZwpLinuxDmabufV1,
        surface: Option<WlSurface>,
        layer_surface: Option<LayerSurface>,
        input_region: Option<WlRegion>,
        /// For the popup overlay: xdg-shell, and the 1x1 transparent buffer
        /// that maps the layer surface the popup hangs off.
        xdg: Option<XdgShell>,
        shm: Shm,
        pool: Option<SlotPool>,
        parent_buf: Option<ShmBuffer>,
        popup: Option<Popup>,
        popup_ready: bool,
        slots: Vec<DmaSlot>,
        slot_w: u32,
        slot_h: u32,
        surf_w: u32,
        surf_h: u32,
        pub prev_dirty: Option<(i32, i32, i32, i32)>,
    }

    impl ProvidesRegistryState for App {
        fn registry(&mut self) -> &mut RegistryState {
            &mut self.registry_state
        }
        fn runtime_add_global(
            &mut self,
            _conn: &Connection,
            _qh: &QueueHandle<Self>,
            _name: u32,
            _interface: &str,
            _version: u32,
        ) {
        }
        fn runtime_remove_global(
            &mut self,
            _conn: &Connection,
            _qh: &QueueHandle<Self>,
            _name: u32,
            _interface: &str,
        ) {
        }
    }

    impl CompositorHandler for App {
        fn scale_factor_changed(
            &mut self,
            _conn: &Connection,
            _qh: &QueueHandle<Self>,
            _surface: &WlSurface,
            _new_factor: i32,
        ) {
        }
        fn transform_changed(
            &mut self,
            _conn: &Connection,
            _qh: &QueueHandle<Self>,
            _surface: &WlSurface,
            _new_transform: wl_output::Transform,
        ) {
        }
        fn frame(
            &mut self,
            _conn: &Connection,
            _qh: &QueueHandle<Self>,
            _surface: &WlSurface,
            _time: u32,
        ) {
        }
        fn surface_enter(
            &mut self,
            _conn: &Connection,
            _qh: &QueueHandle<Self>,
            _surface: &WlSurface,
            _output: &wl_output::WlOutput,
        ) {
        }
        fn surface_leave(
            &mut self,
            _conn: &Connection,
            _qh: &QueueHandle<Self>,
            _surface: &WlSurface,
            _output: &wl_output::WlOutput,
        ) {
        }
    }

    impl OutputHandler for App {
        fn output_state(&mut self) -> &mut OutputState {
            &mut self.outputs
        }
        fn new_output(
            &mut self,
            _conn: &Connection,
            _qh: &QueueHandle<Self>,
            _output: wl_output::WlOutput,
        ) {
        }
        fn update_output(
            &mut self,
            _conn: &Connection,
            _qh: &QueueHandle<Self>,
            _output: wl_output::WlOutput,
        ) {
        }
        fn output_destroyed(
            &mut self,
            _conn: &Connection,
            _qh: &QueueHandle<Self>,
            _output: wl_output::WlOutput,
        ) {
        }
    }

    impl Dispatch<zwp_linux_dmabuf_v1::ZwpLinuxDmabufV1, ()> for App {
        fn event(
            _state: &mut Self,
            _: &zwp_linux_dmabuf_v1::ZwpLinuxDmabufV1,
            _: <zwp_linux_dmabuf_v1::ZwpLinuxDmabufV1 as Proxy>::Event,
            _: &(),
            _: &Connection,
            _: &QueueHandle<Self>,
        ) {
        }
    }

    impl Dispatch<zwp_linux_buffer_params_v1::ZwpLinuxBufferParamsV1, ()> for App {
        fn event(
            _state: &mut Self,
            _: &zwp_linux_buffer_params_v1::ZwpLinuxBufferParamsV1,
            event: <zwp_linux_buffer_params_v1::ZwpLinuxBufferParamsV1 as Proxy>::Event,
            _: &(),
            _: &Connection,
            _: &QueueHandle<Self>,
        ) {
            use zwp_linux_buffer_params_v1::Event as E;
            match event {
                E::Created { .. } => {}
                E::Failed => {
                    eprintln!("kyber-ignite: dmabuf import failed (format/modifier rejected?)")
                }
                _ => {}
            }
        }
    }

    impl Dispatch<WlBuffer, BufTag> for App {
        fn event(
            state: &mut Self,
            _: &WlBuffer,
            event: <WlBuffer as Proxy>::Event,
            tag: &BufTag,
            _: &Connection,
            _: &QueueHandle<Self>,
        ) {
            if matches!(event, wayland_client::protocol::wl_buffer::Event::Release) {
                if let Some(slot) = state.slots.get_mut(tag.0) {
                    slot.busy = false;
                }
            }
        }
    }

    impl LayerShellHandler for App {
        fn closed(&mut self, _conn: &Connection, _qh: &QueueHandle<Self>, _layer: &LayerSurface) {}
        fn configure(
            &mut self,
            _conn: &Connection,
            _qh: &QueueHandle<Self>,
            _layer: &LayerSurface,
            _configure: LayerSurfaceConfigure,
            _serial: u32,
        ) {
            // sctk already acked the configure on the protocol object
            // before calling us; our size was set explicitly in show().
        }
    }

    impl ShmHandler for App {
        fn shm_state(&mut self) -> &mut Shm {
            &mut self.shm
        }
    }

    // The xdg-shell delegate also covers toplevels, which this overlay never
    // creates.
    impl WindowHandler for App {
        fn request_close(&mut self, _: &Connection, _: &QueueHandle<Self>, _: &Window) {}
        fn configure(
            &mut self,
            _: &Connection,
            _: &QueueHandle<Self>,
            _: &Window,
            _: WindowConfigure,
            _: u32,
        ) {
        }
    }

    impl PopupHandler for App {
        fn configure(
            &mut self,
            _: &Connection,
            _: &QueueHandle<Self>,
            _: &Popup,
            _: PopupConfigure,
        ) {
            // sctk acked it; the first buffer may be attached from here on.
            self.popup_ready = true;
        }
        fn done(&mut self, _: &Connection, _: &QueueHandle<Self>, _: &Popup) {
            // The compositor dismissed it: nothing is on screen now, and
            // `is_shown` says so, so the daemon maps a fresh one.
            self.popup = None;
            self.surface = None;
            self.popup_ready = false;
        }
    }

    sctk::delegate_compositor!(App);
    sctk::delegate_shm!(App);
    sctk::delegate_xdg_shell!(App);
    sctk::delegate_xdg_popup!(App);
    sctk::delegate_output!(App);
    sctk::delegate_layer!(App);
    sctk::delegate_registry!(App);

    // WlRegion carries no events; the manual Dispatch only satisfies
    // create_region's bound.
    impl Dispatch<WlRegion, ()> for App {
        fn event(
            _state: &mut Self,
            _: &WlRegion,
            _: <WlRegion as Proxy>::Event,
            _: &(),
            _: &Connection,
            _: &QueueHandle<Self>,
        ) {
        }
    }

    pub struct Overlay {
        pub app: App,
        pub qh: QueueHandle<App>,
        pub queue: EventQueue<App>,
    }

    impl Overlay {
        pub fn connect() -> Result<Self, String> {
            let conn =
                Connection::connect_to_env().map_err(|e| format!("no Wayland display ({e})"))?;
            let (globals, queue) = wayland_client::globals::registry_queue_init(&conn)
                .map_err(|e| format!("registry ({e})"))?;
            let qh = queue.handle();
            let mut app = App {
                registry_state: RegistryState::new(&globals),
                compositor: CompositorState::bind(&globals, &qh)
                    .map_err(|e| format!("no compositor ({e:?})"))?,
                layer_shell: LayerShell::bind(&globals, &qh).map_err(|_| {
                    "no wlr-layer-shell (GNOME and some proprietary compositors lack it)"
                        .to_string()
                })?,
                outputs: OutputState::new(&globals, &qh),
                dmabuf: globals
                    .bind::<zwp_linux_dmabuf_v1::ZwpLinuxDmabufV1, App, ()>(&qh, 1..=4, ())
                    .map_err(|e| format!("no linux-dmabuf ({e:?})"))?,
                surface: None,
                layer_surface: None,
                input_region: None,
                xdg: XdgShell::bind(&globals, &qh).ok(),
                shm: Shm::bind(&globals, &qh).map_err(|e| format!("no wl_shm ({e:?})"))?,
                pool: None,
                parent_buf: None,
                popup: None,
                popup_ready: false,
                slots: Vec::new(),
                slot_w: 0,
                slot_h: 0,
                surf_w: 0,
                surf_h: 0,
                prev_dirty: None,
            };
            // Pull the initial globals (outputs, …) before first use.
            let mut queue = queue;
            queue
                .roundtrip(&mut app)
                .map_err(|e| format!("roundtrip ({e})"))?;
            Ok(Self { app, qh, queue })
        }

        pub(crate) fn output_by_name(&mut self, name: &str) -> Option<wl_output::WlOutput> {
            // Refresh the output list; fall back to the first output.
            let _ = self.queue.roundtrip(&mut self.app);
            let outs: Vec<_> = self.app.outputs.outputs().collect();
            outs.iter()
                .find(|o| {
                    self.app
                        .outputs
                        .info(o)
                        .and_then(|i| i.name.clone())
                        .as_deref()
                        == Some(name)
                })
                .cloned()
                .or_else(|| outs.into_iter().next())
        }

        /// Map + size the overlay on `output`. Logical size, buffer scale up.
        ///
        /// `above_popups` draws it in an xdg-popup of the layer surface
        /// instead of on the surface itself. Hyprland draws popups after every
        /// layer, so a bar popup (Jarvis, a drawer) covers anything drawn
        /// straight on an overlay layer, but not another popup. If the popup
        /// cannot be made this falls back to the plain layer surface.
        pub fn show(
            &mut self,
            output: &wl_output::WlOutput,
            w: u32,
            h: u32,
            buf_scale: i32,
            above_popups: bool,
        ) -> Result<(), String> {
            if above_popups {
                match self.show_in_popup(output, w, h, buf_scale) {
                    Ok(()) => return Ok(()),
                    Err(e) => {
                        eprintln!(
                            "kyber-ignite: popup overlay unavailable ({e}); using a plain layer surface"
                        );
                    }
                }
            }
            self.hide();
            let surface = self.app.compositor.create_surface(&self.qh);
            let layer = self.app.layer_shell.create_layer_surface(
                &self.qh,
                surface.clone(),
                Layer::Overlay,
                Some("cursor-ignite".to_string()),
                Some(output),
            );
            layer.set_anchor(Anchor::TOP | Anchor::BOTTOM | Anchor::LEFT | Anchor::RIGHT);
            layer.set_exclusive_zone(-1);
            layer.set_keyboard_interactivity(KeyboardInteractivity::None);
            layer.set_size(w, h);
            surface.set_buffer_scale(buf_scale);
            // Empty input region: clicks always pass through. The region
            // object must outlive the surface, so keep it in App.
            let region = self
                .app
                .compositor
                .wl_compositor()
                .create_region(&self.qh, ());
            surface.set_input_region(Some(&region));
            self.app.input_region = Some(region);
            surface.commit();
            self.app.surface = Some(surface);
            self.app.layer_surface = Some(layer);
            self.app.surf_w = w;
            self.app.surf_h = h;
            self.app.slots.clear();
            self.app.slot_w = 0;
            self.app.slot_h = 0;
            self.app.prev_dirty = None;
            // A layer surface may not receive a buffer until its initial
            // configure has been acknowledged. `roundtrip` processes that
            // configure (the SCTK handler acks it) before draw() can attach.
            self.queue
                .roundtrip(&mut self.app)
                .map_err(|e| format!("initial layer configure ({e})"))?;
            Ok(())
        }

        /// The popup form of `show`: a 1x1 transparent layer surface (the
        /// parent has to be mapped) and, hung off it, a popup the size of the
        /// monitor at its origin. The cursor is drawn in the popup.
        fn show_in_popup(
            &mut self,
            output: &wl_output::WlOutput,
            w: u32,
            h: u32,
            buf_scale: i32,
        ) -> Result<(), String> {
            if self.app.xdg.is_none() {
                return Err("no xdg-shell".into());
            }
            self.hide();
            let parent = self.app.compositor.create_surface(&self.qh);
            let layer = self.app.layer_shell.create_layer_surface(
                &self.qh,
                parent.clone(),
                Layer::Overlay,
                Some("cursor-ignite".to_string()),
                Some(output),
            );
            layer.set_anchor(Anchor::TOP | Anchor::LEFT);
            layer.set_exclusive_zone(-1);
            layer.set_keyboard_interactivity(KeyboardInteractivity::None);
            layer.set_size(1, 1);
            // Empty input region on both surfaces: clicks always pass through.
            let region = self
                .app
                .compositor
                .wl_compositor()
                .create_region(&self.qh, ());
            parent.set_input_region(Some(&region));
            parent.commit();
            self.app.input_region = Some(region);
            self.app.layer_surface = Some(layer);
            self.queue
                .roundtrip(&mut self.app)
                .map_err(|e| format!("layer configure ({e})"))?;
            // Map the parent with a transparent pixel.
            if self.app.pool.is_none() {
                self.app.pool = Some(
                    SlotPool::new(4096, &self.app.shm).map_err(|e| format!("shm pool ({e})"))?,
                );
            }
            let pool = self.app.pool.as_mut().expect("pool");
            let (buffer, canvas) = pool
                .create_buffer(1, 1, 4, wayland_client::protocol::wl_shm::Format::Argb8888)
                .map_err(|e| format!("shm buffer ({e})"))?;
            canvas.fill(0);
            buffer.attach_to(&parent).map_err(|e| format!("attach ({e})"))?;
            parent.damage_buffer(0, 0, 1, 1);
            parent.commit();
            self.app.parent_buf = Some(buffer);
            // The popup, covering the monitor from its origin.
            let positioner = {
                let xdg = self.app.xdg.as_ref().ok_or("no xdg-shell")?;
                let positioner = XdgPositioner::new(xdg).map_err(|e| format!("positioner ({e})"))?;
                positioner.set_size(w as i32, h as i32);
                positioner.set_anchor_rect(0, 0, 1, 1);
                positioner.set_anchor(PopupAnchor::TopLeft);
                positioner.set_gravity(PopupGravity::BottomRight);
                positioner
            };
            let surface = self.app.compositor.create_surface(&self.qh);
            let popup = {
                let xdg = self.app.xdg.as_ref().ok_or("no xdg-shell")?;
                Popup::from_surface(None, &positioner, &self.qh, surface.clone(), xdg)
                    .map_err(|e| format!("popup ({e})"))?
            };
            self.app
                .layer_surface
                .as_ref()
                .ok_or("layer surface gone")?
                .get_popup(popup.xdg_popup());
            surface.set_buffer_scale(buf_scale);
            if let Some(region) = &self.app.input_region {
                surface.set_input_region(Some(region));
            }
            surface.commit();
            self.app.popup = Some(popup);
            self.app.popup_ready = false;
            for _ in 0..5 {
                self.queue
                    .roundtrip(&mut self.app)
                    .map_err(|e| format!("popup configure ({e})"))?;
                if self.app.popup_ready {
                    break;
                }
            }
            if !self.app.popup_ready {
                return Err("popup was never configured".into());
            }
            self.app.surface = Some(surface);
            self.app.surf_w = w;
            self.app.surf_h = h;
            self.app.slots.clear();
            self.app.slot_w = 0;
            self.app.slot_h = 0;
            self.app.prev_dirty = None;
            Ok(())
        }

        /// Is the overlay currently presented on some output?
        pub fn is_shown(&self) -> bool {
            self.app.surface.is_some()
        }

        pub fn hide(&mut self) {
            // The popup goes before its parent.
            self.app.popup = None;
            self.app.popup_ready = false;
            self.app.parent_buf = None;
            self.app.surface = None;
            self.app.layer_surface = None;
            self.app.input_region = None;
            self.app.slots.clear();
            self.app.slot_w = 0;
            self.app.slot_h = 0;
            self.app.prev_dirty = None;
            // Dropping the layer/surface only queues their Wayland destroy
            // requests.  The daemon can sleep for five seconds immediately
            // after this, so flush now; otherwise the compositor may keep the
            // final glow buffer visible until the next input event.
            let _ = self.queue.flush();
        }

        /// Upload one straight-RGBA frame and damage `dirty` (plus the
        /// previous frame's dirty rect, to clear movement trails).
        pub fn draw(
            &mut self,
            rgba: &[u8],
            w: u32,
            h: u32,
            dirty: (i32, i32, i32, i32),
        ) -> Result<(), String> {
            // Read release events before selecting a slot. If the compositor
            // still owns all three, skip this frame instead of rewriting a
            // buffer it may currently be scanning out.
            self.poll(Duration::ZERO);
            let Some(surface) = self.app.surface.clone() else {
                return Err("not shown".into());
            };
            // (Re)allocate the triple-buffer pool when the size changes.
            if self.app.slot_w != w || self.app.slot_h != h {
                self.app.slots.clear();
                for _ in 0..3 {
                    let alloc =
                        DmaAlloc::new(w, h).map_err(|e| format!("GBM buffer {w}x{h} ({e})"))?;
                    self.app.slots.push(DmaSlot {
                        alloc,
                        buffer: None,
                        busy: false,
                    });
                }
                self.app.slot_w = w;
                self.app.slot_h = h;
            }
            let Some(idx) = self.app.slots.iter().position(|s| !s.busy) else {
                return Ok(());
            };
            let slot = &mut self.app.slots[idx];
            let stride = slot.alloc.stride();
            slot.alloc
                .write_rgba(rgba)
                .map_err(|e| format!("map GBM buffer ({e})"))?;
            if slot.buffer.is_none() {
                let modifier = slot.alloc.modifier();
                let params = self.app.dmabuf.create_params(&self.qh, ());
                params.add(
                    slot.alloc.dma_fd(),
                    0,
                    0,
                    stride as u32,
                    (modifier >> 32) as u32,
                    (modifier & 0xffff_ffff) as u32,
                );
                let buffer = params.create_immed(
                    w as i32,
                    h as i32,
                    DRM_FORMAT_ARGB8888,
                    zwp_linux_buffer_params_v1::Flags::empty(),
                    &self.qh,
                    BufTag(idx),
                );
                params.destroy();
                slot.buffer = Some(buffer);
            }
            let buf = slot.buffer.clone().expect("imported");
            slot.busy = true;
            surface.attach(Some(&buf), 0, 0);
            // Damage union of this frame and the last, so old pixels clear.
            let (x, y, dw, dh) = dirty;
            let (x, y, dw, dh) = match self.app.prev_dirty {
                Some((px, py, pw, ph)) => (
                    x.min(px),
                    y.min(py),
                    (x + dw).max(px + pw) - x.min(px),
                    (y + dh).max(py + ph) - y.min(py),
                ),
                None => (x, y, dw, dh),
            };
            self.app.prev_dirty = Some(dirty);
            surface.damage_buffer(x, y, dw, dh);
            surface.commit();
            // Submit now rather than after the next input/poll iteration.
            self.queue
                .flush()
                .map_err(|e| format!("submit frame ({e})"))?;
            Ok(())
        }

        pub fn poll(&mut self, timeout: Duration) {
            // Dispatch already-buffered events, then prepare the Wayland read
            // before polling its fd (the ordering required by wayland-client).
            if let Err(e) = self.queue.dispatch_pending(&mut self.app) {
                eprintln!("kyber-ignite: wayland dispatch ({e:?})");
            }
            let _ = self.queue.flush();
            let Some(guard) = self.queue.prepare_read() else {
                return;
            };
            let mut pfd = libc::pollfd {
                fd: guard.connection_fd().as_raw_fd(),
                events: libc::POLLIN,
                revents: 0,
            };
            let millis = timeout.as_millis().min(i32::MAX as u128) as i32;
            // SAFETY: `pfd` points to one initialized pollfd for this call.
            let ready = unsafe { libc::poll(&mut pfd, 1, millis) };
            if ready > 0 && pfd.revents & libc::POLLIN != 0 {
                if let Err(e) = guard.read() {
                    eprintln!("kyber-ignite: wayland read ({e:?})");
                }
                let _ = self.queue.dispatch_pending(&mut self.app);
            }
        }
    }
}

// ------------------------------------------------------------ rendering

/// One frame: premultiplied RGBA at device size + dirty rect (device px).
pub struct Frame {
    pub pixels: Vec<u8>,
    pub w: u32,
    pub h: u32,
    pub dirty: (i32, i32, i32, i32),
}

pub struct FrameInput<'a> {
    pub art: &'a Art,
    pub e: f32,
    pub hum_haze: f32,
    pub hum_beam: f32,
    pub light: f32,
    /// Logical px per overlay viewBox unit.
    pub f: f64,
    pub bs: f64,
    pub mon_x: f64,
    pub mon_y: f64,
    pub pos_x: f64,
    pub pos_y: f64,
    /// Movement-trail points (x, y, time) shared from `Engine.trail`.
    pub trail: &'a [TrailPt],
    /// The drag-trail look (path style and sparks).
    pub look: TrailLook,
    /// Aim mode: the baked cursor body to draw turned about the tip.
    ///
    /// `None` is the unchanged behaviour: only the glow is ever drawn, and the
    /// compositor's own pointer stays on screen.
    pub aim: Option<&'a AimCursor>,
    /// Aim angle in degrees (0 = right, 90 = down).
    pub aim_heading: f64,
    /// Reflect the artwork across the pointer axis this frame.
    pub aim_mirror: bool,
    pub surf_w: u32,
    pub surf_h: u32,
}

thread_local! {
    static OVERLAY_TRAIL: std::cell::RefCell<Trail> = std::cell::RefCell::new(Trail::default());
}

pub fn render_frame(fr: &FrameInput, now: f64) -> Frame {
    let dw = (fr.surf_w as f64 * fr.bs).round() as u32;
    let dh = (fr.surf_h as f64 * fr.bs).round() as u32;
    let mut px = tiny_skia::Pixmap::new(dw, dh).expect("frame pixmap");
    let meta = &fr.art.meta;
    let ppu = meta.px_per_unit as f64;
    let s = (fr.f * fr.bs / ppu) as f32;
    let ox = ((fr.pos_x - fr.mon_x - (meta.hotspot[0] - meta.origin[0]) * fr.f) * fr.bs) as f32;
    let oy = ((fr.pos_y - fr.mon_y - (meta.hotspot[1] - meta.origin[1]) * fr.f) * fr.bs) as f32;
    let (lw, lh) = fr
        .art
        .dark
        .first()
        .map(|l| (l.px.width(), l.px.height()))
        .unwrap_or((0, 0));
    let img_w = (lw as f64 * s as f64) as i32;
    let img_h = (lh as f64 * s as f64) as i32;
    let mut dirty = (ox as i32, oy as i32, img_w, img_h);

    // Aim adds the whole cursor body, which is not the bloom: it has a tip
    // (the hotspot) and a reach of its own, and it turns, so its footprint is
    // the bounding circle of the turned glyph, not the glow's rectangle.
    let aim_scale = fr
        .aim
        .map(|aim| fr.f * fr.bs / aim.ppu())
        .unwrap_or(1.0);
    let aim_cursor = (
        (fr.pos_x - fr.mon_x) * fr.bs,
        (fr.pos_y - fr.mon_y) * fr.bs,
    );
    if let Some(aim) = fr.aim {
        // Round up: the damage must cover the anti-aliased edge too.
        let reach = (aim.radius() * aim_scale).ceil() as i32 + 1;
        let (cx, cy) = (aim_cursor.0.round() as i32, aim_cursor.1.round() as i32);
        dirty = union_rect(dirty, (cx - reach, cy - reach, 2 * reach, 2 * reach));
    }

    // With aim on, the glow turns with the blade: same transform as the body,
    // about the hotspot. Its footprint is the circle through the bitmap's
    // farthest corner, so the damage rect covers every heading.
    let glow_tip = (
        (meta.hotspot[0] - meta.origin[0]) * ppu,
        (meta.hotspot[1] - meta.origin[1]) * ppu,
    );
    let glow_turn = fr.aim.map(|_| {
        let scale = s as f64;
        let m = kyber_core::aim::glyph_transform(
            fr.aim_heading,
            fr.aim_mirror,
            (glow_tip.0 * scale, glow_tip.1 * scale),
            aim_cursor,
        )
        .then(kyber_core::aim::Mat::scale(scale, scale));
        let [a, b, c, d, e, f] = m.row();
        tiny_skia::Transform::from_row(a, b, c, d, e, f)
    });
    if glow_turn.is_some() {
        let far = [(0.0, 0.0), (lw as f64, 0.0), (0.0, lh as f64), (lw as f64, lh as f64)]
            .iter()
            .map(|c| (c.0 - glow_tip.0).hypot(c.1 - glow_tip.1))
            .fold(0.0, f64::max);
        let reach = (far * s as f64).ceil() as i32 + 1;
        let (cx, cy) = (aim_cursor.0.round() as i32, aim_cursor.1.round() as i32);
        dirty = union_rect(dirty, (cx - reach, cy - reach, 2 * reach, 2 * reach));
    }

    // Drag trail, under the glow. One `Trail` lives across frames so its
    // sparks keep flying between them; only points newer than the last frame
    // are fed in, and it wears this blade's trail color.
    OVERLAY_TRAIL.with_borrow_mut(|trail| {
        trail.set_look(fr.look);
        trail.set_trail_rgb(meta.trail_rgb);
        let newest = trail.newest_time();
        if fr.trail.is_empty() {
            trail.break_path();
        }
        for pt in fr.trail.iter().filter(|pt| newest.is_none_or(|n| pt.t > n)) {
            trail.sample(pt.t, pt.x, pt.y);
        }
        let view = Bounds { x: fr.mon_x, y: fr.mon_y, width: fr.surf_w as f64, height: fr.surf_h as f64 };
        if let Some(TrailImage { bounds, pixels }) = trail.render(now, view, fr.light > 0.5, fr.bs) {
            // `render` may lower the density for a huge trail; scale it back
            // up to the frame's pixels.
            let factor = pixels.width() as f64 / bounds.width;
            let bx = ((bounds.x - fr.mon_x) * fr.bs) as i32;
            let by = ((bounds.y - fr.mon_y) * fr.bs) as i32;
            let scale = (fr.bs / factor) as f32;
            px.draw_pixmap(
                0,
                0,
                pixels.as_ref(),
                &tiny_skia::PixmapPaint::default(),
                tiny_skia::Transform::from_row(scale, 0.0, 0.0, scale, bx as f32, by as f32),
                None,
            );
            let (w, h) = ((pixels.width() as f32 * scale).ceil() as i32, (pixels.height() as f32 * scale).ceil() as i32);
            dirty = union_rect(dirty, (bx, by, w, h));
        }
    });

    if fr.e > 0.0 {
        let weight = |is_light: bool| {
            if meta.light_layers.is_empty() {
                1.0
            } else if is_light {
                fr.light
            } else {
                1.0 - fr.light
            }
        };
        for (layers, is_light) in [(&fr.art.dark, false), (&fr.art.light, true)] {
            let w = weight(is_light);
            if w <= 0.0 {
                continue;
            }
            for layer in layers {
                let alpha = match layer.kind {
                    LayerKind::Haze => fr.e * fr.hum_haze * w,
                    LayerKind::Beam => fr.hum_beam * (fr.e * 3.0).min(1.0) * w,
                };
                if alpha > 0.01 {
                    let masked;
                    let (src, opacity) = if layer.kind == LayerKind::Beam && fr.e < 0.999 {
                        masked = masked_beam(layer, fr.e, alpha, ppu as f32);
                        (&masked, 1.0)
                    } else {
                        (&layer.px, alpha)
                    };
                    // tiny-skia transforms the x/y passed to draw_pixmap too.
                    // Put translation directly in the matrix so the artwork
                    // is scaled around its own origin, then placed at the
                    // already-device-scaled cursor position.
                    let transform = glow_turn.unwrap_or_else(|| {
                        tiny_skia::Transform::from_row(s, 0.0, 0.0, s, ox.round(), oy.round())
                    });
                    px.draw_pixmap(
                        0,
                        0,
                        src.as_ref(),
                        &tiny_skia::PixmapPaint {
                            opacity,
                            quality: if glow_turn.is_some() {
                                tiny_skia::FilterQuality::Bilinear
                            } else {
                                tiny_skia::FilterQuality::Nearest
                            },
                            ..Default::default()
                        },
                        transform,
                        None,
                    );
                }
            }
        }
    }

    // Aim: the whole cursor body, tip on the pointer, over the glow. It is
    // drawn whether or not the blade is lit, because while aim is on this is
    // the only pointer the session has.
    if let Some(aim) = fr.aim {
        aim.draw(&mut px, fr.aim_heading, fr.aim_mirror, aim_cursor, aim_scale);
    }

    // Keep tiny-skia's premultiplied pixels. Wayland ARGB8888 requires
    // premultiplied alpha; unpremultiplying here turns soft haze into a
    // bright opaque-looking rectangle when the compositor blends it.
    let pixels = px.take();
    dirty = (
        dirty.0.max(0).min(dw as i32),
        dirty.1.max(0).min(dh as i32),
        dirty.2.min(dw as i32 - dirty.0.max(0)),
        dirty.3.min(dh as i32 - dirty.1.max(0)),
    );
    Frame {
        pixels,
        w: dw,
        h: dh,
        dirty,
    }
}

fn union_rect(a: (i32, i32, i32, i32), b: (i32, i32, i32, i32)) -> (i32, i32, i32, i32) {
    let x = a.0.min(b.0);
    let y = a.1.min(b.1);
    (
        x,
        y,
        (a.0 + a.2).max(b.0 + b.2) - x,
        (a.1 + a.3).max(b.1 + b.3) - y,
    )
}

/// Beam mid-ignition: visible up to `e * length`, then a soft leading edge
/// over FADE_UNITS. Port of the Python's gradient-masked beam draw.
fn masked_beam(layer: &crate::art::Layer, e: f32, alpha: f32, ppu: f32) -> tiny_skia::Pixmap {
    let beam = layer.beam.as_ref().expect("beam geom");
    let reach = e * beam.len;
    let fade = kyber_core::overlay::FADE_UNITS as f32 * ppu;
    let mut out = layer.px.clone();
    let w = out.width();
    for (i, c) in out.data_mut().chunks_exact_mut(4).enumerate() {
        let qx = (i as u32 % w) as f32;
        let qy = (i as u32 / w) as f32;
        let along = (qx - beam.fx) * beam.dx + (qy - beam.fy) * beam.dy;
        let m = if along <= reach {
            1.0
        } else {
            ((reach + fade - along) / fade).clamp(0.0, 1.0)
        };
        let k = alpha * m;
        for v in c.iter_mut() {
            *v = (*v as f32 * k).round().clamp(0.0, 255.0) as u8;
        }
    }
    out
}

// ---------------------------------------------------------------- loop

struct Placement {
    mon: usize,
    mon_x: f64,
    mon_y: f64,
    light: f32,
    f: f64,
    bs: i32,
    surf_w: u32,
    surf_h: u32,
    /// The monitor list this placement came from: moving the pointer to
    /// another screen can then be noticed without an IPC round trip a frame.
    mons: Vec<cursor::Monitor>,
}

fn place() -> Option<(Vec<cursor::Monitor>, Placement, (f64, f64))> {
    let mons = cursor::monitors().ok()?;
    let pos = cursor::cursor_pos().ok()?;
    let idx = cursor::monitor_at(&mons, pos.0, pos.1)?;
    let m = mons[idx].clone();
    let f = cursor::units_to_logical(&kyber_core::geometry::SIZES, m.scale);
    let bs = m.scale.ceil() as i32;
    Some((
        mons.clone(),
        Placement {
            mon: idx,
            mon_x: m.x,
            mon_y: m.y,
            light: 0.0,
            f,
            bs,
            surf_w: m.w.round() as u32,
            surf_h: m.h.round() as u32,
            mons,
        },
        pos,
    ))
}

/// The engine's whole trail (a `VecDeque` can wrap, so both halves).
fn trail_snapshot(engine: &Engine) -> Vec<TrailPt> {
    engine.trail.iter().copied().collect()
}

/// Spawn watchers for devices not already watched.
pub fn watch_new(
    watched: &mut std::collections::HashMap<std::path::PathBuf, std::thread::JoinHandle<()>>,
    tx: &std::sync::mpsc::Sender<ButtonEvent>,
) {
    input::reap_watchers(watched);
    for p in input::pointer_devices() {
        watched
            .entry(p.clone())
            .or_insert_with(|| input::spawn_watcher(p, tx.clone()));
    }
}

pub fn run_daemon() -> i32 {
    let mut overlay = match wl::Overlay::connect() {
        Ok(o) => o,
        Err(e) => {
            eprintln!("kyber-ignite: {e}");
            return 1;
        }
    };
    let (tx, rx) = std::sync::mpsc::channel::<ButtonEvent>();
    let mut watched = std::collections::HashMap::new();
    watch_new(&mut watched, &tx);
    if watched.is_empty() {
        eprintln!("kyber-ignite: no readable pointer devices — is this user in the 'input' group (and has the session restarted since)?");
        return 1;
    }
    let player = Player::new();
    let mut engine = Engine::new();
    engine.load_trail_look();
    let mut rng = SmallRng::seed(1);
    let mut art: Option<Art> = None;
    let mut placement: Option<Placement> = None;
    let mut last_rescan = Instant::now();
    let mut last_pos = (0.0, 0.0);

    // Aim borrows the compositor's pointer. The guard restores it on every
    // way out of this process that can be caught, so install it before any
    // path that can return.
    let _pointer_guard = crate::aim::PointerGuard::install();
    let mut aim = AimRuntime::new();
    let mut aim_polled = Instant::now() - Duration::from_secs(1);
    let mut aim_warned = false;
    let mut art_checked = Instant::now();
    let mut last_drawn: Option<((f64, f64), f64)>;
    // Another layer opened: when the overlay should be mapped again so it is
    // back above it. Debounced, since a menu opens several surfaces at once.
    let layers = cursor::watch_layers();
    let mut remap_due: Option<Instant> = None;

    loop {
        // Aim switches are two small files: read them a few times a second so
        // `kyber aim on` in a terminal takes effect without a restart.
        if aim_polled.elapsed() >= Duration::from_millis(250) {
            aim_polled = Instant::now();
            aim.reload();
        }
        // Idle: sleep until input (rescan every 5 s, like the Python). Aim on
        // means the cursor has to be redrawn even with no input, so the loop
        // below keeps running instead.
        if !aim.settings.enabled && !engine.is_live() && engine.trail.is_empty() {
            match rx.recv_timeout(Duration::from_secs(5)) {
                Ok(ev) => handle_button(ev, &mut engine, &player, now_s()),
                Err(RecvTimeoutError::Timeout) => {}
                Err(RecvTimeoutError::Disconnected) => return 0,
            }
            if last_rescan.elapsed() > Duration::from_secs(5) {
                watch_new(&mut watched, &tx);
                last_rescan = Instant::now();
            }
            // Service any press that arrived while idle.
            pump_press(
                &mut engine,
                &player,
                &mut art,
                &mut overlay,
                &mut placement,
                &mut last_pos,
            );
            continue;
        }
        // Active frame loop.
        let mut last_draw = Instant::now() - Duration::from_millis(100);
        last_drawn = None;
        loop {
            // Measure the interval from frame start, not completion: render
            // and upload must fit inside the interval rather than extend it.
            let frame_interval = Duration::from_millis(16);
            let remaining = frame_interval.saturating_sub(last_draw.elapsed());
            if !remaining.is_zero() {
                // One wait for the whole remainder, not 2 ms slices: the same
                // 60 fps with a fraction of the wakeups while aim holds the
                // loop open with a still pointer.
                overlay.poll(remaining);
                continue;
            }
            if aim_polled.elapsed() >= Duration::from_millis(250) {
                aim_polled = Instant::now();
                aim.reload();
            }
            // `kyber off` and recolors have to reach aim too: the art cache
            // is re-checked once a second, not on every frame.
            if aim.settings.enabled && art_checked.elapsed() >= Duration::from_secs(1) {
                art_checked = Instant::now();
                match selected_art() {
                    Some(fresh) => {
                        if art.as_ref().is_none_or(|old| old.key != fresh.key) {
                            art = Some(fresh);
                            aim_warned = false;
                            // Redraw: the colors may look different now.
                            last_drawn = None;
                        }
                    }
                    None => art = None,
                }
            }
            last_draw = Instant::now();
            while let Ok(ev) = rx.try_recv() {
                handle_button(ev, &mut engine, &player, now_s());
            }
            pump_press(
                &mut engine,
                &player,
                &mut art,
                &mut overlay,
                &mut placement,
                &mut last_pos,
            );
            if engine.poll_ignite_sound(
                now_s(),
                audio::sound_mode() == kyber_core::config::SoundMode::Mute,
            ) {
                player.play_clip("ignite");
            }
            if last_rescan.elapsed() > Duration::from_secs(5) {
                watch_new(&mut watched, &tx);
                last_rescan = Instant::now();
            }
            if let Ok(p) = cursor::cursor_pos() {
                last_pos = p;
            }
            let now = now_s();
            let live = engine.tick(now, Some(last_pos));
            let aimed = aim.settings.enabled
                && arm_aim(&mut aim, &mut art, &mut overlay, &mut placement, last_pos, &mut aim_warned);
            while layers.try_recv().is_ok() {
                remap_due = Some(Instant::now() + LAYER_SETTLE);
            }
            if remap_due.is_some_and(|due| Instant::now() >= due) {
                remap_due = None;
                if aimed && remap_overlay(&mut overlay, placement.as_ref()) {
                    last_drawn = None;
                }
            }
            if !aimed {
                // Nothing of ours is on screen at the pointer: hand it back.
                aim.release_pointer();
            } else {
                aim.keep_pointer_hidden();
            }
            if !live && !aimed {
                break;
            }
            if aimed {
                aim.tick(last_pos, now);
            }
            // A still pointer over a settled heading draws nothing new: sixty
            // checks a second, but only a frame when the picture can change.
            let changed = live
                || aimed
                    && last_drawn.is_none_or(|(pos, heading)| {
                        pos != last_pos || (heading - aim.heading()).abs() > 0.01
                    });
            if changed {
                if let (Some(a), Some(pl)) = (art.as_ref(), placement.as_ref()) {
                    let flicker = rng.next_f64() as f32 * 2.0 - 1.0;
                    let (hum_haze, hum_beam) = engine.hum(flicker);
                    let frame = render_frame(
                        &FrameInput {
                            art: a,
                            e: engine.ease(),
                            hum_haze,
                            hum_beam,
                            light: engine.light,
                            f: pl.f,
                            bs: pl.bs as f64,
                            mon_x: pl.mon_x,
                            mon_y: pl.mon_y,
                            pos_x: last_pos.0,
                            pos_y: last_pos.1,
                            trail: &trail_snapshot(&engine),
                            look: engine.trail_look,
                            aim: if aimed { aim.cursor() } else { None },
                            aim_heading: aim.heading(),
                            aim_mirror: aim.mirror(),
                            surf_w: pl.surf_w,
                            surf_h: pl.surf_h,
                        },
                        engine.t,
                    );
                    if let Err(e) = overlay.draw(&frame.pixels, frame.w, frame.h, frame.dirty) {
                        eprintln!("kyber-ignite: draw failed: {e}");
                    }
                    last_drawn = Some((last_pos, aim.heading()));
                }
            }
            overlay.poll(Duration::ZERO);
        }
        aim.release_pointer();
        overlay.hide();
        // Aim on but nothing to draw (Kyber off, no overlay art, no compositor
        // answer): retry a few times a second instead of spinning.
        if aim.settings.enabled {
            overlay.poll(Duration::from_millis(250));
        }
    }
}

/// How long after the last foreign layer opens before the overlay climbs back.
const LAYER_SETTLE: Duration = Duration::from_millis(80);

/// Map the overlay again, which puts it on top of its level. `show` replaces
/// the surface in one go, so the pointer is missing for one frame at most.
fn remap_overlay(overlay: &mut wl::Overlay, placement: Option<&Placement>) -> bool {
    let Some(pl) = placement else {
        return false;
    };
    let Some(out) = overlay.output_by_name(&pl.mons[pl.mon].name) else {
        return false;
    };
    overlay.show(&out, pl.surf_w, pl.surf_h, pl.bs, true).is_ok()
}

/// Keep aim's replacement cursor on screen: overlay presented on the cursor's
/// monitor, body baked, compositor pointer hidden.
///
/// Returns whether aim can draw this frame. The pointer is hidden only after
/// all of that succeeded — a hidden pointer with no replacement would lose the
/// user's cursor, which is much worse than losing the effect.
fn arm_aim(
    aim: &mut AimRuntime,
    art: &mut Option<Art>,
    overlay: &mut wl::Overlay,
    placement: &mut Option<Placement>,
    pos: (f64, f64),
    warned: &mut bool,
) -> bool {
    if art.is_none() {
        match selected_art() {
            Some(fresh) => *art = Some(fresh),
            None => {
                if !*warned {
                    eprintln!(
                        "kyber-ignite: aim is on but there is no overlay art, so the \
                         pointer stays visible. Run: kyber apply"
                    );
                    *warned = true;
                }
                return false;
            }
        }
    }
    let stale = match placement.as_ref() {
        None => true,
        Some(p) => {
            !overlay.is_shown() || cursor::monitor_at(&p.mons, pos.0, pos.1) != Some(p.mon)
        }
    };
    if stale {
        let Some((mons, mut pl, _)) = place() else {
            return false;
        };
        let name = mons[pl.mon].name.clone();
        let Some(out) = overlay.output_by_name(&name) else {
            return false;
        };
        pl.mons = mons;
        if overlay.show(&out, pl.surf_w, pl.surf_h, pl.bs, true).is_err() {
            return false;
        }
        *placement = Some(pl);
    }
    let Some(pl) = placement.as_ref() else {
        return false;
    };
    // Bake at twice the size the compositor would show: the turn resamples the
    // body, so a sharper source keeps the turned blade as crisp as the XCursor.
    let device_ppu = pl.f * pl.bs as f64;
    let baked = ((kyber_core::geometry::VIEW[2] * device_ppu) * 2.0)
        .round()
        .clamp(32.0, 512.0) as u32;
    if !aim.ensure_cursor(baked) {
        return false;
    }
    aim.hide_pointer()
}

/// Button event outside/between frames: transitions + sounds only.
fn handle_button(ev: ButtonEvent, engine: &mut Engine, player: &Player, now: f64) {
    match ev {
        ButtonEvent::Down => {
            // pick up a changed trail look on every press
            engine.load_trail_look();
            let action = engine.button_down(now);
            if matches!(
                action,
                DownAction::NeedPlace { clash: true } | DownAction::Reignite { clash: true }
            ) && audio::sound_mode() == kyber_core::config::SoundMode::All
            {
                player.play_clash();
            }
        }
        ButtonEvent::Up => {
            if engine.button_up() && audio::sound_mode() == kyber_core::config::SoundMode::All {
                player.play_clip("off");
            }
        }
    }
}

/// A fresh press needing placement: reload art (theme may have changed),
/// place, sample background, show. Idempotent per press.
fn pump_press(
    engine: &mut Engine,
    _player: &Player,
    art: &mut Option<Art>,
    overlay: &mut wl::Overlay,
    placement: &mut Option<Placement>,
    last_pos: &mut (f64, f64),
) {
    if engine.held == 0 || engine.is_live() {
        return;
    }
    // Phase is Off with buttons held: (re)try placement (covers Kyber
    // being applied mid-hold, or the first press after `kyber off`).
    let fresh = selected_art();
    let changed = match (&fresh, &*art) {
        (Some(n), Some(o)) => n.key != o.key,
        (Some(_), None) => true,
        _ => false,
    };
    if fresh.is_some() && (art.is_none() || changed) {
        *art = fresh;
    }
    let Some(a) = art.as_ref() else { return };
    let Some((mons, mut pl, pos)) = place() else {
        return;
    };
    let luma = shot::background_luma(pos.0, pos.1);
    pl.light = luma
        .map(|l| ((l as f64 - LIGHT_LO) / (LIGHT_HI - LIGHT_LO)).clamp(0.0, 1.0) as f32)
        .unwrap_or(0.0);
    // Aim may already have the overlay up on this exact monitor: re-creating
    // the surface would flash, so only (re)show when something changed.
    let already = overlay.is_shown()
        && placement.as_ref().is_some_and(|p| {
            p.mon == pl.mon && p.surf_w == pl.surf_w && p.surf_h == pl.surf_h && p.bs == pl.bs
        });
    let wlname = mons[pl.mon].name.clone();
    pl.mons = mons;
    if !already {
        let Some(out) = overlay.output_by_name(&wlname) else {
            return;
        };
        if overlay.show(&out, pl.surf_w, pl.surf_h, pl.bs, false).is_err() {
            return;
        }
    }
    *last_pos = pos;
    engine.begin_visual(now_s(), trail_enabled(), pl.light);
    *placement = Some(pl);
    let _ = a;
}

fn trail_enabled() -> bool {
    let base = std::env::var("XDG_CONFIG_HOME")
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|_| {
            std::env::var("HOME")
                .map(|h| std::path::PathBuf::from(h).join(".config"))
                .unwrap_or_else(|_| std::path::PathBuf::from("/tmp"))
        });
    std::fs::read_to_string(base.join("kyber-cursor/trail-mode"))
        .unwrap_or_default()
        .trim()
        != "off"
}

fn now_s() -> f64 {
    use std::sync::OnceLock;
    static T0: OnceLock<Instant> = OnceLock::new();
    T0.get_or_init(Instant::now).elapsed().as_secs_f64()
}

pub fn run_demo(seconds: f64) -> i32 {
    let mut overlay = match wl::Overlay::connect() {
        Ok(o) => o,
        Err(e) => {
            eprintln!("kyber-ignite: {e}");
            return 1;
        }
    };
    let Some(a) = selected_art() else {
        eprintln!("kyber-ignite: selected cursor theme has no overlay art");
        return 1;
    };
    let Some((mons, mut pl, pos)) = place() else {
        eprintln!("kyber-ignite: Hyprland did not answer");
        return 1;
    };
    pl.light = shot::background_luma(pos.0, pos.1)
        .map(|l| ((l as f64 - LIGHT_LO) / (LIGHT_HI - LIGHT_LO)).clamp(0.0, 1.0) as f32)
        .unwrap_or(0.0);
    let m = &mons[pl.mon];
    let out = overlay.output_by_name(&m.name);
    let Some(out) = out else {
        eprintln!("kyber-ignite: no output");
        return 1;
    };
    if overlay.show(&out, pl.surf_w, pl.surf_h, pl.bs, false).is_err() {
        return 1;
    }
    let mut rng = SmallRng::seed(7);
    let demo_start = Instant::now();
    let mut last_pos = pos;
    // A realistic repeating preview: clearly idle, a short button press,
    // then complete retraction. The old demo held the button for `seconds`,
    // which made the overlay look permanently on and hid off-state bugs.
    while demo_start.elapsed().as_secs_f64() < seconds {
        let idle_until = Instant::now() + Duration::from_millis(900);
        while Instant::now() < idle_until && demo_start.elapsed().as_secs_f64() < seconds {
            overlay.poll(Duration::from_millis(16));
        }
        if demo_start.elapsed().as_secs_f64() >= seconds {
            break;
        }

        let mut engine = Engine::new();
        engine.load_trail_look();
        engine.button_down(now_s());
        engine.begin_visual(now_s(), true, pl.light);
        let release_at = Instant::now() + Duration::from_millis(420);
        let mut released = false;
        let mut last_draw = Instant::now() - Duration::from_millis(100);
        loop {
            if let Ok(p) = cursor::cursor_pos() {
                last_pos = p;
            }
            if !released && Instant::now() >= release_at {
                engine.button_up();
                released = true;
            }
            if !engine.tick(now_s(), Some(last_pos)) {
                break;
            }
            if last_draw.elapsed() >= Duration::from_millis(16) {
                let flicker = rng.next_f64() as f32 * 2.0 - 1.0;
                let (hum_haze, hum_beam) = engine.hum(flicker);
                let frame = render_frame(
                    &FrameInput {
                        art: &a,
                        e: engine.ease(),
                        hum_haze,
                        hum_beam,
                        light: engine.light,
                        f: pl.f,
                        bs: pl.bs as f64,
                        mon_x: m.x,
                        mon_y: m.y,
                        pos_x: last_pos.0,
                        pos_y: last_pos.1,
                        trail: &trail_snapshot(&engine),
                        look: engine.trail_look,
                        aim: None,
                        aim_heading: 0.0,
                        aim_mirror: false,
                        surf_w: pl.surf_w,
                        surf_h: pl.surf_h,
                    },
                    engine.t,
                );
                if let Err(e) = overlay.draw(&frame.pixels, frame.w, frame.h, frame.dirty) {
                    eprintln!("kyber-ignite: draw failed: {e}");
                }
                last_draw = Instant::now();
            }
            overlay.poll(Duration::from_millis(4));
        }

        // Submit a fully transparent frame so idle is visibly and
        // verifiably off between pulses.
        let frame = render_frame(&FrameInput {
            art: &a,
            e: 0.0,
            hum_haze: 0.0,
            hum_beam: 0.0,
            light: pl.light,
            f: pl.f,
            bs: pl.bs as f64,
            mon_x: m.x,
            mon_y: m.y,
            pos_x: last_pos.0,
            pos_y: last_pos.1,
            trail: &[],
            look: engine.trail_look,
            aim: None,
            aim_heading: 0.0,
            aim_mirror: false,
            surf_w: pl.surf_w,
            surf_h: pl.surf_h,
        }, engine.t);
        if let Err(e) = overlay.draw(&frame.pixels, frame.w, frame.h, frame.dirty) {
            eprintln!("kyber-ignite: clear failed: {e}");
            overlay.hide();
            return 1;
        }
    }
    overlay.hide();
    0
}

#[cfg(test)]
mod render_tests {
    use super::*;

    #[test]
    fn draw_opacity_matches_baked_premultiplied_alpha() {
        let mut src = tiny_skia::Pixmap::new(8, 8).unwrap();
        src.fill(tiny_skia::Color::from_rgba8(30, 120, 255, 180));
        let mut reference = src.clone();
        for v in reference.data_mut() {
            *v = (*v as f32 * 0.37).round() as u8;
        }
        let mut drawn = tiny_skia::Pixmap::new(8, 8).unwrap();
        drawn.draw_pixmap(
            0,
            0,
            src.as_ref(),
            &tiny_skia::PixmapPaint {
                opacity: 0.37,
                ..Default::default()
            },
            tiny_skia::Transform::identity(),
            None,
        );
        for (actual, expected) in drawn.data().iter().zip(reference.data()) {
            assert!(actual.abs_diff(*expected) <= 1);
        }
        let mut idle = tiny_skia::Pixmap::new(8, 8).unwrap();
        idle.draw_pixmap(
            0,
            0,
            src.as_ref(),
            &tiny_skia::PixmapPaint {
                opacity: 0.0,
                ..Default::default()
            },
            tiny_skia::Transform::identity(),
            None,
        );
        assert!(idle.data().iter().all(|v| *v == 0));
    }

    #[test]
    fn aim_frame_draws_the_whole_cursor_inside_a_small_damage_rect() {
        // No glow layers at all: whatever is drawn here is the aim body.
        let art = Art {
            meta: kyber_core::overlay::OverlayMeta::new("#0009f5"),
            dark: Vec::new(),
            light: Vec::new(),
            key: (std::path::PathBuf::new(), std::time::UNIX_EPOCH),
        };
        let body = kyber_core::render::AimCursor::build("#fcef0c", 96).unwrap();
        let input = FrameInput {
            art: &art,
            e: 0.0,
            hum_haze: 0.0,
            hum_beam: 0.0,
            light: 0.0,
            f: 0.8,
            bs: 1.0,
            mon_x: 0.0,
            mon_y: 0.0,
            pos_x: 400.0,
            pos_y: 300.0,
            trail: &[],
            look: TrailLook::default(),
            aim: Some(&body),
            aim_heading: 0.0,
            aim_mirror: false,
            surf_w: 800,
            surf_h: 600,
        };
        let frame = render_frame(&input, 0.0);
        let alpha = |x: u32, y: u32| frame.pixels[((y * frame.w + x) * 4 + 3) as usize];
        // The tip is a hairline point, so look around the hotspot: something
        // of the spearhead has to be there.
        let near_tip = (396..=404)
            .flat_map(|x| (296..=304).map(move |y| (x, y)))
            .any(|(x, y)| alpha(x, y) > 0);
        assert!(near_tip, "nothing drawn at the pointer");
        // The head and then the hilt trail behind it (heading 0 = travelling
        // right), so the blade is drawn pointing where the pointer came from.
        assert!(alpha(390, 300) > 0, "the spearhead is missing");
        let hilt = (297..=303).any(|y| alpha(372, y) > 0);
        assert!(hilt, "the hilt is missing");
        // Nothing anywhere else, so the empty overlay stays click-through and
        // the compositor has nothing to blend.
        assert_eq!(alpha(20, 20), 0);
        assert_eq!(alpha(780, 580), 0);
        // Damage covers the cursor footprint, not the screen.
        let (_, _, w, h) = frame.dirty;
        assert!(w > 0 && h > 0, "{:?}", frame.dirty);
        assert!(w <= 220 && h <= 220, "full-screen damage: {:?}", frame.dirty);
    }

    #[test]
    #[ignore = "manual release-mode renderer timing"]
    fn held_frame_timing() {
        let layers = || {
            (0..4)
                .map(|_| {
                    let mut px = tiny_skia::Pixmap::new(768, 768).unwrap();
                    px.fill(tiny_skia::Color::from_rgba8(90, 100, 255, 120));
                    crate::art::Layer {
                        kind: LayerKind::Haze,
                        px,
                        beam: None,
                    }
                })
                .collect()
        };
        let art = Art {
            meta: kyber_core::overlay::OverlayMeta::new("#0009f5"),
            dark: layers(),
            light: layers(),
            key: (std::path::PathBuf::new(), std::time::UNIX_EPOCH),
        };
        let input = FrameInput {
            art: &art,
            e: 1.0,
            hum_haze: 0.55,
            hum_beam: 0.22,
            light: 0.5,
            f: 0.8,
            bs: 2.0,
            mon_x: 0.0,
            mon_y: 0.0,
            pos_x: 500.0,
            pos_y: 300.0,
            trail: &[],
            look: TrailLook::default(),
            aim: None,
            aim_heading: 0.0,
            aim_mirror: false,
            surf_w: 1200,
            surf_h: 750,
        };
        let start = Instant::now();
        for _ in 0..30 {
            std::hint::black_box(render_frame(&input, 0.0));
        }
        eprintln!(
            "held renderer: {:.2} ms/frame",
            start.elapsed().as_secs_f64() * 1000.0 / 30.0
        );
    }
}
