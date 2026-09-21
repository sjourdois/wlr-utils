//! Native Wayland client: enumerate foreign toplevels and outputs, and capture
//! them via `ext-image-copy-capture-v1`, or via `zwlr-screencopy-v1` where that is
//! the only capture protocol the compositor offers.
//!
//! The whole point of doing this natively (instead of shelling out to `grim -T`)
//! is to create the shm buffer with the *correct* stride (`width * 4`), which is
//! where grim 1.5 trips up ("Invalid stride") on some toplevels (Firefox, …).
//!
//! Both protocols are driven behind one [`Client`] API: open a session on a source,
//! [`Client::poll`] it for frames, close it. [`Protocol`] says which one is in use.
//! `zwlr-screencopy` only addresses a `wl_output`, so window capture is unavailable
//! under it and window paths return [`CaptureError::WindowsUnsupported`].

use crate::error::{CaptureError, Context, Result};
#[cfg(feature = "gpu")]
use gbm::{BufferObject, BufferObjectFlags, Device as GbmDevice, Format as GbmFormat, Modifier};
use rustix::event::{PollFd, PollFlags, Timespec};
use std::collections::HashMap;
use std::ffi::c_void;
#[cfg(feature = "gpu")]
use std::fs::File;
use std::os::fd::{AsFd, OwnedFd};
use std::time::{Duration, Instant};
use wayland_client::{
    Connection, Dispatch, EventQueue, Proxy, QueueHandle, WEnum, delegate_noop,
    event_created_child,
    globals::{GlobalList, GlobalListContents, registry_queue_init},
    protocol::{
        wl_buffer::WlBuffer,
        wl_output::{self, Transform, WlOutput},
        wl_registry::WlRegistry,
        wl_seat::WlSeat,
        wl_shm::{self, WlShm},
        wl_shm_pool::WlShmPool,
    },
};
#[cfg(feature = "gpu")]
use wayland_protocols::wp::linux_dmabuf::zv1::client::{
    zwp_linux_buffer_params_v1::{self, ZwpLinuxBufferParamsV1},
    zwp_linux_dmabuf_feedback_v1::{self, ZwpLinuxDmabufFeedbackV1},
    zwp_linux_dmabuf_v1::ZwpLinuxDmabufV1,
};
use wayland_protocols_wlr::foreign_toplevel::v1::client::{
    zwlr_foreign_toplevel_handle_v1::{self, ZwlrForeignToplevelHandleV1},
    zwlr_foreign_toplevel_manager_v1::{self, ZwlrForeignToplevelManagerV1},
};
use wayland_protocols_wlr::screencopy::v1::client::{
    zwlr_screencopy_frame_v1::{self, ZwlrScreencopyFrameV1},
    zwlr_screencopy_manager_v1::ZwlrScreencopyManagerV1,
};

/// DRM "invalid"/"let the driver choose" modifier sentinel — not a real layout.
#[cfg(feature = "gpu")]
const DRM_MOD_INVALID: u64 = 0x00ff_ffff_ffff_ffff;

/// Process-wide off switch for the dma-buf path, set once at startup by a
/// `--no-gpu` flag. A per-`Client` setter would have to be threaded through every
/// construction site in every tool for a decision that is taken once for the whole
/// run; `WLR_NO_GPU` covers the same ground from the environment.
static GPU_DISABLED: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

/// Capture through shm in every [`Client`] created from now on. See [`GPU_DISABLED`].
pub fn disable_gpu_globally() {
    GPU_DISABLED.store(true, std::sync::atomic::Ordering::Relaxed);
}

/// Whether `WLR_FORCE_SCREENCOPY` asks for the `zwlr-screencopy` path even where
/// `ext-image-copy-capture` is available. A debug knob, so it lives in the
/// environment only: it exists to exercise the fallback on a compositor that
/// offers both protocols.
fn screencopy_forced() -> bool {
    std::env::var_os("WLR_FORCE_SCREENCOPY").is_some()
}

/// The capture protocol a [`Client`] drives.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Protocol {
    /// `ext-image-copy-capture-v1` + `ext-image-capture-source-v1`: outputs and
    /// windows, persistent sessions.
    ImageCopyCapture,
    /// `zwlr-screencopy-v1`: outputs only, one protocol object per frame.
    Screencopy,
}

impl Protocol {
    /// The protocol's interface name, for diagnostics.
    pub fn interface(self) -> &'static str {
        match self {
            Protocol::ImageCopyCapture => "ext-image-copy-capture-v1",
            Protocol::Screencopy => "zwlr-screencopy-v1",
        }
    }
}

/// Names of the globals each protocol needs, so [`select_protocol`] and `doctor`
/// apply one rule instead of two.
const IMAGE_COPY_GLOBALS: [&str; 2] = [
    "ext_image_copy_capture_manager_v1",
    "ext_output_image_capture_source_manager_v1",
];
const SCREENCOPY_GLOBAL: &str = "zwlr_screencopy_manager_v1";

/// Which capture protocol to drive, given the globals a compositor advertises.
/// `ext-image-copy-capture` wins when both are there — it captures windows too and
/// is the protocol `zwlr-screencopy` was deprecated in favour of — unless
/// `WLR_FORCE_SCREENCOPY` asks otherwise. `None` means neither is available.
pub fn select_protocol(globals: &[(String, u32)]) -> Option<Protocol> {
    let has = |iface: &str| globals.iter().any(|(n, _)| n == iface);
    let image_copy = IMAGE_COPY_GLOBALS.iter().all(|i| has(i));
    let screencopy = has(SCREENCOPY_GLOBAL);
    match (image_copy, screencopy) {
        (true, true) if screencopy_forced() => Some(Protocol::Screencopy),
        (true, _) => Some(Protocol::ImageCopyCapture),
        (false, true) => Some(Protocol::Screencopy),
        (false, false) => None,
    }
}

/// Most planes `EGL_EXT_image_dma_buf_import(_modifiers)` can describe.
#[cfg(feature = "gpu")]
const MAX_DMABUF_PLANES: u32 = 4;

/// `zwlr_screencopy_manager_v1.capture_output`'s `overlay_cursor`. The
/// `ext-image-copy-capture` counterpart is `Options::PaintCursors`, so both
/// protocols capture the same thing either way.
const CURSOR_OFF: i32 = 0;
const CURSOR_ON: i32 = 1;
use wayland_protocols::ext::{
    foreign_toplevel_list::v1::client::{
        ext_foreign_toplevel_handle_v1::{self, ExtForeignToplevelHandleV1},
        ext_foreign_toplevel_list_v1::{self, ExtForeignToplevelListV1},
    },
    image_capture_source::v1::client::{
        ext_foreign_toplevel_image_capture_source_manager_v1::ExtForeignToplevelImageCaptureSourceManagerV1,
        ext_image_capture_source_v1::ExtImageCaptureSourceV1,
        ext_output_image_capture_source_manager_v1::ExtOutputImageCaptureSourceManagerV1,
    },
    image_copy_capture::v1::client::{
        ext_image_copy_capture_frame_v1::{self, ExtImageCopyCaptureFrameV1, FailureReason},
        ext_image_copy_capture_manager_v1::{ExtImageCopyCaptureManagerV1, Options},
        ext_image_copy_capture_session_v1::{self, ExtImageCopyCaptureSessionV1},
    },
};
use wayland_protocols::xdg::xdg_output::zv1::client::{
    zxdg_output_manager_v1::ZxdgOutputManagerV1,
    zxdg_output_v1::{self, ZxdgOutputV1},
};

/// A capturable window.
#[derive(Clone)]
pub struct Toplevel {
    /// The protocol handle used to capture or act on this window.
    pub handle: ExtForeignToplevelHandleV1,
    /// Compositor-assigned stable identifier for the toplevel.
    pub identifier: String,
    /// The window title.
    pub title: String,
    /// The application id (used to match an icon / desktop entry). Empty only for a
    /// window neither foreign-toplevel protocol names — typically a system surface;
    /// an XWayland window carries its X11 class here.
    pub app_id: String,
}

/// A capturable output, with its placement in the global logical space.
///
/// Logical position/size come from `xdg-output` (`zxdg_output_manager_v1`) when the
/// compositor exposes it — the only reliable source for multi-monitor positions and
/// fractional-scale logical sizes. Physical pixel size, integer scale and transform
/// come from `wl_output`; if `xdg-output` is absent we fall back to computing the
/// logical size from those.
#[derive(Clone)]
pub struct Output {
    /// The underlying `wl_output` protocol object.
    pub wl_output: WlOutput,
    /// The output's connector name (e.g. `DP-1`).
    pub name: String,
    /// Left edge in the global logical coordinate space.
    pub logical_x: i32,
    /// Top edge in the global logical coordinate space.
    pub logical_y: i32,
    /// Logical width from xdg-output (0 until received; see [`Output::logical_size`]).
    pub logical_w: i32,
    /// Logical height from xdg-output (0 until received; see [`Output::logical_size`]).
    pub logical_h: i32,
    /// Width of the current mode, in physical pixels (pre-transform).
    pub phys_width: i32,
    /// Height of the current mode, in physical pixels (pre-transform).
    pub phys_height: i32,
    /// Integer buffer scale (wl_output; may be coarser than the real scale).
    pub scale: i32,
    /// Output transform (rotation/flip); swaps logical width/height for 90/270.
    pub transform: Transform,
    /// Whether xdg-output supplied authoritative logical geometry.
    pub have_xdg: bool,
}

/// Logical dimensions from physical pixels: divide by `scale`, swapping
/// width/height for 90°/270° transforms. Free function so it's unit-testable
/// without a live `WlOutput`.
fn logical_dims(phys_w: i32, phys_h: i32, scale: i32, transform: Transform) -> (i32, i32) {
    let s = scale.max(1);
    let (w, h) = (phys_w / s, phys_h / s);
    if matches!(
        transform,
        Transform::_90 | Transform::_270 | Transform::Flipped90 | Transform::Flipped270
    ) {
        (h, w)
    } else {
        (w, h)
    }
}

impl Output {
    /// Logical size (points). Prefers xdg-output's authoritative size (handles
    /// fractional scale); otherwise physical pixels divided by the integer scale,
    /// with width/height swapped for 90°/270° transforms.
    pub fn logical_size(&self) -> (i32, i32) {
        if self.have_xdg && self.logical_w > 0 && self.logical_h > 0 {
            (self.logical_w, self.logical_h)
        } else {
            logical_dims(
                self.phys_width,
                self.phys_height,
                self.scale,
                self.transform,
            )
        }
    }
}

/// An axis-aligned rectangle. Used both for capture cropping (in an image's pixel
/// space) and for selection geometry (in the global logical space), so `x`/`y` may
/// be negative; `w`/`h` are unsigned.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Region {
    /// Left edge (may be negative in logical space).
    pub x: i32,
    /// Top edge (may be negative in logical space).
    pub y: i32,
    /// Width.
    pub w: u32,
    /// Height.
    pub h: u32,
}

impl Region {
    /// Whether the region has zero area (`w` or `h` is 0).
    pub fn is_empty(&self) -> bool {
        self.w == 0 || self.h == 0
    }

    /// The overlapping rectangle of two regions, or `None` if they don't overlap.
    pub fn intersect(&self, o: &Region) -> Option<Region> {
        let x0 = self.x.max(o.x);
        let y0 = self.y.max(o.y);
        let x1 = (self.x + self.w as i32).min(o.x + o.w as i32);
        let y1 = (self.y + self.h as i32).min(o.y + o.h as i32);
        (x1 > x0 && y1 > y0).then_some(Region {
            x: x0,
            y: y0,
            w: (x1 - x0) as u32,
            h: (y1 - y0) as u32,
        })
    }
}

impl Output {
    /// The output's placement in the global logical space, as a [`Region`].
    pub fn logical_rect(&self) -> Region {
        let (w, h) = self.logical_size();
        Region {
            x: self.logical_x,
            y: self.logical_y,
            w: w.max(0) as u32,
            h: h.max(0) as u32,
        }
    }
}

/// Decoded RGBA8 image.
pub struct CapturedImage {
    /// Width in pixels.
    pub width: u32,
    /// Height in pixels.
    pub height: u32,
    /// Tightly-packed RGBA8 pixels, row-major (`width * height * 4` bytes).
    pub rgba: Vec<u8>,
}

impl CapturedImage {
    /// The RGBA bytes of the pixel at `(x, y)`, or `None` if out of bounds.
    pub fn pixel(&self, x: u32, y: u32) -> Option<[u8; 4]> {
        if x >= self.width || y >= self.height {
            return None;
        }
        let i = ((y * self.width + x) * 4) as usize;
        self.rgba.get(i..i + 4).map(|s| [s[0], s[1], s[2], s[3]])
    }

