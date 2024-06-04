use std::collections::HashMap;

use provide_any::provide_any::{request_mut, request_ref, Demand, Provider};
use raw_window_handle::HasRawDisplayHandle;
use skia_safe::{Canvas, Surface};
use winit::{
    dpi::PhysicalSize,
    event_loop::EventLoopWindowTarget,
    window::{Window, WindowBuilder, WindowId},
};

use crate::{
    d3d12::D3d12Env, opengl::OpenGlEnv, softbuffer::SoftBufferEnv,
    windows_ui_composition::WindowsUiCompositionEnv,
};

#[derive(Default)]
pub struct WindowManager<State = ()> {
    env: Env,
    windows: HashMap<WindowId, StatefulWindow<State>>,
}
impl WindowManager {
    pub fn new() -> Self {
        Self {
            env: Env::default(),
            windows: HashMap::new(),
        }
    }
    pub fn create<Env, T>(
        &mut self,
        elwt: &EventLoopWindowTarget<T>,
        builder: WindowBuilder,
    ) -> Result<WindowId, Env::Error>
    where
        Env: SkiaGraphicsEnv + InitEnv + 'static,
    {
        self.create_with_state::<Env, _>(elwt, builder, ())
    }
    pub fn draw(&mut self, window_id: &WindowId, mut f: impl FnMut(&Canvas, &Window)) {
        self.draw_with_state(window_id, |canvas, window, _| f(canvas, window))
    }
}
impl<State> WindowManager<State> {
    pub fn with_state() -> Self {
        Self {
            env: Env::default(),
            windows: HashMap::new(),
        }
    }
    pub fn create_with_state<Env, T>(
        &mut self,
        elwt: &EventLoopWindowTarget<T>,
        builder: WindowBuilder,
        state: State,
    ) -> Result<WindowId, Env::Error>
    where
        Env: SkiaGraphicsEnv + InitEnv + 'static,
    {
        self.create_env_if_absent::<Env, _>(elwt);

        let env = request_mut::<Env>(&mut self.env).unwrap();

        let render_window = env.create_window(elwt, builder)?;
        let id = render_window.window.id();

        self.windows.insert(
            id,
            StatefulWindow {
                state,
                render_window,
            },
        );

        Ok(id)
    }
    pub fn remove(&mut self, window_id: &WindowId) {
        self.windows.remove(window_id);
    }
    pub fn draw_with_state(
        &mut self,
        window_id: &WindowId,
        mut f: impl FnMut(&Canvas, &Window, &State),
    ) {
        let window = self.windows.get_mut(window_id).unwrap();
        window
            .render_window
            .draw(&mut self.env, |canvas, winit_window| {
                f(canvas, winit_window, &window.state)
            });
    }
    pub fn resize(&mut self, window_id: &WindowId, size: PhysicalSize<u32>) {
        let window = self.windows.get_mut(window_id).unwrap();

        window
            .render_window
            .render
            .resize(&mut self.env, size, &window.render_window.window);
    }
    fn create_env_if_absent<Env, T>(&mut self, elwt: &EventLoopWindowTarget<T>)
    where
        Env: SkiaGraphicsEnv + InitEnv + 'static,
    {
        let exists = request_ref::<Env>(&self.env).is_some();
        if !exists {
            let env = Env::env(&mut self.env);
            *env = Some(Env::create(elwt));
        }
    }
}

pub enum CreateWindowError {
    Winit(winit::error::OsError),
}

