use provide_any::provide_any::{request_mut, Provider};

use skia_d3d12_swap_chain::{CompositionBackend, CompositionSwapChain, CompositionTarget};
use windows::{Foundation::Numerics::Vector2, UI::Composition::CompositionSurfaceBrush};
use winit::{
    dpi::PhysicalSize,
    error::OsError,
    event_loop::EventLoopWindowTarget,
    window::{Window, WindowBuilder},
};

use crate::generic::{RenderWindow, SkiaGraphicsBackend, SkiaRender};

#[derive(Debug)]
pub enum CreateWindowError {
    BuildWindow(OsError),
    CreateTarget(windows::core::Error),
}

pub struct WindowsUiCompositionBackend(CompositionBackend);

impl WindowsUiCompositionBackend {
    fn create_renderer(
        &mut self,
        window: &Window,
    ) -> windows::core::Result<WindowsUiCompositionRenderer> {
        let size = window.inner_size();
        let target = CompositionTarget::with_window(&window)?;

        let swap_chain = self.0.create_swap_chain(size.width, size.height).unwrap();

        let surface = target.create_surface(&swap_chain).unwrap().unwrap();
        let brush = target
            .compositor
            .CreateSurfaceBrushWithSurface(&surface)
            .unwrap();
        brush
            .SetStretch(windows::UI::Composition::CompositionStretch::UniformToFill)
            .unwrap();

        let visual = target.compositor.CreateSpriteVisual().unwrap();
        visual.SetBrush(&brush).unwrap();
        visual
            .SetRelativeSizeAdjustment(Vector2 { X: 1.0, Y: 1.0 })
            .unwrap();

        target.desktop_window_target.SetRoot(&visual).unwrap();

        Ok(WindowsUiCompositionRenderer {
            target,
            brush,
            swap_chain,
        })
    }
}

impl SkiaGraphicsBackend for WindowsUiCompositionBackend {
    type CreateError = windows::core::Error;

    type CreateWindowError = CreateWindowError;

    fn composited(&self) -> bool {
        true
    }

    fn create<D: raw_window_handle::HasRawDisplayHandle>(_: &D) -> Result<Self, Self::CreateError> {
        Ok(Self(CompositionBackend::new()?))
    }

    fn create_window<WinitUserEvent>(
        &mut self,
        elwt: &EventLoopWindowTarget<WinitUserEvent>,
        builder: WindowBuilder,
    ) -> Result<RenderWindow, Self::CreateWindowError> {
        let window = builder
            .build(elwt)
            .map_err(CreateWindowError::BuildWindow)?;

        let render = self
            .create_renderer(&window)
            .map_err(CreateWindowError::CreateTarget)?;

        Ok(RenderWindow::new(render, window))
    }
}

pub struct WindowsUiCompositionRenderer {
    target: CompositionTarget,
    brush: CompositionSurfaceBrush,
    swap_chain: CompositionSwapChain,
}

impl SkiaRender for WindowsUiCompositionRenderer {
    fn prepare_and_get_surface(&mut self, env: &mut dyn Provider) -> &mut skia_safe::Surface {
        let env = request_mut::<WindowsUiCompositionBackend>(env).unwrap();

        if let Some(new_composition_surface) = self
            .swap_chain
            .new_composition_surface(&mut env.0, &self.target)
            .unwrap()
        {
            self.brush.SetSurface(&new_composition_surface).unwrap();
        }
        self.swap_chain.unwrap_surface_mut()
    }

    fn present(&mut self, env: &mut dyn Provider) {
        let env = request_mut::<WindowsUiCompositionBackend>(env).unwrap();
        self.swap_chain.present(&mut env.0)
    }

    fn resize(&mut self, env: &mut dyn Provider, size: PhysicalSize<u32>, _: &Window) {
        let env = request_mut::<WindowsUiCompositionBackend>(env).unwrap();
        self.swap_chain.resize(&mut env.0, size.width, size.height)
    }
}
