//! Shared egui → `egui_glow` rendering core on an EGL/GLES context bound to a
//! Wayland surface, plus zero-copy dma-buf → GL texture import.
//!
//! This is the toolkit half of `wlr-capture`: any windowing host (the
//! `wlr-chooser` layer-shell overlay, the `wlr-pip` xdg-toplevel mirror, …) binds
//! a [`Gpu`] to its `wl_surface` and drives one egui frame per repaint with
//! [`Gpu::render`]. The host owns the GL context, so it (via the importer handed
//! to the UI closure) turns capture dma-bufs into drawable textures.

use crate::gl::{
    DmabufEgl, EGL_LINUX_DMA_BUF_EXT, Egl, EglImage, GL_TEXTURE_2D, dmabuf_image_attribs,
    load_dmabuf_egl,
};
use crate::wl;
use edgefirst_egl as egl;
use std::collections::HashMap;
use std::ffi::c_void;
use std::sync::Arc;
use wayland_client::{Connection, Proxy, protocol::wl_surface::WlSurface};

/// Host-side importer for GPU dma-buf frames. The windowing host owns the GL
/// context, so it (not a toolkit-agnostic UI) turns a dma-buf into a drawable
/// egui texture. Returns the texture id + source pixel size.
pub trait DmabufImporter {
    /// Import `frame` as a GL-backed egui texture, caching it under `key` (the
    /// swapchain slot). Returns the texture id and the source pixel size.
    ///
    /// `None` means the import **failed**, not that the frame is still on its way:
    /// this is only ever called with a frame in hand. The failure is deterministic
    /// (same buffer, same driver), so a caller should record it and show something
    /// other than a loading state rather than waiting for a frame that will never
    /// import.
    #[must_use]
    fn import(
        &mut self,
        key: &str,
        frame: wl::DmabufFrame,
    ) -> Option<(egui::TextureId, egui::Vec2)>;
    /// Release any GPU resources cached for a source that went away.
    fn forget(&mut self, key: &str);
}

// dma-buf → GL texture import: the EGL/GL core (entry points, EGLImage creation,
// readback) lives in `crate::gl`; here we only sample the imported texture for
// display. These two GL swizzle constants are display-only.
const GL_TEXTURE_SWIZZLE_A: u32 = 0x8E45;
const GL_ONE: i32 = 1;

/// A dma-buf imported as a GL texture, cached per source key.
struct NativeTex {
    image: EglImage,
    tex: glow::Texture,
    id: egui::TextureId,
    size: egui::Vec2,
}

/// Host-side [`DmabufImporter`]: turns a dma-buf fd into a GL texture egui can
/// sample. Borrows the painter (to register native textures) and the persistent
/// texture cache; `egl` is `None` if the driver can't import dma-bufs.
struct HostImporter<'a> {
    egl: Option<DmabufEgl>,
    gl: Arc<glow::Context>,
    painter: &'a mut egui_glow::Painter,
    cache: &'a mut HashMap<String, NativeTex>,
}

impl DmabufImporter for HostImporter<'_> {
    fn import(
        &mut self,
        key: &str,
        frame: wl::DmabufFrame,
    ) -> Option<(egui::TextureId, egui::Vec2)> {
        use glow::HasContext as _;
        let egl = self.egl?;
        let size = egui::vec2(frame.width as f32, frame.height as f32);
        let attribs = dmabuf_image_attribs(&frame, egl.modifiers);
        // EGL_NO_CONTEXT for dma-buf import; EGL dups the fd, so we may close ours.
        let image = unsafe {
            (egl.create_image)(
                egl.display,
                std::ptr::null_mut(),
                EGL_LINUX_DMA_BUF_EXT,
                std::ptr::null_mut(),
                attribs.as_ptr(),
            )
        };
        if image.is_null() {
            return None;
        }

        let ckey = key.to_string();
        // Refresh the existing texture in place (the dma-buf is the same kernel
        // object; just rebind the fresh image), keeping a stable egui texture id.
        if let Some(nt) = self.cache.get_mut(&ckey) {
            unsafe {
                self.gl.bind_texture(GL_TEXTURE_2D, Some(nt.tex));
                (egl.image_target)(GL_TEXTURE_2D, image);
                self.gl.bind_texture(GL_TEXTURE_2D, None);
                (egl.destroy_image)(egl.display, nt.image);
            }
            nt.image = image;
            nt.size = size;
            return Some((nt.id, nt.size));
        }

        // First frame for this slot: create the GL texture and register it.
        let tex = unsafe {
            let t = self.gl.create_texture().ok()?;
            self.gl.bind_texture(GL_TEXTURE_2D, Some(t));
            let lin = glow::LINEAR as i32;
            let clamp = glow::CLAMP_TO_EDGE as i32;
            self.gl
                .tex_parameter_i32(GL_TEXTURE_2D, glow::TEXTURE_MIN_FILTER, lin);
            self.gl
                .tex_parameter_i32(GL_TEXTURE_2D, glow::TEXTURE_MAG_FILTER, lin);
            self.gl
                .tex_parameter_i32(GL_TEXTURE_2D, glow::TEXTURE_WRAP_S, clamp);
            self.gl
                .tex_parameter_i32(GL_TEXTURE_2D, glow::TEXTURE_WRAP_T, clamp);
            // Captured buffers are XRGB (no real alpha): the X byte is undefined,
            // so force sampled alpha to 1, else egui blends with garbage alpha.
            self.gl
                .tex_parameter_i32(GL_TEXTURE_2D, GL_TEXTURE_SWIZZLE_A, GL_ONE);
            (egl.image_target)(GL_TEXTURE_2D, image);
            self.gl.bind_texture(GL_TEXTURE_2D, None);
            t
        };
        let id = self.painter.register_native_texture(tex);
        self.cache.insert(
            ckey,
            NativeTex {
                image,
                tex,
                id,
                size,
            },
        );
        Some((id, size))
    }

    fn forget(&mut self, key: &str) {
        use glow::HasContext as _;
        let Some(egl) = self.egl else { return };
        if let Some(nt) = self.cache.remove(key) {
            self.painter.free_texture(nt.id);
            unsafe {
                self.gl.delete_texture(nt.tex);
                (egl.destroy_image)(egl.display, nt.image);
            }
        }
    }
}

