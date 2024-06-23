use std::{collections::HashMap, fmt::Debug};

use provide_any::provide_any::{Demand, Provider};
use raw_window_handle::HasRawDisplayHandle;
use skia_safe::{Canvas, Surface};
use winit::{
    dpi::PhysicalSize,
    event_loop::EventLoopWindowTarget,
    window::{Window, WindowBuilder, WindowId},
};

#[cfg(windows)]
use crate::{
    d3d12::D3d12Backend, dcomp::DirectCompositionBackend,
    windows_ui_composition::WindowsUiCompositionBackend,
};
use crate::{opengl::OpenGlBackend, softbuffer::SoftBufferBackend};

#[derive(Default)]
pub struct WindowManager<Backend = DefaultBackend, State = ()> {
    env: Backend,
    windows: HashMap<WindowId, StatefulWindow<State>>,
}
impl<Backend: Provider + SkiaGraphicsBackend + 'static> WindowManager<Backend> {
    pub fn new<D: HasRawDisplayHandle>(display: &D) -> Result<Self, Backend::CreateError> {
        let b = Self::with_state::<D>(display);
        dbg!(b.is_ok());
        b
    }
    pub fn create<T>(
        &mut self,
        elwt: &EventLoopWindowTarget<T>,
        builder: WindowBuilder,
    ) -> Result<WindowId, Backend::CreateWindowError> {
        self.create_with_state(elwt, builder, ())
    }
    pub fn draw(&mut self, window_id: &WindowId, mut f: impl FnMut(&Canvas, &Window)) {
        self.draw_with_state(window_id, |canvas, window, _| f(canvas, window))
    }
}
impl<Backend: Provider + SkiaGraphicsBackend + 'static, State> WindowManager<Backend, State> {
    pub fn composited(&self) -> bool {
        self.env.composited()
    }
    pub fn with_state<D: HasRawDisplayHandle>(display: &D) -> Result<Self, Backend::CreateError> {
        Ok(Self {
            env: Backend::create::<D>(display)?,
            windows: HashMap::new(),
        })
    }
    pub fn create_with_state<T>(
        &mut self,
        elwt: &EventLoopWindowTarget<T>,
        builder: WindowBuilder,
        state: State,
    ) -> Result<WindowId, Backend::CreateWindowError> {
        self.create_with_state_fn(elwt, builder, |_| state)
    }
    pub fn create_with_state_fn<T>(
        &mut self,
        elwt: &EventLoopWindowTarget<T>,
        builder: WindowBuilder,
        state: impl FnOnce(&Window) -> State,
    ) -> Result<WindowId, Backend::CreateWindowError> {
        let visible = builder.window_attributes().visible;

        let render_window = self.env.create_window(elwt, builder.with_visible(false))?;
        let id = render_window.window.id();

        let state = state(&render_window.window);

        if visible {
            render_window.window.set_visible(true);
        }

        self.windows.insert(
            id,
            StatefulWindow {
                state,
                render_window,
            },
        );

        Ok(id)
    }
    pub fn get(&self, window_id: &WindowId) -> Option<&StatefulWindow<State>> {
        self.windows.get(window_id)
    }
    pub fn get_mut(&mut self, window_id: &WindowId) -> Option<&mut StatefulWindow<State>> {
        self.windows.get_mut(window_id)
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
    pub fn ids(&self) -> impl Iterator<Item = &WindowId> {
        self.windows.keys()
    }
    pub fn iter(&self) -> impl Iterator<Item = &StatefulWindow<State>> {
        self.windows.values()
    }
    pub fn iter_mut(&mut self) -> impl Iterator<Item = &mut StatefulWindow<State>> {
        self.windows.values_mut()
    }
    pub fn close(&mut self, id: &WindowId) -> bool {
        self.windows.remove(&id);
        dbg!("close");
        self.windows.is_empty()
    }
}

pub enum CreateWindowError {
    Winit(winit::error::OsError),
}

pub enum DefaultBackend {
    #[cfg(windows)]
    WindowsUiComposition(WindowsUiCompositionBackend),
    #[cfg(windows)]
    DirectComposition(DirectCompositionBackend),
    #[cfg(windows)]
    D3d12(D3d12Backend),
    OpenGl(OpenGlBackend),
    SoftBuffer(SoftBufferBackend),
}

#[derive(Debug)]
pub struct NoBackendsAvailable;
impl SkiaGraphicsBackend for DefaultBackend {
    type CreateError = NoBackendsAvailable;
    type CreateWindowError = DefaultBackendCreateWindowError;

