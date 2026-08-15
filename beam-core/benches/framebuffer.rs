use std::hint::black_box;
use std::time::Instant;

use beam_core::session::framebuffer::{DirtyRect, Framebuffer};

fn main() {
    for (label, width, height) in [
        ("1080p", 1920, 1080),
        ("1440p", 2560, 1440),
        ("4K", 3840, 2160),
    ] {
        let framebuffer = Framebuffer::new(width, height);
        let source = vec![0x7f; usize::from(width) * usize::from(height) * 4];
        let rect = DirtyRect {
            x: width / 4,
            y: height / 4,
            width: width / 2,
            height: height / 2,
        };
        let iterations = 60;
        let start = Instant::now();
        for _ in 0..iterations {
            framebuffer.update_region(black_box(&source), width, height, rect);
            black_box(framebuffer.snapshot());
        }
        let elapsed = start.elapsed();
        let frames_per_second = f64::from(iterations) / elapsed.as_secs_f64();
        println!("{label}: {frames_per_second:.1} update+snapshot frames/s ({elapsed:?})");
        assert!(
            frames_per_second >= 30.0,
            "{label} fell below the 30 update+snapshot frames/s floor"
        );
    }
}
