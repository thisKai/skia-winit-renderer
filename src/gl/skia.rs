use super::{
    bindings::{self as gl, types::GLint, Gl},
    manager::GlWindowManagerState,
    window::GlWindowRenderer,
};

use glutin::{config::Config, prelude::*, surface::SwapInterval};
use raw_window_handle::RawWindowHandle;
use skia_safe::{
    gpu::{gl::FramebufferInfo, BackendRenderTarget, SurfaceOrigin},
    Canvas, Color, ColorType, Surface,
};
use std::num::NonZeroU32;

pub(crate) struct SkiaGlRenderer {
    skia: SkiaGlSurface,
    gl: GlWindowRenderer,
}
impl SkiaGlRenderer {
    pub(crate) fn new(
        raw_window_handle: RawWindowHandle,
        width: u32,
        height: u32,
        gl_state: &GlWindowManagerState,
    ) -> Result<Self, glutin::error::Error> {
        let gl_renderer = GlWindowRenderer::new(
            raw_window_handle,
            width.try_into().unwrap(),
            height.try_into().unwrap(),
            &gl_state,
        )?;

        // The context needs to be current for the Renderer to set up shaders and
        // buffers. It also performs function loading, which needs a current context on
        // WGL.
        let skia = SkiaGlSurface::new(
            width.try_into().unwrap(),
            height.try_into().unwrap(),
            &gl_state.gl,
            &gl_state.gl_config,
        );

        // Try setting vsync.
        if let Err(res) = gl_renderer.surface.set_swap_interval(
            gl_renderer.gl_context(),
            SwapInterval::Wait(NonZeroU32::new(1).unwrap()),
        ) {
            eprintln!("Error setting vsync: {:?}", res);
        }

        Ok(Self {
            skia,
            gl: gl_renderer,
        })
    }
    pub(crate) fn resize(&mut self, gl_state: &mut GlWindowManagerState, width: u32, height: u32) {
        let (Some(gl_width), Some(gl_height)) = (NonZeroU32::new(width), NonZeroU32::new(height)) else {
            return;
        };

        self.gl.resize(gl_width, gl_height);

        let (width, height) = (width.try_into().unwrap(), height.try_into().unwrap());
        gl_state.resize_viewport(width, height);
        self.skia.resize(width, height, &gl_state.gl_config);
    }
    pub(crate) fn draw(&mut self, mut f: impl FnMut(&mut Canvas)) {
        self.gl.make_current_if_needed();
        self.skia.draw(|canvas| f(canvas));
        self.gl.swap_buffers();
    }
}

pub(crate) struct SkiaGlSurface {
    fb_info: FramebufferInfo,
    surface: Surface,
    gr_context: skia_safe::gpu::DirectContext,
}
impl SkiaGlSurface {
    pub(crate) fn new(width: i32, height: i32, gl: &Gl, gl_config: &Config) -> Self {
        let mut gr_context = skia_safe::gpu::DirectContext::new_gl(None, None).unwrap();

        let fb_info = {
            let mut fboid: GLint = 0;
            unsafe { gl.GetIntegerv(gl::FRAMEBUFFER_BINDING, &mut fboid) };

            FramebufferInfo {
                fboid: fboid.try_into().unwrap(),
                format: skia_safe::gpu::gl::Format::RGBA8.into(),
            }
        };
        let surface = create_skia_surface(width, height, gl_config, &fb_info, &mut gr_context);

        Self {
            fb_info,
            surface,
            gr_context,
        }
    }
    pub(crate) fn resize(&mut self, width: i32, height: i32, gl_config: &Config) {
        self.surface = create_skia_surface(
            width,
            height,
            gl_config,
            &self.fb_info,
            &mut self.gr_context,
        );
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
    width: i32,
    height: i32,
    gl_config: &Config,
    fb_info: &FramebufferInfo,
    gr_context: &mut skia_safe::gpu::DirectContext,
) -> skia_safe::Surface {
    let backend_render_target = BackendRenderTarget::new_gl(
        (width, height),
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
