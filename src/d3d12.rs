use std::collections::HashMap;

use provide_any::provide_any::{request_mut, Provider};
use raw_window_handle::{HasRawWindowHandle, RawWindowHandle};
use skia_safe::{
    gpu::{
        d3d::{BackendContext, TextureResourceInfo},
        surfaces, BackendRenderTarget, DirectContext, Protected, SurfaceOrigin,
    },
    Canvas, ColorType, Surface,
};
use windows::{
    core::Interface,
    Win32::{
        Foundation::HWND,
        Graphics::{
            Direct3D::D3D_FEATURE_LEVEL_11_0,
            Direct3D12::{
                D3D12CreateDevice, ID3D12CommandQueue, ID3D12Device, D3D12_RESOURCE_STATE_COMMON,
            },
            Dxgi::{
                Common::{
                    DXGI_ALPHA_MODE_PREMULTIPLIED, DXGI_FORMAT_R8G8B8A8_UNORM, DXGI_FORMAT_UNKNOWN,
                    DXGI_SAMPLE_DESC, DXGI_STANDARD_MULTISAMPLE_QUALITY_PATTERN,
                },
                CreateDXGIFactory1, IDXGIAdapter1, IDXGIFactory4, IDXGISwapChain3,
                DXGI_ADAPTER_FLAG, DXGI_ADAPTER_FLAG_NONE, DXGI_ADAPTER_FLAG_SOFTWARE,
                DXGI_SWAP_CHAIN_DESC1, DXGI_SWAP_EFFECT_FLIP_SEQUENTIAL,
                DXGI_USAGE_RENDER_TARGET_OUTPUT,
            },
        },
    },
};
use winit::{
    dpi::PhysicalSize,
    error::OsError,
    event_loop::EventLoopWindowTarget,
    window::{Window, WindowBuilder, WindowId},
};

use crate::generic::{RenderWindow, SkiaGraphicsBackend, SkiaRender};

pub struct D3d12WindowManager<State = ()> {
    env: D3d12Backend,
    windows: HashMap<WindowId, SkiaD3d12Window<State>>,
}
impl D3d12WindowManager {
    pub fn new() -> windows::core::Result<Self> {
        Ok(Self {
            env: D3d12Backend::new()?,
            windows: HashMap::new(),
        })
    }
    pub fn create_window<T>(
        &mut self,
        elwt: &EventLoopWindowTarget<T>,
        builder: WindowBuilder,
    ) -> Result<WindowId, CreateD3d12WindowError> {
        self.create_window_with_state(elwt, builder, ())
    }
    pub fn draw(
        &mut self,
        window_id: &WindowId,
        mut f: impl FnMut(&Canvas, &Window),
    ) -> windows::core::Result<()> {
        self.draw_with_state(window_id, |canvas, window, _| f(canvas, window))
    }
}
impl<State> D3d12WindowManager<State> {
    pub fn with_state() -> windows::core::Result<Self> {
        Ok(Self {
            env: D3d12Backend::new()?,
            windows: HashMap::new(),
        })
    }
    pub fn create_window_with_state<T>(
        &mut self,
        elwt: &EventLoopWindowTarget<T>,
        builder: WindowBuilder,
        state: State,
    ) -> Result<WindowId, CreateD3d12WindowError> {
        let window = self.env.create_window_with_state(elwt, builder, state)?;
        let window_id = window.winit_window.id();

        self.windows.insert(window_id, window);

        Ok(window_id)
    }
    pub fn get_window(&self, window_id: &WindowId) -> Option<&SkiaD3d12Window<State>> {
        self.windows.get(window_id)
    }
    pub fn get_window_mut(&mut self, window_id: &WindowId) -> Option<&mut SkiaD3d12Window<State>> {
        self.windows.get_mut(window_id)
    }
    pub fn remove_window(&mut self, window_id: &WindowId) -> Option<SkiaD3d12Window<State>> {
        self.windows.remove(window_id)
    }
    pub fn draw_with_state(
        &mut self,
        window_id: &WindowId,
        f: impl FnMut(&Canvas, &Window, &State),
    ) -> windows::core::Result<()> {
        let Some(window) = self.windows.get_mut(window_id) else {
            return Err(windows::core::Error::empty());
        };
        window.draw(&mut self.env, f)
    }
    pub fn resize_window(
        &mut self,
        window_id: &WindowId,
        size: PhysicalSize<u32>,
    ) -> windows::core::Result<()> {
        let window: &mut SkiaD3d12Window<State> = self
            .windows
            .get_mut(window_id)
            .ok_or(windows::core::Error::empty())?;

        self.env.cleanup();

        window
            .swap_chain
            .resize(&mut self.env, size.width, size.height)?;

        window.winit_window.request_redraw();
        Ok(())
    }
}

