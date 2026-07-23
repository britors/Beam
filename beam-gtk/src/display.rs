//! `RemoteDisplay`: a `gtk::Widget` that paints a session's [`beam_core::session::Framebuffer`].
//!
//! Framebuffer updates arrive at arbitrary, possibly high, frequency from the network task; we
//! never rebuild the GPU texture more than once per frame. [`RemoteDisplay::mark_dirty`] just
//! flips a flag, and an `add_tick_callback` registered at construction time is the only thing
//! that ever calls `queue_draw()`, coalescing a burst of updates into a single repaint at vsync.

use std::cell::{Cell, RefCell};
use std::sync::Arc;

use beam_core::session::Framebuffer;
use gtk::glib;
use gtk::prelude::*;
use gtk::subclass::prelude::*;
use gtk::{gdk, graphene, gsk};

mod imp {
    use super::*;

    #[derive(Default)]
    pub struct RemoteDisplay {
        pub framebuffer: RefCell<Option<Arc<Framebuffer>>>,
        pub dirty: Cell<bool>,
    }

    #[glib::object_subclass]
    impl ObjectSubclass for RemoteDisplay {
        const NAME: &'static str = "BeamRemoteDisplay";
        type Type = super::RemoteDisplay;
        type ParentType = gtk::Widget;
    }

    impl ObjectImpl for RemoteDisplay {
        fn constructed(&self) {
            self.parent_constructed();

            let obj = self.obj().clone();
            obj.add_tick_callback(move |widget, _frame_clock| {
                let imp = widget.imp();
                if imp.dirty.replace(false) {
                    widget.queue_draw();
                }
                glib::ControlFlow::Continue
            });
        }
    }

    impl WidgetImpl for RemoteDisplay {
        fn measure(&self, orientation: gtk::Orientation, _for_size: i32) -> (i32, i32, i32, i32) {
            let (width, height, _) = match self.framebuffer.borrow().as_ref() {
                Some(fb) => fb.snapshot(),
                None => (0, 0, Vec::new()),
            };
            let size = match orientation {
                gtk::Orientation::Horizontal => i32::from(width),
                _ => i32::from(height),
            };
            // No natural minimum: the widget scales the remote desktop to whatever space it's given.
            (0, size, -1, -1)
        }

        fn snapshot(&self, snapshot: &gtk::Snapshot) {
            let widget_width = self.obj().width() as f32;
            let widget_height = self.obj().height() as f32;
            if widget_width <= 0.0 || widget_height <= 0.0 {
                return;
            }

            let Some(fb) = self.framebuffer.borrow().as_ref().cloned() else {
                return;
            };
            let (width, height, data) = fb.snapshot();
            if width == 0 || height == 0 {
                return;
            }

            let bytes = glib::Bytes::from_owned(data);
            let texture = gdk::MemoryTexture::new(
                i32::from(width),
                i32::from(height),
                gdk::MemoryFormat::B8g8r8a8,
                &bytes,
                usize::from(width) * 4,
            );

            // Scale to fit the widget while preserving the desktop's aspect ratio, centered.
            let scale = (widget_width / f32::from(width)).min(widget_height / f32::from(height));
            let draw_width = f32::from(width) * scale;
            let draw_height = f32::from(height) * scale;
            let offset_x = (widget_width - draw_width) / 2.0;
            let offset_y = (widget_height - draw_height) / 2.0;

            snapshot.save();
            snapshot.translate(&graphene::Point::new(offset_x, offset_y));
            snapshot.append_scaled_texture(
                &texture,
                gsk::ScalingFilter::Linear,
                &graphene::Rect::new(0.0, 0.0, draw_width, draw_height),
            );
            snapshot.restore();
        }
    }
}

glib::wrapper! {
    pub struct RemoteDisplay(ObjectSubclass<imp::RemoteDisplay>)
        @extends gtk::Widget,
        @implements gtk::Accessible, gtk::Buildable, gtk::ConstraintTarget;
}

impl Default for RemoteDisplay {
    fn default() -> Self {
        glib::Object::new()
    }
}

impl RemoteDisplay {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn set_framebuffer(&self, framebuffer: Arc<Framebuffer>) {
        *self.imp().framebuffer.borrow_mut() = Some(framebuffer);
        self.mark_dirty();
        self.queue_resize();
    }

    /// Request a repaint on the next frame; safe to call at any frequency.
    pub fn mark_dirty(&self) {
        self.imp().dirty.set(true);
    }

    /// The current remote desktop size, if known.
    pub fn desktop_size(&self) -> Option<(u16, u16)> {
        let fb = self.imp().framebuffer.borrow();
        let fb = fb.as_ref()?;
        let (width, height, _) = fb.snapshot();
        (width > 0 && height > 0).then_some((width, height))
    }

    /// Map a pointer position in widget-local coordinates to remote desktop coordinates,
    /// accounting for the letterboxing done in `snapshot()`. Returns `None` outside the
    /// displayed image (e.g. in the letterbox bars).
    pub fn widget_to_remote(&self, x: f64, y: f64) -> Option<(u16, u16)> {
        let (width, height) = self.desktop_size()?;
        let widget_width = f64::from(self.width());
        let widget_height = f64::from(self.height());
        if widget_width <= 0.0 || widget_height <= 0.0 {
            return None;
        }

        let scale = (widget_width / f64::from(width)).min(widget_height / f64::from(height));
        let draw_width = f64::from(width) * scale;
        let draw_height = f64::from(height) * scale;
        let offset_x = (widget_width - draw_width) / 2.0;
        let offset_y = (widget_height - draw_height) / 2.0;

        let rel_x = x - offset_x;
        let rel_y = y - offset_y;
        if rel_x < 0.0 || rel_y < 0.0 || rel_x >= draw_width || rel_y >= draw_height {
            return None;
        }

        let remote_x = (rel_x / scale).round().clamp(0.0, f64::from(width - 1));
        let remote_y = (rel_y / scale).round().clamp(0.0, f64::from(height - 1));
        Some((remote_x as u16, remote_y as u16))
    }
}
