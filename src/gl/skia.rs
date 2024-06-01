use crate::skia::SkiaRenderer;

use super::{
    bindings::{self as gl, types::GLint, Gl},
    manager::GlWindowManagerState,
    window::GlWindowRenderer,
};

use glutin::{config::Config, display::GetGlDisplay, prelude::*, surface::SwapInterval};
use raw_window_handle::RawWindowHandle;
use skia_safe::{
    gpu::{gl::FramebufferInfo, SurfaceOrigin},
    Canvas, Color, ColorType, Surface,
};
use std::{error::Error, ffi::CString, fmt::Display, num::NonZeroU32};

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
    ) -> Result<Self, SkiaGlRendererNewError> {
        let (non_zero_width, non_zero_height) = NonZeroU32::new(width)
            .zip(NonZeroU32::new(height))
            .ok_or(SkiaGlRendererNewError::ZeroSize)?;

        let gl_renderer = GlWindowRenderer::new(
            raw_window_handle,
            non_zero_width,
            non_zero_height,
            &gl_state,
        )
        .map_err(SkiaGlRendererNewError::Glutin)?;

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
    pub(crate) fn resize(&mut self, gl_state: &GlWindowManagerState, width: u32, height: u32) {
        let (Some(gl_width), Some(gl_height)) = (NonZeroU32::new(width), NonZeroU32::new(height))
        else {
            return;
        };

        self.gl.resize(gl_width, gl_height);

        let (width, height) = (width.try_into().unwrap(), height.try_into().unwrap());
        gl_state.resize_viewport(width, height);
        self.skia.resize(width, height, &gl_state.gl_config);
    }
    pub(crate) fn draw(&mut self, mut f: impl FnMut(&Canvas)) {
        self.gl.make_current_if_needed();
        self.skia.draw(|canvas| f(canvas));
        self.gl.swap_buffers();
    }
}

impl SkiaRenderer for SkiaGlRenderer {
    type ResizeDependency = GlWindowManagerState;

    fn draw(&mut self, f: &mut dyn FnMut(&Canvas)) {
        self.draw(f);
    }

    fn resize(&mut self, gl_state: &Self::ResizeDependency, width: NonZeroU32, height: NonZeroU32) {
        self.gl.resize(width, height);

        let (width, height) = (
            width.get().try_into().unwrap(),
            height.get().try_into().unwrap(),
        );
        gl_state.resize_viewport(width, height);
        self.skia.resize(width, height, &gl_state.gl_config);
    }
}

#[derive(Debug)]
pub(crate) enum SkiaGlRendererNewError {
    ZeroSize,
    Glutin(glutin::error::Error),
}
impl Display for SkiaGlRendererNewError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            SkiaGlRendererNewError::ZeroSize => f.write_str("Renderer created with zero size"),
            SkiaGlRendererNewError::Glutin(error) => error.fmt(f),
        }
    }
}
impl Error for SkiaGlRendererNewError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            SkiaGlRendererNewError::ZeroSize => None,
            SkiaGlRendererNewError::Glutin(error) => Some(error),
        }
    }
    fn cause(&self) -> Option<&dyn Error> {
        self.source()
    }
}

pub(crate) struct SkiaGlSurface {
    fb_info: FramebufferInfo,
    surface: Surface,
    gr_context: skia_safe::gpu::DirectContext,
}
impl SkiaGlSurface {
    pub(crate) fn new(width: i32, height: i32, gl: &Gl, gl_config: &Config) -> Self {
        let interface = skia_safe::gpu::gl::Interface::new_load_with(|name| {
            if name == "eglGetCurrentDisplay" {
                return std::ptr::null();
            }
            gl_config
                .display()
                .get_proc_address(CString::new(name).unwrap().as_c_str())
        })
        .expect("Could not create interface");

        let mut gr_context = skia_safe::gpu::direct_contexts::make_gl(interface, None)
            .expect("Could not create direct context");

        let fb_info = {
            let mut fboid: GLint = 0;
            unsafe { gl.GetIntegerv(gl::FRAMEBUFFER_BINDING, &mut fboid) };

            FramebufferInfo {
                fboid: fboid.try_into().unwrap(),
                format: skia_safe::gpu::gl::Format::RGBA8.into(),
                ..Default::default()
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
    pub(crate) fn draw(&mut self, paint: impl FnOnce(&Canvas)) {
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
    let backend_render_target = skia_safe::gpu::backend_render_targets::make_gl(
        (width, height),
        Some(gl_config.num_samples().into()),
        gl_config.stencil_size().into(),
        *fb_info,
    );

    skia_safe::gpu::surfaces::wrap_backend_render_target(
        gr_context,
        &backend_render_target,
        SurfaceOrigin::BottomLeft,
        ColorType::RGBA8888,
        None,
        None,
    )
    .expect("Could not create skia surface")
}
