use super::Gl;

use glutin::{
    config::{Config, ConfigTemplateBuilder},
    context::{ContextApi, ContextAttributesBuilder, NotCurrentContext, Version},
    display::{Display, GetGlDisplay},
    prelude::*,
};
use glutin_winit::DisplayBuilder;
use raw_window_handle::RawWindowHandle;
use std::{error::Error, ffi::CString};
use winit::{
    event_loop::EventLoopWindowTarget,
    window::{Window, WindowBuilder},
};

pub(crate) struct GlWindowManagerState {
    pub(crate) gl_config: Config,
    pub(crate) gl_display: Display,
    pub(crate) gl: Gl,
}
impl GlWindowManagerState {
    pub(crate) fn create_with_first_winit_window(
        window_target: &EventLoopWindowTarget<()>,
        window_builder: &WindowBuilder,
    ) -> Result<(Self, Option<Window>), Box<dyn Error>> {
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
