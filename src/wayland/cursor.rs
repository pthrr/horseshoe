use horseshoe::num::u32_to_i32;
use smithay_client_toolkit::seat::pointer::{CursorIcon, ThemedPointer};
use smithay_client_toolkit::shm::Shm;
use smithay_client_toolkit::shm::slot::{Buffer, SlotPool};
use wayland_client::Connection;
use wayland_client::protocol::{wl_pointer, wl_shm, wl_surface};

// -- Custom SHM cursor fallback for systems without cursor-shape-v1 or cursor themes --

pub(super) const IBEAM_W: u32 = 7;
pub(super) const IBEAM_H: u32 = 17;
pub(super) const IBEAM_HOT: (i32, i32) = (3, 8);

#[rustfmt::skip]
pub(super) const IBEAM_BITMAP: [u8; 119] = [
    0,1,1,1,1,1,0,
    0,1,2,2,2,1,0,
    0,0,1,2,1,0,0,
    0,0,1,2,1,0,0,
    0,0,1,2,1,0,0,
    0,0,1,2,1,0,0,
    0,0,1,2,1,0,0,
    0,0,1,2,1,0,0,
    0,0,1,2,1,0,0,
    0,0,1,2,1,0,0,
    0,0,1,2,1,0,0,
    0,0,1,2,1,0,0,
    0,0,1,2,1,0,0,
    0,0,1,2,1,0,0,
    0,0,1,2,1,0,0,
    0,1,2,2,2,1,0,
    0,1,1,1,1,1,0,
];

pub(super) const ARROW_W: u32 = 11;
pub(super) const ARROW_H: u32 = 15;
pub(super) const ARROW_HOT: (i32, i32) = (0, 0);

#[rustfmt::skip]
pub(super) const ARROW_BITMAP: [u8; 165] = [
    1,0,0,0,0,0,0,0,0,0,0,
    1,1,0,0,0,0,0,0,0,0,0,
    1,2,1,0,0,0,0,0,0,0,0,
    1,2,2,1,0,0,0,0,0,0,0,
    1,2,2,2,1,0,0,0,0,0,0,
    1,2,2,2,2,1,0,0,0,0,0,
    1,2,2,2,2,2,1,0,0,0,0,
    1,2,2,2,2,2,2,1,0,0,0,
    1,2,2,2,2,2,2,2,1,0,0,
    1,2,2,2,2,2,2,2,2,1,0,
    1,2,2,2,2,1,1,1,1,1,0,
    1,2,2,1,2,2,1,0,0,0,0,
    1,2,1,0,1,2,1,0,0,0,0,
    1,1,0,0,0,1,2,1,0,0,0,
    1,0,0,0,0,0,1,1,0,0,0,
];

/// Render a cursor bitmap (0=transparent, 1=black, 2=white) to ARGB8888 bytes.
pub(super) fn render_cursor_bitmap(bitmap: &[u8], canvas: &mut [u8]) {
    for (&pixel, chunk) in bitmap.iter().zip(canvas.chunks_exact_mut(4)) {
        let argb: [u8; 4] = match pixel {
            1 => [0, 0, 0, 255],
            2 => [255, 255, 255, 255],
            _ => [0, 0, 0, 0],
        };
        chunk.copy_from_slice(&argb);
    }
}

/// Fallback cursor using programmatically drawn SHM buffers.
pub(crate) struct ShmCursor {
    pub(crate) pointer: wl_pointer::WlPointer,
    surface: wl_surface::WlSurface,
    text_buf: Buffer,
    arrow_buf: Buffer,
    _pool: SlotPool,
    pub(crate) enter_serial: u32,
}

impl ShmCursor {
    pub(crate) fn new(
        pointer: wl_pointer::WlPointer,
        surface: wl_surface::WlSurface,
        shm: &Shm,
    ) -> Option<Self> {
        let mut pool = match SlotPool::new(4096, shm) {
            Ok(p) => p,
            Err(e) => {
                eprintln!("Failed to create cursor SHM pool: {e}");
                return None;
            }
        };
        let text_buf = Self::make_buffer(&mut pool, &IBEAM_BITMAP, IBEAM_W, IBEAM_H)?;
        let arrow_buf = Self::make_buffer(&mut pool, &ARROW_BITMAP, ARROW_W, ARROW_H)?;
        Some(Self {
            pointer,
            surface,
            text_buf,
            arrow_buf,
            _pool: pool,
            enter_serial: 0,
        })
    }

    fn make_buffer(pool: &mut SlotPool, bitmap: &[u8], w: u32, h: u32) -> Option<Buffer> {
        let stride = u32_to_i32(w * 4);
        let (buf, canvas) = pool
            .create_buffer(
                u32_to_i32(w),
                u32_to_i32(h),
                stride,
                wl_shm::Format::Argb8888,
            )
            .ok()?;
        render_cursor_bitmap(bitmap, canvas);
        Some(buf)
    }

    fn set_cursor(&self, icon: CursorIcon) {
        let (buf, hotspot) = if matches!(icon, CursorIcon::Text) {
            (&self.text_buf, IBEAM_HOT)
        } else {
            (&self.arrow_buf, ARROW_HOT)
        };
        let _ = buf.attach_to(&self.surface);
        self.surface.damage_buffer(0, 0, i32::MAX, i32::MAX);
        self.surface.commit();
        self.pointer
            .set_cursor(self.enter_serial, Some(&self.surface), hotspot.0, hotspot.1);
    }
}

/// Cursor backend: themed (cursor-shape-v1 / xcursor themes) or SHM fallback.
pub enum Cursor {
    Themed(ThemedPointer),
    Shm(ShmCursor),
}

impl Cursor {
    pub(super) fn set_cursor(&self, conn: &Connection, icon: CursorIcon) {
        match self {
            Cursor::Themed(tp) => {
                let _ = tp.set_cursor(conn, icon);
            }
            Cursor::Shm(sc) => sc.set_cursor(icon),
        }
    }

    pub(super) const fn set_enter_serial(&mut self, serial: u32) {
        if let Cursor::Shm(sc) = self {
            sc.enter_serial = serial;
        }
    }
}