    /// Crop to `rect` (in this image's pixel space), clamped to the bounds. Returns
    /// the overlapping sub-image; empty (0×0) if there is no overlap.
    pub fn crop(&self, rect: Region) -> CapturedImage {
        let bounds = Region {
            x: 0,
            y: 0,
            w: self.width,
            h: self.height,
        };
        let Some(r) = rect.intersect(&bounds) else {
            return CapturedImage {
                width: 0,
                height: 0,
                rgba: Vec::new(),
            };
        };
        let row_bytes = (r.w * 4) as usize;
        let mut out = vec![0u8; row_bytes * r.h as usize];
        for row in 0..r.h {
            let sy = r.y as u32 + row;
            let src = ((sy * self.width + r.x as u32) * 4) as usize;
            let dst = row as usize * row_bytes;
            out[dst..dst + row_bytes].copy_from_slice(&self.rgba[src..src + row_bytes]);
        }
        CapturedImage {
            width: r.w,
            height: r.h,
            rgba: out,
        }
    }

    /// Composite this image into a `dst_w × dst_h` RGBA8 buffer at `(at_x, at_y)`,
    /// clipping to the destination. Used to stitch per-output captures into one
    /// multi-output region.
    pub fn blit_into(&self, dst: &mut [u8], dst_w: u32, dst_h: u32, at_x: i32, at_y: i32) {
        let dst_rect = Region {
            x: 0,
            y: 0,
            w: dst_w,
            h: dst_h,
        };
        let src_rect = Region {
            x: at_x,
            y: at_y,
            w: self.width,
            h: self.height,
        };
        let Some(r) = src_rect.intersect(&dst_rect) else {
            return;
        };
        let row_bytes = (r.w * 4) as usize;
        for row in 0..r.h {
            let dy = r.y as u32 + row;
            let sy = (r.y - at_y) as u32 + row;
            let sx = (r.x - at_x) as u32;
            let src = ((sy * self.width + sx) * 4) as usize;
            let dpos = ((dy * dst_w + r.x as u32) * 4) as usize;
            dst[dpos..dpos + row_bytes].copy_from_slice(&self.rgba[src..src + row_bytes]);
        }
    }
}

/// Byte layout of a wl_shm pixel format (memory order, little-endian), so we can
/// convert to RGBA8 and — crucially — compute the correct stride (`width * bpp`).
struct PixelLayout {
    bpp: usize,
    r: usize,
    g: usize,
    b: usize,
    a: Option<usize>,
}

impl PixelLayout {
    fn of(f: wl_shm::Format) -> Option<Self> {
        use wl_shm::Format::*;
        Some(match f {
            Argb8888 => Self {
                bpp: 4,
                r: 2,
                g: 1,
                b: 0,
                a: Some(3),
            },
            Xrgb8888 => Self {
                bpp: 4,
                r: 2,
                g: 1,
                b: 0,
                a: None,
            },
            Abgr8888 => Self {
                bpp: 4,
                r: 0,
                g: 1,
                b: 2,
                a: Some(3),
            },
            Xbgr8888 => Self {
                bpp: 4,
                r: 0,
                g: 1,
                b: 2,
                a: None,
            },
            Bgr888 => Self {
                bpp: 3,
                r: 0,
                g: 1,
                b: 2,
                a: None,
            },
            Rgb888 => Self {
                bpp: 3,
                r: 2,
                g: 1,
                b: 0,
                a: None,
            },
            _ => return None,
        })
    }

    /// Same, keyed by DRM fourcc (for dma-buf). DRM 32-bit codes use the same
    /// little-endian memory order as their wl_shm counterparts.
    #[cfg(feature = "gpu")]
    fn of_fourcc(f: u32) -> Option<Self> {
        Some(match f {
            // XR24 / AR24: little-endian B,G,R,(X|A)
            f if f == fourcc(b'X', b'R', b'2', b'4') => Self::of(wl_shm::Format::Xrgb8888)?,
            f if f == fourcc(b'A', b'R', b'2', b'4') => Self::of(wl_shm::Format::Argb8888)?,
            // XB24 / AB24: little-endian R,G,B,(X|A)
            f if f == fourcc(b'X', b'B', b'2', b'4') => Self::of(wl_shm::Format::Xbgr8888)?,
            f if f == fourcc(b'A', b'B', b'2', b'4') => Self::of(wl_shm::Format::Abgr8888)?,
            _ => return None,
        })
    }
}

#[derive(Default)]
struct PendingToplevel {
    identifier: String,
    title: String,
    app_id: String,
}

/// Opaque handle to a capture session, valid until [`Client::close_session`].
///
/// A plain counter rather than a protocol object id: a `zwlr-screencopy` session
/// owns no long-lived object (its frame object is recreated for every capture), so
/// there is nothing durable to name it after.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct SessionId(u64);

impl SessionId {
    fn next() -> Self {
        static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(1);
        SessionId(NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed))
    }
}

/// Per-session bookkeeping, updated by the session/frame Dispatch impls and keyed
/// by the session object id so multiple live sessions never clobber each other.
#[derive(Default)]
struct SessionData {
    /// Latest buffer constraints advertised by the compositor.
    width: u32,
    height: u32,
    format: Option<wl_shm::Format>,
    /// Row stride the compositor dictates, when it dictates one. `zwlr-screencopy`
    /// names the stride it will write with; `ext-image-copy-capture` leaves it to us.
    stride: Option<u32>,
    /// dma-buf device the compositor wants buffers allocated on (raw dev_t).
    #[cfg(feature = "gpu")]
    dmabuf_dev: Option<u64>,
    /// dma-buf formats advertised: (drm fourcc, supported modifiers). An empty
    /// modifier list means the compositor named a format but no layout, so the
    /// driver picks one (`zwlr-screencopy`'s `linux_dmabuf` event).
    #[cfg(feature = "gpu")]
    dmabuf_formats: Vec<(u32, Vec<u64>)>,
    /// Set once a constraints group (`done`) has been received.
    constraints_done: bool,
    /// `zwlr-screencopy`: the in-flight frame has announced its buffer types and is
    /// waiting for a `copy` request.
    awaiting_copy: bool,
    /// Whether this session has delivered a frame yet. The first `zwlr-screencopy`
    /// capture of a session is an unconditional `copy` (so a static source still
    /// shows something at once); later ones wait for damage.
    delivered: bool,
    /// The compositor reports the captured contents bottom-up (`y_invert`).
    y_invert: bool,
    /// Constraints changed since the buffer was last (re)allocated (e.g. resize).
    dirty: bool,
    /// Set when the current in-flight frame is ready to read.
    ready: bool,
    /// A transient per-frame failure (retry next round); `buffer_constraints`
    /// additionally triggers a reallocation. Not terminal.
    frame_failed: Option<FailureReason>,
    /// Terminal: the session/source stopped and won't produce more frames.
    stopped: bool,
    /// Set for a single capture that is closed right after (`capture_*_once`).
    ///
    /// Such a capture ends up as CPU pixels anyway — it is encoded, cropped or
    /// sampled — so importing a dma-buf only to read it straight back would build a
    /// whole EGL context per call for nothing. Live sessions, whose frames are
    /// sampled as GL textures, keep the zero-copy path.
    one_shot: bool,
}

/// A reusable buffer backing one session, kept alive between frames. Either a
/// CPU shm buffer (fallback) or a GPU dma-buf swapchain allocated through gbm.
enum Buf {
    Shm(ShmBuf),
    #[cfg(feature = "gpu")]
    Dmabuf(DmaBuf),
}

impl Buf {
    fn wl_buffer(&self) -> &WlBuffer {
        match self {
            Buf::Shm(b) => &b.buffer,
            #[cfg(feature = "gpu")]
            Buf::Dmabuf(b) => &b.buffer,
        }
    }
    /// Whether this is a GPU dma-buf rather than CPU shared memory.
    fn is_dmabuf(&self) -> bool {
        match self {
            Buf::Shm(_) => false,
            #[cfg(feature = "gpu")]
            Buf::Dmabuf(_) => true,
        }
    }
    /// Did the advertised constraints (size) change vs this buffer?
    fn matches(&self, w: u32, h: u32) -> bool {
        match self {
            Buf::Shm(b) => b.width == w && b.height == h,
            #[cfg(feature = "gpu")]
            Buf::Dmabuf(b) => b.width == w && b.height == h,
        }
    }
}

/// CPU shm buffer with a correct, format-specific stride.
struct ShmBuf {
    pool: WlShmPool,
    buffer: WlBuffer,
    _fd: OwnedFd,
    map: *mut c_void,
    size: usize,
    width: u32,
    height: u32,
    stride: usize,
    format: wl_shm::Format,
}

impl Drop for ShmBuf {
    fn drop(&mut self) {
        self.buffer.destroy();
        self.pool.destroy();
        unsafe {
            let _ = rustix::mm::munmap(self.map, self.size);
        }
    }
}

/// One dma-buf allocated via gbm: the compositor captures into it, and it is
/// imported zero-copy as a GL texture for display.
///
/// Single-buffered on purpose: `ext-image-copy-capture` captures *incrementally*
/// by damage, assuming the buffer it's given already holds the previous frame.
/// Reusing one buffer lets it accumulate the full image; alternating buffers
/// would leave undamaged regions of the "other" buffer empty (black) for static
/// windows. Sampling the buffer while the compositor updates a small damage
/// region is imperceptible at thumbnail scale.
#[cfg(feature = "gpu")]
struct DmaBuf {
    buffer: WlBuffer,
    bo: BufferObject<()>,
    width: u32,
    height: u32,
    fourcc: u32,
    modifier: u64,
    /// Plane count of the layout the driver picked. Compressed modifiers (Intel
    /// CCS, AMD DCC) add auxiliary planes on top of the pixel data; each one must
    /// be declared, or the buffer is malformed. Per-plane offsets and strides come
    /// from `bo` on demand.
    plane_count: u32,
}

#[cfg(feature = "gpu")]
impl Drop for DmaBuf {
    fn drop(&mut self) {
        self.buffer.destroy();
        // `bo` drops here, releasing the underlying dma-buf.
    }
}

/// A captured frame handed to the UI: either CPU pixels (shm) or a dma-buf
/// descriptor to import as a GL texture (GPU, zero-copy).
pub enum Frame {
    /// CPU pixels read back into shared memory (the shm fallback path).
    Shm(CapturedImage),
    /// A dma-buf descriptor to import as a GL texture (zero-copy GPU path).
    // Constructed only with the `gpu` feature; the display side (EGL import) is
    // always built since it needs no gbm.
    #[cfg_attr(not(feature = "gpu"), allow(dead_code))]
    Dmabuf(DmabufFrame),
}

/// One plane of a dma-buf: its own fd, offset and stride.
pub struct DmabufPlane {
    /// Owned file descriptor backing the plane (closed when this is dropped).
    pub fd: OwnedFd,
    /// Byte offset of the plane within the buffer.
    pub offset: u32,
    /// Row stride in bytes.
    pub stride: u32,
}

/// dma-buf descriptor for zero-copy GL import on the UI thread. The fds are owned
/// by the receiver; `buf_id` identifies the swapchain slot so the importer can cache
/// one GL texture per slot (their backing memory is stable).
pub struct DmabufFrame {
    /// Planes, in DRM order. A plain layout has one; compressed modifiers add
    /// auxiliary planes. EGL accepts at most 4.
    pub planes: Vec<DmabufPlane>,
    /// Width in pixels.
    pub width: u32,
    /// Height in pixels.
    pub height: u32,
    /// DRM `FourCC` pixel format code.
    pub fourcc: u32,
    /// DRM format modifier (tiling/compression layout).
    pub modifier: u64,
}

/// The protocol objects behind one open session.
///
/// `ext-image-copy-capture` has a session object that outlives the frames taken
/// from it, and announces its buffer constraints once. `zwlr-screencopy` has no
/// session object at all: each capture is a fresh frame created from the output,
/// which re-announces the constraints before it will accept a buffer. Both keep at
/// most one in-flight frame, since a frame object captures exactly one frame.
enum SessionObjects {
    ImageCopy {
        frame: Option<ExtImageCopyCaptureFrameV1>,
        session: ExtImageCopyCaptureSessionV1,
        src: ExtImageCaptureSourceV1,
    },
    Screencopy {
        frame: Option<ZwlrScreencopyFrameV1>,
        output: WlOutput,
    },
}

impl SessionObjects {
    /// Whether a frame is currently in flight.
    fn armed(&self) -> bool {
        match self {
            SessionObjects::ImageCopy { frame, .. } => frame.is_some(),
            SessionObjects::Screencopy { frame, .. } => frame.is_some(),
        }
    }

    /// Destroy the in-flight frame, if any.
    fn drop_frame(&mut self) {
        match self {
            SessionObjects::ImageCopy { frame, .. } => {
                if let Some(f) = frame.take() {
                    f.destroy();
                }
            }
            SessionObjects::Screencopy { frame, .. } => {
                if let Some(f) = frame.take() {
                    f.destroy();
                }
            }
        }
    }
}

