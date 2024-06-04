use std::{collections::HashMap, fmt::Debug};

use provide_any::provide_any::{request_mut, request_ref, Demand, Provider};
use raw_window_handle::HasRawDisplayHandle;
use skia_safe::{Canvas, Surface};
use winit::{
    dpi::PhysicalSize,
    event_loop::EventLoopWindowTarget,
    window::{Window, WindowBuilder, WindowId},
};

use crate::{
    d3d12::D3d12Backend, opengl::OpenGlBackend, softbuffer::SoftBufferBackend,
    windows_ui_composition::WindowsUiCompositionBackend,
};

#[derive(Default)]
pub struct WindowManager<State = ()> {
    env: AnyBackend,
    windows: HashMap<WindowId, StatefulWindow<State>>,
}
impl WindowManager {
    pub fn new() -> Self {
        Self {
            env: AnyBackend::default(),
            windows: HashMap::new(),
        }
    }
    pub fn create<Env, T>(
        &mut self,
        elwt: &EventLoopWindowTarget<T>,
        builder: WindowBuilder,
    ) -> Result<WindowId, BackendError<Env>>
    where
        Env: SkiaGraphicsBackend + InitEnv + 'static,
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
            env: AnyBackend::default(),
            windows: HashMap::new(),
        }
    }
    pub fn create_with_state<Env, T>(
        &mut self,
        elwt: &EventLoopWindowTarget<T>,
        builder: WindowBuilder,
        state: State,
    ) -> Result<WindowId, BackendError<Env>>
    where
        Env: SkiaGraphicsBackend + InitEnv + 'static,
    {
        self.create_env_if_absent::<Env, _>(elwt)
            .map_err(BackendError::Create)?;

        let env = request_mut::<Env>(&mut self.env).unwrap();

        let render_window = env
            .create_window(elwt, builder)
            .map_err(BackendError::CreateWindow)?;
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
    fn create_env_if_absent<Env, T>(
        &mut self,
        elwt: &EventLoopWindowTarget<T>,
    ) -> Result<(), Env::CreateError>
    where
        Env: SkiaGraphicsBackend + InitEnv + 'static,
    {
        let exists = request_ref::<Env>(&self.env).is_some();
        if !exists {
            let env = Env::env(&mut self.env);
            *env = Some(Env::create(elwt)?);
        }
        Ok(())
    }
}

pub enum CreateWindowError {
    Winit(winit::error::OsError),
}

#[derive(Default)]
pub struct AnyBackend {
    softbuffer: Option<SoftBufferBackend>,
    opengl: Option<OpenGlBackend>,
    windows_ui_composition: Option<WindowsUiCompositionBackend>,
    d3d12: Option<D3d12Backend>,
}
impl Provider for AnyBackend {
    fn provide<'a>(&'a self, request: &mut Demand<'a>) {
        if let Some(env) = &self.softbuffer {
            request.provide_ref::<SoftBufferBackend>(env);
        }
        if let Some(env) = &self.opengl {
            request.provide_ref::<OpenGlBackend>(env);
        }
        if let Some(env) = &self.windows_ui_composition {
            request.provide_ref::<WindowsUiCompositionBackend>(env);
        }
        if let Some(env) = &self.d3d12 {
            request.provide_ref::<D3d12Backend>(env);
        }
    }
    fn provide_mut<'a>(&'a mut self, request: &mut Demand<'a>) {
        if let Some(env) = &mut self.softbuffer {
            request.provide_mut::<SoftBufferBackend>(env);
        }
        if let Some(env) = &mut self.opengl {
            request.provide_mut::<OpenGlBackend>(env);
        }
        if let Some(env) = &mut self.windows_ui_composition {
            request.provide_mut::<WindowsUiCompositionBackend>(env);
        }
        if let Some(env) = &mut self.d3d12 {
            request.provide_mut::<D3d12Backend>(env);
        }
    }
}
impl Provider for SoftBufferBackend {
    fn provide<'a>(&'a self, req: &mut Demand<'a>) {
        req.provide_ref::<Self>(self);
    }
    fn provide_mut<'a>(&'a mut self, req: &mut Demand<'a>) {
        req.provide_mut::<Self>(self);
    }
}
impl Provider for OpenGlBackend {
    fn provide<'a>(&'a self, req: &mut Demand<'a>) {
        req.provide_ref::<Self>(self);
    }
    fn provide_mut<'a>(&'a mut self, req: &mut Demand<'a>) {
        req.provide_mut::<Self>(self);
    }
}
impl Provider for WindowsUiCompositionBackend {
    fn provide<'a>(&'a self, req: &mut Demand<'a>) {
        req.provide_ref::<Self>(self);
    }
    fn provide_mut<'a>(&'a mut self, req: &mut Demand<'a>) {
        req.provide_mut::<Self>(self);
    }
}
impl Provider for D3d12Backend {
    fn provide<'a>(&'a self, req: &mut Demand<'a>) {
        req.provide_ref::<Self>(self);
    }
    fn provide_mut<'a>(&'a mut self, req: &mut Demand<'a>) {
        req.provide_mut::<Self>(self);
    }
}

pub trait InitEnv: SkiaGraphicsBackend + Sized {
    fn env(env: &mut AnyBackend) -> &mut Option<Self>;
}
impl InitEnv for SoftBufferBackend {
    fn env(env: &mut AnyBackend) -> &mut Option<Self> {
        &mut env.softbuffer
    }
}
impl InitEnv for OpenGlBackend {
    fn env(env: &mut AnyBackend) -> &mut Option<Self> {
        &mut env.opengl
    }
}
impl InitEnv for D3d12Backend {
    fn env(env: &mut AnyBackend) -> &mut Option<Self> {
        &mut env.d3d12
    }
}
impl InitEnv for WindowsUiCompositionBackend {
    fn env(env: &mut AnyBackend) -> &mut Option<Self> {
        &mut env.windows_ui_composition
    }
}

pub trait SkiaGraphicsBackend: Sized {
    type CreateError: Debug;
    type CreateWindowError: Debug;

    fn create<D: HasRawDisplayHandle>(display: &D) -> Result<Self, Self::CreateError>;
    fn create_window<WinitUserEvent>(
        &mut self,
        elwt: &EventLoopWindowTarget<WinitUserEvent>,
        builder: WindowBuilder,
    ) -> Result<RenderWindow, Self::CreateWindowError>;
}

pub enum BackendError<B: SkiaGraphicsBackend> {
    Create(B::CreateError),
    CreateWindow(B::CreateWindowError),
}
impl<B: SkiaGraphicsBackend> Debug for BackendError<B> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Create(arg0) => f.debug_tuple("Create").field(arg0).finish(),
            Self::CreateWindow(arg0) => f.debug_tuple("CreateWindow").field(arg0).finish(),
        }
    }
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
    fn draw(&mut self, env: &mut AnyBackend, mut f: impl FnMut(&Canvas, &Window)) {
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