pub struct D3d12Backend {
    factory: IDXGIFactory4,
    backend_context: BackendContext,
    direct_context: DirectContext,
}
impl D3d12Backend {
    pub(crate) fn new() -> windows::core::Result<Self> {
        let factory: IDXGIFactory4 = unsafe { CreateDXGIFactory1() }?;
        let (adapter, device) = get_hardware_adapter_and_device(&factory)?;
        let queue: ID3D12CommandQueue = unsafe { device.CreateCommandQueue(&Default::default()) }?;

        let backend_context = BackendContext {
            adapter,
            device,
            queue,
            memory_allocator: None,
            protected_context: Protected::No,
        };
        let direct_context = unsafe { DirectContext::new_d3d(&backend_context, None) }.unwrap();

        Ok(Self {
            factory,
            backend_context,
            direct_context,
        })
    }

    pub fn create_window_with_state<UserEvent, State>(
        &mut self,
        elwt: &EventLoopWindowTarget<UserEvent>,
        builder: WindowBuilder,
        state: State,
    ) -> Result<SkiaD3d12Window<State>, CreateD3d12WindowError> {
        let window = builder
            .build(elwt)
            .map_err(CreateD3d12WindowError::BuildWindow)?;

        let swap_chain = self
            .create_window_swap_chain(&window)
            .map_err(CreateD3d12WindowError::CreateSurface)?;

        Ok(SkiaD3d12Window {
            winit_window: window,
            swap_chain,
            state,
        })
    }
    fn create_window_swap_chain(
        &mut self,
        window: &Window,
    ) -> windows::core::Result<SkiaD3d12SwapChain> {
        let size = window.inner_size();

        let hwnd = match window.raw_window_handle() {
            RawWindowHandle::Win32(window_handle) => HWND(window_handle.hwnd as _),
            _ => panic!("not win32"),
        };
        self.create_hwnd_swap_chain(hwnd, size.width, size.height)
    }
    fn create_hwnd_swap_chain(
        &mut self,
        hwnd: HWND,
        width: u32,
        height: u32,
    ) -> windows::core::Result<SkiaD3d12SwapChain> {
        let swap_chain: IDXGISwapChain3 = unsafe {
            self.factory.CreateSwapChainForHwnd(
                &self.backend_context.queue,
                hwnd,
                &DXGI_SWAP_CHAIN_DESC1 {
                    Width: width,
                    Height: height,
                    Format: DXGI_FORMAT_R8G8B8A8_UNORM,
                    BufferUsage: DXGI_USAGE_RENDER_TARGET_OUTPUT,
                    BufferCount: BUFFER_COUNT,
                    SwapEffect: DXGI_SWAP_EFFECT_FLIP_SEQUENTIAL,
                    SampleDesc: DXGI_SAMPLE_DESC {
                        Count: 1,
                        Quality: 0,
                    },
                    ..Default::default()
                },
                None,
                None,
            )
        }?
        .cast()?;

        let surfaces = self.create_swap_chain_surfaces(&swap_chain, width, height);

        Ok(SkiaD3d12SwapChain::new(swap_chain, surfaces))
    }
    pub(crate) fn create_composition_swap_chain(
        &mut self,
        width: u32,
        height: u32,
    ) -> windows::core::Result<SkiaD3d12SwapChain> {
        let swap_chain: IDXGISwapChain3 = unsafe {
            self.factory.CreateSwapChainForComposition(
                &self.backend_context.queue,
                &DXGI_SWAP_CHAIN_DESC1 {
                    Width: width,
                    Height: height,
                    Format: DXGI_FORMAT_R8G8B8A8_UNORM,
                    BufferUsage: DXGI_USAGE_RENDER_TARGET_OUTPUT,
                    BufferCount: BUFFER_COUNT,
                    SwapEffect: DXGI_SWAP_EFFECT_FLIP_SEQUENTIAL,
                    SampleDesc: DXGI_SAMPLE_DESC {
                        Count: 1,
                        Quality: 0,
                    },
                    AlphaMode: DXGI_ALPHA_MODE_PREMULTIPLIED,
                    ..Default::default()
                },
                None,
            )
        }?
        .cast()?;

        let surfaces = self.create_swap_chain_surfaces(&swap_chain, width, height);

        Ok(SkiaD3d12SwapChain::new(swap_chain, surfaces))
    }
    fn create_swap_chain_surfaces(
        &mut self,
        swap_chain: &IDXGISwapChain3,
        width: u32,
        height: u32,
    ) -> SkiaD3d12SwapChainSurfaceArray {
        std::array::from_fn(|i| {
            let resource = unsafe { swap_chain.GetBuffer(i as u32).unwrap() };

            let backend_render_target = BackendRenderTarget::new_d3d(
                (width.try_into().unwrap(), height.try_into().unwrap()),
                &TextureResourceInfo {
                    resource,
                    alloc: None,
                    resource_state: D3D12_RESOURCE_STATE_COMMON,
                    format: DXGI_FORMAT_R8G8B8A8_UNORM,
                    sample_count: 1,
                    level_count: 0,
                    sample_quality_pattern: DXGI_STANDARD_MULTISAMPLE_QUALITY_PATTERN,
                    protected: Protected::No,
                },
            );

            let surface = surfaces::wrap_backend_render_target(
                &mut self.direct_context,
                &backend_render_target,
                SurfaceOrigin::TopLeft,
                ColorType::RGBA8888,
                None,
                None,
            )
            .unwrap();

            (surface, backend_render_target)
        })
    }
    pub(crate) fn cleanup(&mut self) {
        self.direct_context
            .perform_deferred_cleanup(Default::default(), None);
    }
}
impl SkiaGraphicsBackend for D3d12Backend {
    type CreateError = windows::core::Error;
    type CreateWindowError = CreateD3d12WindowError;