impl Drop for SessionObjects {
    fn drop(&mut self) {
        self.drop_frame();
        match self {
            SessionObjects::ImageCopy { session, src, .. } => {
                session.destroy();
                src.destroy();
            }
            // The wl_output is the registry's, not the session's.
            SessionObjects::Screencopy { .. } => {}
        }
    }
}

/// An open capture session: its protocol objects plus the reusable buffer, re-armed
/// each round rather than torn down.
struct OpenSession {
    /// Declared first so it drops first: the buffer is released before the objects
    /// that referenced it.
    buf: Option<Buf>,
    objects: SessionObjects,
}

#[derive(Default)]
struct State {
    toplevels: Vec<Toplevel>,
    pending: Vec<(ExtForeignToplevelHandleV1, PendingToplevel)>,
    outputs: Vec<Output>,
    shm: Option<WlShm>,
    tl_src: Option<ExtForeignToplevelImageCaptureSourceManagerV1>,
    out_src: Option<ExtOutputImageCaptureSourceManagerV1>,
    copy: Option<ExtImageCopyCaptureManagerV1>,
    /// The `zwlr-screencopy` manager, bound only when that is the protocol in use.
    screencopy: Option<ZwlrScreencopyManagerV1>,
    /// The foreign-toplevel list, kept alive so the compositor keeps emitting
    /// toplevel events. `None` on compositors without window capture (wlroots < 0.20).
    list: Option<ExtForeignToplevelListV1>,
    /// The same windows as zwlr-foreign-toplevel-management describes them, used
    /// only to fill in app-ids the `ext` list leaves empty (see
    /// [`State::complete_app_ids`]).
    wlr_toplevels: Vec<ZwlrToplevel>,
    /// The manager those handles come from: it outlives them, and keeping it bound
    /// keeps the list current. `None` where the compositor lacks the protocol.
    _wlr_manager: Option<ZwlrForeignToplevelManagerV1>,
    /// linux-dmabuf manager, if the compositor exposes it (enables the GPU path).
    #[cfg(feature = "gpu")]
    dmabuf: Option<ZwpLinuxDmabufV1>,
    /// The GPU the compositor renders on (raw dev_t), read from linux-dmabuf's
    /// default feedback. `zwlr-screencopy` names no device of its own, and on a
    /// multi-GPU machine allocating on the wrong one makes every copy fail.
    #[cfg(feature = "gpu")]
    dmabuf_main_device: Option<u64>,
    /// Event bookkeeping for every live session.
    sessions: HashMap<SessionId, SessionData>,
}

/// A Wayland client that enumerates capturable toplevels and outputs and drives
/// their capture sessions, over `ext-image-copy-capture` or `zwlr-screencopy`.
pub struct Client {
    queue: EventQueue<State>,
    qh: QueueHandle<State>,
    state: State,
    /// The protocol this client drives; see [`select_protocol`].
    protocol: Protocol,
    /// Session-owned Wayland objects + buffers.
    open: HashMap<SessionId, OpenSession>,
    /// gbm device for dma-buf allocation, opened lazily on the first dma-buf
    /// session (matching the compositor's advertised device). `None` until then,
    /// or if the GPU path is unavailable (we then fall back to shm).
    #[cfg(feature = "gpu")]
    gbm: Option<GbmDevice<File>>,
    /// Whether the zero-copy dma-buf path is off, either because `WLR_NO_GPU` is
    /// set or because an import failed and [`Client::disable_gpu`] dropped us to
    /// shm. Present in every build so callers need no `cfg`.
    gpu_disabled: bool,
    /// Whether the compositor composites the mouse cursor into captured frames.
    /// Off by default: a screenshot should show the screen, not where the pointer
    /// happened to rest. Read when a session is opened, so it must be set before.
    paint_cursors: bool,
}

impl Client {
    /// Connect, bind the capture managers, and enumerate windows + outputs.
    pub fn connect() -> Result<Self> {
        let conn = Connection::connect_to_env().context("Wayland connection")?;
        let (globals, mut queue) =
            registry_queue_init::<State>(&conn).context("Wayland registry")?;
        let qh = queue.handle();

        let shm = globals.bind(&qh, 1..=1, ()).context("wl_shm")?;

        let mut advertised = Vec::new();
        globals.contents().with_list(|list| {
            for g in list {
                advertised.push((g.interface.clone(), g.version));
            }
        });
        let protocol = select_protocol(&advertised).ok_or_else(|| {
            CaptureError::msg(format!(
                "no capture protocol: neither {} nor {SCREENCOPY_GLOBAL}",
                IMAGE_COPY_GLOBALS.join(" + ")
            ))
        })?;

        // Window capture (the foreign-toplevel source + list) only landed in
        // wlroots >= 0.20 / Sway >= 1.12, and `zwlr-screencopy` never captures a
        // window at all. Bind them optionally so screen-only capture still works;
        // window-specific paths then fail with a clear error (see
        // `open_toplevel_session`).
        let (copy, tl_src, out_src, screencopy) = match protocol {
            Protocol::ImageCopyCapture => (
                Some(
                    globals
                        .bind(&qh, 1..=1, ())
                        .context("ext_image_copy_capture_manager_v1")?,
                ),
                globals.bind(&qh, 1..=1, ()).ok(),
                Some(
                    globals
                        .bind(&qh, 1..=1, ())
                        .context("ext_output_image_capture_source_manager_v1")?,
                ),
                None,
            ),
            // v3 is the floor: `linux_dmabuf` and `buffer_done` arrived with it, and
            // wlroots has shipped it since 0.11.
            Protocol::Screencopy => (
                None,
                None,
                None,
                Some(
                    globals
                        .bind(&qh, 3..=3, ())
                        .context("zwlr_screencopy_manager_v1 v3")?,
                ),
            ),
        };
        let list: Option<ExtForeignToplevelListV1> = globals.bind(&qh, 1..=1, ()).ok();
        // The older window list, bound alongside the `ext` one for its app-ids: on
        // Sway an XWayland window reaches `ext-foreign-toplevel-list` without one,
        // and zwlr reports its X11 class. Binding it here rather than on demand
        // costs no extra roundtrip — the compositor describes its windows within
        // the two roundtrips below, which we make anyway.
        let wlr_manager: Option<ZwlrForeignToplevelManagerV1> = globals.bind(&qh, 1..=3, ()).ok();

        // Optional: authoritative logical geometry (multi-monitor positions,
        // fractional scale). Absent on a few compositors — we then fall back to
        // wl_output-derived sizes.
        let xdg_mgr: Option<ZxdgOutputManagerV1> = globals.bind(&qh, 1..=3, ()).ok();

        globals.contents().with_list(|list| {
            for g in list {
                if g.interface == WlOutput::interface().name {
                    let out: WlOutput = globals.registry().bind(g.name, g.version.min(4), &qh, ());
                    if let Some(mgr) = &xdg_mgr {
                        // udata = the wl_output, so the xdg_output's logical-geometry
                        // events update the matching Output.
                        mgr.get_xdg_output(&out, &qh, out.clone());
                    }
                }
            }
        });

        let mut state = State {
            shm: Some(shm),
            copy,
            tl_src,
            out_src,
            screencopy,
            list,
            _wlr_manager: wlr_manager,
            ..Default::default()
        };
        // Optional: enables the GPU dma-buf path. Absence just means shm-only.
        #[cfg(feature = "gpu")]
        let feedback = {
            state.dmabuf = globals.bind(&qh, 3..=4, ()).ok();
            // An ext-image-copy-capture session names the device it wants buffers
            // on; a screencopy frame doesn't, so ask linux-dmabuf's default
            // feedback which GPU the compositor renders on. `get_default_feedback`
            // needs version 4.
            match (&state.dmabuf, protocol) {
                (Some(mgr), Protocol::Screencopy) if mgr.version() >= 4 => {
                    Some(mgr.get_default_feedback(&qh, ()))
                }
                _ => None,
            }
        };
        queue.roundtrip(&mut state).context("Wayland roundtrip")?;
        queue.roundtrip(&mut state).context("Wayland roundtrip")?;
        state.complete_app_ids();
        #[cfg(feature = "gpu")]
        if let Some(fb) = feedback {
            fb.destroy();
        }

        Ok(Self {
            queue,
            qh,
            state,
            protocol,
            open: HashMap::new(),
            #[cfg(feature = "gpu")]
            gbm: None,
            gpu_disabled: GPU_DISABLED.load(std::sync::atomic::Ordering::Relaxed)
                || std::env::var_os("WLR_NO_GPU").is_some(),
            paint_cursors: false,
        })
    }

    /// Ask the compositor to composite the mouse cursor into captured frames.
    /// Sessions read this when they open, so set it before the first capture.
    pub fn set_paint_cursors(&mut self, on: bool) {
        self.paint_cursors = on;
    }

    /// Whether captures include the mouse cursor (see [`Client::set_paint_cursors`]).
    pub fn paint_cursors(&self) -> bool {
        self.paint_cursors
    }

    /// The `ext-image-copy-capture` session options matching the cursor setting.
    fn capture_options(&self) -> Options {
        if self.paint_cursors {
            Options::PaintCursors
        } else {
            Options::empty()
        }
    }

    /// The `zwlr-screencopy` `overlay_cursor` argument matching the cursor setting.
    fn overlay_cursor(&self) -> i32 {
        if self.paint_cursors {
            CURSOR_ON
        } else {
            CURSOR_OFF
        }
    }

    /// Stop using the dma-buf path and capture into shm from now on. Call this when
    /// an import fails: the buffers already handed to the compositor are dropped and
    /// reallocated, so the next frame arrives as CPU pixels rather than as a
    /// descriptor nothing can import.
    pub fn disable_gpu(&mut self) {
        if self.gpu_disabled {
            return;
        }
        self.gpu_disabled = true;
        for d in self.state.sessions.values_mut() {
            d.dirty = true;
        }
    }

    /// Whether the dma-buf path is off (see [`Client::disable_gpu`]).
    pub fn gpu_disabled(&self) -> bool {
        self.gpu_disabled
    }

    /// The currently known capturable windows.
    pub fn toplevels(&self) -> &[Toplevel] {
        &self.state.toplevels
    }
    /// The currently known capturable outputs.
    pub fn outputs(&self) -> &[Output] {
        &self.state.outputs
    }

    /// Whether this compositor can capture individual windows (the foreign-toplevel
    /// image-capture source *and* list — wlroots >= 0.20 / Sway >= 1.12). When `false`,
    /// only screen (output) capture works; window paths return a clear error.
    ///
    /// Always `false` under [`Protocol::Screencopy`], which only addresses outputs.
    pub fn can_capture_windows(&self) -> bool {
        self.state.tl_src.is_some() && self.state.list.is_some()
    }

    /// The capture protocol this client drives.
    pub fn protocol(&self) -> Protocol {
        self.protocol
    }

    /// Drain pending Wayland events (new/closed toplevels, etc.) without blocking
    /// on a capture, so the source list stays current between capture rounds.
    pub fn refresh(&mut self) -> Result<()> {
        self.queue
            .roundtrip(&mut self.state)
            .context("Wayland roundtrip")?;
        self.state.complete_app_ids();
        Ok(())
    }

    /// Open a persistent capture session for a window. The session and its buffer
    /// live until [`Client::close_session`] (or the source disappears); re-arm a
    /// frame each cycle with [`Client::capture`].
    pub fn open_toplevel_session(&mut self, t: &Toplevel) -> Result<SessionId, CaptureError> {
        let (Some(tl_src), Some(copy)) = (self.state.tl_src.clone(), self.state.copy.clone())
        else {
            return Err(CaptureError::WindowsUnsupported);
        };
        let id = self.new_session();
        let src = tl_src.create_source(&t.handle, &self.qh, ());
        let session = copy.create_session(&src, self.capture_options(), &self.qh, id);
        self.await_constraints(
            id,
            SessionObjects::ImageCopy {
                frame: None,
                session,
                src,
            },
        )
    }

    /// Open a persistent capture session for an output. See [`Client::open_toplevel_session`].
    pub fn open_output_session(&mut self, o: &Output) -> Result<SessionId> {
        let id = self.new_session();
        let objects = match self.protocol {
            Protocol::ImageCopyCapture => {
                let out_src = self
                    .state
                    .out_src
                    .clone()
                    .context("ext_output_image_capture_source_manager_v1 missing")?;
                let copy = self
                    .state
                    .copy
                    .clone()
                    .context("ext_image_copy_capture_manager_v1 missing")?;
                let src = out_src.create_source(&o.wl_output, &self.qh, ());
                let session = copy.create_session(&src, self.capture_options(), &self.qh, id);
                SessionObjects::ImageCopy {
                    frame: None,
                    session,
                    src,
                }
            }
            Protocol::Screencopy => {
                let mgr = self
                    .state
                    .screencopy
                    .clone()
                    .context("zwlr_screencopy_manager_v1 missing")?;
                // The first frame doubles as the constraints probe: screencopy only
                // announces them from a frame, and the frame we learn them from is
                // the one `poll` then copies into.
                let frame = mgr.capture_output(self.overlay_cursor(), &o.wl_output, &self.qh, id);
                SessionObjects::Screencopy {
                    frame: Some(frame),
                    output: o.wl_output.clone(),
                }
            }
        };
        self.await_constraints(id, objects)
    }

