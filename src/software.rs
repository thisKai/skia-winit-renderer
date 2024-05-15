use std::num::NonZeroU32;

use raw_window_handle::{RawDisplayHandle, RawWindowHandle};
use skia_safe::{Canvas, Color, Surface as SkiaSurface};
use softbuffer::{Context as SoftBufferContext, Surface as SoftBufferSurface};

use crate::skia::SkiaRenderer;

pub(crate) struct SkiaSoftwareRenderer {
    skia_surface: SkiaSurface,
    surface: SoftBufferSurface,
    context: SoftBufferContext,
}
impl SkiaSoftwareRenderer {
    pub(crate) fn new(
        display: RawDisplayHandle,
        window: RawWindowHandle,
        width: i32,
        height: i32,
    ) -> Self {
        let context = unsafe { SoftBufferContext::from_raw(display).unwrap() };
        let surface = unsafe { softbuffer::Surface::from_raw(&context, window) }.unwrap();
        let skia_surface = skia_safe::surfaces::raster_n32_premul((width, height)).unwrap();

        Self {
            skia_surface,
            surface,
            context,
        }
    }
    pub(crate) fn resize(&mut self, width: NonZeroU32, height: NonZeroU32) {
        self.surface.resize(width, height).unwrap();

        let width = width.get() as i32;
        let height = height.get() as i32;
        self.skia_surface = skia_safe::surfaces::raster_n32_premul((width, height)).unwrap();
    }
    pub(crate) fn draw(&mut self, paint: impl FnOnce(&Canvas)) {
        {
            let canvas = self.skia_surface.canvas();
            canvas.clear(Color::TRANSPARENT);
            paint(canvas);
        }

        let snapshot = self.skia_surface.image_snapshot();

        let peek = snapshot.peek_pixels().unwrap();
        let pixels: &[u32] = peek.pixels().unwrap();

        let mut buffer = self.surface.buffer_mut().unwrap();
        buffer.copy_from_slice(pixels);
        buffer.present().unwrap();
    }
}

impl SkiaRenderer for SkiaSoftwareRenderer {
    type ResizeDependency = ();

    fn draw(&mut self, f: &mut dyn FnMut(&Canvas)) {
        self.draw(f);
    }

    fn resize(&mut self, _: &Self::ResizeDependency, width: NonZeroU32, height: NonZeroU32) {
        self.resize(width, height)
    }
}
