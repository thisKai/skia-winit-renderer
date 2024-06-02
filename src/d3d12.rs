use std::collections::HashMap;

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
                    DXGI_FORMAT_R8G8B8A8_UNORM, DXGI_SAMPLE_DESC,
                    DXGI_STANDARD_MULTISAMPLE_QUALITY_PATTERN,
                },
                CreateDXGIFactory1, IDXGIAdapter1, IDXGIFactory4, IDXGISwapChain3,
                DXGI_ADAPTER_FLAG, DXGI_ADAPTER_FLAG_NONE, DXGI_ADAPTER_FLAG_SOFTWARE,
                DXGI_SWAP_CHAIN_DESC1, DXGI_SWAP_EFFECT_FLIP_DISCARD,
                DXGI_USAGE_RENDER_TARGET_OUTPUT,
            },
        },
    },
};
use winit::{
    error::OsError,
    event_loop::EventLoopWindowTarget,
    window::{Window, WindowBuilder, WindowId},
};

pub struct D3d12WindowManager<State = ()> {
    env: D3d12Env,
    windows: HashMap<WindowId, SkiaD3d12Window<State>>,
}
impl D3d12WindowManager {
    pub fn new() -> windows::core::Result<Self> {
        Ok(Self {
            env: D3d12Env::new()?,
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
    pub fn draw(&mut self, window_id: &WindowId, mut f: impl FnMut(&Canvas, &Window)) {
        self.draw_with_state(window_id, |canvas, window, _| f(canvas, window));
    }
}
impl<State> D3d12WindowManager<State> {
    pub fn with_state() -> windows::core::Result<Self> {
        Ok(Self {
            env: D3d12Env::new()?,
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
        mut f: impl FnMut(&Canvas, &Window, &State),
    ) {
        let Some(window) = self.windows.get_mut(window_id) else {
            return;
        };
        window.skia.draw(&mut self.env, |canvas| {
            f(canvas, &window.winit_window, &window.state);
            window.winit_window.pre_present_notify();
        })
    }
}

pub(crate) struct D3d12Env {
    factory: IDXGIFactory4,
    backend_context: BackendContext,
    direct_context: DirectContext,
}
impl D3d12Env {
    fn new() -> windows::core::Result<Self> {
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
        let hwnd = match window.raw_window_handle() {
            RawWindowHandle::Win32(window_handle) => HWND(window_handle.hwnd as _),
            _ => panic!("not win32"),
        };
        let size = window.inner_size();
        let skia = self
            .create_hwnd_surface(hwnd, size.width, size.height)
            .map_err(CreateD3d12WindowError::CreateSurface)?;

        Ok(SkiaD3d12Window {
            winit_window: window,
            skia,
            state,
        })
    }
    fn create_hwnd_surface(
        &mut self,
        hwnd: HWND,
        width: u32,
        height: u32,
    ) -> windows::core::Result<SkiaD3d12Renderer> {
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
                    SwapEffect: DXGI_SWAP_EFFECT_FLIP_DISCARD,
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

        let surfaces: [_; BUFFER_COUNT as _] = std::array::from_fn(|i| {
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
                SurfaceOrigin::BottomLeft,
                ColorType::RGBA8888,
                None,
                None,
            )
            .unwrap();

            (surface, backend_render_target)
        });
        Ok(SkiaD3d12Renderer {
            swap_chain,
            surfaces,
        })
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
    skia: SkiaD3d12Renderer,
    winit_window: Window,
}

struct SkiaD3d12Renderer {
    swap_chain: IDXGISwapChain3,
    surfaces: [(Surface, BackendRenderTarget); BUFFER_COUNT as _],
}
impl SkiaD3d12Renderer {
    fn draw(&mut self, env: &mut D3d12Env, mut f: impl FnMut(&Canvas)) {
        let index = unsafe { self.swap_chain.GetCurrentBackBufferIndex() };
        let (surface, _) = &mut self.surfaces[index as usize];
        let canvas = surface.canvas();

        f(&canvas);

        env.direct_context.flush_and_submit_surface(surface, None);
        unsafe { self.swap_chain.Present(1, 0) }.unwrap();
    }
}

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