/// EGL/GL state for one host: the display, the context and everything realised on
/// it — the egui_glow painter with its compiled shaders, the glyph atlas and the
/// dma-buf texture cache — plus the window surface it currently draws to.
///
/// The context outlives the surface. A host that shows one overlay builds both
/// together and drops both; the switcher's daemon keeps the context for the session
/// and [`Gpu::bind`]s it to each overlay's own surface in turn, which is what makes
/// the second overlay cost milliseconds instead of ninety.
pub struct Gpu {
    egl: Egl,
    display: egl::Display,
    /// The framebuffer configuration every window surface is created with.
    config: egl::Config,
    context: egl::Context,
    /// Where the context draws: the EGL surface wrapping a `wl_surface`, with the
    /// native window it is built on. `None` between two overlays — a daemon's host
    /// holds the context long after the surface it last drew to is gone.
    target: Option<Target>,
    painter: egui_glow::Painter,
    /// dma-buf import entry points, if the driver supports them.
    dmabuf_egl: Option<DmabufEgl>,
    /// dma-buf textures imported for display, keyed by source key.
    dmabuf_tex: HashMap<String, NativeTex>,
}

impl Gpu {
    /// Build the EGL/GLES context and point it at `surface`, at physical size
    /// `pw`×`ph`. Panics on EGL setup failure (the host can't render without it).
    pub fn new(conn: &Connection, surface: &WlSurface, pw: i32, ph: i32) -> Gpu {
        let lib = unsafe { egl::DynamicInstance::<egl::EGL1_4>::load_required() }
            .expect("libEGL not found");
        let egl: Egl = lib;

        let display_ptr = conn.backend().display_ptr() as *mut c_void;
        let display = unsafe { egl.get_display(display_ptr).expect("eglGetDisplay") };
        egl.initialize(display).expect("eglInitialize");
        egl.bind_api(egl::OPENGL_ES_API).expect("eglBindAPI");

        let attribs = [
            egl::SURFACE_TYPE,
            egl::WINDOW_BIT,
            egl::RENDERABLE_TYPE,
            egl::OPENGL_ES2_BIT,
            egl::RED_SIZE,
            8,
            egl::GREEN_SIZE,
            8,
            egl::BLUE_SIZE,
            8,
            egl::ALPHA_SIZE,
            8,
            egl::NONE,
        ];
        let config = egl
            .choose_first_config(display, &attribs)
            .expect("eglChooseConfig")
            .expect("no EGL config with alpha");

        let ctx_attribs = [egl::CONTEXT_CLIENT_VERSION, 3, egl::NONE];
        let context = egl
            .create_context(display, config, None, &ctx_attribs)
            .or_else(|_| {
                let a = [egl::CONTEXT_CLIENT_VERSION, 2, egl::NONE];
                egl.create_context(display, config, None, &a)
            })
            .expect("eglCreateContext");

        let target = Target::new(&egl, display, config, context, surface, pw, ph);

        let gl = unsafe {
            glow::Context::from_loader_function(|s| {
                egl.get_proc_address(s)
                    .map_or(std::ptr::null(), |p| p as *const _)
            })
        };
        let painter = egui_glow::Painter::new(Arc::new(gl), "", None, false).expect("egui_glow");
        let dmabuf_egl = load_dmabuf_egl(&egl, display);
        if dmabuf_egl.is_none() {
            eprintln!("wlr-capture: EGL dma-buf import unavailable (GPU display disabled)");
        }

        Gpu {
            egl,
            display,
            config,
            context,
            target: Some(target),
            painter,
            dmabuf_egl,
            dmabuf_tex: HashMap::new(),
        }
    }

