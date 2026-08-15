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
    generation: u64,
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
                generation: 0,
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
        inner.generation = inner.generation.wrapping_add(1);
    }

    /// Copy a fresh region from `src` (the session's live `DecodedImage` buffer, same
    /// dimensions) into our shared buffer.
    /// Copy only `rect` from a same-sized BGRA source image.
    pub fn update_region(&self, src: &[u8], width: u16, height: u16, rect: DirtyRect) {
        let mut inner = self.inner.lock().expect("framebuffer mutex poisoned");
        if inner.width != width || inner.height != height {
            inner.width = width;
            inner.height = height;
            inner.data.resize(src.len(), 0);
        }
        let stride = usize::from(width) * 4;
        let left = usize::from(rect.x) * 4;
        let right = (usize::from(rect.x) + usize::from(rect.width)).min(usize::from(width)) * 4;
        let bottom = (usize::from(rect.y) + usize::from(rect.height)).min(usize::from(height));
        for row in usize::from(rect.y)..bottom {
            let start = row * stride + left;
            let end = row * stride + right;
            if end <= src.len() && end <= inner.data.len() {
                inner.data[start..end].copy_from_slice(&src[start..end]);
            }
        }
        inner.generation = inner.generation.wrapping_add(1);
    }

    /// Take a snapshot of the current buffer for rendering. Returns `(width, height, bgra_bytes)`.
    pub fn snapshot(&self) -> (u16, u16, Vec<u8>) {
        let inner = self.inner.lock().expect("framebuffer mutex poisoned");
        (inner.width, inner.height, inner.data.clone())
    }

    /// Return cheap render metadata, allowing frontends to reuse an existing texture.
    pub fn metadata(&self) -> (u16, u16, u64) {
        let inner = self.inner.lock().expect("framebuffer mutex poisoned");
        (inner.width, inner.height, inner.generation)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn update_region_only_copies_dirty_pixels() {
        let framebuffer = Framebuffer::new(3, 2);
        let source = vec![7; 3 * 2 * 4];
        framebuffer.update_region(
            &source,
            3,
            2,
            DirtyRect {
                x: 1,
                y: 0,
                width: 1,
                height: 2,
            },
        );
        let (_, _, data) = framebuffer.snapshot();
        assert_eq!(&data[4..8], &[7; 4]);
        assert_eq!(&data[16..20], &[7; 4]);
        assert!(data[..4].iter().all(|byte| *byte == 0));
        assert!(data[8..12].iter().all(|byte| *byte == 0));
    }
}
