use crate::window::{GlWindowManagerState, SkiaGlWinitWindow, SkiaWinitWindowManager, Window};
use winit::{
    event::Event,
    event_loop::{EventLoop, EventLoopBuilder, EventLoopWindowTarget},
};

#[allow(unused_variables)]
pub trait App: 'static {
    fn resume(&self, cx: AppCx) {}
}

pub fn run<T: App>(app: T) -> ! {
    let runtime = MultiWindowApplication::new();
    runtime.start(app)
}

pub struct MultiWindowApplication {
    window_manager: SkiaWinitWindowManager<SkiaGlWinitWindow>,
    event_loop: Option<EventLoop<()>>,
}
impl MultiWindowApplication {
    fn new() -> Self {
        let event_loop = EventLoopBuilder::new().build();
        Self {
            window_manager: SkiaWinitWindowManager::new(GlWindowManagerState::new(&event_loop)),
            event_loop: Some(event_loop),
        }
    }
    fn context<'a>(&'a mut self, window_target: &'a EventLoopWindowTarget<()>) -> AppCx<'a> {
        AppCx {
            window_target,
            app: self,
        }
    }
    fn start<T: App>(mut self, app: T) -> ! {
        self.event_loop
            .take()
            .unwrap()
            .run(move |event, window_target, control_flow| {
                control_flow.set_wait();
                match event {
                    Event::Resumed => {
                        app.resume(self.context(window_target));
                    }

                    Event::WindowEvent { window_id, event } => self
                        .window_manager
                        .handle_window_event(window_id, event, window_target, control_flow),
                    Event::RedrawRequested(window_id) => self.window_manager.draw(&window_id),
                    _ => (),
                }
            })
    }
}

pub struct AppCx<'a> {
    window_target: &'a EventLoopWindowTarget<()>,
    app: &'a mut MultiWindowApplication,
}
impl<'a> AppCx<'a> {
    pub fn spawn_window<T: Window>(&mut self, window: T) {
        self.app
            .window_manager
            .create_window(self.window_target, Box::new(window));
    }
}
