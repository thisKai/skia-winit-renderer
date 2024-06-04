use std::{collections::HashMap, num::NonZeroU32};

use raw_window_handle::{
    HasRawDisplayHandle, HasRawWindowHandle, RawDisplayHandle, RawWindowHandle,
};
use skia_safe::{Canvas, Surface};
use softbuffer::{Context, SoftBufferError, Surface as SoftBufferSurface};
use winit::{
    dpi::PhysicalSize,
    error::OsError,
    event_loop::EventLoopWindowTarget,
    window::{Window, WindowBuilder, WindowId},
};

use crate::generic::{Env, RenderWindow, SkiaGraphicsEnv, SkiaRender};

pub struct SoftBufferWindowManager {
    env: SoftBufferEnv,
    windows: HashMap<WindowId, SoftBufferWindow>,
}
impl SoftBufferWindowManager {
    pub fn new<D: HasRawDisplayHandle>(display: &D) -> Result<Self, SoftBufferError> {
        Ok(Self {
            env: SoftBufferEnv::new(display.raw_display_handle())?,
            windows: HashMap::new(),
        })
    }
    pub fn create_window<T>(
        &mut self,
        elwt: &EventLoopWindowTarget<T>,
        builder: WindowBuilder,
    ) -> Result<WindowId, CreateWindowError> {
        let window = self.create_window_object(elwt, builder)?;
        let window_id = window.winit_window.id();

        self.windows.insert(window_id, window);

        Ok(window_id)
    }
    pub fn get_window(&self, window_id: &WindowId) -> Option<&SoftBufferWindow> {
        self.windows.get(window_id)
    }
    pub fn get_window_mut(&mut self, window_id: &WindowId) -> Option<&mut SoftBufferWindow> {
        self.windows.get_mut(window_id)
    }
    pub fn remove_window(&mut self, window_id: &WindowId) -> Option<SoftBufferWindow> {
        self.windows.remove(window_id)
    }
    pub fn draw(&mut self, window_id: &WindowId, f: impl FnMut(&Canvas, &Window)) {
        let Some(window) = self.windows.get_mut(window_id) else {
            return;
        };
        window.draw(f);
    }
    pub fn resize_window(&mut self, window_id: &WindowId, size: PhysicalSize<u32>) {
        let Some(window) = self.windows.get_mut(window_id) else {
            return;
        };
        window.resize(
            NonZeroU32::new(size.width).unwrap(),
            NonZeroU32::new(size.height).unwrap(),
        );
    }
    pub fn create_window_object<T>(
        &mut self,
        elwt: &EventLoopWindowTarget<T>,
        builder: WindowBuilder,
    ) -> Result<SoftBufferWindow, CreateWindowError> {
        let window = builder
            .build(elwt)
            .map_err(CreateWindowError::CreateWindow)?;
        let size: (i32, i32) = window.inner_size().into();

        let softbuffer_surface = self
            .env
            .create_surface(window.raw_window_handle())
            .map_err(CreateWindowError::SoftBuffer)?;

        let skia_surface = skia_safe::surfaces::raster_n32_premul(size).unwrap();

        Ok(SoftBufferWindow {
            skia_surface,
            softbuffer_surface,
            winit_window: window,
        })
    }
}

#[derive(Debug)]
pub enum CreateWindowError {
    CreateWindow(OsError),
    SoftBuffer(SoftBufferError),
}

pub struct SoftBufferEnv {
    context: Context,
}
impl SoftBufferEnv {
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
impl SkiaGraphicsEnv for SoftBufferEnv {
    type Error = CreateWindowError;

    fn create<D: HasRawDisplayHandle>(display: &D) -> Self {
        Self::new(display.raw_display_handle()).unwrap()
    }
    fn create_window<WinitUserEvent>(
        &mut self,
        elwt: &EventLoopWindowTarget<WinitUserEvent>,
        builder: WindowBuilder,
    ) -> Result<crate::generic::RenderWindow, Self::Error> {
        let window = builder
            .build(elwt)
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

pub struct SoftBufferWindow {
    skia_surface: Surface,
    softbuffer_surface: SoftBufferSurface,
    winit_window: Window,
}
impl SoftBufferWindow {
    fn draw(&mut self, mut f: impl FnMut(&Canvas, &Window)) {
        {
            let canvas = self.skia_surface.canvas();
            f(canvas, &self.winit_window);
        }

        let snapshot = self.skia_surface.image_snapshot();

        let peek = snapshot.peek_pixels().unwrap();
        let pixels: &[u32] = peek.pixels().unwrap();

        let mut buffer = self.softbuffer_surface.buffer_mut().unwrap();
        buffer.copy_from_slice(pixels);
        buffer.present().unwrap();
    }
    pub(crate) fn resize(&mut self, width: NonZeroU32, height: NonZeroU32) {
        self.softbuffer_surface.resize(width, height).unwrap();

        let width = width.get() as i32;
        let height = height.get() as i32;
        self.skia_surface = skia_safe::surfaces::raster_n32_premul((width, height)).unwrap();
    }
}

pub struct SkiaSoftBufferRenderer {
    skia_surface: Surface,
    softbuffer_surface: SoftBufferSurface,
}
impl SkiaRender for SkiaSoftBufferRenderer {
    fn prepare_and_get_surface(&mut self, _: &mut Env) -> &mut Surface {
        &mut self.skia_surface
    }

    fn present(&mut self, _: &mut Env) {
        let snapshot = self.skia_surface.image_snapshot();

        let peek = snapshot.peek_pixels().unwrap();
        let pixels: &[u32] = peek.pixels().unwrap();

        let mut buffer = self.softbuffer_surface.buffer_mut().unwrap();
        buffer.copy_from_slice(pixels);
        buffer.present().unwrap();
    }
    fn resize(&mut self, env: &mut Env, size: PhysicalSize<u32>, _: &Window) {
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
