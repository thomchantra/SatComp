//! An [`Editor`] implementation for egui.

use crate::egui::Vec2;
use crate::egui::ViewportCommand;
use crate::EguiState;
use baseview::gl::GlConfig;
use baseview::PhySize;
use baseview::{Size, WindowHandle, WindowOpenOptions, WindowScalePolicy};
use crossbeam::atomic::AtomicCell;
use egui_baseview::egui::Context;
use egui_baseview::EguiWindow;
use nih_plug::prelude::{Editor, GuiContext, ParamSetter, ParentWindowHandle};
use parking_lot::RwLock;
use raw_window_handle::{HasRawWindowHandle, RawWindowHandle};
use std::sync::atomic::Ordering;
use std::sync::Arc;

/// An [`Editor`] implementation that calls an egui draw loop.
pub(crate) struct EguiEditor<T> {
    pub(crate) egui_state: Arc<EguiState>,
    /// The plugin's state. This is kept in between editor openenings.
    pub(crate) user_state: Arc<RwLock<T>>,

    /// The user's build function. Applied once at the start of the application.
    pub(crate) build: Arc<dyn Fn(&Context, &mut T) + 'static + Send + Sync>,
    /// The user's update function.
    pub(crate) update: Arc<dyn Fn(&Context, &ParamSetter, &mut T) + 'static + Send + Sync>,

    /// The scaling factor reported by the host, if any. On macOS this will never be set and we
    /// should use the system scaling factor instead.
    pub(crate) scaling_factor: AtomicCell<Option<f32>>,
}

/// This version of `baseview` uses a different version of `raw_window_handle than NIH-plug, so we
/// need to adapt it ourselves.
struct ParentWindowHandleAdapter(nih_plug::editor::ParentWindowHandle);

unsafe impl HasRawWindowHandle for ParentWindowHandleAdapter {
    fn raw_window_handle(&self) -> RawWindowHandle {
        match self.0 {
            ParentWindowHandle::X11Window(window) => {
                let mut handle = raw_window_handle::XcbWindowHandle::empty();
                handle.window = window;
                RawWindowHandle::Xcb(handle)
            }
            ParentWindowHandle::AppKitNsView(ns_view) => {
                let mut handle = raw_window_handle::AppKitWindowHandle::empty();
                handle.ns_view = ns_view;
                RawWindowHandle::AppKit(handle)
            }
            ParentWindowHandle::Win32Hwnd(hwnd) => {
                let mut handle = raw_window_handle::Win32WindowHandle::empty();
                handle.hwnd = hwnd;
                RawWindowHandle::Win32(handle)
            }
        }
    }
}