    fn composited(&self) -> bool {
        match self {
            #[cfg(windows)]
            Self::WindowsUiComposition(backend) => backend.composited(),
            #[cfg(windows)]
            Self::DirectComposition(backend) => backend.composited(),
            #[cfg(windows)]
            Self::D3d12(backend) => backend.composited(),
            Self::OpenGl(backend) => backend.composited(),
            Self::SoftBuffer(backend) => backend.composited(),
        }
    }

    fn create<D: HasRawDisplayHandle>(display: &D) -> Result<Self, Self::CreateError> {
        #[cfg(windows)]
        {
            DirectCompositionBackend::create(display)
                .map(Self::DirectComposition)
                // WindowsUiCompositionBackend::create(display)
                //     .map(Self::WindowsUiComposition)
                //     .or_else(|err| {
                //         eprintln!("Windows.UI.Composition error: {err}.\nTrying DirectComposition.");
                //         DirectCompositionBackend::create(display).map(Self::DirectComposition)
                //     })
                .or_else(|err| {
                    eprintln!("DirectComposition error: {err}.\nTrying D3D12.");
                    D3d12Backend::create(display).map(Self::D3d12)
                })
                .or_else(|err| {
                    eprintln!("D3D12 error: {err}.\nTrying OpenGL.");
                    OpenGlBackend::create(display).map(Self::OpenGl)
                })
                .or_else(|err| {
                    eprintln!("OpenGL error {err}.\nTrying software rendering.");
                    SoftBufferBackend::create(display).map(Self::SoftBuffer)
                })
                .map_err(|err| {
                    eprintln!("Software rendering error: {err}");
                    NoBackendsAvailable
                })
        }

        #[cfg(unix)]
        {
            OpenGlBackend::create(display)
                .map(Self::OpenGl)
                .or_else(|err| {
                    eprintln!("OpenGL error {err}.\nTrying software rendering.");
                    SoftBufferBackend::create(display).map(Self::SoftBuffer)
                })
                .map_err(|err| {
                    eprintln!("Software rendering error: {err}");
                    NoBackendsAvailable
                })
        }
    }
    fn create_window<WinitUserEvent>(
        &mut self,
        elwt: &EventLoopWindowTarget<WinitUserEvent>,
        builder: WindowBuilder,
    ) -> Result<RenderWindow, Self::CreateWindowError> {
        match self {
            #[cfg(windows)]
            Self::WindowsUiComposition(backend) => backend
                .create_window(elwt, builder)
                .map_err(DefaultBackendCreateWindowError::WindowsUiComposition),
            #[cfg(windows)]
            Self::DirectComposition(backend) => backend
                .create_window(elwt, builder)
                .map_err(DefaultBackendCreateWindowError::DirectComposition),
            #[cfg(windows)]
            Self::D3d12(backend) => backend
                .create_window(elwt, builder)
                .map_err(DefaultBackendCreateWindowError::D3d12),
            Self::OpenGl(backend) => backend
                .create_window(elwt, builder)
                .map_err(DefaultBackendCreateWindowError::OpenGl),
            Self::SoftBuffer(backend) => backend
                .create_window(elwt, builder)
                .map_err(DefaultBackendCreateWindowError::SoftBuffer),
        }
    }
}
#[derive(Debug)]
pub enum DefaultBackendCreateWindowError {
    #[cfg(windows)]
    WindowsUiComposition(<WindowsUiCompositionBackend as SkiaGraphicsBackend>::CreateWindowError),
    #[cfg(windows)]
    DirectComposition(<DirectCompositionBackend as SkiaGraphicsBackend>::CreateWindowError),
    #[cfg(windows)]
    D3d12(<D3d12Backend as SkiaGraphicsBackend>::CreateWindowError),
    OpenGl(<OpenGlBackend as SkiaGraphicsBackend>::CreateWindowError),
    SoftBuffer(<SoftBufferBackend as SkiaGraphicsBackend>::CreateWindowError),
}
impl Provider for DefaultBackend {
    fn provide<'a>(&'a self, request: &mut Demand<'a>) {
        match self {
            #[cfg(windows)]
            Self::WindowsUiComposition(backend) => {
                request.provide_ref::<WindowsUiCompositionBackend>(backend)
            }
            #[cfg(windows)]
            Self::DirectComposition(backend) => {
                request.provide_ref::<DirectCompositionBackend>(backend)
            }
            #[cfg(windows)]
            Self::D3d12(backend) => request.provide_ref::<D3d12Backend>(backend),
            Self::OpenGl(backend) => request.provide_ref::<OpenGlBackend>(backend),
            Self::SoftBuffer(backend) => request.provide_ref::<SoftBufferBackend>(backend),
        };
    }
    fn provide_mut<'a>(&'a mut self, request: &mut Demand<'a>) {
        match self {
            #[cfg(windows)]
            Self::WindowsUiComposition(backend) => {
                request.provide_mut::<WindowsUiCompositionBackend>(backend)
            }
            #[cfg(windows)]
            Self::DirectComposition(backend) => {
                request.provide_mut::<DirectCompositionBackend>(backend)
            }
            #[cfg(windows)]
            Self::D3d12(backend) => request.provide_mut::<D3d12Backend>(backend),
            Self::OpenGl(backend) => request.provide_mut::<OpenGlBackend>(backend),
            Self::SoftBuffer(backend) => request.provide_mut::<SoftBufferBackend>(backend),
        };
    }
}