    /// Allocate a session id and its bookkeeping slot, before any protocol object
    /// is created with it as user data.
    fn new_session(&mut self) -> SessionId {
        let id = SessionId::next();
        self.state.sessions.insert(id, SessionData::default());
        id
    }

    /// Block until the compositor has described the buffer it wants, then register
    /// the session. `ext-image-copy-capture` announces that once per session
    /// (`buffer_size` + `shm_format` + `done`); `zwlr-screencopy` announces it per
    /// frame (`buffer` + `linux_dmabuf` + `buffer_done`).
    fn await_constraints(&mut self, id: SessionId, objects: SessionObjects) -> Result<SessionId> {
        loop {
            self.queue
                .blocking_dispatch(&mut self.state)
                .context("Wayland dispatch")?;
            let d = self.state.sessions.get(&id).context("session gone")?;
            if d.constraints_done || d.stopped {
                break;
            }
        }
        if self.state.sessions.get(&id).is_none_or(|d| d.stopped) {
            self.state.sessions.remove(&id);
            // `objects` drops here, releasing whatever was created.
            return Err(CaptureError::msg(
                "capture session stopped before first frame",
            ));
        }

        self.open.insert(id, OpenSession { buf: None, objects });
        Ok(id)
    }

    /// Tear down a session (e.g. its window closed).
    pub fn close_session(&mut self, id: &SessionId) {
        self.open.remove(id); // Drop releases frame + buffer + session + source
        self.state.sessions.remove(id);
    }

    /// One-shot: capture a single frame of `output`, then tear the session down.
    /// Blocks up to `budget`. For screenshots / timelapse ticks.
    pub fn capture_output_once(
        &mut self,
        output: &Output,
        budget: Duration,
    ) -> Result<Frame, CaptureError> {
        self.capture_output_frame(output, budget, true)
    }

    /// One-shot: capture a single frame of `toplevel`, then tear the session down.
    pub fn capture_toplevel_once(
        &mut self,
        toplevel: &Toplevel,
        budget: Duration,
    ) -> Result<Frame, CaptureError> {
        let id = self.open_toplevel_session(toplevel)?;
        if let Some(d) = self.state.sessions.get_mut(&id) {
            d.one_shot = true;
        }
        let r = self.poll_one(&id, budget);
        self.close_session(&id);
        r
    }

    /// Capture one frame from `output`, through the shm path (`one_shot`) or the
    /// live dma-buf path.
    fn capture_output_frame(
        &mut self,
        output: &Output,
        budget: Duration,
        one_shot: bool,
    ) -> Result<Frame, CaptureError> {
        let id = self.open_output_session(output)?;
        if one_shot && let Some(d) = self.state.sessions.get_mut(&id) {
            d.one_shot = true;
        }
        let r = self.poll_one(&id, budget);
        self.close_session(&id);
        r
    }

    /// Capture one frame the way a live consumer would, so the dma-buf path is
    /// exercised even for a single frame. This is what `doctor` probes with;
    /// ordinary single captures want [`Client::capture_output_once`].
    pub fn probe_gpu_capture(
        &mut self,
        output: &Output,
        budget: Duration,
    ) -> Result<Frame, CaptureError> {
        self.capture_output_frame(output, budget, false)
    }

    /// Poll until session `id` yields a frame, it stops, or `budget` elapses.
    /// Frames from other open sessions in this round are discarded.
    fn poll_one(&mut self, id: &SessionId, budget: Duration) -> Result<Frame, CaptureError> {
        let deadline = Instant::now() + budget;
        loop {
            let now = Instant::now();
            if now >= deadline {
                return Err(CaptureError::CaptureTimeout);
            }
            let step = Duration::from_millis(50).min(deadline - now);
            let (frames, stopped) = self.poll(step);
            for (sid, frame) in frames {
                if &sid == id {
                    return Ok(frame);
                }
            }
            if stopped.iter().any(|s| s == id) {
                return Err(CaptureError::msg(
                    "capture: session stopped before first frame",
                ));
            }
        }
    }

    /// Drive all open sessions for up to `budget`: arm a frame on every idle
    /// session, wait for events, and return the frames that became ready (the
    /// sources that produced new content). Sessions whose source is static simply
    /// keep their frame armed and deliver nothing — which is exactly right, there
    /// is nothing new to show.
    ///
    /// Also returns the ids of sessions the compositor stopped (e.g. their window
    /// closed), so the caller can drop and (if still listed) reopen them.
    pub fn poll(&mut self, budget: Duration) -> (Vec<(SessionId, Frame)>, Vec<SessionId>) {
        let deadline = Instant::now() + budget;
        loop {
            self.arm_sessions();
            let remaining = deadline.saturating_duration_since(Instant::now());
            let _ = self.dispatch_timeout(remaining);
            // A screencopy capture is armed in two steps — create the frame, then
            // copy into it once the compositor has described the buffer it wants —
            // so go round again and let both happen inside one `poll` rather than
            // one per call. `ext-image-copy-capture` never waits like this and
            // leaves after a single pass.
            let pending_copy = self
                .state
                .sessions
                .values()
                .any(|d| d.awaiting_copy && !d.stopped);
            if !pending_copy || remaining.is_zero() {
                break;
            }
        }
        self.harvest_sessions()
    }

    /// Arm every idle session, and hand a buffer to every screencopy frame that has
    /// announced its constraints. Idempotent: a session with a capture already in
    /// flight is left alone.
    fn arm_sessions(&mut self) {
        let qh = self.qh.clone();
        let screencopy = self.state.screencopy.clone();
        // Read before the loop: re-arming borrows `self` mutably per session.
        let overlay_cursor = self.overlay_cursor();
        for id in self.open.keys().copied().collect::<Vec<_>>() {
            let awaiting_copy = match self.state.sessions.get(&id) {
                Some(d) if !d.stopped => d.awaiting_copy,
                _ => continue,
            };
            let armed = match self.open.get(&id) {
                Some(os) => os.objects.armed(),
                None => continue,
            };
            // The screencopy frame that carried the constraints is already in
            // flight; what it still needs is the buffer.
            if armed && !awaiting_copy {
                continue;
            }

            // A screencopy session with nothing in flight starts a frame and stops
            // there: the buffer can only be handed over once that frame has said
            // which one it wants, and it says so afresh every time.
            let is_screencopy = matches!(
                self.open.get(&id).map(|os| &os.objects),
                Some(SessionObjects::Screencopy { .. })
            );
            if !armed && is_screencopy {
                let Some(mgr) = screencopy.as_ref() else {
                    continue;
                };
                if let Some(d) = self.state.sessions.get_mut(&id) {
                    d.ready = false;
                    d.constraints_done = false;
                }
                if let Some(os) = self.open.get_mut(&id)
                    && let SessionObjects::Screencopy { frame, output } = &mut os.objects
                {
                    *frame = Some(mgr.capture_output(overlay_cursor, output, &qh, id));
                }
                continue;
            }

            if self.ensure_buffer(&id).is_err() {
                continue;
            }
            let Some(wl_buffer) = self
                .open
                .get(&id)
                .and_then(|os| os.buf.as_ref())
                .map(|b| b.wl_buffer().clone())
            else {
                continue;
            };
            // The first capture of a session must land whatever the source is doing,
            // so the caller sees something at once; only afterwards does waiting for
            // damage keep a static source quiet.
            let delivered = self.state.sessions.get(&id).is_some_and(|d| d.delivered);
            if let Some(d) = self.state.sessions.get_mut(&id) {
                d.ready = false;
                d.awaiting_copy = false;
            }
            let Some(os) = self.open.get_mut(&id) else {
                continue;
            };
            match &mut os.objects {
                SessionObjects::ImageCopy { frame, session, .. } => {
                    let f = session.create_frame(&qh, id);
                    f.attach_buffer(&wl_buffer);
                    f.capture();
                    *frame = Some(f);
                }
                SessionObjects::Screencopy { frame, .. } => {
                    let Some(f) = frame.as_ref() else { continue };
                    if delivered {
                        f.copy_with_damage(&wl_buffer);
                    } else {
                        f.copy(&wl_buffer);
                    }
                }
            }
        }
    }

    /// Collect the frames that became ready, retry transient failures, and report
    /// the sessions the compositor stopped.
    fn harvest_sessions(&mut self) -> (Vec<(SessionId, Frame)>, Vec<SessionId>) {
        let mut frames = Vec::new();
        let mut stopped = Vec::new();
        for id in self.open.keys().copied().collect::<Vec<_>>() {
            let (ready, is_stopped, frame_failed, y_invert) = self
                .state
                .sessions
                .get(&id)
                .map(|d| (d.ready, d.stopped, d.frame_failed, d.y_invert))
                .unwrap_or((false, false, None, false));

            // Terminal: source gone. Drop the in-flight frame and report it.
            if is_stopped {
                if let Some(os) = self.open.get_mut(&id) {
                    os.objects.drop_frame();
                }
                stopped.push(id);
                continue;
            }

            if ready {
                // A bottom-up dma-buf cannot be handed on as it is (see `harvest`):
                // discard it and reallocate, so the next round comes through shm.
                let gpu_inverted =
                    y_invert && self.open[&id].buf.as_ref().is_some_and(Buf::is_dmabuf);
                let frame = self.open[&id]
                    .buf
                    .as_ref()
                    .filter(|_| !gpu_inverted)
                    .and_then(|b| harvest(b, y_invert));
                if let Some(os) = self.open.get_mut(&id) {
                    os.objects.drop_frame();
                }
                if let Some(d) = self.state.sessions.get_mut(&id) {
                    d.ready = false;
                    d.frame_failed = None;
                    d.dirty |= gpu_inverted;
                    d.delivered |= frame.is_some();
                }
                if let Some(frame) = frame {
                    frames.push((id, frame));
                }
            } else if let Some(reason) = frame_failed {
                // Transient: drop the failed frame and re-arm next round. A
                // buffer_constraints failure also means our buffer is stale.
                if let Some(os) = self.open.get_mut(&id) {
                    os.objects.drop_frame();
                }
                if let Some(d) = self.state.sessions.get_mut(&id) {
                    d.frame_failed = None;
                    d.awaiting_copy = false;
                    if matches!(reason, FailureReason::BufferConstraints) {
                        d.dirty = true; // size/format changed → reallocate
                    }
                }
            }
        }
        (frames, stopped)
    }

    /// Dispatch Wayland events for at most `budget`, returning early once the fd
    /// goes quiet. Mirrors the crate's `blocking_read` but with a `poll` timeout
    /// so a desktop with no damage doesn't block us forever.
    fn dispatch_timeout(&mut self, budget: Duration) -> Result<()> {
        self.queue
            .dispatch_pending(&mut self.state)
            .context("Wayland dispatch")?;
        self.queue.flush().context("Wayland flush")?;
        let deadline = Instant::now() + budget;
        loop {
            let remaining = deadline.saturating_duration_since(Instant::now());
            if remaining.is_zero() {
                break;
            }
            let Some(guard) = self.queue.prepare_read() else {
                // Events already queued: dispatch them and re-check.
                self.queue
                    .dispatch_pending(&mut self.state)
                    .context("Wayland dispatch")?;
                continue;
            };
            let ts = Timespec {
                tv_sec: remaining.as_secs() as _,
                tv_nsec: remaining.subsec_nanos() as _,
            };
            // Scope the borrowed fd so it is released before `guard.read()` (which
            // consumes the guard).
            let poll_res = {
                let fd = guard.connection_fd();
                let mut fds = [PollFd::new(&fd, PollFlags::IN | PollFlags::ERR)];
                rustix::event::poll(&mut fds, Some(&ts))
            };
            match poll_res {
                Ok(0) => break, // timeout: no events within the budget
                Ok(_) => {
                    guard.read().context("reading Wayland events")?;
                    self.queue
                        .dispatch_pending(&mut self.state)
                        .context("Wayland dispatch")?;
                }
                Err(rustix::io::Errno::INTR) => continue,
                Err(e) => return Err(CaptureError::msg(format!("poll: {e}"))),
            }
        }
        Ok(())
    }

