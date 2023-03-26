use skia_winit_renderer::{run, App, AppCx, Window};
use winit::window::WindowBuilder;

struct ExampleApp;
impl App for ExampleApp {
    fn resume(&self, mut cx: AppCx) {
        cx.spawn_window(MainWindow, WindowBuilder::new().with_transparent(true));
        cx.spawn_window(MainWindow, WindowBuilder::new().with_transparent(true));
    }
}

struct MainWindow;
impl Window for MainWindow {
    fn draw(&self, canvas: &mut skia_safe::Canvas) {
        canvas.draw_circle(
            (200, 200),
            50.,
            &skia_safe::Paint::new(skia_safe::colors::CYAN, None),
        );
    }
}

pub fn main() {
    run(ExampleApp)
}
