use provide_any::provide_any::{request_mut, Provider};
use skia_d3d12_swap_chain::{Backend, HwndSwapChain};
use skia_safe::Surface;
use winit::{
    dpi::PhysicalSize,
    error::OsError,
    event_loop::ActiveEventLoop,
    raw_window_handle::HasDisplayHandle,
    window::{Window, WindowAttributes},
};

use crate::generic::{RenderWindow, SkiaGraphicsBackend, SkiaRender};

#[derive(Debug)]
pub enum CreateD3d12WindowError {
    BuildWindow(OsError),
    CreateSurface(windows::core::Error),
}

pub struct D3d12Backend(Backend);

impl SkiaGraphicsBackend for D3d12Backend {
    type CreateError = windows::core::Error;

    type CreateWindowError = CreateD3d12WindowError;

    fn create<D: HasDisplayHandle>(_: &D) -> Result<Self, Self::CreateError> {
        Ok(Self(Backend::new()?))
    }

    fn create_window(
        &mut self,
        event_loop: &ActiveEventLoop,
        attributes: WindowAttributes,
    ) -> Result<RenderWindow, Self::CreateWindowError> {
        let window = event_loop
            .create_window(attributes)
            .map_err(CreateD3d12WindowError::BuildWindow)?;

        let size = window.inner_size();

        let render = self
            .0
            .create_window_swap_chain(&window, size.width, size.height)
            .map_err(CreateD3d12WindowError::CreateSurface)?;

        Ok(RenderWindow::new(render, window))
    }
}

impl SkiaRender for HwndSwapChain {
    fn prepare_and_get_surface(&mut self, env: &mut dyn Provider) -> &mut Surface {
        let env = request_mut::<D3d12Backend>(env).unwrap();
        self.get_surface(&mut env.0).unwrap()
    }

    fn present(&mut self, env: &mut dyn Provider) {
        let env = request_mut::<D3d12Backend>(env).unwrap();
        self.present(&mut env.0).unwrap()
    }

    fn resize(&mut self, env: &mut dyn Provider, size: PhysicalSize<u32>, _: &Window) {
        let env = request_mut::<D3d12Backend>(env).unwrap();
        self.resize(&mut env.0, size.width, size.height)
    }
}