#[derive(Default)]
pub struct Env {
    softbuffer: Option<SoftBufferEnv>,
    opengl: Option<OpenGlEnv>,
    windows_ui_composition: Option<WindowsUiCompositionEnv>,
    d3d12: Option<D3d12Env>,
}
impl Provider for Env {
    fn provide<'a>(&'a self, request: &mut Demand<'a>) {
        if let Some(env) = &self.softbuffer {
            request.provide_ref::<SoftBufferEnv>(env);
        }
        if let Some(env) = &self.opengl {
            request.provide_ref::<OpenGlEnv>(env);
        }
        if let Some(env) = &self.windows_ui_composition {
            request.provide_ref::<WindowsUiCompositionEnv>(env);
        }
        if let Some(env) = &self.d3d12 {
            request.provide_ref::<D3d12Env>(env);
        }
    }
    fn provide_mut<'a>(&'a mut self, request: &mut Demand<'a>) {
        if let Some(env) = &mut self.softbuffer {
            request.provide_mut::<SoftBufferEnv>(env);
        }
        if let Some(env) = &mut self.opengl {
            request.provide_mut::<OpenGlEnv>(env);
        }
        if let Some(env) = &mut self.windows_ui_composition {
            request.provide_mut::<WindowsUiCompositionEnv>(env);
        }
        if let Some(env) = &mut self.d3d12 {
            request.provide_mut::<D3d12Env>(env);
        }
    }
}
impl Provider for SoftBufferEnv {
    fn provide<'a>(&'a self, req: &mut Demand<'a>) {
        req.provide_ref::<Self>(self);
    }
    fn provide_mut<'a>(&'a mut self, req: &mut Demand<'a>) {
        req.provide_mut::<Self>(self);
    }
}
impl Provider for OpenGlEnv {
    fn provide<'a>(&'a self, req: &mut Demand<'a>) {
        req.provide_ref::<Self>(self);
    }
    fn provide_mut<'a>(&'a mut self, req: &mut Demand<'a>) {
        req.provide_mut::<Self>(self);
    }
}
impl Provider for WindowsUiCompositionEnv {
    fn provide<'a>(&'a self, req: &mut Demand<'a>) {
        req.provide_ref::<Self>(self);
    }
    fn provide_mut<'a>(&'a mut self, req: &mut Demand<'a>) {
        req.provide_mut::<Self>(self);
    }
}
impl Provider for D3d12Env {
    fn provide<'a>(&'a self, req: &mut Demand<'a>) {
        req.provide_ref::<Self>(self);
    }
    fn provide_mut<'a>(&'a mut self, req: &mut Demand<'a>) {
        req.provide_mut::<Self>(self);
    }
}

pub trait InitEnv: SkiaGraphicsEnv + Sized {
    fn env(env: &mut Env) -> &mut Option<Self>;
    fn init_env<D: HasRawDisplayHandle>(env: &mut Env, display: &D) {
        let env = Self::env(env);
        *env = Some(Self::create(display));
    }
}
impl InitEnv for SoftBufferEnv {
    fn env(env: &mut Env) -> &mut Option<Self> {
        &mut env.softbuffer
    }
}
impl InitEnv for OpenGlEnv {
    fn env(env: &mut Env) -> &mut Option<Self> {
        &mut env.opengl
    }
}
impl InitEnv for D3d12Env {
    fn env(env: &mut Env) -> &mut Option<Self> {
        &mut env.d3d12
    }
}
impl InitEnv for WindowsUiCompositionEnv {
    fn env(env: &mut Env) -> &mut Option<Self> {
        &mut env.windows_ui_composition
    }
}

pub trait SkiaGraphicsEnv {
    type Error;

    fn create<D: HasRawDisplayHandle>(display: &D) -> Self;
    fn create_window<WinitUserEvent>(
        &mut self,
        elwt: &EventLoopWindowTarget<WinitUserEvent>,
        builder: WindowBuilder,
    ) -> Result<RenderWindow, Self::Error>;
}

pub struct StatefulWindow<State = ()> {
    state: State,
    render_window: RenderWindow,
}

pub struct RenderWindow {
    render: Box<dyn SkiaRender>,
    window: Window,
}
impl RenderWindow {
    pub(crate) fn new<R: SkiaRender + 'static>(render: R, window: Window) -> Self {
        Self {
            render: Box::new(render),
            window,
        }
    }
    fn draw(&mut self, env: &mut Env, mut f: impl FnMut(&Canvas, &Window)) {
        let surface = self.render.prepare_and_get_surface(env);
        let canvas = surface.canvas();

        f(canvas, &self.window);

        self.render.present(env);
    }
}

pub trait SkiaRender {
    fn prepare_and_get_surface(&mut self, env: &mut dyn Provider) -> &mut Surface;
    fn present(&mut self, env: &mut dyn Provider);
    fn resize(&mut self, env: &mut dyn Provider, size: PhysicalSize<u32>, window: &Window);
}