    /// Ensure the session has a usable buffer, (re)allocating only when it is
    /// missing or the size changed (window resized). Prefers the GPU dma-buf path
    /// and falls back to shm. The buffer is reused across frames otherwise.
    fn ensure_buffer(&mut self, id: &SessionId) -> Result<()> {
        let (w, h, dirty) = {
            let d = self.state.sessions.get(id).context("unknown session")?;
            (d.width, d.height, d.dirty)
        };
        let fits = self
            .open
            .get(id)
            .and_then(|o| o.buf.as_ref())
            .is_some_and(|b| b.matches(w, h));
        // Reuse unless the size changed or a buffer_constraints failure marked it
        // dirty (then reallocate even at the same size).
        if fits && !dirty {
            return Ok(());
        }
        if w == 0 || h == 0 {
            return Err(CaptureError::msg("zero-sized capture"));
        }

        // Prefer dma-buf (GPU); fall back to shm if it isn't available/usable.
        #[cfg(feature = "gpu")]
        let buf = match self.alloc_dmabuf(id, w, h) {
            Some(b) => b,
            None => self.alloc_shm(id, w, h)?,
        };
        #[cfg(not(feature = "gpu"))]
        let buf = self.alloc_shm(id, w, h)?;
        // Install the new buffer; the old one (if any) drops here, releasing it.
        self.open.get_mut(id).context("session not open")?.buf = Some(buf);
        self.state.sessions.get_mut(id).unwrap().dirty = false;
        Ok(())
    }

    /// Allocate a CPU shm buffer with the format-correct stride.
    fn alloc_shm(&mut self, id: &SessionId, w: u32, h: u32) -> Result<Buf> {
        let (format, wanted_stride) = self
            .state
            .sessions
            .get(id)
            .and_then(|d| Some((d.format?, d.stride)))
            .context("compositor offered no shm format")?;
        let layout = PixelLayout::of(format)
            .with_context(|| format!("unsupported shm format: {format:?}"))?;
        // The format's own bytes-per-pixel is the floor; honour a wider stride when
        // the compositor names one, since it will write rows at that pitch.
        let packed = w as usize * layout.bpp;
        let stride = wanted_stride.map_or(packed, |s| (s as usize).max(packed));
        let size = stride * h as usize;

        let fd = rustix::fs::memfd_create("wlr-chooser-shm", rustix::fs::MemfdFlags::CLOEXEC)
            .context("memfd_create")?;
        rustix::fs::ftruncate(&fd, size as u64).context("ftruncate")?;
        let map = unsafe {
            rustix::mm::mmap(
                std::ptr::null_mut(),
                size,
                rustix::mm::ProtFlags::READ | rustix::mm::ProtFlags::WRITE,
                rustix::mm::MapFlags::SHARED,
                &fd,
                0,
            )
            .context("mmap")?
        };
        let pool =
            self.state
                .shm
                .as_ref()
                .unwrap()
                .create_pool(fd.as_fd(), size as i32, &self.qh, ());
        let buffer = pool.create_buffer(0, w as i32, h as i32, stride as i32, format, &self.qh, ());
        Ok(Buf::Shm(ShmBuf {
            pool,
            buffer,
            _fd: fd,
            map,
            size,
            width: w,
            height: h,
            stride,
            format,
        }))
    }

    /// Try to allocate a dma-buf (via gbm) and wrap it as a wl_buffer. Returns
    /// `None` whenever the GPU path isn't usable (no manager, no suitable format,
    /// gbm/allocation failure) so the caller falls back to shm.
    #[cfg(feature = "gpu")]
    fn alloc_dmabuf(&mut self, id: &SessionId, w: u32, h: u32) -> Option<Buf> {
        // `y_invert`: the compositor writes this source bottom-up, and the engine's
        // contract is top-down frames. Flipping shm pixels is a memcpy; flipping a
        // dma-buf would mean a blit in every consumer, so such a session stays on shm.
        if self.gpu_disabled
            || self
                .state
                .sessions
                .get(id)
                .is_some_and(|d| d.one_shot || d.y_invert)
        {
            return None;
        }
        let dmabuf_mgr = self.state.dmabuf.as_ref().cloned()?;
        let (formats, dev) = {
            let d = self.state.sessions.get(id)?;
            (
                d.dmabuf_formats.clone(),
                d.dmabuf_dev.or(self.state.dmabuf_main_device),
            )
        };
        let Some((fourcc, mods)) = pick_dmabuf_format(&formats) else {
            if debug() {
                eprintln!("wlr-capture: no usable dma-buf format");
            }
            return None;
        };
        self.ensure_gbm(dev)?;
        let gbm = self.gbm.as_ref()?;
        let gfmt = GbmFormat::try_from(fourcc).ok()?;
        let qh = &self.qh;

        // Allocate one swapchain slot: a gbm bo wrapped as a dma-buf wl_buffer.
        let alloc_slot = || -> Option<DmaBuf> {
            // No modifier list means the compositor named a format but no layout
            // (`zwlr-screencopy`), so the driver picks one and we report what it
            // chose; otherwise it must come from the advertised set.
            let bo = if mods.is_empty() {
                gbm.create_buffer_object::<()>(w, h, gfmt, BufferObjectFlags::RENDERING)
                    .ok()?
            } else {
                gbm.create_buffer_object_with_modifiers2::<()>(
                    w,
                    h,
                    gfmt,
                    mods.iter().map(|&m| Modifier::from(m)),
                    BufferObjectFlags::RENDERING,
                )
                .ok()?
            };
            let modifier: u64 = bo.modifier().into();
            let plane_count = bo.plane_count();
            if plane_count == 0 || plane_count > MAX_DMABUF_PLANES {
                if debug() {
                    eprintln!("wlr-capture: dma-buf with {plane_count} planes is not importable");
                }
                return None;
            }

            // Every plane must be declared: the compositor rejects (or silently
            // mis-imports) a buffer whose plane count doesn't match its modifier.
            let params = dmabuf_mgr.create_params(qh, ());
            for i in 0..plane_count {
                let fd = bo.fd_for_plane(i as i32).ok()?;
                params.add(
                    fd.as_fd(),
                    i,
                    bo.offset(i as i32),
                    bo.stride_for_plane(i as i32),
                    (modifier >> 32) as u32,
                    (modifier & 0xffff_ffff) as u32,
                );
            }
            let buffer = params.create_immed(
                w as i32,
                h as i32,
                fourcc,
                zwp_linux_buffer_params_v1::Flags::empty(),
                qh,
                (),
            );
            params.destroy();
            Some(DmaBuf {
                buffer,
                bo,
                width: w,
                height: h,
                fourcc,
                modifier,
                plane_count,
            })
        };

        let buf = alloc_slot()?;
        if debug() {
            eprintln!(
                "wlr-capture: dma-buf {w}x{h} fourcc={fourcc:#010x} modifier={} planes={}",
                buf.modifier, buf.plane_count
            );
        }
        Some(Buf::Dmabuf(buf))
    }

    /// Open the gbm device for the compositor's advertised dma-buf device, once.
    /// Returns `None` (so callers fall back to shm) if it can't be opened.
    #[cfg(feature = "gpu")]
    fn ensure_gbm(&mut self, dev: Option<u64>) -> Option<()> {
        if self.gbm.is_some() {
            return Some(());
        }
        let path = render_node_for(dev);
        let file = std::fs::OpenOptions::new()
            .read(true)
            .write(true)
            .open(&path)
            .ok()?;
        let device = GbmDevice::new(file).ok()?;
        if debug() {
            eprintln!("wlr-capture: gbm device {}", path.display());
        }
        self.gbm = Some(device);
        Some(())
    }
}

/// Build a DRM fourcc code from its four ASCII bytes.
#[cfg(feature = "gpu")]
const fn fourcc(a: u8, b: u8, c: u8, d: u8) -> u32 {
    (a as u32) | ((b as u32) << 8) | ((c as u32) << 16) | ((d as u32) << 24)
}

/// Pick a dma-buf format we can both allocate and decode, plus its usable
/// modifiers (dropping `INVALID`). Prefers the common 32-bit RGB layouts.
///
/// An empty modifier list on the way in means the compositor named the format
/// without naming a layout, and an empty list on the way out says the same to the
/// allocator: let the driver choose.
#[cfg(feature = "gpu")]
fn pick_dmabuf_format(formats: &[(u32, Vec<u64>)]) -> Option<(u32, Vec<u64>)> {
    let preferred = [
        fourcc(b'X', b'R', b'2', b'4'), // XRGB8888
        fourcc(b'A', b'R', b'2', b'4'), // ARGB8888
        fourcc(b'X', b'B', b'2', b'4'), // XBGR8888
        fourcc(b'A', b'B', b'2', b'4'), // ABGR8888
    ];
    for want in preferred {
        if PixelLayout::of_fourcc(want).is_none() {
            continue;
        }
        if let Some((_, mods)) = formats.iter().find(|(f, _)| *f == want) {
            let usable: Vec<u64> = mods
                .iter()
                .copied()
                .filter(|&m| m != DRM_MOD_INVALID)
                .collect();
            if !usable.is_empty() || mods.is_empty() {
                return Some((want, usable));
            }
        }
    }
    None
}

/// The render node of the DRM device with this `dev_t`, through sysfs. A
/// compositor may name its *primary* node (`card0`), whose render node is its
/// sibling in `/sys/dev/char/<major>:<minor>/device/drm/`.
#[cfg(feature = "gpu")]
fn sysfs_render_node(dev: u64) -> Option<std::path::PathBuf> {
    let dir = format!(
        "/sys/dev/char/{}:{}/device/drm",
        rustix::fs::major(dev),
        rustix::fs::minor(dev)
    );
    std::fs::read_dir(dir).ok()?.flatten().find_map(|e| {
        let name = e.file_name();
        let name = name.to_str()?;
        name.starts_with("renderD")
            .then(|| std::path::Path::new("/dev/dri").join(name))
    })
}

/// Resolve the DRM render node to allocate on. Best effort: match the advertised
/// dev_t against `/dev/dri/renderD*`, then through sysfs, else the first render
/// node, else renderD128.
#[cfg(feature = "gpu")]
fn render_node_for(dev: Option<u64>) -> std::path::PathBuf {
    use std::path::PathBuf;
    let render_nodes = || -> Vec<PathBuf> {
        let mut v: Vec<PathBuf> = std::fs::read_dir("/dev/dri")
            .into_iter()
            .flatten()
            .flatten()
            .map(|e| e.path())
            .filter(|p| {
                p.file_name()
                    .and_then(|n| n.to_str())
                    .is_some_and(|n| n.starts_with("renderD"))
            })
            .collect();
        v.sort();
        v
    };
    let nodes = render_nodes();
    if let Some(dev) = dev {
        for p in &nodes {
            if rustix::fs::stat(p).is_ok_and(|st| st.st_rdev == dev) {
                return p.clone();
            }
        }
        if let Some(p) = sysfs_render_node(dev) {
            return p;
        }
    }
    nodes
        .into_iter()
        .next()
        .unwrap_or_else(|| PathBuf::from("/dev/dri/renderD128"))
}

/// Whether verbose capture diagnostics are enabled (`WLR_UTILS_DEBUG`).
fn debug() -> bool {
    std::env::var_os("WLR_UTILS_DEBUG").is_some()
}

/// Turn a ready capture into a [`Frame`] for the UI. shm is read back + converted
/// to RGBA on the CPU; dma-buf is handed off zero-copy as an fd to import as a GL
/// texture (re-exporting an fd for the buffer the compositor just wrote).
///
/// `y_invert` means the compositor wrote the rows bottom-up; every [`Frame`] this
/// crate hands out is top-down, so the shm path flips here. A dma-buf never reaches
/// this point inverted: [`Client::ensure_buffer`] keeps an inverted session on shm,
/// since flipping a GPU buffer would cost a blit at every consumer.
fn harvest(buf: &Buf, y_invert: bool) -> Option<Frame> {
    match buf {
        Buf::Shm(b) => {
            let layout = PixelLayout::of(b.format).expect("format validated at alloc time");
            let raw = unsafe { std::slice::from_raw_parts(b.map as *const u8, b.size) };
            let mut img = convert(raw, b.width, b.height, b.stride, &layout);
            if y_invert {
                flip_vertically(&mut img);
            }
            Some(Frame::Shm(img))
        }
        #[cfg(feature = "gpu")]
        Buf::Dmabuf(b) => {
            let mut planes = Vec::with_capacity(b.plane_count as usize);
            for i in 0..b.plane_count as i32 {
                planes.push(DmabufPlane {
                    fd: b.bo.fd_for_plane(i).ok()?,
                    offset: b.bo.offset(i),
                    stride: b.bo.stride_for_plane(i),
                });
            }
            Some(Frame::Dmabuf(DmabufFrame {
                planes,
                width: b.width,
                height: b.height,
                fourcc: b.fourcc,
                modifier: b.modifier,
            }))
        }
    }
}

/// Reverse the row order of an RGBA8 image, in place.
fn flip_vertically(img: &mut CapturedImage) {
    let row = img.width as usize * 4;
    if row == 0 {
        return;
    }
    let rows = img.rgba.len() / row;
    let (top, bottom) = img.rgba.split_at_mut(rows / 2 * row);
    for (t, b) in top
        .chunks_exact_mut(row)
        .zip(bottom.chunks_exact_mut(row).rev())
    {
        t.swap_with_slice(b);
    }
}

