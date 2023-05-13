use glutin::{
    config::Config,
    context::{NotCurrentContext, PossiblyCurrentContext},
    display::GetGlDisplay,
    prelude::*,
    surface::{GlSurface, Surface, SurfaceAttributesBuilder, WindowSurface},
};
use raw_window_handle::RawWindowHandle;
use std::num::NonZeroU32;

pub(crate) struct GlWindowRenderer {
    gl_context: Option<PossiblyCurrentContext>,
    // XXX the surface must be dropped before the window.
    pub(crate) surface: Surface<WindowSurface>,
}

impl GlWindowRenderer {
    pub(crate) fn new(
        raw_window_handle: RawWindowHandle,
        not_current_gl_context: NotCurrentContext,
        width: NonZeroU32,
        height: NonZeroU32,
        config: &Config,
    ) -> Self {
        let attrs = SurfaceAttributesBuilder::<WindowSurface>::new().build(
            raw_window_handle,
            width,
            height,
        );

        let surface = unsafe {
            config
                .display()
                .create_window_surface(config, &attrs)
                .unwrap()
        };
        // Make it current.
        let gl_context = not_current_gl_context.make_current(&surface).unwrap();

        Self {
            surface,
            gl_context: Some(gl_context),
        }
    }
    pub(crate) fn gl_context(&self) -> &PossiblyCurrentContext {
        self.gl_context.as_ref().unwrap()
    }
    pub(crate) fn make_not_current(&mut self) -> NotCurrentContext {
        self.gl_context.take().unwrap().make_not_current().unwrap()
    }
    pub(crate) fn make_current_if_needed(&self) {
        let gl_context = self.gl_context();
        if !gl_context.is_current() {
            gl_context.make_current(&self.surface).unwrap();
        }
    }
    pub(crate) fn resize(&self, width: NonZeroU32, height: NonZeroU32) {
        self.make_current_if_needed();
        let gl_context = self.gl_context();
        // Some platforms like EGL require resizing GL surface to update the size
        // Notable platforms here are Wayland and macOS, other don't require it
        // and the function is no-op, but it's wise to resize it for portability
        // reasons.
        self.surface.resize(gl_context, width, height);
    }
    pub(crate) fn swap_buffers(&self) {
        self.surface.swap_buffers(&self.gl_context()).unwrap();
    }
}
impl Drop for GlWindowRenderer {
    fn drop(&mut self) {
        self.make_not_current();
    }
}
