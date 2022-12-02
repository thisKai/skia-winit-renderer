use std::ffi::CString;

use glutin::{config::Config, prelude::*};
use skia_safe::{
    gpu::{gl::FramebufferInfo, BackendRenderTarget, SurfaceOrigin},
    Canvas, Color, ColorType, Surface,
};
use winit::dpi::PhysicalSize;

use crate::gl::{self, types::GLint, Gl};

pub struct SkiaGlRenderer {
    gl: Gl,
    fb_info: FramebufferInfo,
    surface: Surface,
    gr_context: skia_safe::gpu::DirectContext,
}
impl SkiaGlRenderer {
    pub fn new<D: GlDisplay>(gl_config: &Config, gl_display: &D, size: PhysicalSize<u32>) -> Self {
        let gl = Gl::load_with(|symbol| {
            let symbol = CString::new(symbol).unwrap();
            gl_display.get_proc_address(symbol.as_c_str()).cast()
        });
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
            gl,
            fb_info,
            surface,
            gr_context,
        }
    }
    pub fn resize(
        &mut self,
        gl_config: &Config,
        size: PhysicalSize<u32>,
    ) {
        self.resize_viewport(
            size.width.try_into().unwrap(),
            size.height.try_into().unwrap(),
        );
        self.create_surface(gl_config, size);
    }
    fn resize_viewport(&self, width: i32, height: i32) {
        unsafe {
            self.gl.Viewport(0, 0, width, height);
        }
    }
    fn create_surface(
        &mut self,
        gl_config: &Config,
        size: PhysicalSize<u32>,
    ) {
        self.surface = create_skia_surface(
            gl_config,
            size,
            &self.fb_info,
            &mut self.gr_context,
        );
    }
    pub fn draw(&mut self, paint: impl FnOnce(&mut Canvas)) {
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
        None,
    )
    .unwrap()
}
