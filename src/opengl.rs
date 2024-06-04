use std::{collections::HashMap, error::Error, ffi::CString, num::NonZeroU32};

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
use provide_any::provide_any::request_mut;
use raw_window_handle::HasRawWindowHandle;
use skia_safe::{
    gpu::{self, backend_render_targets, gl::FramebufferInfo, DirectContext, SurfaceOrigin},
    Canvas, ColorType, Surface,
};
use winit::{
    dpi::PhysicalSize,
    event_loop::EventLoopWindowTarget,
    window::{Window, WindowBuilder, WindowId},
};

use crate::generic::{Env, RenderWindow, SkiaGraphicsEnv, SkiaRender};

pub struct GlWindowManager {
    env: Option<GlEnv>,
    windows: HashMap<WindowId, GlWindow>,
}
impl GlWindowManager {
    pub fn new() -> Self {
        Self {
            env: None,
            windows: HashMap::new(),
        }
    }
    pub fn create_window<T>(
        &mut self,
        elwt: &EventLoopWindowTarget<T>,
        builder: WindowBuilder,
    ) -> Result<WindowId, Box<dyn Error>> {
        let window = self.create_window_object(elwt, builder)?;
        let window_id = window.winit_window.id();

        self.windows.insert(window_id, window);

        Ok(window_id)
    }
    pub fn get_window(&self, window_id: &WindowId) -> Option<&GlWindow> {
        self.windows.get(window_id)
    }
    pub fn get_window_mut(&mut self, window_id: &WindowId) -> Option<&mut GlWindow> {
        self.windows.get_mut(window_id)
    }
    pub fn remove_window(&mut self, window_id: &WindowId) -> Option<GlWindow> {
        self.windows.remove(window_id)
    }
    pub fn draw(
        &mut self,
        window_id: &WindowId,
        f: impl FnMut(&Canvas, &Window),
    ) -> glutin::error::Result<()> {
        let Some(window) = self.windows.get_mut(window_id) else {
            return Ok(());
        };
        window.draw(self.env.as_mut().unwrap(), f)
    }
    pub fn resize_window(&mut self, window_id: &WindowId, size: PhysicalSize<u32>) {
        let Some(window) = self.windows.get_mut(window_id) else {
            return;
        };
        window.resize(self.env.as_mut().unwrap(), size);
    }
    pub fn create_window_object<T>(
        &mut self,
        elwt: &EventLoopWindowTarget<T>,
        builder: WindowBuilder,
    ) -> Result<GlWindow, Box<dyn Error>> {
        match &mut self.env {
            Some(env) => todo!(),
            env @ None => {
                let (new_env, window) = GlEnv::create_with_first_window(elwt, builder)?;
                *env = Some(new_env);
                Ok(window)
            }
        }
    }
}

#[derive(Default)]
pub struct OpenGlEnv {
    state: Option<GlEnv>,
}
impl SkiaGraphicsEnv for OpenGlEnv {
    type Error = Box<dyn Error>;

