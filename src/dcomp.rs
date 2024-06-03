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
    error::OsError,
    event_loop::EventLoopWindowTarget,
    window::{Window, WindowBuilder, WindowId},
};

pub struct DCompWindowManager<State = ()> {
    env: DCompEnv,
    windows: HashMap<WindowId, DCompWindow<State>>,
}
impl DCompWindowManager {
    pub fn new() -> Self {
        Self {
            env: Default::default(),
            windows: HashMap::new(),
        }
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
        let window = self.create_window_object(elwt, builder, state)?;
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
    fn create_window_object<UserEvent>(
        &mut self,
        elwt: &EventLoopWindowTarget<UserEvent>,
        builder: WindowBuilder,
        state: State,
    ) -> Result<DCompWindow<State>, CreateDCompWindowError> {
        let window = builder
            .build(elwt)
            .map_err(CreateDCompWindowError::BuildWindow)?;

        let target = self
            .create_window_target(&window)
            .map_err(CreateDCompWindowError::CreateTarget)?;

        Ok(DCompWindow {
            winit_window: window,
            target: Some(target),
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

#[derive(Default)]
struct DCompEnv {
    d3d11_device: Option<ID3D11Device>,
    dcomp_desktop_device: Option<IDCompositionDesktopDevice>,
}
impl DCompEnv {
    fn create_device_resources(&mut self) -> windows::core::Result<()> {
        unsafe {
            debug_assert!(self.d3d11_device.is_none());
            let device_3d = Self::create_device_3d()?;
            let device_2d = Self::create_device_2d(&device_3d)?;
            self.d3d11_device = Some(device_3d);
            let desktop: IDCompositionDesktopDevice = DCompositionCreateDevice2(&device_2d)?;
            self.dcomp_desktop_device = Some(desktop);
            Ok(())
        }
    }
    fn create_hwnd_target(&mut self, hwnd: HWND) -> windows::core::Result<IDCompositionTarget> {
        if self.dcomp_desktop_device.is_none() {
            self.create_device_resources()?;
        }
        unsafe {
            let desktop = self.dcomp_desktop_device.as_ref().unwrap();
            desktop.CreateTargetForHwnd(hwnd, true)
        }
    }
    fn draw_handler(&mut self) -> windows::core::Result<()> {
        unsafe {
            if let Some(device) = &self.d3d11_device {
                if cfg!(debug_assertions) {
                    println!("check device");
                }
                device.GetDeviceRemovedReason()?;
            } else {
                if cfg!(debug_assertions) {
                    println!("build device");
                }
                self.create_device_resources()?;
            }
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
        let device = self.dcomp_desktop_device.as_ref().unwrap();
        unsafe {
            let visual = device.CreateVisual()?;
            visual.SetBackFaceVisibility(DCOMPOSITION_BACKFACE_VISIBILITY_HIDDEN)?;
            Ok(visual)
        }
    }
}

pub struct DCompWindow<State> {
    state: State,
    target: Option<IDCompositionTarget>,
    winit_window: Window,
}
impl<State> DCompWindow<State> {
    fn draw_handler(&mut self, env: &mut DCompEnv) -> windows::core::Result<()> {
        let hwnd = match self.winit_window.raw_window_handle() {
            RawWindowHandle::Win32(window_handle) => HWND(window_handle.hwnd as _),
            _ => panic!("not win32"),
        };
        unsafe {
            if self.target.is_none() {
                let target = env.create_hwnd_target(hwnd)?;
                let root_visual = env.create_visual()?;

                target.SetRoot(&root_visual)?;
                env.dcomp_desktop_device.as_ref().unwrap().Commit()?;
                self.target = Some(target);
            }
            ValidateRect(hwnd, None).ok()
        }
    }
}

struct DCompTarget {
    target: Option<IDCompositionTarget>,
}
impl DCompTarget {}
