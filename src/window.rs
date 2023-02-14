use crate::skia::{SkiaGlRenderer, SkiaSoftwareRenderer};
use glutin::{
    config::{Config, ConfigTemplateBuilder},
    context::{ContextApi, ContextAttributesBuilder, NotCurrentContext, PossiblyCurrentContext},
    display::{Display, GetGlDisplay},
    prelude::*,
    surface::{GlSurface, Surface, SurfaceAttributesBuilder, SwapInterval, WindowSurface},
};
use glutin_winit::DisplayBuilder;
use raw_window_handle::{HasRawWindowHandle, RawWindowHandle};
use skia_safe::Canvas;
use softbuffer::GraphicsContext;
use std::{cell::RefCell, collections::HashMap, num::NonZeroU32, rc::Rc};
use winit::{
    dpi::{PhysicalPosition, PhysicalSize},
    event::WindowEvent,
    event_loop::{ControlFlow, EventLoopWindowTarget},
    window::{Window as WinitWindow, WindowBuilder, WindowId},
};

#[allow(unused_variables)]
pub trait Window: 'static {
    fn open(&mut self) {}
    fn close(&mut self) -> bool {
        true
    }
    fn draw(&self, canvas: &mut Canvas) {}
    fn resize(&mut self, width: u32, height: u32) {}
    fn cursor_enter(&mut self) {}
    fn cursor_leave(&mut self) {}
    fn cursor_move(&mut self, x: f64, y: f64) {}
}

pub struct SoftwareRenderedWindowManager {
    windows: HashMap<WindowId, (Rc<SkiaSoftwareRenderedWindow>, Box<dyn Window>)>,
}
impl SoftwareRenderedWindowManager {
    pub fn new() -> Self {
        Self {
            windows: HashMap::new(),
        }
    }
    pub fn close_window(&mut self, id: &WindowId) -> bool {
        self.windows.remove(&id);
        self.windows.is_empty()
    }
    pub fn create_window(
        &mut self,
        window_target: &EventLoopWindowTarget<()>,
        mut state: Box<dyn Window>,
    ) -> Rc<SkiaSoftwareRenderedWindow> {
        let window = WindowBuilder::new()
            .with_transparent(true)
            .with_visible(false)
            .build(window_target)
            .unwrap();
        let size = window.inner_size();

        let window = Rc::new(SkiaSoftwareRenderedWindow::new(window, window_target));
        let id = window.window.id();

        state.resize(size.width, size.height);

        window.window.set_visible(true);
        state.open();

        self.windows.insert(id, (window.clone(), state));
        window
    }
}

pub struct SkiaSoftwareRenderedWindow {
    renderer: RefCell<SkiaSoftwareRenderer>,
    window: WinitWindow,
}
impl SkiaSoftwareRenderedWindow {
    fn new(window: WinitWindow, display: &EventLoopWindowTarget<()>) -> Self {
        let gc = unsafe { GraphicsContext::new(&window, display).unwrap() };
        let renderer = RefCell::new(SkiaSoftwareRenderer::new(gc, window.inner_size()));

        Self { renderer, window }
    }
    fn resize(&self, size: PhysicalSize<u32>) {
        let mut renderer = self.renderer.borrow_mut();
        renderer.resize(size);
    }
    fn draw<F>(&self, f: F)
    where
        F: FnMut(&mut Canvas),
    {
        self.renderer.borrow_mut().draw(f);
    }
}

pub struct GlWindow {
    gl_context: Option<PossiblyCurrentContext>,
    // XXX the surface must be dropped before the window.
    pub surface: Surface<WindowSurface>,
    pub window: WinitWindow,
}

impl GlWindow {
    pub fn new(
        window: WinitWindow,
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
            window,
            surface,
            gl_context: Some(gl_context),
        }
    }
    pub fn gl_context(&self) -> &PossiblyCurrentContext {
        self.gl_context.as_ref().unwrap()
    }
    pub fn make_not_current(mut self) -> NotCurrentContext {
        self.gl_context.take().unwrap().make_not_current().unwrap()
    }
    fn make_current_if_needed(&self) {
        let gl_context = self.gl_context();
        if !gl_context.is_current() {
            gl_context.make_current(&self.surface).unwrap();
        }
    }
    pub fn resize(&self, width: NonZeroU32, height: NonZeroU32) {
        self.make_current_if_needed();
        let gl_context = self.gl_context();
        // Some platforms like EGL require resizing GL surface to update the size
        // Notable platforms here are Wayland and macOS, other don't require it
        // and the function is no-op, but it's wise to resize it for portability
        // reasons.
        self.surface.resize(gl_context, width, height);
    }
    pub fn swap_buffers(&self) {
        self.surface.swap_buffers(&self.gl_context()).unwrap();
    }
}
impl Drop for GlWindow {
    fn drop(&mut self) {
        self.gl_context.take().unwrap().make_not_current().unwrap();
    }
}