    fn create<D: raw_window_handle::HasRawDisplayHandle>(_: &D) -> Result<Self, Self::CreateError> {
        Self::new()
    }
    fn create_window<WinitUserEvent>(
        &mut self,
        elwt: &EventLoopWindowTarget<WinitUserEvent>,
        builder: WindowBuilder,
    ) -> Result<crate::generic::RenderWindow, Self::CreateWindowError> {
        let window = builder
            .build(elwt)
            .map_err(CreateD3d12WindowError::BuildWindow)?;

        let render = self
            .create_window_swap_chain(&window)
            .map_err(CreateD3d12WindowError::CreateSurface)?;

        Ok(RenderWindow::new(render, window))
    }
}

#[derive(Debug)]
pub enum CreateD3d12WindowError {
    BuildWindow(OsError),
    CreateSurface(windows::core::Error),
}

const BUFFER_COUNT: u32 = 2;

pub struct SkiaD3d12Window<State = ()> {
    state: State,
    swap_chain: SkiaD3d12SwapChain,
    winit_window: Window,
}
impl<State> SkiaD3d12Window<State> {
    fn draw(
        &mut self,
        env: &mut D3d12Backend,
        mut f: impl FnMut(&Canvas, &Window, &State),
    ) -> windows::core::Result<()> {
        self.swap_chain
            .draw(env, |canvas| {
                f(canvas, &self.winit_window, &self.state);

                self.winit_window.pre_present_notify();
            })
            .ok()
    }
}

