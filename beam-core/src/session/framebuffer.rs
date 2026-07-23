//! Shared BGRA framebuffer for one session.
//!
//! The active session task owns the decode side (writing pixels as PDUs arrive) while the
//! frontend reads snapshots to paint. Reads and writes are decoupled from event delivery: the
//! event channel only carries *notifications* that pixels changed ([`crate::events::SessionEvent::FramebufferUpdated`]),
//! never the pixel data itself, so a slow/backpressured UI thread never blocks the network task
//! and large frames are never cloned through the channel just to report a heartbeat.

use std::sync::Mutex;

/// An axis-aligned region of the framebuffer that changed in a single update.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DirtyRect {
    pub x: u16,
    pub y: u16,
    pub width: u16,
    pub height: u16,
}

struct Inner {
    width: u16,
    height: u16,
    /// Tightly packed BGRA8888 pixels, `height` rows of `width * 4` bytes.
    data: Vec<u8>,
}

/// Thread-safe holder for the current desktop image of a session.
pub struct Framebuffer {
    inner: Mutex<Inner>,
}

impl Framebuffer {
    pub fn new(width: u16, height: u16) -> Self {
        Self {
            inner: Mutex::new(Inner {
                width,
                height,
                data: vec![0; usize::from(width) * usize::from(height) * 4],
            }),
        }
    }

    /// Replace the buffer contents wholesale (used after a resize / reactivation).
    pub(crate) fn replace(&self, width: u16, height: u16, data: Vec<u8>) {
        let mut inner = self.inner.lock().expect("framebuffer mutex poisoned");
        inner.width = width;
        inner.height = height;
        inner.data = data;
    }

    /// Copy a fresh region from `src` (the session's live `DecodedImage` buffer, same
    /// dimensions) into our shared buffer.
    pub(crate) fn update_from(&self, src: &[u8], width: u16, height: u16) {
        let mut inner = self.inner.lock().expect("framebuffer mutex poisoned");
        if inner.width != width || inner.height != height {
            inner.width = width;
            inner.height = height;
            inner.data.resize(src.len(), 0);
        }
        inner.data.copy_from_slice(src);
    }

    /// Take a snapshot of the current buffer for rendering. Returns `(width, height, bgra_bytes)`.
    pub fn snapshot(&self) -> (u16, u16, Vec<u8>) {
        let inner = self.inner.lock().expect("framebuffer mutex poisoned");
        (inner.width, inner.height, inner.data.clone())
    }
}
