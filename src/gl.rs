#![allow(clippy::all)]
include!(concat!(env!("OUT_DIR"), "/gl_bindings.rs"));
pub(crate) use Gles2 as Gl;

use glutin::{
    config::{Config, ConfigTemplateBuilder},
    context::{
        ContextApi, ContextAttributesBuilder, NotCurrentContext, PossiblyCurrentContext, Version,
    },
    display::{Display, GetGlDisplay},
    prelude::*,
    surface::{GlSurface, Surface, SurfaceAttributesBuilder, WindowSurface},
};
use glutin_winit::DisplayBuilder;
use raw_window_handle::{HasRawWindowHandle, RawWindowHandle};
use std::{error::Error, ffi::CString, num::NonZeroU32};
use winit::{
    event_loop::EventLoopWindowTarget,
    window::{Window as WinitWindow, WindowBuilder},
};

pub(crate) struct GlWindowRenderer {
    gl_context: Option<PossiblyCurrentContext>,
    // XXX the surface must be dropped before the window.
    pub(crate) surface: Surface<WindowSurface>,
}

impl GlWindowRenderer {
    pub(crate) fn new(
        window: &WinitWindow,
        config: &Config,
        not_current_gl_context: NotCurrentContext,
    ) -> Self {
        let (width, height): (u32, u32) = window.inner_size().into();
        let raw_window_handle = window.raw_window_handle();
        let attrs = SurfaceAttributesBuilder::<WindowSurface>::new().build(
            raw_window_handle,
            NonZeroU32::new(width).unwrap(),
            NonZeroU32::new(height).unwrap(),
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

pub(crate) struct GlWindowManagerState {
    pub(crate) gl_config: Config,
    pub(crate) gl_display: Display,
    pub(crate) gl: Gl,
}
impl GlWindowManagerState {
    pub(crate) fn create_with_first_window(
        window_target: &EventLoopWindowTarget<()>,
        window_builder: &WindowBuilder,
    ) -> Result<(Self, Option<WinitWindow>), Box<dyn Error>> {
        // Only windows requires the window to be present before creating the display.
        // Other platforms don't really need one.
        //
        // XXX if you don't care about running on android or so you can safely remove
        // this condition and always pass the window builder.
        let window_builder = if cfg!(wgl_backend) {
            Some(window_builder.clone())
        } else {
            None
        };

        // The template will match only the configurations supporting rendering to
        // windows.
        let template = ConfigTemplateBuilder::new()
            .with_alpha_size(8)
            .with_transparency(cfg!(cgl_backend));

        let display_builder = DisplayBuilder::new().with_window_builder(window_builder);

        let (first_window, gl_config) =
            display_builder.build(&window_target, template, |configs| {
                // Find the config with the maximum number of samples, so our triangle will
                // be smooth.
                configs
                    .reduce(|accum, config| {
                        let transparency_check = config.supports_transparency().unwrap_or(false)
                            & !accum.supports_transparency().unwrap_or(false);

                        if transparency_check || config.num_samples() > accum.num_samples() {
                            config
                        } else {
                            accum
                        }
                    })
                    .unwrap()
            })?;

        println!("Picked a config with {} samples", gl_config.num_samples());

        // XXX The display could be obtained from the any object created by it, so we
        // can query it from the config.
        let gl_display = gl_config.display();

        let gl = Gl::load_with(|symbol| {
            let symbol = CString::new(symbol).unwrap();
            gl_display.get_proc_address(symbol.as_c_str()).cast()
        });

        Ok((
            Self {
                gl_config,
                gl_display,
                gl,
            },
            first_window,
        ))
    }
    pub(crate) fn try_create_context(
        &self,
        raw_window_handle: RawWindowHandle,
    ) -> glutin::error::Result<NotCurrentContext> {
        // The context creation part. It can be created before surface and that's how
        // it's expected in multithreaded + multiwindow operation mode, since you
        // can send NotCurrentContext, but not Surface.
        let context_attributes = ContextAttributesBuilder::new().build(Some(raw_window_handle));

        // Since glutin by default tries to create OpenGL core context, which may not be
        // present we should try gles.
        let fallback_context_attributes = ContextAttributesBuilder::new()
            .with_context_api(ContextApi::Gles(None))
            .build(Some(raw_window_handle));

        // There are also some old devices that support neither modern OpenGL nor GLES.
        // To support these we can try and create a 2.1 context.
        let legacy_context_attributes = ContextAttributesBuilder::new()
            .with_context_api(ContextApi::OpenGl(Some(Version::new(2, 1))))
            .build(Some(raw_window_handle));

        unsafe {
            self.gl_display
                .create_context(&self.gl_config, &context_attributes)
                .or_else(|_| {
                    self.gl_display
                        .create_context(&self.gl_config, &fallback_context_attributes)
                        .or_else(|_| {
                            self.gl_display
                                .create_context(&self.gl_config, &legacy_context_attributes)
                        })
                })
        }
    }
    pub(crate) fn resize_viewport(&self, width: i32, height: i32) {
        unsafe {
            self.gl.Viewport(0, 0, width, height);
        }
    }
}