impl<T> Editor for EguiEditor<T>
where
    T: 'static + Send + Sync,
{
    fn spawn(
        &self,
        parent: ParentWindowHandle,
        context: Arc<dyn GuiContext>,
    ) -> Box<dyn std::any::Any + Send> {
        let build = self.build.clone();
        let update = self.update.clone();
        let state = self.user_state.clone();
        let egui_state = self.egui_state.clone();

        let (unscaled_width, unscaled_height) = self.egui_state.size();
        let scaling_factor = self.scaling_factor.load();
        // SatComp patch (Linux frame pacing): the GL swap interval. A blocking vsync swap on top of baseview's own frame timer
        // made the meters stutter on X11, so Linux starts with vsync off; `SATCOMP_VSYNC=1` / `=0` forces it either way (A/B).
        let vsync = std::env::var("SATCOMP_VSYNC").map(|v| v != "0").unwrap_or(!cfg!(target_os = "linux"));
        // SatComp patch: `SATCOMP_FPS=1` prints the frame rate and the average / worst frame time to stderr once a second.
        let fps_log = std::env::var_os("SATCOMP_FPS").is_some();
        let mut fps = (std::time::Instant::now(), std::time::Instant::now(), 0u32, 0f32);
        let window = EguiWindow::open_parented(
            &ParentWindowHandleAdapter(parent),
            WindowOpenOptions {
                title: String::from("egui window"),
                // Baseview should be doing the DPI scaling for us
                size: Size::new(unscaled_width as f64, unscaled_height as f64),
                // NOTE: For some reason passing 1.0 here causes the UI to be scaled on macOS but
                //       not the mouse events.
                scale: scaling_factor
                    .map(|factor| WindowScalePolicy::ScaleFactor(factor as f64))
                    .unwrap_or(WindowScalePolicy::SystemScaleFactor),

                #[cfg(feature = "opengl")]
                gl_config: Some(GlConfig {
                    version: (3, 2),
                    red_bits: 8,
                    blue_bits: 8,
                    green_bits: 8,
                    alpha_bits: 8,
                    depth_bits: 24,
                    stencil_bits: 8,
                    samples: None,
                    srgb: true,
                    double_buffer: true,
                    vsync,
                    ..Default::default()
                }),
            },
            Default::default(),
            state,
            move |egui_ctx, _queue, state| build(egui_ctx, &mut state.write()),
            move |egui_ctx, queue, state| {
                let setter = ParamSetter::new(context.as_ref());
                if fps_log {
                    let now = std::time::Instant::now();
                    let dt = now.duration_since(fps.1).as_secs_f32() * 1000.0;
                    fps.1 = now;
                    fps.2 += 1;
                    fps.3 = fps.3.max(dt);
                    if now.duration_since(fps.0).as_secs_f32() >= 1.0 {
                        let secs = now.duration_since(fps.0).as_secs_f32();
                        eprintln!("[satcomp] {:.0} frames/s, average {:.1} ms, worst {:.1} ms, vsync {}", fps.2 as f32 / secs, secs * 1000.0 / fps.2 as f32, fps.3, vsync);
                        fps = (now, now, 0, 0.0);
                    }
                }

                // If the window was requested to resize
                // SatComp patch: keep `requested_size` set while asking the host. The host answers `request_resize()`
                // by calling `Editor::size()`, which returns `requested_size` if set; clearing it first made the host
                // read the previous size (REAPER's FX window lagged one step behind the content).
                if let Some(new_size) = egui_state.requested_size.load() {
                    // Ask the plugin host to resize to self.size()
                    let accepted = context.request_resize();
                    if accepted {
                        // The new size becomes the stored one before the request is cleared, so `size()` never
                        // reports the old value to a late host query.
                        egui_state.size.store(new_size);
                    }
                    egui_state.requested_size.store(None);
                    if accepted {
                        // Resize the content of egui window
                        // SatComp patch: `queue.resize` wants physical pixels, `new_size` is logical (broken on HiDPI otherwise)
                        let ppp = egui_ctx.pixels_per_point();
                        queue.resize(PhySize::new(
                            (new_size.0 as f32 * ppp).round() as u32,
                            (new_size.1 as f32 * ppp).round() as u32,
                        ));
                        egui_ctx.send_viewport_cmd(ViewportCommand::InnerSize(Vec2::new(
                            new_size.0 as f32,
                            new_size.1 as f32,
                        )));

                        // Update the state
                        egui_state.size.store(new_size);
                    }
                }

                // For now, just always redraw. Most plugin GUIs have meters, and those almost always
                // need a redraw. Later we can try to be a bit more sophisticated about this. Without
                // this we would also have a blank GUI when it gets first opened because most DAWs open
                // their GUI while the window is still unmapped.
                egui_ctx.request_repaint();
                (update)(egui_ctx, &setter, &mut state.write());
            },
        );

        self.egui_state.open.store(true, Ordering::Release);
        Box::new(EguiEditorHandle {
            egui_state: self.egui_state.clone(),
            window,
        })
    }

    /// Size of the editor window
    fn size(&self) -> (u32, u32) {
        let new_size = self.egui_state.requested_size.load();
        // This method will be used to ask the host for new size.
        // If the editor is currently being resized and new size hasn't been consumed and set yet, return new requested size.
        if let Some(new_size) = new_size {
            new_size
        } else {
            self.egui_state.size()
        }
    }

    fn set_scale_factor(&self, factor: f32) -> bool {
        // If the editor is currently open then the host must not change the current HiDPI scale as
        // we don't have a way to handle that. Ableton Live does this.
        if self.egui_state.is_open() {
            return false;
        }

        self.scaling_factor.store(Some(factor));
        true
    }

    fn param_value_changed(&self, _id: &str, _normalized_value: f32) {
        // As mentioned above, for now we'll always force a redraw to allow meter widgets to work
        // correctly. In the future we can use an `Arc<AtomicBool>` and only force a redraw when
        // that boolean is set.
    }

    fn param_modulation_changed(&self, _id: &str, _modulation_offset: f32) {}

    fn param_values_changed(&self) {
        // Same
    }
}

/// The window handle used for [`EguiEditor`].
struct EguiEditorHandle {
    egui_state: Arc<EguiState>,
    window: WindowHandle,
}

/// The window handle enum stored within 'WindowHandle' contains raw pointers. Is there a way around
/// having this requirement?
unsafe impl Send for EguiEditorHandle {}

impl Drop for EguiEditorHandle {
    fn drop(&mut self) {
        self.egui_state.open.store(false, Ordering::Release);
        // XXX: This should automatically happen when the handle gets dropped, but apparently not
        self.window.close();
    }
}