pub(crate) struct SkiaD3d12SwapChain {
    pub(crate) swap_chain: IDXGISwapChain3,
    surfaces: Option<SkiaD3d12SwapChainSurfaceArray>,
}
impl SkiaD3d12SwapChain {
    fn new(swap_chain: IDXGISwapChain3, surfaces: SkiaD3d12SwapChainSurfaceArray) -> Self {
        Self {
            swap_chain,
            surfaces: Some(surfaces),
        }
    }
    pub(crate) fn resize(
        &mut self,
        env: &mut D3d12Backend,
        width: u32,
        height: u32,
    ) -> windows::core::Result<()> {
        env.cleanup();

        self.surfaces = None;

        unsafe {
            self.swap_chain
                .ResizeBuffers(BUFFER_COUNT, width, height, DXGI_FORMAT_UNKNOWN, 0)
        }
        .unwrap();

        self.surfaces
            .replace(env.create_swap_chain_surfaces(&self.swap_chain, width, height));
        Ok(())
    }
    pub(crate) fn draw(
        &mut self,
        env: &mut D3d12Backend,
        mut f: impl FnMut(&Canvas),
    ) -> windows::core::HRESULT {
        let index = unsafe { self.swap_chain.GetCurrentBackBufferIndex() };
        let surface = &mut self.surfaces.as_mut().unwrap()[index as usize].0;

        let canvas = surface.canvas();

        f(&canvas);

        env.direct_context.flush_and_submit_surface(surface, None);
        unsafe { self.swap_chain.Present(1, 0) }
    }
    pub(crate) fn present(&mut self, env: &mut D3d12Backend) {
        let surface = self.get_surface();
        env.direct_context.flush_and_submit_surface(surface, None);
        unsafe { self.swap_chain.Present(1, 0) }.ok().unwrap()
    }
    fn get_surface(&mut self) -> &mut Surface {
        let index = unsafe { self.swap_chain.GetCurrentBackBufferIndex() };
        &mut self.surfaces.as_mut().unwrap()[index as usize].0
    }
}
impl SkiaRender for SkiaD3d12SwapChain {
    fn prepare_and_get_surface(&mut self, _: &mut dyn Provider) -> &mut Surface {
        self.get_surface()
    }

    fn present(&mut self, env: &mut dyn Provider) {
        let env = request_mut::<D3d12Backend>(env).unwrap();
        self.present(env);
    }
    fn resize(&mut self, env: &mut dyn Provider, size: PhysicalSize<u32>, _: &Window) {
        let env = request_mut::<D3d12Backend>(env).unwrap();
        self.resize(env, size.width, size.height).unwrap()
    }
}

type SkiaD3d12SwapChainSurfaceArray = [(Surface, BackendRenderTarget); BUFFER_COUNT as _];

fn get_hardware_adapter_and_device(
    factory: &IDXGIFactory4,
) -> windows::core::Result<(IDXGIAdapter1, ID3D12Device)> {
    for i in 0.. {
        let adapter = unsafe { factory.EnumAdapters1(i) }?;

        let mut adapter_desc = Default::default();
        unsafe { adapter.GetDesc1(&mut adapter_desc) }?;

        if (DXGI_ADAPTER_FLAG(adapter_desc.Flags as _) & DXGI_ADAPTER_FLAG_SOFTWARE)
            != DXGI_ADAPTER_FLAG_NONE
        {
            continue; // Don't select the Basic Render Driver adapter.
        }

        let mut device = None;
        if unsafe { D3D12CreateDevice(&adapter, D3D_FEATURE_LEVEL_11_0, &mut device) }.is_ok() {
            return Ok((adapter, device.unwrap()));
        }
    }
    unreachable!()
}
