use std::collections::HashMap;

use raw_window_handle::{HasRawWindowHandle, RawWindowHandle};
use windows::{
    core::Interface,
    Win32::{
        Foundation::HWND,
        Graphics::{
            Direct2D::{D2D1CreateDevice, ID2D1Device},
            Direct3D::D3D_DRIVER_TYPE_HARDWARE,
            Direct3D11::{
                D3D11CreateDevice, ID3D11Device, D3D11_CREATE_DEVICE_BGRA_SUPPORT,
                D3D11_SDK_VERSION,
            },
            DirectComposition::{
                DCompositionCreateDevice2, IDCompositionDesktopDevice, IDCompositionTarget,
                IDCompositionVisual2, DCOMPOSITION_BACKFACE_VISIBILITY_HIDDEN,
            },
            Dxgi::IDXGIDevice3,
            Gdi::ValidateRect,
        },
    },
};
use winit::{
    dpi::PhysicalSize,
    error::OsError,
    event_loop::EventLoopWindowTarget,
    platform::windows::WindowBuilderExtWindows,
    window::{Window, WindowBuilder, WindowId},
};

use crate::d3d12::{D3d12Backend, SkiaD3d12SwapChain};

pub struct DCompWindowManager<State = ()> {
    env: DCompEnv,
    windows: HashMap<WindowId, DCompWindow<State>>,
}
impl DCompWindowManager {
    pub fn new() -> windows::core::Result<Self> {
        Ok(Self {
            env: DCompEnv::new()?,
            windows: HashMap::new(),
        })
    }
    pub fn create_window<UserEvent>(
        &mut self,
        elwt: &EventLoopWindowTarget<UserEvent>,
        builder: WindowBuilder,
    ) -> Result<WindowId, CreateDCompWindowError> {
        self.create_window_with_state(elwt, builder, ())
    }
}
impl<State> DCompWindowManager<State> {
    pub fn create_window_with_state<UserEvent>(
        &mut self,
        elwt: &EventLoopWindowTarget<UserEvent>,
        builder: WindowBuilder,
        state: State,
    ) -> Result<WindowId, CreateDCompWindowError> {
        let window =
            self.create_window_object(elwt, builder.with_no_redirection_bitmap(true), state)?;
        let window_id = window.winit_window.id();

        self.windows.insert(window_id, window);

        Ok(window_id)
    }
    pub fn get_window(&self, window_id: &WindowId) -> Option<&DCompWindow<State>> {
        self.windows.get(window_id)
    }
    pub fn get_window_mut(&mut self, window_id: &WindowId) -> Option<&mut DCompWindow<State>> {
        self.windows.get_mut(window_id)
    }
    pub fn remove_window(&mut self, window_id: &WindowId) -> Option<DCompWindow<State>> {
        self.windows.remove(window_id)
    }
    pub fn resize_window(
        &mut self,
        window_id: &WindowId,
        size: PhysicalSize<u32>,
    ) -> windows::core::Result<()> {
        self.env.d3d12.cleanup();

        let window = self.windows.get_mut(window_id).unwrap();

        window.resize(&mut self.env, size.width, size.height)?;

        window.winit_window.request_redraw();
        Ok(())
    }
    fn create_window_object<UserEvent>(
        &mut self,
        elwt: &EventLoopWindowTarget<UserEvent>,
        builder: WindowBuilder,
        state: State,
    ) -> Result<DCompWindow<State>, CreateDCompWindowError> {
        let window = builder
            .build(elwt)
            .map_err(CreateDCompWindowError::BuildWindow)?;

        // let target = self
        //     .create_window_target(&window)
        //     .map_err(CreateDCompWindowError::CreateTarget)?;

        Ok(DCompWindow {
            winit_window: window,
            target: None,
            swap_chain: None,
            root_visual: None,
            state,
        })
    }
    fn create_window_target(
        &mut self,
        window: &Window,
    ) -> windows::core::Result<IDCompositionTarget> {
        let hwnd = match window.raw_window_handle() {
            RawWindowHandle::Win32(window_handle) => HWND(window_handle.hwnd as _),
            _ => panic!("not win32"),
        };
        self.env.create_hwnd_target(hwnd)
    }
    pub fn draw_handler(&mut self, window_id: &WindowId) -> windows::core::Result<()> {
        self.env.draw_handler()?;

        let window = self.windows.get_mut(window_id).unwrap();
        window.draw_handler(&mut self.env)
    }
}

#[derive(Debug)]
pub enum CreateDCompWindowError {
    BuildWindow(OsError),
    CreateTarget(windows::core::Error),
}