/// Pixel-format conversion to RGBA8 shared by the shm and dma-buf paths.
fn convert(raw: &[u8], w: u32, h: u32, stride: usize, layout: &PixelLayout) -> CapturedImage {
    let (w, h) = (w as usize, h as usize);
    let mut rgba = vec![0u8; w * h * 4];
    for y in 0..h {
        for x in 0..w {
            let s = y * stride + x * layout.bpp;
            let d = (y * w + x) * 4;
            rgba[d] = raw[s + layout.r];
            rgba[d + 1] = raw[s + layout.g];
            rgba[d + 2] = raw[s + layout.b];
            rgba[d + 3] = match layout.a {
                Some(a) => raw[s + a],
                None => 255,
            };
        }
    }
    CapturedImage {
        width: w as u32,
        height: h as u32,
        rgba,
    }
}

// --- Window activation and focus (zwlr-foreign-toplevel-management) ---
//
// Capture uses ext-foreign-toplevel-list (stable `identifier`), but activation and
// the "which window is active right now" question need zwlr handles, a separate
// object namespace. We correlate the two by app_id + title — the only key both
// expose — plus a creation-order index among identical windows. Each is a
// self-contained, one-shot path on its own connection.
//
// cosmic-comp advertises none of this; `activate_window` falls back to the COSMIC
// toplevel manager there, which needs no such correlation. See `cosmic_activate`.

/// A toplevel as zwlr-foreign-toplevel-management advertises it.
struct ZwlrToplevel {
    handle: ZwlrForeignToplevelHandleV1,
    app_id: String,
    title: String,
    /// Whether the compositor reports this window as `activated` (focused).
    activated: bool,
}

/// The identity of a window, in the terms both foreign-toplevel protocols share.
/// Correlates a zwlr handle to a capture [`Toplevel`] (and to a chooser tile).
///
/// The app-ids on both sides line up because [`Client`] completes the ones `ext`
/// leaves empty from this very protocol (see `State::complete_app_ids`); a window
/// zwlr does not name stays unmatched. Callers treat a failed match as "unknown",
/// never as an error.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct WindowIdentity {
    /// The application id.
    pub app_id: String,
    /// The window title.
    pub title: String,
    /// Ordinal among the windows sharing this (app_id, title), in creation order.
    pub dup_index: usize,
}

/// Enumeration state for [`activate_window`] and [`active_window`].
#[derive(Default)]
struct ActState {
    toplevels: Vec<ZwlrToplevel>,
}

impl ActState {
    /// The ordinal of the toplevel at `i` among those sharing its (app_id, title),
    /// in the order the compositor advertised them — which is creation order on
    /// wlroots, for both foreign-toplevel protocols.
    fn dup_index(&self, i: usize) -> usize {
        let t = &self.toplevels[i];
        dup_index(
            self.toplevels[..i]
                .iter()
                .map(|o| (o.app_id.as_str(), o.title.as_str())),
            &t.app_id,
            &t.title,
        )
    }
}

/// How many of the windows advertised before this one share its identity. Free
/// function so the rule is unit-testable without a live compositor.
fn dup_index<'a>(
    prior: impl Iterator<Item = (&'a str, &'a str)>,
    app_id: &str,
    title: &str,
) -> usize {
    prior.filter(|&(a, t)| a == app_id && t == title).count()
}

/// A live zwlr-foreign-toplevel-management connection, with the toplevels the
/// compositor advertised on bind. Kept as a whole so the handles stay usable.
struct ToplevelEnumeration {
    globals: GlobalList,
    queue: EventQueue<ActState>,
    state: ActState,
    /// The manager and its connection outlive every handle taken from them.
    _manager: ZwlrForeignToplevelManagerV1,
    _conn: Connection,
}

/// Connect, bind zwlr-foreign-toplevel-management and collect the toplevels it
/// advertises, with their app-id, title and activation state.
///
/// `Ok(None)` means the compositor advertises no `zwlr_foreign_toplevel_manager_v1` —
/// cosmic-comp, for one. That is a fact about the compositor, not a failure, and each
/// caller answers it its own way.
fn enumerate_toplevels() -> Result<Option<ToplevelEnumeration>> {
    let conn = Connection::connect_to_env().context("Wayland connection")?;
    let (globals, mut queue) =
        registry_queue_init::<ActState>(&conn).context("Wayland registry")?;
    let qh = queue.handle();
    let Ok(manager) = globals.bind::<ZwlrForeignToplevelManagerV1, _, _>(&qh, 1..=3, ()) else {
        return Ok(None);
    };

    // Binding the manager makes the compositor advertise current toplevels: the
    // first roundtrip brings the handles, the second the events describing them.
    let mut state = ActState::default();
    queue.roundtrip(&mut state).context("Wayland roundtrip")?;
    queue.roundtrip(&mut state).context("Wayland roundtrip")?;
    Ok(Some(ToplevelEnumeration {
        globals,
        queue,
        state,
        _manager: manager,
        _conn: conn,
    }))
}

/// The window that holds the focus right now, or `None` if no window does.
///
/// Portable across wlroots compositors: it reads zwlr's `activated` state rather
/// than a compositor-specific IPC. It must run *before* the caller maps a layer
/// surface that takes the keyboard — under an exclusive keyboard grab no toplevel
/// is activated any more, and the answer becomes `None`. A compositor without the
/// protocol answers `None` too: callers use this to pre-select a tile, and no tile
/// is a usable answer.
pub fn active_window() -> Result<Option<WindowIdentity>> {
    let Some(e) = enumerate_toplevels()? else {
        return Ok(None);
    };
    // A multi-seat compositor can activate one window per seat; the first is as
    // good a choice as any, since a client cannot tell which seat is "ours".
    let Some(i) = e.state.toplevels.iter().position(|t| t.activated) else {
        return Ok(None);
    };
    let t = &e.state.toplevels[i];
    Ok(Some(WindowIdentity {
        app_id: t.app_id.clone(),
        title: t.title.clone(),
        dup_index: e.state.dup_index(i),
    }))
}

/// Focus a window, through whichever activation protocol the compositor offers.
///
/// `zwlr-foreign-toplevel-management` comes first: it is the portable one, and the one
/// `active_window` already reads. Where it is absent — cosmic-comp — the COSMIC toplevel
/// manager takes over, addressing the window by its `identifier` alone (see
/// [`crate::cosmic_activate`]). A compositor with neither gets
/// [`CaptureError::ActivationUnsupported`].
///
/// `identifier` is the `ext-foreign-toplevel-list-v1` identifier of the target, i.e.
/// [`Toplevel::identifier`]; `identity` is the same window in the terms zwlr exposes.
///
/// Run it after the picker closes, so our overlay's keyboard grab is already gone
/// and focus can move to the target.
pub fn activate_window(identifier: &str, identity: &WindowIdentity) -> Result<()> {
    let Some(e) = enumerate_toplevels()? else {
        return crate::cosmic_activate::activate(identifier);
    };
    zwlr_activate(e, identity)
}

/// Focus a window over zwlr-foreign-toplevel-management.
///
/// zwlr exposes no identifier, so the target is matched on app-id + title, with
/// `dup_index` selecting among identical windows by creation order (both
/// ext-foreign-toplevel-list and zwlr enumerate in that order on wlroots).
fn zwlr_activate(mut e: ToplevelEnumeration, identity: &WindowIdentity) -> Result<()> {
    let seat: WlSeat = e
        .globals
        .bind(&e.queue.handle(), 1..=8, ())
        .context("wl_seat missing")?;

    let matching = |t: &&ZwlrToplevel| t.app_id == identity.app_id && t.title == identity.title;
    let handle = e
        .state
        .toplevels
        .iter()
        .filter(matching)
        .nth(identity.dup_index)
        .or_else(|| e.state.toplevels.iter().find(matching))
        .map(|t| t.handle.clone())
        .with_context(|| {
            format!(
                "window to activate not found: {} / {}",
                identity.app_id, identity.title
            )
        })?;
    handle.activate(&seat);
    e.queue
        .roundtrip(&mut e.state)
        .context("Wayland roundtrip")?; // flush the activate request
    Ok(())
}

/// List the Wayland globals the current compositor advertises, as
/// `(interface, version)`. Used by `wlr-peek doctor` to report which capture
/// protocols (and therefore which features) are available.
pub fn advertised_globals() -> Result<Vec<(String, u32)>> {
    let conn = Connection::connect_to_env().context("Wayland connection")?;
    let (globals, _queue) = registry_queue_init::<ActState>(&conn).context("Wayland registry")?;
    let mut list = Vec::new();
    globals.contents().with_list(|globals| {
        for g in globals {
            list.push((g.interface.clone(), g.version));
        }
    });
    Ok(list)
}

impl Dispatch<WlRegistry, GlobalListContents> for ActState {
    fn event(
        _: &mut Self,
        _: &WlRegistry,
        _: <WlRegistry as Proxy>::Event,
        _: &GlobalListContents,
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
    }
}

impl Dispatch<ZwlrForeignToplevelManagerV1, ()> for ActState {
    fn event(
        state: &mut Self,
        _: &ZwlrForeignToplevelManagerV1,
        event: zwlr_foreign_toplevel_manager_v1::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        if let zwlr_foreign_toplevel_manager_v1::Event::Toplevel { toplevel } = event {
            state.toplevels.push(ZwlrToplevel {
                handle: toplevel,
                app_id: String::new(),
                title: String::new(),
                activated: false,
            });
        }
    }

    event_created_child!(ActState, ZwlrForeignToplevelManagerV1, [
        zwlr_foreign_toplevel_manager_v1::EVT_TOPLEVEL_OPCODE => (ZwlrForeignToplevelHandleV1, ()),
    ]);
}

impl Dispatch<ZwlrForeignToplevelHandleV1, ()> for ActState {
    fn event(
        state: &mut Self,
        handle: &ZwlrForeignToplevelHandleV1,
        event: zwlr_foreign_toplevel_handle_v1::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        apply_zwlr_event(&mut state.toplevels, handle, event);
    }
}

/// Apply one zwlr toplevel event to a list of [`ZwlrToplevel`]s. Shared by the two
/// consumers of this protocol: the one-shot activation/focus path ([`ActState`])
/// and the capture [`Client`], which reads it for the app-ids `ext` omits.
fn apply_zwlr_event(
    toplevels: &mut Vec<ZwlrToplevel>,
    handle: &ZwlrForeignToplevelHandleV1,
    event: zwlr_foreign_toplevel_handle_v1::Event,
) {
    use zwlr_foreign_toplevel_handle_v1::Event;
    if matches!(event, Event::Closed) {
        toplevels.retain(|t| &t.handle != handle);
        // The protocol hands the object back to us on `closed`; a long-lived client
        // that never destroyed it would leak one object id per window closed.
        handle.destroy();
        return;
    }
    let Some(t) = toplevels.iter_mut().find(|t| &t.handle == handle) else {
        return;
    };
    match event {
        Event::AppId { app_id } => t.app_id = app_id,
        Event::Title { title } => t.title = title,
        Event::State { state } => t.activated = has_activated(&state),
        _ => {}
    }
}

/// Whether a zwlr `state` event carries the `activated` flag. The payload is a
/// `wl_array` of `zwlr_foreign_toplevel_handle_v1::state` values, i.e. `u32`s in
/// the host byte order.
fn has_activated(state: &[u8]) -> bool {
    let (values, _) = state.as_chunks::<4>();
    values
        .iter()
        .map(|&v| u32::from_ne_bytes(v))
        .any(|v| v == zwlr_foreign_toplevel_handle_v1::State::Activated as u32)
}

delegate_noop!(ActState: ignore WlSeat);

// --- Dispatch ---

impl Dispatch<WlRegistry, GlobalListContents> for State {
    fn event(
        _: &mut Self,
        _: &WlRegistry,
        _: <WlRegistry as Proxy>::Event,
        _: &GlobalListContents,
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
    }
}

impl Dispatch<ExtForeignToplevelListV1, ()> for State {
    fn event(
        state: &mut Self,
        _: &ExtForeignToplevelListV1,
        event: ext_foreign_toplevel_list_v1::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        if let ext_foreign_toplevel_list_v1::Event::Toplevel { toplevel } = event {
            state.pending.push((toplevel, PendingToplevel::default()));
        }
    }

    event_created_child!(State, ExtForeignToplevelListV1, [
        ext_foreign_toplevel_list_v1::EVT_TOPLEVEL_OPCODE => (ExtForeignToplevelHandleV1, ()),
    ]);
}

