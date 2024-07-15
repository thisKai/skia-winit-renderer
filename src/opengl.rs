use std::{convert::Infallible, error::Error, ffi::CString, num::NonZeroU32};

use glutin::{
    config::{Config, ConfigTemplateBuilder, GlConfig},
    context::{
        ContextApi, ContextAttributesBuilder, NotCurrentContext, NotCurrentGlContext,
        PossiblyCurrentContext, PossiblyCurrentGlContext, Version,
    },
    display::{Display, GetGlDisplay, GlDisplay},
    surface::{GlSurface, SurfaceAttributesBuilder, SwapInterval, WindowSurface},
};
use glutin_winit::DisplayBuilder;
use provide_any::provide_any::{request_mut, Provider};
use skia_safe::{
    gpu::{self, backend_render_targets, gl::FramebufferInfo, DirectContext, SurfaceOrigin},
    ColorType, Surface,
};
use winit::{
    dpi::PhysicalSize,
    event_loop::ActiveEventLoop,
    raw_window_handle::HasWindowHandle,
    window::{Window, WindowAttributes},
};

use crate::generic::{RenderWindow, SkiaGraphicsBackend, SkiaRender};

#[derive(Default)]
pub struct OpenGlBackend {
    state: Option<GlEnv>,
}
impl SkiaGraphicsBackend for OpenGlBackend {
    type CreateError = Infallible;
    type CreateWindowError = Box<dyn Error>;

    fn create<D: raw_window_handle::HasRawDisplayHandle>(_: &D) -> Result<Self, Self::CreateError> {
        Ok(Self::default())
    }
    fn create_window(
        &mut self,
        event_loop: &ActiveEventLoop,
        attributes: WindowAttributes,
    ) -> Result<crate::generic::RenderWindow, Self::CreateWindowError> {
        match &mut self.state {
            Some(env) => todo!(),
            env @ None => {
                let (new_env, window, render) =
                    GlEnv::create_with_first_window(event_loop, attributes)?;
                *env = Some(new_env);
                Ok(RenderWindow::new(render, window))
            }
        }
    }
}

pub(crate) struct GlEnv {
    config: Config,
    display: Display,
    fb_info: FramebufferInfo,
}
impl GlEnv {
    fn create_with_first_window(
        event_loop: &ActiveEventLoop,
        attributes: WindowAttributes,
    ) -> Result<(Self, Window, SkiaOpenGlRenderer), Box<dyn Error>> {
        // Only Windows requires the window to be present before creating the display.
        // Other platforms don't really need one.
        //
        // XXX if you don't care about running on Android or so you can safely remove
        // this condition and always pass the window builder.
        let window_attributes = cfg!(wgl_backend).then(|| attributes.clone());

        // The template will match only the configurations supporting rendering
        // to windows.
        //
        // XXX We force transparency only on macOS, given that EGL on X11 doesn't
        // have it, but we still want to show window. The macOS situation is like
        // that, because we can query only one config at a time on it, but all
        // normal platforms will return multiple configs, so we can find the config
        // with transparency ourselves inside the `reduce`.
        let template = ConfigTemplateBuilder::new()
            .with_alpha_size(8)
            .with_transparency(cfg!(cgl_backend));

        let display_builder = DisplayBuilder::new().with_window_attributes(window_attributes);

        let (window, gl_config) = display_builder.build(event_loop, template, gl_config_picker)?;

        println!("Picked a config with {} samples", gl_config.num_samples());

        let window = window.ok_or("Could not create window with OpenGL context")?;
        let raw_window_handle = window.window_handle().unwrap().as_raw();
        // XXX The display could be obtained from any object created by it, so we can
        // query it from the config.
        let gl_display = gl_config.display();

        // The context creation part.
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

        let not_current_gl_context = unsafe {
            gl_display
                .create_context(&gl_config, &context_attributes)
                .unwrap_or_else(|_| {
                    gl_display
                        .create_context(&gl_config, &fallback_context_attributes)
                        .unwrap_or_else(|_| {
                            gl_display
                                .create_context(&gl_config, &legacy_context_attributes)
                                .expect("failed to create context")
                        })
                })
        };

        let (width, height): (u32, u32) = window.inner_size().into();

        let attrs = SurfaceAttributesBuilder::<WindowSurface>::new().build(
            raw_window_handle,
            NonZeroU32::new(width).unwrap(),
            NonZeroU32::new(height).unwrap(),
        );

        let gl_surface = unsafe {
            gl_config
                .display()
                .create_window_surface(&gl_config, &attrs)
                .expect("Could not create gl window surface")
        };

        let gl_context = not_current_gl_context
            .make_current(&gl_surface)
            .expect("Could not make GL context current when setting up skia renderer");

        gl::load_with(|symbol| {
            gl_display
                .get_proc_address(CString::new(symbol).unwrap().as_c_str())
                .cast()
        });

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
            let mut fboid: gl::types::GLint = 0;
            unsafe { gl::GetIntegerv(gl::FRAMEBUFFER_BINDING, &mut fboid) };

            FramebufferInfo {
                fboid: fboid.try_into().unwrap(),
                format: skia_safe::gpu::gl::Format::RGBA8.into(),
                ..Default::default()
            }
        };

