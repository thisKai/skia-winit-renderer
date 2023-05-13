use crate::{
    gl::{GlWindowManagerState, SkiaGlRenderer},
    software::SkiaSoftwareRenderer,
};
use skia_safe::Canvas;
use winit::{
    dpi::{PhysicalPosition, PhysicalSize},
    event::{ElementState, MouseButton, MouseScrollDelta, TouchPhase},
    event_loop::ControlFlow,
    window::{Window as WinitWindow, WindowId},
};

#[allow(unused_variables)]
pub trait Window: 'static {
    fn open(&mut self, cx: &WindowCx) {}
    fn close(&mut self, cx: &WindowCx) -> bool {
        true
    }
    fn draw(&mut self, canvas: &mut Canvas, cx: &WindowCx) {}
    fn after_draw(&mut self, cx: &WindowCx, control_flow: &mut ControlFlow) {}
    fn resize(&mut self, size: PhysicalSize<u32>, cx: &WindowCx) {}
    fn cursor_enter(&mut self, cx: &WindowCx) {}
    fn cursor_leave(&mut self, cx: &WindowCx) {}
    fn cursor_move(&mut self, position: PhysicalPosition<f64>, cx: &WindowCx) {}
    fn mouse_input(&mut self, state: ElementState, button: MouseButton, cx: &WindowCx) {}
    fn mouse_wheel(&mut self, delta: MouseScrollDelta, phase: TouchPhase, cx: &WindowCx) {}
}

pub struct WindowCx<'a> {
    pub window: &'a WinitWindow,
}

pub(crate) trait SkiaWinitWindow {
    fn winit_window(&self) -> &WinitWindow;
    fn id(&self) -> WindowId {
        self.winit_window().id()
    }

    fn draw(&mut self, f: &mut dyn FnMut(&mut Canvas, &WinitWindow));
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

    fn draw(&mut self, f: &mut dyn FnMut(&mut Canvas, &WinitWindow)) {
        self.skia.draw(|canvas| f(canvas, &self.window));
    }
}

pub(crate) struct GlWindow {
    skia: SkiaGlRenderer,
    window: WinitWindow,
}
impl GlWindow {
    pub(crate) fn new(skia: SkiaGlRenderer, window: WinitWindow) -> Self {
        Self { skia, window }
    }
    pub(crate) fn resize(&mut self, gl_state: &mut GlWindowManagerState, size: PhysicalSize<u32>) {
        self.skia.resize(gl_state, size.width, size.height)
    }
}
impl SkiaWinitWindow for GlWindow {
    fn winit_window(&self) -> &WinitWindow {
        &self.window
    }

    fn draw(&mut self, f: &mut dyn FnMut(&mut Canvas, &WinitWindow)) {
        self.skia.draw(|canvas| f(canvas, &self.window));
    }
}
