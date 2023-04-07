use glutin::{config::Config, prelude::*};
use skia_safe::{
    gpu::{gl::FramebufferInfo, BackendRenderTarget, SurfaceOrigin},
    Canvas, Color, ColorType, Surface, SurfaceProps, SurfacePropsFlags,
};
use softbuffer::GraphicsContext;
use winit::dpi::PhysicalSize;

use crate::gl::{self, types::GLint, Gl};

pub(crate) struct SkiaSoftwareRenderer {
    surface: Surface,
    graphics_context: GraphicsContext,
}
impl SkiaSoftwareRenderer {
    pub(crate) fn new(graphics_context: GraphicsContext, size: PhysicalSize<u32>) -> Self {
        let surface =
            Surface::new_raster_n32_premul((size.width as i32, size.height as i32)).unwrap();

        Self {
            surface,
            graphics_context,
        }
    }
    pub(crate) fn resize(&mut self, size: PhysicalSize<u32>) {
        self.surface =
            Surface::new_raster_n32_premul((size.width as i32, size.height as i32)).unwrap();
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

pub(crate) struct SkiaGlRenderer {
    fb_info: FramebufferInfo,
    surface: Surface,
    gr_context: skia_safe::gpu::DirectContext,
}
impl SkiaGlRenderer {
    pub(crate) fn new(gl: &Gl, gl_config: &Config, size: PhysicalSize<u32>) -> Self {
        let mut gr_context = skia_safe::gpu::DirectContext::new_gl(None, None).unwrap();

        let fb_info = {
            let mut fboid: GLint = 0;
            unsafe { gl.GetIntegerv(gl::FRAMEBUFFER_BINDING, &mut fboid) };

            FramebufferInfo {
                fboid: fboid.try_into().unwrap(),
                format: skia_safe::gpu::gl::Format::RGBA8.into(),
            }
        };
        let surface = create_skia_surface(gl_config, size, &fb_info, &mut gr_context);

        Self {
            fb_info,
            surface,
            gr_context,
        }
    }
    pub(crate) fn resize(&mut self, gl_config: &Config, size: PhysicalSize<u32>) {
        self.surface = create_skia_surface(gl_config, size, &self.fb_info, &mut self.gr_context);
    }
    pub(crate) fn draw(&mut self, paint: impl FnOnce(&mut Canvas)) {
        {
            let canvas = self.surface.canvas();
            canvas.clear(Color::TRANSPARENT);
            paint(canvas);
        }
        self.gr_context.flush(None);
    }
}
fn create_skia_surface(
    gl_config: &Config,
    size: PhysicalSize<u32>,
    fb_info: &FramebufferInfo,
    gr_context: &mut skia_safe::gpu::DirectContext,
) -> skia_safe::Surface {
    let backend_render_target = BackendRenderTarget::new_gl(
        (
            size.width.try_into().unwrap(),
            size.height.try_into().unwrap(),
        ),
        Some(gl_config.num_samples().into()),
        gl_config.stencil_size().into(),
        *fb_info,
    );
    Surface::from_backend_render_target(
        gr_context,
        &backend_render_target,
        SurfaceOrigin::BottomLeft,
        ColorType::RGBA8888,
        None,
        Some(&SurfaceProps::new(
            SurfacePropsFlags::empty(),
            skia_safe::PixelGeometry::RGBH,
        )),
    )
    .unwrap()
}
