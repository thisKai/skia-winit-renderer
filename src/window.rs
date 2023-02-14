use crate::{
    gl::Gl,
    skia::{SkiaGlRenderer, SkiaSoftwareRenderer},
};
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
use std::{collections::HashMap, ffi::CString, num::NonZeroU32};
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

pub trait SkiaWinitWindow {
    fn winit_window(&self) -> &WinitWindow;

    type WindowManagerState;
    fn create(
        window_manager_state: &mut Self::WindowManagerState,
        window_target: &EventLoopWindowTarget<()>,
    ) -> Self;

    fn resize(
        &mut self,
        window_manager_state: &mut Self::WindowManagerState,
        size: PhysicalSize<u32>,
    );

    fn draw<F>(&mut self, f: F)
    where
        F: FnMut(&mut Canvas);
}

pub type SkiaWinitWindowManager<W> =
    WinitWindowManager<W, <W as SkiaWinitWindow>::WindowManagerState>;

pub struct WinitWindowManager<W, S = ()> {
    state: S,
    windows: HashMap<WindowId, (W, Box<dyn Window>)>,
}
impl<W, S> WinitWindowManager<W, S> {
    pub fn new(state: S) -> Self {
        Self {
            windows: HashMap::new(),
            state,
        }
    }
    pub fn close_window(&mut self, id: &WindowId) -> bool {
        self.windows.remove(&id);
        dbg!("close");
        self.windows.is_empty()
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
}
impl<W: SkiaWinitWindow> WinitWindowManager<W, W::WindowManagerState> {
    pub fn draw(&mut self, id: &WindowId) {
        let (window, state) = self.windows.get_mut(id).unwrap();
        window.draw(|canvas| state.draw(canvas));
    }
    pub fn create_window(
        &mut self,
        window_target: &EventLoopWindowTarget<()>,
        mut state: Box<dyn Window>,
    ) -> WindowId {
        let window = W::create(&mut self.state, window_target);
        let id = window.winit_window().id();

        let size = window.winit_window().inner_size();

        state.resize(size.width, size.height);

        window.winit_window().set_visible(true);
        state.open();

        self.windows.insert(id, (window, state));
        id
    }
    pub fn resize(&mut self, id: &WindowId, size: PhysicalSize<u32>) {
        if size.width != 0 && size.height != 0 {
            let (window, state) = self.windows.get_mut(id).unwrap();
            state.resize(size.width, size.height);
            window.resize(&mut self.state, size);
            window.winit_window().request_redraw();
        }
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

pub struct SkiaSoftwareRenderedWinitWindow {
    renderer: SkiaSoftwareRenderer,
    window: WinitWindow,
}

impl SkiaWinitWindow for SkiaSoftwareRenderedWinitWindow {
    fn winit_window(&self) -> &WinitWindow {
        &self.window
    }

    type WindowManagerState = ();

    fn create(_: &mut Self::WindowManagerState, window_target: &EventLoopWindowTarget<()>) -> Self {
        let window = WindowBuilder::new()
            .with_transparent(true)
            .with_visible(false)
            .build(window_target)
            .unwrap();

        let gc = unsafe { GraphicsContext::new(&window, window_target).unwrap() };
        let renderer = SkiaSoftwareRenderer::new(gc, window.inner_size());

        Self { renderer, window }
    }

    fn resize(&mut self, _: &mut Self::WindowManagerState, size: PhysicalSize<u32>) {
        self.renderer.resize(size);
    }

    fn draw<F>(&mut self, f: F)
    where
        F: FnMut(&mut Canvas),
    {
        self.renderer.draw(f);
    }
}

pub struct GlWinitWindow {
    gl_context: Option<PossiblyCurrentContext>,
    // XXX the surface must be dropped before the window.
    pub surface: Surface<WindowSurface>,
    pub window: WinitWindow,
}

impl GlWinitWindow {
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
    pub fn make_not_current(&mut self) -> NotCurrentContext {
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
impl Drop for GlWinitWindow {
    fn drop(&mut self) {
        self.make_not_current();
    }
}

pub struct SkiaGlWinitWindow {
    renderer: SkiaGlRenderer,
    gl_window: GlWinitWindow,
}

impl SkiaWinitWindow for SkiaGlWinitWindow {
    fn winit_window(&self) -> &WinitWindow {
        &self.gl_window.window
    }

    type WindowManagerState = GlWindowManagerState;

    fn create(
        window_manager_state: &mut Self::WindowManagerState,
        window_target: &EventLoopWindowTarget<()>,
    ) -> Self {
        #[cfg(target_os = "android")]
        println!("Android window available");

        let window = window_manager_state.first_window.take().unwrap_or_else(|| {
            let window_builder = WindowBuilder::new()
                .with_transparent(true)
                .with_visible(false);
            glutin_winit::finalize_window(
                window_target,
                window_builder,
                &window_manager_state.gl_config,
            )
            .unwrap()
        });
        let size = window.inner_size();

        let not_current_gl_context =
            window_manager_state.create_context(window.raw_window_handle());

        let gl_window = GlWinitWindow::new(
            window,
            &window_manager_state.gl_config,
            not_current_gl_context,
        );

        // The context needs to be current for the Renderer to set up shaders and
        // buffers. It also performs function loading, which needs a current context on
        // WGL.
        let renderer = SkiaGlRenderer::new(
            &window_manager_state.gl,
            &window_manager_state.gl_config,
            size,
        );

        // Try setting vsync.
        if let Err(res) = gl_window.surface.set_swap_interval(
            gl_window.gl_context(),
            SwapInterval::Wait(NonZeroU32::new(1).unwrap()),
        ) {
            eprintln!("Error setting vsync: {:?}", res);
        }

        SkiaGlWinitWindow {
            renderer,
            gl_window,
        }
    }

    fn resize(
        &mut self,
        window_manager_state: &mut Self::WindowManagerState,
        size: PhysicalSize<u32>,
    ) {
        self.gl_window.resize(
            NonZeroU32::new(size.width).unwrap(),
            NonZeroU32::new(size.height).unwrap(),
        );
        window_manager_state.resize_viewport(
            size.width.try_into().unwrap(),
            size.height.try_into().unwrap(),
        );
        self.renderer.resize(&window_manager_state.gl_config, size);
    }

    fn draw<F>(&mut self, f: F)
    where
        F: FnMut(&mut Canvas),
    {
        self.gl_window.make_current_if_needed();
        self.renderer.draw(f);
        self.gl_window.swap_buffers();
    }
}

pub struct GlWindowManagerState {
    gl_config: Config,
    gl_display: Display,
    gl: Gl,
    first_window: Option<WinitWindow>,
}
impl GlWindowManagerState {
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

        let gl = Gl::load_with(|symbol| {
            let symbol = CString::new(symbol).unwrap();
            gl_display.get_proc_address(symbol.as_c_str()).cast()
        });

        Self {
            gl_config,
            gl_display,
            gl,
            first_window,
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
    fn resize_viewport(&self, width: i32, height: i32) {
        unsafe {
            self.gl.Viewport(0, 0, width, height);
        }
    }
}
