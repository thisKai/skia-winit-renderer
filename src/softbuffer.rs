use std::num::NonZeroU32;

use provide_any::provide_any::Provider;
use skia_safe::Surface;
use softbuffer::{Context, SoftBufferError, Surface as SoftBufferSurface};
use winit::{
    dpi::PhysicalSize,
    error::OsError,
    event_loop::ActiveEventLoop,
    raw_window_handle::HasDisplayHandle,
    raw_window_handle_05::{
        HasRawDisplayHandle, HasRawWindowHandle, RawDisplayHandle, RawWindowHandle,
    },
    window::{Window, WindowAttributes},
};

use crate::generic::{RenderWindow, SkiaGraphicsBackend, SkiaRender};

#[derive(Debug)]
pub enum CreateWindowError {
    CreateWindow(OsError),
    SoftBuffer(SoftBufferError),
}

pub struct SoftBufferBackend {
    context: Context,
}
impl SoftBufferBackend {
    pub(crate) fn new(raw_display_handle: RawDisplayHandle) -> Result<Self, SoftBufferError> {
        Ok(Self {
            context: unsafe { Context::from_raw(raw_display_handle) }?,
        })
    }
    fn create_surface(
        &mut self,
        raw_window_handle: RawWindowHandle,
    ) -> Result<SoftBufferSurface, SoftBufferError> {
        Ok(unsafe { SoftBufferSurface::from_raw(&self.context, raw_window_handle)? })
    }
}
impl SkiaGraphicsBackend for SoftBufferBackend {
    type CreateError = SoftBufferError;
    type CreateWindowError = CreateWindowError;

    fn create<D: HasDisplayHandle + HasRawDisplayHandle>(
        display: &D,
    ) -> Result<Self, Self::CreateError> {
        Self::new(display.raw_display_handle())
    }
    fn create_window(
        &mut self,
        event_loop: &ActiveEventLoop,
        attributes: WindowAttributes,
    ) -> Result<crate::generic::RenderWindow, Self::CreateWindowError> {
        let window = event_loop
            .create_window(attributes)
            .map_err(CreateWindowError::CreateWindow)?;
        let size: (i32, i32) = window.inner_size().into();

        let softbuffer_surface = self
            .create_surface(window.raw_window_handle())
            .map_err(CreateWindowError::SoftBuffer)?;

        let skia_surface = skia_safe::surfaces::raster_n32_premul(size).unwrap();

        Ok(RenderWindow::new(
            SkiaSoftBufferRenderer {
                skia_surface,
                softbuffer_surface,
            },
            window,
        ))
    }
}

pub struct SkiaSoftBufferRenderer {
    skia_surface: Surface,
    softbuffer_surface: SoftBufferSurface,
}
impl SkiaRender for SkiaSoftBufferRenderer {
    fn prepare_and_get_surface(&mut self, _: &mut dyn Provider) -> &mut Surface {
        &mut self.skia_surface
    }

    fn present(&mut self, _: &mut dyn Provider) {
        let snapshot = self.skia_surface.image_snapshot();

        let peek = snapshot.peek_pixels().unwrap();
        let pixels: &[u32] = peek.pixels().unwrap();

        let mut buffer = self.softbuffer_surface.buffer_mut().unwrap();

        if pixels.len() != buffer.len() {
            return;
        }
        buffer.copy_from_slice(pixels);
        buffer.present().unwrap();
    }
    fn resize(&mut self, _: &mut dyn Provider, size: PhysicalSize<u32>, _: &Window) {
        self.softbuffer_surface
            .resize(
                NonZeroU32::new(size.width).unwrap(),
                NonZeroU32::new(size.height).unwrap(),
            )
            .unwrap();

        let width = size.width as i32;
        let height = size.height as i32;
        self.skia_surface = skia_safe::surfaces::raster_n32_premul((width, height)).unwrap();
    }
}
