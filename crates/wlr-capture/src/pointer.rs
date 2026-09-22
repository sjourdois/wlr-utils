//! Setting the cursor image for a seat's pointer.
//!
//! The cursor image is the focused client's to set: it is undefined after each
//! `wl_pointer.enter` until that client sets one, so a surface that never sets one
//! inherits whatever the previous client left — including nothing at all. [`Pointer`]
//! pairs the seat's pointer with a `cursor-shape-v1` device, holds the [`Shape`] the
//! host wants, and re-sends it on every enter.

use smithay_client_toolkit::globals::GlobalData;
use smithay_client_toolkit::reexports::protocols::wp::cursor_shape::v1::client::{
    wp_cursor_shape_device_v1::WpCursorShapeDeviceV1,
    wp_cursor_shape_manager_v1::WpCursorShapeManagerV1,
};
use smithay_client_toolkit::seat::SeatState;
use smithay_client_toolkit::seat::pointer::{
    PointerData, PointerEvent, PointerEventKind, PointerHandler, cursor_shape::CursorShapeManager,
};
use wayland_client::globals::GlobalList;
use wayland_client::protocol::{wl_pointer::WlPointer, wl_seat::WlSeat};
use wayland_client::{Dispatch, Proxy, QueueHandle};

pub use smithay_client_toolkit::reexports::protocols::wp::cursor_shape::v1::client::wp_cursor_shape_device_v1::Shape;

/// A seat's pointer and the device that sets its cursor image.
///
/// [`new`](Self::new) binds the cursor-shape global up front; the pointer follows
/// from [`create`](Self::create) once the seat announces the capability.
pub struct Pointer {
    /// Bound before the pointer exists, and kept to make its device with.
    manager: Option<CursorShapeManager>,
    pointer: Option<Bound>,
    cursor: Shape,
}

/// The pointer, once the seat has announced the capability.
struct Bound {
    pointer: WlPointer,
    cursor_shape: Option<WpCursorShapeDeviceV1>,
}

impl Pointer {
    /// Tries to bind `cursor-shape-v1`. Without it nothing here can set an image, and
    /// the surface keeps whatever the pointer arrived with; `doctor` reports the global.
    pub fn new<D>(globals: &GlobalList, qh: &QueueHandle<D>) -> Self
    where
        D: Dispatch<WpCursorShapeManagerV1, GlobalData> + 'static,
    {
        Self {
            manager: CursorShapeManager::bind(globals, qh).ok(),
            pointer: None,
            cursor: Shape::Default,
        }
    }

    /// A no-op if the pointer already exists.
    pub fn create<D>(&mut self, seat_state: &mut SeatState, seat: &WlSeat, qh: &QueueHandle<D>)
    where
        D: Dispatch<WlPointer, PointerData<()>>
            + Dispatch<WpCursorShapeDeviceV1, GlobalData>
            + PointerHandler
            + 'static,
    {
        if self.pointer.is_some() {
            return;
        }
        self.pointer = seat_state.get_pointer(qh, seat).ok().map(|pointer| Bound {
            cursor_shape: self
                .manager
                .as_ref()
                .map(|m| m.get_shape_device(&pointer, qh)),
            pointer,
        });
    }

    /// The image to show from here on, sent right away if the pointer is over one of
    /// our surfaces. It can be set before the pointer exists — and before any enter —
    /// since [`on_event`](Self::on_event) re-sends it on each one.
    pub fn set_cursor(&mut self, cursor: Shape) {
        if cursor == self.cursor {
            return;
        }
        self.cursor = cursor;
        // The compositor takes this only against the current enter serial; with the
        // pointer elsewhere there is none to match, and the next enter sends it.
        if let Some(serial) = self
            .wl()
            .and_then(|p| p.data::<PointerData<()>>())
            .and_then(PointerData::latest_enter_serial)
        {
            self.send(serial);
        }
    }

    /// Re-sends the image on every enter, which is what makes it stick: the compositor
    /// leaves it undefined each time the pointer enters one of our surfaces. Call it for
    /// every event of a [`PointerHandler::pointer_frame`]; anything else is ignored.
    pub fn on_event(&self, event: &PointerEvent) {
        if let PointerEventKind::Enter { serial } = event.kind {
            self.send(serial);
        }
    }

    /// `None` until [`create`](Self::create) has made one.
    fn wl(&self) -> Option<&WlPointer> {
        self.pointer.as_ref().map(|p| &p.pointer)
    }

    /// A no-op if the seat has no pointer, or if the compositor has no `cursor-shape-v1`.
    fn send(&self, serial: u32) {
        if let Some(device) = self.pointer.as_ref().and_then(|b| b.cursor_shape.as_ref()) {
            device.set_shape(serial, self.cursor);
        }
    }
}
