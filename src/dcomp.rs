use provide_any::provide_any::request_mut;
use skia_d3d12_swap_chain::{DCompBackend, DCompSwapChain};
use windows::Win32::Graphics::DirectComposition::{
    IDCompositionTarget, IDCompositionVisual2, DCOMPOSITION_BACKFACE_VISIBILITY_HIDDEN,
};
use winit::{
    dpi::PhysicalSize,
    error::OsError,
    event_loop::EventLoopWindowTarget,
    platform::windows::WindowBuilderExtWindows,
    window::{Window, WindowBuilder},
};

use crate::generic::{RenderWindow, SkiaGraphicsBackend, SkiaRender};

#[derive(Debug)]
pub enum CreateWindowError {
    BuildWindow(OsError),
    CreateTarget(windows::core::Error),
}

pub struct DirectCompositionBackend(DCompBackend);
impl DirectCompositionBackend {
    fn create_renderer(
        &mut self,
        window: &Window,
    ) -> windows::core::Result<DirectCompositionRenderer> {
        let target = self.0.create_target_for_window(&window).unwrap();

        let size = window.inner_size();
        let swap_chain = self.0.create_swap_chain(size.width, size.height).unwrap();

        let visual = unsafe {
            let visual = self.0.dcomp_desktop_device.CreateVisual().unwrap();
            visual
                .SetContent(swap_chain.unwrap_inner_swap_chain())
                .unwrap();
            visual
                .SetBackFaceVisibility(DCOMPOSITION_BACKFACE_VISIBILITY_HIDDEN)
                .unwrap();

            target.SetRoot(&visual).unwrap();

            self.0.dcomp_desktop_device.Commit().unwrap();

            visual
        };

        Ok(DirectCompositionRenderer {
            target,
            visual,
            swap_chain,
        })
    }
}
impl SkiaGraphicsBackend for DirectCompositionBackend {
    type CreateError = windows::core::Error;

    type CreateWindowError = CreateWindowError;

    fn composited(&self) -> bool {
        true
    }

    fn create<D: raw_window_handle::HasRawDisplayHandle>(_: &D) -> Result<Self, Self::CreateError> {
        Ok(Self(DCompBackend::new()?))
    }

    fn create_window<WinitUserEvent>(
        &mut self,
        elwt: &EventLoopWindowTarget<WinitUserEvent>,
        builder: WindowBuilder,
    ) -> Result<RenderWindow, Self::CreateWindowError> {
        let window = builder
            .with_no_redirection_bitmap(true)
            .build(elwt)
            .map_err(CreateWindowError::BuildWindow)?;

        let render = self
            .create_renderer(&window)
            .map_err(CreateWindowError::CreateTarget)?;

        Ok(RenderWindow::new(render, window))
    }
}

pub struct DirectCompositionRenderer {
    target: IDCompositionTarget,
    visual: IDCompositionVisual2,
    swap_chain: DCompSwapChain,
}
impl SkiaRender for DirectCompositionRenderer {
    fn prepare_and_get_surface(
        &mut self,
        env: &mut dyn provide_any::provide_any::Provider,
    ) -> &mut skia_safe::Surface {
        let env = request_mut::<DirectCompositionBackend>(env).unwrap();

        if let Some(inner_swap_chain) = self.swap_chain.new_inner_swap_chain(&mut env.0).unwrap() {
            unsafe {
                self.visual.SetContent(inner_swap_chain).unwrap();
            }
        }

        self.swap_chain.unwrap_surface(&mut env.0)
    }

    fn present(&mut self, env: &mut dyn provide_any::provide_any::Provider) {
        let env = request_mut::<DirectCompositionBackend>(env).unwrap();

        self.swap_chain.present(&mut env.0).unwrap()
    }

    fn resize(
        &mut self,
        env: &mut dyn provide_any::provide_any::Provider,
        size: PhysicalSize<u32>,
        _: &Window,
    ) {
        let env = request_mut::<DirectCompositionBackend>(env).unwrap();

        self.swap_chain.resize(&mut env.0, size.width, size.height)
    }
}

#[derive(Debug)]
pub enum CreateDCompWindowError {
    BuildWindow(OsError),
    CreateTarget(windows::core::Error),
}
