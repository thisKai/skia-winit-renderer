use crate::{
    gl::{GlWindowManagerState, GlWindowRenderer},
    skia::{SkiaGlRenderer, SkiaSoftwareRenderer},
};
use skia_safe::Canvas;
use std::num::NonZeroU32;
use winit::{
    dpi::{PhysicalPosition, PhysicalSize},
    event::{MouseScrollDelta, TouchPhase},
    window::{Window as WinitWindow, WindowId},
};

#[allow(unused_variables)]
pub trait Window: 'static {
    fn open(&mut self) {}
    fn close(&mut self) -> bool {
        true
    }
    fn draw(&self, canvas: &mut Canvas) {}
    fn resize(&mut self, size: PhysicalSize<u32>) {}
    fn cursor_enter(&mut self) {}
    fn cursor_leave(&mut self) {}
    fn cursor_move(&mut self, position: PhysicalPosition<f64>) {}
    fn mouse_wheel(&mut self, delta: MouseScrollDelta, phase: TouchPhase) {}
}

pub(crate) trait SkiaWinitWindow {
    fn winit_window(&self) -> &WinitWindow;
    fn id(&self) -> WindowId {
        self.winit_window().id()
    }

    fn draw(&mut self, f: &mut dyn FnMut(&mut Canvas));
}

pub(crate) struct SoftwareWindow {
    skia: SkiaSoftwareRenderer,
    window: WinitWindow,
}
impl SoftwareWindow {
    pub(crate) fn new(skia: SkiaSoftwareRenderer, window: WinitWindow) -> Self {
        Self { skia, window }
    }
    pub(crate) fn resize(&mut self, size: PhysicalSize<u32>) {
        self.skia.resize(size);
    }
}
impl SkiaWinitWindow for SoftwareWindow {
    fn winit_window(&self) -> &WinitWindow {
        &self.window
    }

    fn draw(&mut self, f: &mut dyn FnMut(&mut Canvas)) {
        self.skia.draw(f);
    }
}

pub(crate) struct GlWindow {
    skia: SkiaGlRenderer,
    gl: GlWindowRenderer,
    window: WinitWindow,
}
impl GlWindow {
    pub(crate) fn new(skia: SkiaGlRenderer, gl: GlWindowRenderer, window: WinitWindow) -> Self {
        Self { skia, gl, window }
    }
    pub(crate) fn resize(&mut self, gl_state: &mut GlWindowManagerState, size: PhysicalSize<u32>) {
        self.gl.resize(
            NonZeroU32::new(size.width).unwrap(),
            NonZeroU32::new(size.height).unwrap(),
        );
        gl_state.resize_viewport(
            size.width.try_into().unwrap(),
            size.height.try_into().unwrap(),
        );
        self.skia.resize(&gl_state.gl_config, size);
    }
}
impl SkiaWinitWindow for GlWindow {
    fn winit_window(&self) -> &WinitWindow {
        &self.window
    }

    fn draw(&mut self, f: &mut dyn FnMut(&mut Canvas)) {
        self.gl.make_current_if_needed();
        self.skia.draw(f);
        self.gl.swap_buffers();
    }
}
