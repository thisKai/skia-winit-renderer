use provide_any::provide_any::{request_mut, Provider};

use skia_d3d12_swap_chain::{WinCompBackend, WinCompSwapChain, WinCompTarget};
use windows::{
    Foundation::{
        Numerics::{Vector2, Vector3},
        TypedEventHandler,
    },
    UI::Composition::{CompositionSurfaceBrush, SpriteVisual},
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

pub struct WindowsUiCompositionBackend(WinCompBackend);

impl WindowsUiCompositionBackend {
    fn create_renderer(
        &mut self,
        window: &Window,
    ) -> windows::core::Result<WindowsUiCompositionRenderer> {
        let size = window.inner_size();
        let target = WinCompTarget::with_window(&window)?;
        target
            .controller
            .CommitNeeded(&TypedEventHandler::new(|sender, r| {
                println!("Commit Needed");
                Ok(())
            }))
            .unwrap();

        let swap_chain = self.0.create_swap_chain(size.width, size.height).unwrap();

        let surface = target.create_surface(&swap_chain).unwrap().unwrap();
        let brush = target
            .controller
            .Compositor()
            .unwrap()
            .CreateSurfaceBrushWithSurface(&surface)
            .unwrap();
        brush
            .SetStretch(windows::UI::Composition::CompositionStretch::None)
            .unwrap();

        let visual = target
            .controller
            .Compositor()
            .unwrap()
            .CreateSpriteVisual()
            .unwrap();
        visual.SetBrush(&brush).unwrap();

        visual.SetOffset(Vector3 {
            X: 0.0,
            Y: 0.0,
            Z: 0.0,
        })?;
        visual.SetSize(Vector2 {
            X: size.width as _,
            Y: size.height as _,
        })?;

        target.desktop_window_target.SetRoot(&visual).unwrap();

        target.controller.Commit()?;

        Ok(WindowsUiCompositionRenderer {
            target,
            visual,
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
        Ok(Self(WinCompBackend::new()?))
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

pub struct WindowsUiCompositionRenderer {
    target: WinCompTarget,
    visual: SpriteVisual,
    brush: CompositionSurfaceBrush,
    swap_chain: WinCompSwapChain,
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
        self.swap_chain.present(&mut env.0, &self.target).unwrap();
    }

    fn resize(&mut self, env: &mut dyn Provider, size: PhysicalSize<u32>, _: &Window) {
        let env = request_mut::<WindowsUiCompositionBackend>(env).unwrap();

        self.swap_chain
            .resize(&mut env.0, &self.target, size.width, size.height);
        self.visual
            .SetOffset(Vector3 {
                X: 0.0,
                Y: 0.0,
                Z: 0.0,
            })
            .unwrap();
        self.visual
            .SetSize(Vector2 {
                X: size.width as _,
                Y: size.height as _,
            })
            .unwrap();
        self.target.controller.Commit().unwrap();
    }
}