impl Dispatch<ExtForeignToplevelHandleV1, ()> for State {
    fn event(
        state: &mut Self,
        handle: &ExtForeignToplevelHandleV1,
        event: ext_foreign_toplevel_handle_v1::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        use ext_foreign_toplevel_handle_v1::Event;
        let Some((_, p)) = state.pending.iter_mut().find(|(h, _)| h == handle) else {
            return;
        };
        match event {
            Event::Identifier { identifier } => p.identifier = identifier,
            Event::Title { title } => p.title = title,
            Event::AppId { app_id } => p.app_id = app_id,
            Event::Done => {
                if let Some(pos) = state.pending.iter().position(|(h, _)| h == handle) {
                    let (h, p) = state.pending.remove(pos);
                    state.toplevels.push(Toplevel {
                        handle: h,
                        identifier: p.identifier,
                        title: p.title,
                        app_id: p.app_id,
                    });
                }
            }
            Event::Closed => {
                state.pending.retain(|(h, _)| h != handle);
                state.toplevels.retain(|t| &t.handle != handle);
            }
            _ => {}
        }
    }
}

impl Dispatch<ZwlrForeignToplevelManagerV1, ()> for State {
    fn event(
        state: &mut Self,
        _: &ZwlrForeignToplevelManagerV1,
        event: zwlr_foreign_toplevel_manager_v1::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        if let zwlr_foreign_toplevel_manager_v1::Event::Toplevel { toplevel } = event {
            state.wlr_toplevels.push(ZwlrToplevel {
                handle: toplevel,
                app_id: String::new(),
                title: String::new(),
                activated: false,
            });
        }
    }

    event_created_child!(State, ZwlrForeignToplevelManagerV1, [
        zwlr_foreign_toplevel_manager_v1::EVT_TOPLEVEL_OPCODE => (ZwlrForeignToplevelHandleV1, ()),
    ]);
}

impl Dispatch<ZwlrForeignToplevelHandleV1, ()> for State {
    fn event(
        state: &mut Self,
        handle: &ZwlrForeignToplevelHandleV1,
        event: zwlr_foreign_toplevel_handle_v1::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        apply_zwlr_event(&mut state.wlr_toplevels, handle, event);
    }
}

/// For each `ext` toplevel `(app_id, title)`, the app-id to adopt from the `zwlr`
/// view of the same windows, or `None` to leave it as it is.
///
/// The title is the only field both protocols spell the same way, so it is the key;
/// windows sharing a title are told apart by position. Windows the two protocols
/// already agree on claim their counterpart first, so a nameless window can never
/// adopt the app-id of a window that has its own.
fn adopt_app_ids<'a>(ext: &[(&str, &str)], zwlr: &[(&'a str, &'a str)]) -> Vec<Option<&'a str>> {
    let mut taken = vec![false; zwlr.len()];
    let mut adopted = vec![None; ext.len()];
    for &(app_id, title) in ext {
        if app_id.is_empty() {
            continue;
        }
        if let Some(i) = first_free(zwlr, &taken, |a, t| a == app_id && t == title) {
            taken[i] = true;
        }
    }
    for (slot, &(app_id, title)) in adopted.iter_mut().zip(ext) {
        if !app_id.is_empty() {
            continue;
        }
        if let Some(i) = first_free(zwlr, &taken, |a, t| !a.is_empty() && t == title) {
            taken[i] = true;
            *slot = Some(zwlr[i].0);
        }
    }
    adopted
}

/// The first zwlr toplevel not yet claimed that satisfies `pred`.
fn first_free(
    zwlr: &[(&str, &str)],
    taken: &[bool],
    pred: impl Fn(&str, &str) -> bool,
) -> Option<usize> {
    zwlr.iter()
        .enumerate()
        .position(|(i, &(a, t))| !taken[i] && pred(a, t))
}

impl State {
    /// Give every window an app-id, taking from the zwlr list the ones
    /// `ext-foreign-toplevel-list` left empty.
    ///
    /// Sway announces an XWayland window through `ext` with an empty app-id while
    /// zwlr reports its X11 class, so Steam, games and Java applications would
    /// otherwise reach every caller unnamed: unfilterable by `--app-id`, iconless,
    /// and taken for a system surface by the chooser. A window absent from the zwlr
    /// list, or unnamed there too, keeps its empty app-id — that is what a real
    /// system surface looks like.
    fn complete_app_ids(&mut self) {
        // A window is named once and keeps that name, so this is a no-op after the
        // first pass — and free on the sessions that have nothing to complete.
        if !self.toplevels.iter().any(|t| t.app_id.is_empty()) {
            return;
        }
        let zwlr: Vec<(&str, &str)> = self
            .wlr_toplevels
            .iter()
            .map(|t| (t.app_id.as_str(), t.title.as_str()))
            .collect();
        let adopted = {
            let ext: Vec<(&str, &str)> = self
                .toplevels
                .iter()
                .map(|t| (t.app_id.as_str(), t.title.as_str()))
                .collect();
            adopt_app_ids(&ext, &zwlr)
        };
        for (t, app_id) in self.toplevels.iter_mut().zip(adopted) {
            if let Some(app_id) = app_id {
                t.app_id = app_id.to_string();
            }
        }
    }

    /// The `Output` for `wl_output`, created (with neutral geometry) on first sight
    /// so `geometry`/`mode`/`scale` can land before `name`.
    fn output_entry(&mut self, output: &WlOutput) -> &mut Output {
        if let Some(i) = self.outputs.iter().position(|o| &o.wl_output == output) {
            return &mut self.outputs[i];
        }
        self.outputs.push(Output {
            wl_output: output.clone(),
            name: String::new(),
            logical_x: 0,
            logical_y: 0,
            logical_w: 0,
            logical_h: 0,
            phys_width: 0,
            phys_height: 0,
            scale: 1,
            transform: Transform::Normal,
            have_xdg: false,
        });
        self.outputs.last_mut().unwrap()
    }
}

impl Dispatch<WlOutput, ()> for State {
    fn event(
        state: &mut Self,
        output: &WlOutput,
        event: <WlOutput as Proxy>::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        use wayland_client::protocol::wl_output::Event;
        match event {
            Event::Geometry {
                x, y, transform, ..
            } => {
                let o = state.output_entry(output);
                o.transform = transform.into_result().unwrap_or(Transform::Normal);
                // wl_output position is only a fallback; xdg-output is authoritative.
                if !o.have_xdg {
                    o.logical_x = x;
                    o.logical_y = y;
                }
            }
            // Keep only the active mode's resolution (physical pixels).
            Event::Mode {
                flags,
                width,
                height,
                ..
            } => {
                if flags
                    .into_result()
                    .is_ok_and(|f| f.contains(wl_output::Mode::Current))
                {
                    let o = state.output_entry(output);
                    o.phys_width = width;
                    o.phys_height = height;
                }
            }
            Event::Scale { factor } => {
                state.output_entry(output).scale = factor.max(1);
            }
            Event::Name { name } => {
                state.output_entry(output).name = name;
            }
            _ => {}
        }
    }
}

impl Dispatch<ZxdgOutputV1, WlOutput> for State {
    fn event(
        state: &mut Self,
        _: &ZxdgOutputV1,
        event: <ZxdgOutputV1 as Proxy>::Event,
        wl_output: &WlOutput,
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        use zxdg_output_v1::Event;
        match event {
            Event::LogicalPosition { x, y } => {
                let o = state.output_entry(wl_output);
                o.logical_x = x;
                o.logical_y = y;
                o.have_xdg = true;
            }
            Event::LogicalSize { width, height } => {
                let o = state.output_entry(wl_output);
                o.logical_w = width;
                o.logical_h = height;
                o.have_xdg = true;
            }
            _ => {}
        }
    }
}

impl Dispatch<ExtImageCopyCaptureSessionV1, SessionId> for State {
    fn event(
        state: &mut Self,
        _: &ExtImageCopyCaptureSessionV1,
        event: ext_image_copy_capture_session_v1::Event,
        id: &SessionId,
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        use ext_image_copy_capture_session_v1::Event;
        let Some(d) = state.sessions.get_mut(id) else {
            return;
        };
        match event {
            Event::BufferSize { width, height } => {
                d.width = width;
                d.height = height;
            }
            Event::ShmFormat {
                format: WEnum::Value(f),
            } => d.format = Some(f),
            #[cfg(feature = "gpu")]
            Event::DmabufDevice { device } => {
                // dev_t as a native-endian byte array.
                if device.len() == 8 {
                    let mut b = [0u8; 8];
                    b.copy_from_slice(&device);
                    d.dmabuf_dev = Some(u64::from_ne_bytes(b));
                }
            }
            #[cfg(feature = "gpu")]
            Event::DmabufFormat { format, modifiers } => {
                // modifiers: array of native-endian u64.
                let (words, _) = modifiers.as_chunks::<8>();
                let mods = words.iter().map(|&w| u64::from_ne_bytes(w)).collect();
                d.dmabuf_formats.push((format, mods));
            }
            // A constraints group ends with `done`; flag a (re)allocation so a
            // resize between frames grows the buffer.
            Event::Done => {
                d.constraints_done = true;
                d.dirty = true;
            }
            Event::Stopped => d.stopped = true,
            _ => {}
        }
    }
}

impl Dispatch<ExtImageCopyCaptureFrameV1, SessionId> for State {
    fn event(
        state: &mut Self,
        _: &ExtImageCopyCaptureFrameV1,
        event: ext_image_copy_capture_frame_v1::Event,
        session_id: &SessionId,
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        use ext_image_copy_capture_frame_v1::Event;
        let Some(d) = state.sessions.get_mut(session_id) else {
            return;
        };
        match event {
            Event::Ready => d.ready = true,
            Event::Failed { reason } => {
                let reason = match reason {
                    WEnum::Value(r) => r,
                    _ => FailureReason::Unknown,
                };
                // Per the protocol, a frame failure means destroy the frame, not
                // the session. Only `stopped` is terminal; the rest are transient.
                if matches!(reason, FailureReason::Stopped) {
                    d.stopped = true;
                } else {
                    d.frame_failed = Some(reason);
                }
            }
            _ => {}
        }
    }
}