    /// Point the context at another `wl_surface`, releasing the one it was drawing to.
    ///
    /// An overlay's layer surface cannot be handed to the next overlay: the compositor
    /// picks which output it belongs to when it is created, so a host that shows
    /// several builds a fresh one each time and binds it here. Only the window surface
    /// follows — the display, the context, the compiled shaders and the glyph atlas
    /// are what cost, and they stay.
    ///
    /// Bind at the size the surface was configured to: Mesa sizes the back buffer when
    /// the context is made current on it and applies a resize only from the next swap,
    /// so a surface bound too small presents one frame at that size.
    pub fn bind(&mut self, surface: &WlSurface, pw: i32, ph: i32) {
        self.unbind();
        self.target = Some(Target::new(
            &self.egl,
            self.display,
            self.config,
            self.context,
            surface,
            pw,
            ph,
        ));
    }

    /// Whether a window surface is bound, i.e. whether there is anywhere to draw.
    pub fn is_bound(&self) -> bool {
        self.target.is_some()
    }

    /// Release the window surface, keeping the context and everything realised on it.
    ///
    /// Called before the `wl_surface` it wraps is destroyed: the other order leaves
    /// EGL talking about an object the compositor has already forgotten.
    pub fn unbind(&mut self) {
        let Some(target) = self.target.take() else {
            return;
        };
        // Detach first, then destroy the surface while its native window is still
        // alive — `target` is dropped, in field order, once this returns.
        let _ = self.egl.make_current(self.display, None, None, None);
        let _ = self.egl.destroy_surface(self.display, target.surface);
    }

    /// Resize the EGL window to a new physical size (after a surface configure /
    /// scale change).
    pub fn resize(&self, pw: i32, ph: i32) {
        if let Some(target) = self.target.as_ref() {
            target.egl_window.resize(pw, ph, 0, 0);
        }
    }

    /// Rasterise the glyph atlas and upload it, without presenting anything.
    ///
    /// The first egui pass parses the font files and turns their glyphs into an atlas
    /// texture — a few milliseconds paid on the very frame the overlay becomes
    /// visible. A host that keeps a [`Gpu`] across overlays (the switcher's daemon)
    /// pays it at startup instead. Only `paint_and_update_textures` runs: no
    /// primitives and no `swap_buffers`, so nothing is attached to the surface and an
    /// unmapped one stays unmapped.
    pub fn prewarm(&mut self, egui_ctx: &egui::Context) {
        // Text has to be laid out for any glyph to reach the atlas.
        self.pump_textures(egui_ctx, |ui| {
            ui.label("Ag");
        });
    }

    /// Free everything the overlay that just ended left on the GPU.
    ///
    /// A one-shot run leaves this to process exit; a host that outlives its overlays
    /// cannot. An imported dma-buf holds a reference to the compositor's buffer — for
    /// a window that may since have closed — and the cache is keyed by source, so
    /// without this it would grow with every window a session ever previewed. The
    /// overlay's own textures (shm thumbnails, app icons) were dropped with it, but
    /// egui only acts on that at its next pass, which would be the next overlay.
    ///
    /// Call while the surface is still bound: freeing needs the context current.
    pub fn release_textures(&mut self, egui_ctx: &egui::Context) {
        if let Some(egl) = self.dmabuf_egl {
            use glow::HasContext as _;
            let gl = self.painter.gl().clone();
            for (_, nt) in self.dmabuf_tex.drain() {
                self.painter.free_texture(nt.id);
                unsafe {
                    gl.delete_texture(nt.tex);
                    (egl.destroy_image)(egl.display, nt.image);
                }
            }
        }
        self.pump_textures(egui_ctx, |_| {});
    }