struct DCompEnv {
    d3d11_device: ID3D11Device,
    dcomp_desktop_device: IDCompositionDesktopDevice,
    d3d12: D3d12Backend,
}
impl DCompEnv {
    pub fn new() -> windows::core::Result<Self> {
        unsafe {
            let d3d11_device = Self::create_device_3d()?;
            let d2d_device = Self::create_device_2d(&d3d11_device)?;
            let dcomp_desktop_device: IDCompositionDesktopDevice =
                DCompositionCreateDevice2(&d2d_device)?;

            Ok(Self {
                d3d11_device,
                dcomp_desktop_device,
                d3d12: D3d12Backend::new()?,
            })
        }
    }
    // fn create_device_resources(&mut self) -> windows::core::Result<()> {
    //     unsafe {
    //         debug_assert!(self.d3d11_device.is_none());
    //         let device_3d = Self::create_device_3d()?;
    //         let device_2d = Self::create_device_2d(&device_3d)?;
    //         self.d3d11_device = Some(device_3d);
    //         let desktop: IDCompositionDesktopDevice = DCompositionCreateDevice2(&device_2d)?;
    //         self.dcomp_desktop_device = Some(desktop);
    //         Ok(())
    //     }
    // }
    fn create_hwnd_target(&mut self, hwnd: HWND) -> windows::core::Result<IDCompositionTarget> {
        unsafe { self.dcomp_desktop_device.CreateTargetForHwnd(hwnd, true) }
    }
    fn draw_handler(&mut self) -> windows::core::Result<()> {
        unsafe {
            if cfg!(debug_assertions) {
                println!("check device");
            }
            self.d3d11_device.GetDeviceRemovedReason()?;
            Ok(())
        }
    }
    fn create_device_3d() -> windows::core::Result<ID3D11Device> {
        let mut device = None;

        unsafe {
            D3D11CreateDevice(
                None,
                D3D_DRIVER_TYPE_HARDWARE,
                None,
                D3D11_CREATE_DEVICE_BGRA_SUPPORT,
                None,
                D3D11_SDK_VERSION,
                Some(&mut device),
                None,
                None,
            )
            .map(|()| device.unwrap())
        }
    }

    fn create_device_2d(device_3d: &ID3D11Device) -> windows::core::Result<ID2D1Device> {
        let dxgi: IDXGIDevice3 = device_3d.cast()?;
        unsafe { D2D1CreateDevice(&dxgi, None) }
    }

    fn create_visual(&self) -> windows::core::Result<IDCompositionVisual2> {
        unsafe {
            let visual = self.dcomp_desktop_device.CreateVisual()?;
            visual.SetBackFaceVisibility(DCOMPOSITION_BACKFACE_VISIBILITY_HIDDEN)?;
            Ok(visual)
        }
    }
}

pub struct DCompWindow<State> {
    state: State,
    target: Option<IDCompositionTarget>,
    root_visual: Option<IDCompositionVisual2>,
    swap_chain: Option<SkiaD3d12SwapChain>,
    winit_window: Window,
}
impl<State> DCompWindow<State> {
    fn resize(&mut self, env: &mut DCompEnv, width: u32, height: u32) -> windows::core::Result<()> {
        unsafe {
            dbg!("resize");
            let root_visual = env.create_visual()?;

            let mut swap_chain = env.d3d12.create_composition_swap_chain(width, height)?;

            swap_chain.draw(&mut env.d3d12, |canvas| {
                canvas.clear(skia_safe::colors::BLACK);
            });

            root_visual.SetContent(&swap_chain.swap_chain)?;
            self.swap_chain = Some(swap_chain);

            self.target.as_ref().unwrap().SetRoot(&root_visual)?;
            env.dcomp_desktop_device.Commit()?;
        }
        // if let Some(swap_chain) = &mut self.swap_chain {
        //     swap_chain.resize(&mut env.d3d12, width, height)?;
        //     unsafe {
        //         self.root_visual
        //             .as_ref()
        //             .unwrap()
        //             .SetContent(&swap_chain.swap_chain)
        //     }?;
        //     // env.dcomp_desktop_device
        //     // unsafe {
        //     //     let root_visual = env.create_visual()?;
        //     //     root_visual.SetContent(&swap_chain.swap_chain)?;
        //     //     self.target.as_ref().unwrap().SetRoot(&root_visual)?;
        //     //     env.dcomp_desktop_device.Commit()?;
        //     // }
        // }

        Ok(())
    }
    fn draw_handler(&mut self, env: &mut DCompEnv) -> windows::core::Result<()> {
        let hwnd = match self.winit_window.raw_window_handle() {
            RawWindowHandle::Win32(window_handle) => HWND(window_handle.hwnd as _),
            _ => panic!("not win32"),
        };
        unsafe {
            match &self.target {
                Some(target) => {
                    dbg!("paint");
                    let swap_chain = self.swap_chain.as_mut().unwrap();
                    swap_chain.draw(&mut env.d3d12, |canvas| {
                        canvas.clear(skia_safe::colors::BLACK);
                    });
                    // self.swap_chain
                    env.dcomp_desktop_device.Commit()?;
                }
                None => {
                    let target = env.create_hwnd_target(hwnd)?;
                    let root_visual = env.create_visual()?;

                    let size = self.winit_window.inner_size();
                    let mut swap_chain = env
                        .d3d12
                        .create_composition_swap_chain(size.width, size.height)?;

                    swap_chain.draw(&mut env.d3d12, |canvas| {
                        canvas.clear(skia_safe::colors::BLACK);
                    });

                    root_visual.SetContent(&swap_chain.swap_chain)?;
                    self.swap_chain = Some(swap_chain);

                    target.SetRoot(&root_visual)?;
                    env.dcomp_desktop_device.Commit()?;
                    self.target = Some(target);
                }
            }
            if self.target.is_none() {}
            ValidateRect(hwnd, None).ok()
        }
    }
}

struct DCompTarget {
    target: Option<IDCompositionTarget>,
}
impl DCompTarget {}
