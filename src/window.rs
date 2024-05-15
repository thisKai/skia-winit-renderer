use crate::{
    gl::{GlWindowManagerState, SkiaGlRenderer},
    skia::SkiaRenderer,
    software::SkiaSoftwareRenderer,
};
use skia_safe::Canvas;
use winit::{
    dpi::{PhysicalPosition, PhysicalSize},
    event::{ElementState, MouseButton, MouseScrollDelta, TouchPhase},
    event_loop::EventLoopWindowTarget,
    window::{Window as WinitWindow, WindowId},
};

#[allow(unused_variables)]
pub trait Window: 'static {
    fn open(&mut self, cx: &WindowCx) {}
    fn close(&mut self, cx: &WindowCx) -> bool {
        true
    }
    fn draw(&mut self, canvas: &Canvas, cx: &WindowCx) {}
    fn after_draw(&mut self, cx: &WindowCx, window_target: &EventLoopWindowTarget<()>) {}
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

    fn draw(&mut self, f: &mut dyn FnMut(&Canvas, &WinitWindow));
}

pub(crate) struct SkiaWindow<S> {
    skia: S,
    pub(crate) window: WinitWindow,
}
impl<S> SkiaWindow<S> {
    pub(crate) fn id(&self) -> WindowId {
        self.window.id()
    }
}
impl SkiaWindow<SkiaSoftwareRenderer> {
    pub(crate) fn software(skia: SkiaSoftwareRenderer, window: WinitWindow) -> Self {
        Self { skia, window }
    }

    pub(crate) fn resize(&mut self, size: PhysicalSize<u32>) {
        self.skia.resize(
            size.width.try_into().unwrap(),
            size.height.try_into().unwrap(),
        )
    }
}
impl SkiaWindow<SkiaGlRenderer> {
    pub(crate) fn gl(skia: SkiaGlRenderer, window: WinitWindow) -> Self {
        Self { skia, window }
    }
}
impl<S: SkiaRenderer> SkiaWindow<S> {
    pub(crate) fn resize_dependent(
        &mut self,
        dependency: &S::ResizeDependency,
        size: PhysicalSize<u32>,
    ) {
        self.skia.resize(
            dependency,
            size.width.try_into().unwrap(),
            size.height.try_into().unwrap(),
        )
    }
    pub(crate) fn draw(&mut self, f: &mut dyn FnMut(&Canvas, &WinitWindow)) {
        self.skia.draw(&mut |canvas| f(canvas, &self.window));
    }
}
impl<S: SkiaRenderer> SkiaWinitWindow for SkiaWindow<S> {
    fn winit_window(&self) -> &WinitWindow {
        &self.window
    }

    fn draw(&mut self, f: &mut dyn FnMut(&Canvas, &WinitWindow)) {
        self.draw(f);
    }
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
        self.skia.resize(
            size.width.try_into().unwrap(),
            size.height.try_into().unwrap(),
        );
    }
}
impl SkiaWinitWindow for SoftwareWindow {
    fn winit_window(&self) -> &WinitWindow {
        &self.window
    }

    fn draw(&mut self, f: &mut dyn FnMut(&Canvas, &WinitWindow)) {
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
    pub(crate) fn resize(&mut self, gl_state: &GlWindowManagerState, size: PhysicalSize<u32>) {
        self.skia.resize(gl_state, size.width, size.height)
    }
}
impl SkiaWinitWindow for GlWindow {
    fn winit_window(&self) -> &WinitWindow {
        &self.window
    }

    fn draw(&mut self, f: &mut dyn FnMut(&Canvas, &WinitWindow)) {
        self.skia.draw(|canvas| f(canvas, &self.window));
    }
}
