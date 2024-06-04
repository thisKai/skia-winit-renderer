use std::collections::HashMap;

use raw_window_handle::{HasRawWindowHandle, RawWindowHandle};
use skia_safe::Canvas;
use windows::{
    core::Interface,
    Foundation::Numerics::Vector2,
    System::DispatcherQueueController,
    Win32::{
        Foundation::HWND,
        System::WinRT::{
            Composition::{ICompositorDesktopInterop, ICompositorInterop},
            CreateDispatcherQueueController, DispatcherQueueOptions,
            DISPATCHERQUEUE_THREAD_APARTMENTTYPE, DISPATCHERQUEUE_THREAD_TYPE, DQTAT_COM_NONE,
            DQTYPE_THREAD_CURRENT,
        },
    },
    UI::Composition::{
        CompositionSurfaceBrush, Compositor, Desktop::DesktopWindowTarget, SpriteVisual,
    },
};
use winit::{
    dpi::PhysicalSize,
    error::OsError,
    event_loop::EventLoopWindowTarget,
    platform::windows::WindowBuilderExtWindows,
    window::{Window, WindowBuilder, WindowId},
};

use crate::d3d12::{D3d12Env, SkiaD3d12SwapChain};

pub struct WindowsUiCompositionWindowManager<State = ()> {
    env: WindowsUiCompositionEnv,
    windows: HashMap<WindowId, WindowsUiCompositionCompWindow<State>>,
}
impl WindowsUiCompositionWindowManager {
    pub fn new() -> windows::core::Result<Self> {
        Ok(Self {
            env: WindowsUiCompositionEnv::new()?,
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
    pub fn draw(
        &mut self,
        window_id: &WindowId,
        mut f: impl FnMut(&Canvas, &Window),
    ) -> windows::core::Result<()> {
        self.draw_with_state(window_id, |canvas, window, _| f(canvas, window))
    }
}
impl<State> WindowsUiCompositionWindowManager<State> {
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
    pub fn get_window(
        &self,
        window_id: &WindowId,
    ) -> Option<&WindowsUiCompositionCompWindow<State>> {
        self.windows.get(window_id)
    }
    pub fn get_window_mut(
        &mut self,
        window_id: &WindowId,
    ) -> Option<&mut WindowsUiCompositionCompWindow<State>> {
        self.windows.get_mut(window_id)
    }
    pub fn remove_window(
        &mut self,
        window_id: &WindowId,
    ) -> Option<WindowsUiCompositionCompWindow<State>> {
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
    pub fn draw_with_state(
        &mut self,
        window_id: &WindowId,
        f: impl FnMut(&Canvas, &Window, &State),
    ) -> windows::core::Result<()> {
        let window = self.windows.get_mut(window_id).unwrap();
        window.draw(&mut self.env, f)
    }
    fn create_window_object<UserEvent>(
        &mut self,
        elwt: &EventLoopWindowTarget<UserEvent>,
        builder: WindowBuilder,
        state: State,
    ) -> Result<WindowsUiCompositionCompWindow<State>, CreateDCompWindowError> {
        let window = builder
            .build(elwt)
            .map_err(CreateDCompWindowError::BuildWindow)?;

        let size = window.inner_size();
        let swap_chain = self
            .env
            .d3d12
            .create_composition_swap_chain(size.width, size.height)
            .map_err(CreateDCompWindowError::CreateTarget)?;

        WindowsUiCompositionCompWindow::new(window, swap_chain, state)
            .map_err(CreateDCompWindowError::CreateTarget)
    }
}

#[derive(Debug)]
pub enum CreateDCompWindowError {
    BuildWindow(OsError),
    CreateTarget(windows::core::Error),
}

struct WindowsUiCompositionEnv {
    _dispatcher_queue_controller: DispatcherQueueController,
    d3d12: D3d12Env,
}
impl WindowsUiCompositionEnv {
    pub fn new() -> windows::core::Result<Self> {
        Ok(Self {
            _dispatcher_queue_controller: create_dispatcher_queue_controller_for_current_thread()?,
            d3d12: D3d12Env::new()?,
        })
    }
}
pub struct WindowsUiCompositionCompWindow<State> {
    state: State,
    compositor: Compositor,
    desktop_window_target: DesktopWindowTarget,
    swap_chain: SkiaD3d12SwapChain,
    visual: SpriteVisual,
    brush: CompositionSurfaceBrush,
    winit_window: Window,
}
impl<State> WindowsUiCompositionCompWindow<State> {
    fn new(
        winit_window: Window,
        swap_chain: SkiaD3d12SwapChain,
        state: State,
    ) -> windows::core::Result<Self> {
        let hwnd = match winit_window.raw_window_handle() {
            RawWindowHandle::Win32(window_handle) => HWND(window_handle.hwnd as _),
            _ => panic!("not win32"),
        };
        let compositor = Compositor::new()?;
        let compositor_desktop_interop: ICompositorDesktopInterop = compositor.cast()?;
        let desktop_window_target =
            unsafe { compositor_desktop_interop.CreateDesktopWindowTarget(hwnd, true) }?;

        let compositor_interop: ICompositorInterop = compositor.cast()?;

        let surface = unsafe {
            compositor_interop.CreateCompositionSurfaceForSwapChain(&swap_chain.swap_chain)
        }?;

        let brush = compositor.CreateSurfaceBrushWithSurface(&surface)?;
        brush.SetStretch(windows::UI::Composition::CompositionStretch::Fill)?;

        let visual = compositor.CreateSpriteVisual()?;
        visual.SetRelativeSizeAdjustment(Vector2 { X: 1.0, Y: 1.0 })?;
        visual.SetBrush(&brush)?;
        desktop_window_target.SetRoot(&visual)?;

        Ok(Self {
            state,
            compositor,
            desktop_window_target,
            swap_chain,
            brush,
            visual,
            winit_window,
        })
    }
    fn resize(
        &mut self,
        env: &mut WindowsUiCompositionEnv,
        width: u32,
        height: u32,
    ) -> windows::core::Result<()> {
        if width == 0 || height == 0 {
            return Ok(());
        }
        self.swap_chain.resize(&mut env.d3d12, width, height)?;
        // let swap_chain = env.d3d12.create_composition_swap_chain(width, height)?;

        // let compositor_interop: ICompositorInterop = self.compositor.cast()?;

        // let surface = unsafe {
        //     compositor_interop.CreateCompositionSurfaceForSwapChain(&swap_chain.swap_chain)
        // }?;
        // self.swap_chain = swap_chain;
        // let brush = self.compositor.CreateSurfaceBrushWithSurface(&surface)?;
        // brush.SetStretch(windows::UI::Composition::CompositionStretch::None)?;
        // brush.SetHorizontalAlignmentRatio(0.5)?;
        // brush.SetVerticalAlignmentRatio(0.5)?;

        // self.visual.SetBrush(&brush)?;
        Ok(())
    }
    fn draw(
        &mut self,
        env: &mut WindowsUiCompositionEnv,
        mut f: impl FnMut(&Canvas, &Window, &State),
    ) -> windows::core::Result<()> {
        self.swap_chain
            .draw(&mut env.d3d12, |canvas| {
                f(canvas, &self.winit_window, &self.state);

                self.winit_window.pre_present_notify();
            })
            .ok()
            .unwrap();
        Ok(())
    }
}

pub(crate) fn create_dispatcher_queue_controller_for_current_thread(
) -> windows::core::Result<DispatcherQueueController> {
    create_dispatcher_queue_controller(DQTYPE_THREAD_CURRENT, DQTAT_COM_NONE)
}

fn create_dispatcher_queue_controller(
    thread_type: DISPATCHERQUEUE_THREAD_TYPE,
    apartment_type: DISPATCHERQUEUE_THREAD_APARTMENTTYPE,
) -> windows::core::Result<DispatcherQueueController> {
    let options = DispatcherQueueOptions {
        dwSize: std::mem::size_of::<DispatcherQueueOptions>() as u32,
        threadType: thread_type,
        apartmentType: apartment_type,
    };
    unsafe { CreateDispatcherQueueController(options) }
}