pub struct GlWindowManager {
    gl_config: Config,
    gl_display: Display,
    first_window: Option<WinitWindow>,
    windows: HashMap<WindowId, (Rc<SkiaGlAppWindow>, Box<dyn Window>)>,
}
impl GlWindowManager {
    pub fn new(window_target: &EventLoopWindowTarget<()>) -> Self {
        // Only windows requires the window to be present before creating the display.
        // Other platforms don't really need one.
        //
        // XXX if you don't care about running on android or so you can safely remove
        // this condition and always pass the window builder.
        let window_builder = if cfg!(wgl_backend) {
            Some(
                WindowBuilder::new()
                    .with_transparent(true)
                    .with_visible(false),
            )
        } else {
            None
        };

        // The template will match only the configurations supporting rendering to
        // windows.
        let template = ConfigTemplateBuilder::new().with_alpha_size(8);

        let display_builder = DisplayBuilder::new().with_window_builder(window_builder);

        let (first_window, gl_config) = display_builder
            .build(&window_target, template, |configs| {
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
            })
            .unwrap();

        println!("Picked a config with {} samples", gl_config.num_samples());

        // XXX The display could be obtained from the any object created by it, so we
        // can query it from the config.
        let gl_display = gl_config.display();

        Self {
            gl_config,
            gl_display,
            first_window,
            windows: HashMap::new(),
        }
    }
    fn create_context(&self, raw_window_handle: RawWindowHandle) -> NotCurrentContext {
        // The context creation part. It can be created before surface and that's how
        // it's expected in multithreaded + multiwindow operation mode, since you
        // can send NotCurrentContext, but not Surface.
        let context_attributes = ContextAttributesBuilder::new().build(Some(raw_window_handle));

        // Since glutin by default tries to create OpenGL core context, which may not be
        // present we should try gles.
        let fallback_context_attributes = ContextAttributesBuilder::new()
            .with_context_api(ContextApi::Gles(None))
            .build(Some(raw_window_handle));
        unsafe {
            self.gl_display
                .create_context(&self.gl_config, &context_attributes)
                .unwrap_or_else(|_| {
                    self.gl_display
                        .create_context(&self.gl_config, &fallback_context_attributes)
                        .expect("failed to create context")
                })
        }
    }
    pub fn close_window(&mut self, id: &WindowId) -> bool {
        self.windows.remove(&id);
        self.windows.is_empty()
    }
    pub fn create_window(
        &mut self,
        window_target: &EventLoopWindowTarget<()>,
        mut state: Box<dyn Window>,
    ) -> Rc<SkiaGlAppWindow> {
        #[cfg(target_os = "android")]
        println!("Android window available");

        let window = self.first_window.take().unwrap_or_else(|| {
            let window_builder = WindowBuilder::new()
                .with_transparent(true)
                .with_visible(false);
            glutin_winit::finalize_window(window_target, window_builder, &self.gl_config).unwrap()
        });
        let size = window.inner_size();

        let not_current_gl_context = self.create_context(window.raw_window_handle());

        let gl_window = GlWindow::new(window, &self.gl_config, not_current_gl_context);

        // The context needs to be current for the Renderer to set up shaders and
        // buffers. It also performs function loading, which needs a current context on
        // WGL.
        let renderer = SkiaGlRenderer::new(&self.gl_config, &self.gl_display, size);

        // Try setting vsync.
        if let Err(res) = gl_window.surface.set_swap_interval(
            gl_window.gl_context(),
            SwapInterval::Wait(NonZeroU32::new(1).unwrap()),
        ) {
            eprintln!("Error setting vsync: {:?}", res);
        }

        let window = Rc::new(SkiaGlAppWindow {
            renderer: RefCell::new(renderer),
            gl_window,
        });
        let id = window.gl_window.window.id();

        state.resize(size.width, size.height);

        window.gl_window.window.set_visible(true);
        state.open();

        self.windows.insert(id, (window.clone(), state));
        window
    }
    pub fn resize(&mut self, id: &WindowId, size: PhysicalSize<u32>) {
        if size.width != 0 && size.height != 0 {
            let (window, state) = self.windows.get_mut(id).unwrap();
            state.resize(size.width, size.height);
            window.resize(&self.gl_config, size);
            window.gl_window.window.request_redraw();
        }
    }
    pub fn draw(&self, id: &WindowId) {
        let (window, state) = self.windows.get(id).unwrap();
        window.draw(|canvas| state.draw(canvas));
    }
    pub fn cursor_enter(&mut self, id: &WindowId) {
        let (_window, state) = self.windows.get_mut(id).unwrap();
        state.cursor_enter();
    }
    pub fn cursor_leave(&mut self, id: &WindowId) {
        let (_window, state) = self.windows.get_mut(id).unwrap();
        state.cursor_leave();
    }
    pub fn cursor_move(&mut self, id: &WindowId, position: PhysicalPosition<f64>) {
        let (_window, state) = self.windows.get_mut(id).unwrap();
        state.cursor_move(position.x, position.y)
    }
    pub fn handle_window_event(
        &mut self,
        window_id: WindowId,
        event: WindowEvent,
        _window_target: &EventLoopWindowTarget<()>,
        control_flow: &mut ControlFlow,
    ) {
        match event {
            WindowEvent::Resized(size) => self.resize(&window_id, size),
            WindowEvent::CloseRequested => {
                if self.close_window(&window_id) {
                    control_flow.set_exit();
                }
            }
            WindowEvent::CursorEntered { .. } => self.cursor_enter(&window_id),
            WindowEvent::CursorLeft { .. } => self.cursor_leave(&window_id),
            WindowEvent::CursorMoved { position, .. } => self.cursor_move(&window_id, position),
            _ => (),
        }
    }
}

pub struct SkiaGlAppWindow {
    renderer: RefCell<SkiaGlRenderer>,
    gl_window: GlWindow,
}
impl SkiaGlAppWindow {
    fn resize(&self, gl_config: &Config, size: PhysicalSize<u32>) {
        self.gl_window.resize(
            NonZeroU32::new(size.width).unwrap(),
            NonZeroU32::new(size.height).unwrap(),
        );
        let mut renderer = self.renderer.borrow_mut();
        renderer.resize(&gl_config, size);
    }
    fn draw<F>(&self, f: F)
    where
        F: FnMut(&mut Canvas),
    {
        self.gl_window.make_current_if_needed();
        self.renderer.borrow_mut().draw(f);
        self.gl_window.swap_buffers();
    }
}
