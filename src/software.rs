use skia_safe::{Canvas, Color, Surface};
use softbuffer::GraphicsContext;

pub(crate) struct SkiaSoftwareRenderer {
    surface: Surface,
    graphics_context: GraphicsContext,
}
impl SkiaSoftwareRenderer {
    pub(crate) fn new(graphics_context: GraphicsContext, width: i32, height: i32) -> Self {
        let surface = Surface::new_raster_n32_premul((width, height)).unwrap();

        Self {
            surface,
            graphics_context,
        }
    }
    pub(crate) fn resize(&mut self, width: i32, height: i32) {
        self.surface = Surface::new_raster_n32_premul((width, height)).unwrap();
    }
    pub(crate) fn draw(&mut self, paint: impl FnOnce(&mut Canvas)) {
        {
            let canvas = self.surface.canvas();
            canvas.clear(Color::TRANSPARENT);
            paint(canvas);
        }

        let snapshot = self.surface.image_snapshot();

        let peek = snapshot.peek_pixels().unwrap();
        let pixels: &[u32] = peek.pixels().unwrap();

        self.graphics_context.set_buffer(
            &pixels,
            self.surface.width() as u16,
            self.surface.height() as u16,
        );
    }
}