impl Dispatch<ZwlrScreencopyFrameV1, SessionId> for State {
    fn event(
        state: &mut Self,
        _: &ZwlrScreencopyFrameV1,
        event: zwlr_screencopy_frame_v1::Event,
        session_id: &SessionId,
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        use zwlr_screencopy_frame_v1::Event;
        let Some(d) = state.sessions.get_mut(session_id) else {
            return;
        };
        match event {
            // The shm constraints, including the stride the compositor will write
            // with — it dictates the pitch here, unlike ext-image-copy-capture.
            Event::Buffer {
                format: WEnum::Value(f),
                width,
                height,
                stride,
            } => {
                d.dirty |= (d.width, d.height, d.format, d.stride)
                    != (width, height, Some(f), Some(stride));
                d.width = width;
                d.height = height;
                d.format = Some(f);
                d.stride = Some(stride);
            }
            // A bare fourcc: no modifier list, so the allocator lets the driver
            // pick the layout (see `pick_dmabuf_format`).
            #[cfg(feature = "gpu")]
            Event::LinuxDmabuf {
                format,
                width,
                height,
            } => {
                let announced = vec![(format, Vec::new())];
                d.dirty |= d.dmabuf_formats != announced || (d.width, d.height) != (width, height);
                d.width = width;
                d.height = height;
                d.dmabuf_formats = announced;
            }
            // Every buffer type has been announced; the frame now wants a `copy`.
            // The constraints come round again with each frame, so the buffer is
            // only reallocated when they actually changed.
            Event::BufferDone => {
                d.constraints_done = true;
                d.awaiting_copy = true;
            }
            Event::Flags {
                flags: WEnum::Value(f),
            } => {
                let inverted = f.contains(zwlr_screencopy_frame_v1::Flags::YInvert);
                if debug() {
                    eprintln!("wlr-capture: screencopy frame flags {f:?}");
                }
                // The orientation belongs to the source, so a change means the
                // buffer has to be reallocated on the path that can handle it.
                d.dirty |= d.y_invert != inverted;
                d.y_invert = inverted;
            }
            Event::Ready { .. } => d.ready = true,
            // A screencopy frame has no transient failure: the whole capture is off.
            Event::Failed => d.stopped = true,
            _ => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use wayland_client::protocol::wl_shm::Format;

    /// The heart of the grim-vs-wlr-chooser fix: bytes-per-pixel (hence stride) must
    /// match the advertised format. Bgr888 is 24-bit, so stride = width*3, not *4.
    #[test]
    fn pixel_layout_stride_and_alpha() {
        assert_eq!(PixelLayout::of(Format::Bgr888).unwrap().bpp, 3);
        assert_eq!(PixelLayout::of(Format::Rgb888).unwrap().bpp, 3);
        assert_eq!(PixelLayout::of(Format::Xrgb8888).unwrap().bpp, 4);
        assert_eq!(PixelLayout::of(Format::Argb8888).unwrap().bpp, 4);

        assert!(PixelLayout::of(Format::Bgr888).unwrap().a.is_none());
        assert!(PixelLayout::of(Format::Xrgb8888).unwrap().a.is_none());
        assert_eq!(PixelLayout::of(Format::Argb8888).unwrap().a, Some(3));
        assert_eq!(PixelLayout::of(Format::Abgr8888).unwrap().a, Some(3));
    }

    #[test]
    fn activated_is_read_from_the_state_array() {
        let flags = |v: &[u32]| -> Vec<u8> { v.iter().flat_map(|s| s.to_ne_bytes()).collect() };
        let activated = zwlr_foreign_toplevel_handle_v1::State::Activated as u32;
        let maximized = zwlr_foreign_toplevel_handle_v1::State::Maximized as u32;

        assert!(has_activated(&flags(&[maximized, activated])));
        assert!(!has_activated(&flags(&[maximized])));
        // No state at all: the window is not focused (and sends this on unfocus).
        assert!(!has_activated(&[]));
    }

    #[test]
    fn dup_index_counts_identical_windows_before_this_one() {
        let windows = [("foot", "a"), ("firefox", "b"), ("foot", "a")];
        let prior = |n: usize| windows[..n].iter().copied();
        assert_eq!(dup_index(prior(0), "foot", "a"), 0);
        assert_eq!(dup_index(prior(2), "foot", "a"), 1);
        // A different title is a different window, however alike the app.
        assert_eq!(dup_index(prior(3), "foot", "z"), 0);
    }

    #[test]
    fn xwayland_window_adopts_its_x11_class() {
        // What Sway reports for a Steam window: nothing through `ext`, `steam`
        // through zwlr. The windows both protocols name are left alone.
        let ext = [("firefox", "Issue #13"), ("", "Steam")];
        let zwlr = [("firefox", "Issue #13"), ("steam", "Steam")];
        assert_eq!(adopt_app_ids(&ext, &zwlr), vec![None, Some("steam")]);
    }

    #[test]
    fn a_window_zwlr_does_not_know_keeps_no_app_id() {
        // A real system surface: no app-id anywhere, so it stays unnamed and the
        // chooser keeps hiding it.
        let ext = [("", "Notification"), ("", "Steam")];
        let zwlr = [("steam", "Steam")];
        assert_eq!(adopt_app_ids(&ext, &zwlr), vec![None, Some("steam")]);
        // Listed by zwlr but unnamed there too: still nothing to adopt.
        assert_eq!(adopt_app_ids(&[("", "Bar")], &[("", "Bar")]), vec![None]);
    }

    #[test]
    fn a_named_window_keeps_its_counterpart_to_itself() {
        // Two windows share the title "foot"; one is named by both protocols. The
        // nameless one must adopt the *other* zwlr entry, not the taken one.
        let ext = [("btop", "foot"), ("", "foot")];
        let zwlr = [("btop", "foot"), ("java-swing", "foot")];
        assert_eq!(adopt_app_ids(&ext, &zwlr), vec![None, Some("java-swing")]);
        // Order-independent: the same holds when the nameless window comes first.
        let ext = [("", "foot"), ("btop", "foot")];
        assert_eq!(adopt_app_ids(&ext, &zwlr), vec![Some("java-swing"), None]);
    }

    #[test]
    fn identical_windows_are_matched_one_to_one() {
        let ext = [("", "Console"), ("", "Console")];
        let zwlr = [("java", "Console"), ("java", "Console")];
        assert_eq!(adopt_app_ids(&ext, &zwlr), vec![Some("java"), Some("java")]);
        // One zwlr entry for two nameless windows: only the first can take it.
        let zwlr = [("java", "Console")];
        assert_eq!(adopt_app_ids(&ext, &zwlr), vec![Some("java"), None]);
    }

    #[test]
    fn pixel_layout_unknown_format_is_none() {
        // A format we don't decode should be reported, not silently mishandled.
        assert!(PixelLayout::of(Format::C8).is_none());
    }

    #[test]
    fn region_intersect() {
        let a = Region {
            x: 0,
            y: 0,
            w: 10,
            h: 10,
        };
        let b = Region {
            x: 5,
            y: 5,
            w: 10,
            h: 10,
        };
        assert_eq!(
            a.intersect(&b),
            Some(Region {
                x: 5,
                y: 5,
                w: 5,
                h: 5
            })
        );
        // Negative origin (selection partly off the image) clamps correctly.
        let c = Region {
            x: -3,
            y: -3,
            w: 6,
            h: 6,
        };
        assert_eq!(
            a.intersect(&c),
            Some(Region {
                x: 0,
                y: 0,
                w: 3,
                h: 3
            })
        );
        // Disjoint → None.
        let d = Region {
            x: 100,
            y: 100,
            w: 1,
            h: 1,
        };
        assert_eq!(a.intersect(&d), None);
    }

    /// A 2×2 RGBA image: four distinct pixels, to verify pixel/crop addressing.
    fn img_2x2() -> CapturedImage {
        CapturedImage {
            width: 2,
            height: 2,
            rgba: vec![
                1, 1, 1, 255, 2, 2, 2, 255, // row 0: (0,0)=1, (1,0)=2
                3, 3, 3, 255, 4, 4, 4, 255, // row 1: (0,1)=3, (1,1)=4
            ],
        }
    }

    #[test]
    fn captured_pixel_and_crop() {
        let img = img_2x2();
        assert_eq!(img.pixel(0, 0), Some([1, 1, 1, 255]));
        assert_eq!(img.pixel(1, 1), Some([4, 4, 4, 255]));
        assert_eq!(img.pixel(2, 0), None); // out of bounds

        // Crop the bottom-right 1×1 pixel.
        let c = img.crop(Region {
            x: 1,
            y: 1,
            w: 1,
            h: 1,
        });
        assert_eq!((c.width, c.height), (1, 1));
        assert_eq!(c.rgba, vec![4, 4, 4, 255]);

        // Crop overrunning the bounds clamps to the overlap.
        let c2 = img.crop(Region {
            x: 1,
            y: 0,
            w: 5,
            h: 5,
        });
        assert_eq!((c2.width, c2.height), (1, 2));
        assert_eq!(c2.rgba, vec![2, 2, 2, 255, 4, 4, 4, 255]);
    }

    #[test]
    fn captured_blit_into() {
        // Blit the 2×2 image into a 3×2 black canvas at x=1, clipping the overflow.
        let img = img_2x2();
        let (dw, dh) = (3u32, 2u32);
        let mut dst = vec![0u8; (dw * dh * 4) as usize];
        img.blit_into(&mut dst, dw, dh, 1, 0);
        // Column 0 stays black; columns 1..3 get the image's two columns.
        assert_eq!(&dst[0..4], &[0, 0, 0, 0]); // (0,0)
        assert_eq!(&dst[4..8], &[1, 1, 1, 255]); // (1,0) = img (0,0)
        assert_eq!(&dst[8..12], &[2, 2, 2, 255]); // (2,0) = img (1,0)
        assert_eq!(&dst[12..16], &[0, 0, 0, 0]); // (0,1)
        assert_eq!(&dst[16..20], &[3, 3, 3, 255]); // (1,1) = img (0,1)
    }

    fn globals(names: &[&str]) -> Vec<(String, u32)> {
        names.iter().map(|n| ((*n).to_string(), 1)).collect()
    }

    #[test]
    fn protocol_selection_prefers_image_copy_capture() {
        let ext = [IMAGE_COPY_GLOBALS[0], IMAGE_COPY_GLOBALS[1]];
        assert_eq!(
            select_protocol(&globals(&ext)),
            Some(Protocol::ImageCopyCapture)
        );
        // Only the older protocol: that is what the engine drives.
        assert_eq!(
            select_protocol(&globals(&[SCREENCOPY_GLOBAL])),
            Some(Protocol::Screencopy)
        );
        // Both: ext wins (it captures windows too).
        let both = [ext[0], ext[1], SCREENCOPY_GLOBAL];
        assert_eq!(
            select_protocol(&globals(&both)),
            Some(Protocol::ImageCopyCapture)
        );
        // A half-advertised ext (manager without the output source) is not usable.
        assert_eq!(select_protocol(&globals(&ext[..1])), None);
        assert_eq!(select_protocol(&globals(&[])), None);
    }

    #[test]
    fn protocol_interface_names_are_the_protocol_names() {
        assert_eq!(
            Protocol::ImageCopyCapture.interface(),
            "ext-image-copy-capture-v1"
        );
        assert_eq!(Protocol::Screencopy.interface(), "zwlr-screencopy-v1");
    }

    #[test]
    fn dmabuf_format_without_modifiers_is_left_to_the_driver() {
        #[cfg(feature = "gpu")]
        {
            let xr24 = fourcc(b'X', b'R', b'2', b'4');
            // A bare fourcc (what zwlr-screencopy announces) is usable, with no layout.
            assert_eq!(
                pick_dmabuf_format(&[(xr24, vec![])]),
                Some((xr24, Vec::new()))
            );
            // An explicit list keeps its modifiers, minus the INVALID sentinel.
            assert_eq!(
                pick_dmabuf_format(&[(xr24, vec![DRM_MOD_INVALID, 7])]),
                Some((xr24, vec![7]))
            );
            // Only INVALID was offered for a format: nothing to allocate with.
            assert_eq!(pick_dmabuf_format(&[(xr24, vec![DRM_MOD_INVALID])]), None);
        }
    }

    #[test]
    fn flip_vertically_reverses_row_order() {
        // 2×2: rows swap, pixels within a row keep their order.
        let mut img = img_2x2();
        flip_vertically(&mut img);
        assert_eq!(img.pixel(0, 0), Some([3, 3, 3, 255]));
        assert_eq!(img.pixel(1, 0), Some([4, 4, 4, 255]));
        assert_eq!(img.pixel(0, 1), Some([1, 1, 1, 255]));
        assert_eq!(img.pixel(1, 1), Some([2, 2, 2, 255]));
        // Flipping twice is the identity.
        flip_vertically(&mut img);
        assert_eq!(img.rgba, img_2x2().rgba);
    }

    #[test]
    fn flip_vertically_keeps_the_middle_row_of_an_odd_image() {
        // 1×3, one byte-quad per row: only the outer rows move.
        let mut img = CapturedImage {
            width: 1,
            height: 3,
            rgba: vec![1, 1, 1, 255, 2, 2, 2, 255, 3, 3, 3, 255],
        };
        flip_vertically(&mut img);
        assert_eq!(img.rgba, vec![3, 3, 3, 255, 2, 2, 2, 255, 1, 1, 1, 255]);
    }

    #[test]
    fn session_ids_are_distinct() {
        let (a, b) = (SessionId::next(), SessionId::next());
        assert_ne!(a, b);
    }

    #[test]
    fn output_logical_dims_transform() {
        // 4K at scale 2 → 1920×1080 logical.
        assert_eq!(logical_dims(3840, 2160, 2, Transform::Normal), (1920, 1080));
        // 90°/270° swap width and height.
        assert_eq!(logical_dims(3840, 2160, 2, Transform::_90), (1080, 1920));
        assert_eq!(
            logical_dims(3840, 2160, 2, Transform::Flipped270),
            (1080, 1920)
        );
        // 180° keeps orientation; scale 0 is treated as 1.
        assert_eq!(logical_dims(1000, 500, 0, Transform::_180), (1000, 500));
    }
}

// Objects whose events we don't need.
delegate_noop!(State: ignore ZxdgOutputManagerV1);
delegate_noop!(State: ignore WlShm);
delegate_noop!(State: ignore WlShmPool);
delegate_noop!(State: ignore WlBuffer);
delegate_noop!(State: ignore ExtImageCaptureSourceV1);
delegate_noop!(State: ignore ExtForeignToplevelImageCaptureSourceManagerV1);
delegate_noop!(State: ignore ExtOutputImageCaptureSourceManagerV1);
delegate_noop!(State: ignore ExtImageCopyCaptureManagerV1);
// The screencopy manager has no events; everything arrives on its frames.
delegate_noop!(State: ignore ZwlrScreencopyManagerV1);
// dma-buf: we drive allocation ourselves (gbm) and create buffers with
// `create_immed`, so the manager's format/modifier and the params' created/failed
// events carry nothing we need.
#[cfg(feature = "gpu")]
delegate_noop!(State: ignore ZwpLinuxDmabufV1);

#[cfg(feature = "gpu")]
impl Dispatch<ZwpLinuxDmabufFeedbackV1, ()> for State {
    fn event(
        state: &mut Self,
        _: &ZwpLinuxDmabufFeedbackV1,
        event: zwp_linux_dmabuf_feedback_v1::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        use zwp_linux_dmabuf_feedback_v1::Event;
        // `main_device` comes before any tranche, so the first device wins and the
        // tranche target is only a fallback for a compositor that omits it.
        let (Event::MainDevice { device } | Event::TrancheTargetDevice { device }) = event else {
            return;
        };
        if state.dmabuf_main_device.is_none()
            && let Ok(bytes) = <[u8; 8]>::try_from(device.as_slice())
        {
            state.dmabuf_main_device = Some(u64::from_ne_bytes(bytes));
        }
    }
}
#[cfg(feature = "gpu")]
delegate_noop!(State: ignore ZwpLinuxBufferParamsV1);