    /// Run one egui pass and carry out the texture uploads and frees it asks for,
    /// without presenting anything: no primitives and no `swap_buffers`, so the
    /// surface is left exactly as it was — an unmapped one stays unmapped.
    fn pump_textures(&mut self, egui_ctx: &egui::Context, run_ui: impl FnMut(&mut egui::Ui)) {
        let Some(surface) = self.target.as_ref().map(|t| t.surface) else {
            return;
        };
        self.egl
            .make_current(
                self.display,
                Some(surface),
                Some(surface),
                Some(self.context),
            )
            .ok();
        let raw_input = egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(
                egui::Pos2::ZERO,
                egui::vec2(64.0, 64.0),
            )),
            ..Default::default()
        };
        let mut delta = egui_ctx.run_ui(raw_input, run_ui).textures_delta;
        self.painter
            .paint_and_update_textures([64, 64], 1.0, &[], &mut delta);
    }

    /// Run one egui frame and present it. `run_ui` builds the UI; it is handed the
    /// dma-buf importer (this owns the GL context) so capture frames become
    /// drawable textures. `backdrop` is the GL clear colour (premultiplied gamma).
    pub fn render(
        &mut self,
        egui_ctx: &egui::Context,
        mut raw_input: egui::RawInput,
        ppp: f32,
        size_px: (u32, u32),
        backdrop: [f32; 4],
        mut run_ui: impl FnMut(&mut egui::Ui, &mut dyn DmabufImporter),
    ) {
        // Lay text out at the same pixels-per-point we tessellate with, or epaint warns
        // ("pixels_per_point have changed between text layout and tessellation") and
        // text shapes can be mis-scaled — happens on fractional/HiDPI outputs where
        // `ppp != 1.0` but the caller left it unset in `raw_input`.
        raw_input
            .viewports
            .entry(egui::ViewportId::ROOT)
            .or_default()
            .native_pixels_per_point = Some(ppp);

        let (pw, ph) = size_px;
        // Nothing to draw to between two overlays; a host only paints while one is up.
        let Some(surface) = self.target.as_ref().map(|t| t.surface) else {
            return;
        };
        self.egl
            .make_current(
                self.display,
                Some(surface),
                Some(surface),
                Some(self.context),
            )
            .ok();

        // Run the UI. GPU dma-buf frames are imported here via the host importer,
        // since that needs the painter + GL context.
        let (prims, mut textures_delta) = {
            let gl = self.painter.gl().clone();
            let mut importer = HostImporter {
                egl: self.dmabuf_egl,
                gl,
                painter: &mut self.painter,
                cache: &mut self.dmabuf_tex,
            };
            // `run_ui` hands the closure a full-screen root `Ui`; paint functions add
            // their panels into it with `show_inside`.
            let full = egui_ctx.run_ui(raw_input, |ui| run_ui(ui, &mut importer));
            (egui_ctx.tessellate(full.shapes, ppp), full.textures_delta)
        };

        unsafe {
            use glow::HasContext as _;
            let gl = self.painter.gl();
            gl.viewport(0, 0, pw as i32, ph as i32);
            let [r, g, b, a] = backdrop;
            gl.clear_color(r, g, b, a);
            gl.clear(glow::COLOR_BUFFER_BIT);
        }
        self.painter
            .paint_and_update_textures([pw, ph], ppp, &prims, &mut textures_delta);
        self.egl.swap_buffers(self.display, surface).ok();
    }
}

/// An EGL window surface and the native window it wraps.
struct Target {
    surface: egl::Surface,
    /// Declared after the surface so it is dropped after it: `eglDestroySurface` must
    /// run while the native window it was built on is still there.
    egl_window: wayland_egl::WlEglSurface,
}

impl Target {
    /// Wrap `surface` in a native window and make the EGL surface over it current.
    fn new(
        egl: &Egl,
        display: egl::Display,
        config: egl::Config,
        context: egl::Context,
        surface: &WlSurface,
        pw: i32,
        ph: i32,
    ) -> Target {
        let egl_window =
            wayland_egl::WlEglSurface::new(surface.id(), pw.max(1), ph.max(1)).expect("wl_egl");
        let egl_surface = unsafe {
            egl.create_window_surface(
                display,
                config,
                egl_window.ptr() as egl::NativeWindowType,
                None,
            )
            .expect("eglCreateWindowSurface")
        };
        egl.make_current(display, Some(egl_surface), Some(egl_surface), Some(context))
            .expect("eglMakeCurrent");

        // Present without EGL's own throttling: on Wayland the compositor paces us
        // through `wl_surface.frame`, and a blocking `eglSwapBuffers` on top of that is
        // not just redundant but dangerous. It waits on the driver's private event queue
        // for a buffer release that never comes if the output stopped composing — asleep,
        // blanked or gone — wedging the calling thread for as long as that lasts. Callers
        // pace themselves on frame callbacks instead.
        let _ = egl.swap_interval(display, 0);

        Target {
            surface: egl_surface,
            egl_window,
        }
    }
}

impl Drop for Gpu {
    /// Release this surface's EGL context and window surface. Several `Gpu`s share one
    /// `EGLDisplay` (one per output), so dropping one — e.g. when its output is unplugged —
    /// must destroy its own handles. Leaking them corrupts the shared display's state and
    /// the next `eglCreateWindowSurface` (the monitor plugged back in) fails with
    /// `EGL_BAD_ALLOC`. Detach the context first, then destroy the surface while its
    /// `WlEglSurface` native window is still alive — that field is dropped afterwards.
    fn drop(&mut self) {
        self.unbind();
        let _ = self.egl.destroy_context(self.display, self.context);
    }
}