    fn create<D: raw_window_handle::HasRawDisplayHandle>(display: &D) -> Self {
        Self::default()
    }
    fn create_window<WinitUserEvent>(
        &mut self,
        elwt: &EventLoopWindowTarget<WinitUserEvent>,
        builder: WindowBuilder,
    ) -> Result<crate::generic::RenderWindow, Self::Error> {
        match &mut self.state {
            Some(env) => todo!(),
            env @ None => {
                let (new_env, window, render) = GlEnv::create_with_first_window2(elwt, builder)?;
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
    fn create_with_first_window2<T>(
        elwt: &EventLoopWindowTarget<T>,
        builder: WindowBuilder,
    ) -> Result<(Self, Window, SkiaOpenGlRenderer), Box<dyn Error>> {
        // Only Windows requires the window to be present before creating the display.
        // Other platforms don't really need one.
        //
        // XXX if you don't care about running on Android or so you can safely remove
        // this condition and always pass the window builder.
        let window_builder = cfg!(wgl_backend).then(|| builder.clone());

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

        let display_builder = DisplayBuilder::new().with_window_builder(window_builder);

        let (window, gl_config) = display_builder.build(&elwt, template, gl_config_picker)?;

        println!("Picked a config with {} samples", gl_config.num_samples());

        let window = window.expect("Could not create window with OpenGL context");
        let raw_window_handle = window.raw_window_handle();
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
    fn create_with_first_window<T>(
        elwt: &EventLoopWindowTarget<T>,
        builder: WindowBuilder,
    ) -> Result<(Self, GlWindow), Box<dyn Error>> {
        // Only Windows requires the window to be present before creating the display.
        // Other platforms don't really need one.
        //
        // XXX if you don't care about running on Android or so you can safely remove
        // this condition and always pass the window builder.
        let window_builder = cfg!(wgl_backend).then(|| builder.clone());

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

        let display_builder = DisplayBuilder::new().with_window_builder(window_builder);

        let (window, gl_config) = display_builder.build(&elwt, template, gl_config_picker)?;

        println!("Picked a config with {} samples", gl_config.num_samples());

        let window = window.expect("Could not create window with OpenGL context");
        let raw_window_handle = window.raw_window_handle();
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

        let env = Self {
            config: gl_config,
            display: gl_display,
            fb_info,
        };
        let window = GlWindow {
            surface,
            direct_context: gr_context,
            gl_surface,
            gl_context,
            winit_window: window,
        };
        Ok((env, window))
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

pub struct GlWindow {
    surface: Surface,
    direct_context: DirectContext,
    gl_surface: glutin::surface::Surface<WindowSurface>,
    gl_context: PossiblyCurrentContext,
    winit_window: Window,
}
impl GlWindow {
    fn resize(&mut self, env: &mut GlEnv, size: PhysicalSize<u32>) {
        self.make_current_if_needed().unwrap();
        env.resize_viewport(
            size.width.try_into().unwrap(),
            size.height.try_into().unwrap(),
        );

        /* First resize the opengl drawable */
        let (width, height): (u32, u32) = size.into();

        self.gl_surface.resize(
            &self.gl_context,
            NonZeroU32::new(width.max(1)).unwrap(),
            NonZeroU32::new(height.max(1)).unwrap(),
        );

        self.surface = env.create_window_surface(&self.winit_window, &mut self.direct_context);
    }
    fn draw(
        &mut self,
        env: &mut GlEnv,
        mut f: impl FnMut(&Canvas, &Window),
    ) -> glutin::error::Result<()> {
        self.make_current_if_needed()?;
        let canvas = self.surface.canvas();

        f(canvas, &self.winit_window);

        self.direct_context.flush_and_submit();
        self.gl_surface.swap_buffers(&self.gl_context)
    }
    pub(crate) fn make_current_if_needed(&self) -> glutin::error::Result<()> {
        let gl_context = &self.gl_context;
        if !gl_context.is_current() {
            gl_context.make_current(&self.gl_surface)
        } else {
            Ok(())
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
    fn prepare_and_get_surface(&mut self, env: &mut Env) -> &mut Surface {
        self.make_current_if_needed().unwrap();
        &mut self.surface
    }

    fn present(&mut self, env: &mut Env) {
        let gl_context = self.gl_context.as_ref().unwrap();
        let env = request_mut::<OpenGlEnv>(env).unwrap();
        let env = env.state.as_mut().unwrap();

        self.direct_context.flush_and_submit();
        self.gl_surface.swap_buffers(gl_context).unwrap();
    }
    fn resize(&mut self, env: &mut Env, size: PhysicalSize<u32>, window: &Window) {
        let env = request_mut::<OpenGlEnv>(env).unwrap();
        let env = env.state.as_mut().unwrap();

        self.resize(env, size, window);
    }
}