        let num_samples = gl_config.num_samples() as usize;
        let stencil_size = gl_config.stencil_size() as usize;

        let surface =
            Self::create_surface(&window, fb_info, &mut gr_context, num_samples, stencil_size);

        // Try setting vsync.
        if let Err(res) = gl_surface
            .set_swap_interval(&gl_context, SwapInterval::Wait(NonZeroU32::new(1).unwrap()))
        {
            eprintln!("Error setting vsync: {:?}", res);
        }

        let env = Self {
            config: gl_config,
            display: gl_display,
            fb_info,
        };
        let renderer = SkiaOpenGlRenderer {
            surface,
            direct_context: gr_context,
            gl_surface,
            gl_context: Some(gl_context),
        };
        Ok((env, window, renderer))
    }

    fn create_window_surface(
        &mut self,
        window: &Window,
        direct_context: &mut DirectContext,
    ) -> Surface {
        let num_samples = self.config.num_samples() as usize;
        let stencil_size = self.config.stencil_size() as usize;

        Self::create_surface(
            window,
            self.fb_info,
            direct_context,
            num_samples,
            stencil_size,
        )
    }
    fn create_surface(
        window: &Window,
        fb_info: FramebufferInfo,
        gr_context: &mut skia_safe::gpu::DirectContext,
        num_samples: usize,
        stencil_size: usize,
    ) -> Surface {
        let size = window.inner_size();
        let size = (
            size.width.try_into().expect("Could not convert width"),
            size.height.try_into().expect("Could not convert height"),
        );
        let backend_render_target =
            backend_render_targets::make_gl(size, num_samples, stencil_size, fb_info);

        gpu::surfaces::wrap_backend_render_target(
            gr_context,
            &backend_render_target,
            SurfaceOrigin::BottomLeft,
            ColorType::RGBA8888,
            None,
            None,
        )
        .expect("Could not create skia surface")
    }
    pub(crate) fn resize_viewport(&self, width: i32, height: i32) {
        unsafe {
            gl::Viewport(0, 0, width, height);
        }
    }
}

pub fn gl_config_picker(configs: Box<dyn Iterator<Item = Config> + '_>) -> Config {
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
}

pub struct SkiaOpenGlRenderer {
    surface: Surface,
    direct_context: DirectContext,
    gl_context: Option<PossiblyCurrentContext>,
    gl_surface: glutin::surface::Surface<WindowSurface>,
}
impl SkiaOpenGlRenderer {
    pub(crate) fn gl_context(&self) -> &PossiblyCurrentContext {
        self.gl_context.as_ref().unwrap()
    }
    fn make_current_if_needed(&self) -> glutin::error::Result<()> {
        let gl_context = self.gl_context.as_ref().unwrap();
        if !gl_context.is_current() {
            gl_context.make_current(&self.gl_surface)
        } else {
            Ok(())
        }
    }
    fn make_not_current(&mut self) -> NotCurrentContext {
        self.gl_context.take().unwrap().make_not_current().unwrap()
    }
    fn resize(&mut self, env: &mut GlEnv, size: PhysicalSize<u32>, window: &Window) {
        let gl_context = self.gl_context.as_ref().unwrap();

        env.resize_viewport(
            size.width.try_into().unwrap(),
            size.height.try_into().unwrap(),
        );

        /* First resize the opengl drawable */
        let (width, height): (u32, u32) = size.into();

        self.gl_surface.resize(
            gl_context,
            NonZeroU32::new(width.max(1)).unwrap(),
            NonZeroU32::new(height.max(1)).unwrap(),
        );

        self.surface = env.create_window_surface(&window, &mut self.direct_context);
    }
}
impl SkiaRender for SkiaOpenGlRenderer {
    fn prepare_and_get_surface(&mut self, env: &mut dyn Provider) -> &mut Surface {
        self.make_current_if_needed().unwrap();
        &mut self.surface
    }

    fn present(&mut self, env: &mut dyn Provider) {
        let gl_context = self.gl_context.as_ref().unwrap();
        let env = request_mut::<OpenGlBackend>(env).unwrap();
        let env = env.state.as_mut().unwrap();

        self.direct_context.flush_and_submit();
        self.gl_surface.swap_buffers(gl_context).unwrap();
    }
    fn resize(&mut self, env: &mut dyn Provider, size: PhysicalSize<u32>, window: &Window) {
        let env = request_mut::<OpenGlBackend>(env).unwrap();
        let env = env.state.as_mut().unwrap();

        self.resize(env, size, window);
    }
}