#[derive(Default)]
pub struct AnyBackend {
    softbuffer: Option<SoftBufferBackend>,
    opengl: Option<OpenGlBackend>,
    #[cfg(windows)]
    windows_ui_composition: Option<WindowsUiCompositionBackend>,
    #[cfg(windows)]
    direct_composition: Option<DirectCompositionBackend>,
    #[cfg(windows)]
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
        #[cfg(windows)]
        if let Some(env) = &self.windows_ui_composition {
            request.provide_ref::<WindowsUiCompositionBackend>(env);
        }
        #[cfg(windows)]
        if let Some(env) = &self.direct_composition {
            request.provide_ref::<DirectCompositionBackend>(env);
        }
        #[cfg(windows)]
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
        #[cfg(windows)]
        if let Some(env) = &mut self.windows_ui_composition {
            request.provide_mut::<WindowsUiCompositionBackend>(env);
        }
        #[cfg(windows)]
        if let Some(env) = &mut self.direct_composition {
            request.provide_mut::<DirectCompositionBackend>(env);
        }
        #[cfg(windows)]
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
#[cfg(windows)]
impl Provider for WindowsUiCompositionBackend {
    fn provide<'a>(&'a self, req: &mut Demand<'a>) {
        req.provide_ref::<Self>(self);
    }
    fn provide_mut<'a>(&'a mut self, req: &mut Demand<'a>) {
        req.provide_mut::<Self>(self);
    }
}
#[cfg(windows)]
impl Provider for DirectCompositionBackend {
    fn provide<'a>(&'a self, req: &mut Demand<'a>) {
        req.provide_ref::<Self>(self);
    }
    fn provide_mut<'a>(&'a mut self, req: &mut Demand<'a>) {
        req.provide_mut::<Self>(self);
    }
}
#[cfg(windows)]
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
#[cfg(windows)]
impl InitEnv for D3d12Backend {
    fn env(env: &mut AnyBackend) -> &mut Option<Self> {
        &mut env.d3d12
    }
}
#[cfg(windows)]
impl InitEnv for WindowsUiCompositionBackend {
    fn env(env: &mut AnyBackend) -> &mut Option<Self> {
        &mut env.windows_ui_composition
    }
}

pub trait SkiaGraphicsBackend: Provider + Sized {
    type CreateError: Debug;
    type CreateWindowError: Debug;

    fn composited(&self) -> bool {
        false
    }

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
impl<State> StatefulWindow<State> {
    pub fn state(&self) -> &State {
        &self.state
    }
    pub fn state_mut(&mut self) -> &mut State {
        &mut self.state
    }
    pub fn winit_window(&self) -> &Window {
        &self.render_window.window
    }
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
    fn draw(&mut self, env: &mut dyn Provider, mut f: impl FnMut(&Canvas, &Window)) {
        let surface = self.render.prepare_and_get_surface(env);
        let canvas = surface.canvas();

        f(canvas, &self.window);

        self.render.present(env);
    }
}
impl Drop for RenderWindow {
    fn drop(&mut self) {
        self.window.set_visible(false);
    }
}

pub trait SkiaRender {
    fn prepare_and_get_surface(&mut self, env: &mut dyn Provider) -> &mut Surface;
    fn present(&mut self, env: &mut dyn Provider);
    fn resize(&mut self, env: &mut dyn Provider, size: PhysicalSize<u32>, window: &Window);
}
