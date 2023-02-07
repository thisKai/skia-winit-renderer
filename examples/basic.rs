use skia_winit_renderer::{run, App, AppCx, Window};

struct ExampleApp;
impl App for ExampleApp {
    fn resume(&self, mut app: AppCx) {
        app.create_window(MainWindow);
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
