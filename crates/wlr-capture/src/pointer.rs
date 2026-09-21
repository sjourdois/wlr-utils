//! Setting the cursor image for a seat's pointer.
//!
//! The cursor image is the focused client's to set: it is undefined after each
//! `wl_pointer.enter` until that client sets one. [`Pointer`] pairs the seat's
//! pointer with a `cursor-shape-v1` device so a windowing host can set a [`Shape`], or
//! hide the cursor entirely.

use smithay_client_toolkit::globals::GlobalData;
use smithay_client_toolkit::reexports::protocols::wp::cursor_shape::v1::client::{
    wp_cursor_shape_device_v1::WpCursorShapeDeviceV1,
    wp_cursor_shape_manager_v1::WpCursorShapeManagerV1,
};
use smithay_client_toolkit::seat::SeatState;
use smithay_client_toolkit::seat::pointer::{
    PointerData, PointerHandler, cursor_shape::CursorShapeManager,
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
    cursor: Option<Shape>,
}

/// The pointer, once the seat has announced the capability.
struct Bound {
    pointer: WlPointer,
    cursor_shape: Option<WpCursorShapeDeviceV1>,
}

impl Pointer {
    /// Tries to bind `cursor-shape-v1`, which not every compositor has.
    pub fn new<D>(globals: &GlobalList, qh: &QueueHandle<D>) -> Self
    where
        D: Dispatch<WpCursorShapeManagerV1, GlobalData> + 'static,
    {
        Self {
            manager: CursorShapeManager::bind(globals, qh).ok(),
            pointer: None,
            cursor: Some(Shape::Default),
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

    /// `None` until [`create`](Self::create) has made one.
    pub fn wl(&self) -> Option<&WlPointer> {
        self.pointer.as_ref().map(|p| &p.pointer)
    }

    /// The cursor image to show from here on — `None` hides it — sent right away if we
    /// hold the pointer. It can be set before the pointer exists, and is re-sent on every
    /// [`enter`](Self::enter). A shape needs `cursor-shape-v1`; hiding needs only the pointer.
    pub fn set_cursor(&mut self, cursor: Option<Shape>) {
        if cursor == self.cursor {
            return;
        }
        self.cursor = cursor;
        // The compositor applies this only while the pointer is over one of our
        // surfaces; otherwise the next enter sends it.
        if let Some(serial) = self
            .wl()
            .and_then(|p| p.data::<PointerData<()>>())
            .and_then(PointerData::latest_enter_serial)
        {
            self.send(serial);
        }
    }

    /// Sends the image for a surface the pointer just entered with `serial`. The
    /// compositor resets the image on every enter, so this repeats the last one.
    pub fn enter(&self, serial: u32) {
        self.send(serial);
    }

    /// A no-op if the seat has no pointer, or if a shape is wanted and the compositor
    /// has no `cursor-shape-v1`.
    fn send(&self, serial: u32) {
        let Some(bound) = self.pointer.as_ref() else {
            return;
        };
        match self.cursor {
            None => bound.pointer.set_cursor(serial, None, 0, 0),
            Some(shape) => {
                if let Some(device) = bound.cursor_shape.as_ref() {
                    device.set_shape(serial, shape);
                }
            }
        }
    }
}
