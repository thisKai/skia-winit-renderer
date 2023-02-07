pub mod gl {
    #![allow(clippy::all)]
    include!(concat!(env!("OUT_DIR"), "/gl_bindings.rs"));

    pub use Gles2 as Gl;
}

mod app;
mod skia;
mod window;

use app::{App, AppCx};
use window::Window;

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
    app::run(ExampleApp)
}
