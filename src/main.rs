pub mod gl {
    #![allow(clippy::all)]
    include!(concat!(env!("OUT_DIR"), "/gl_bindings.rs"));

    pub use Gles2 as Gl;
}

mod app;
mod skia;
mod window;

use app::{App, AppCx};

struct ExampleApp;
impl App for ExampleApp {
    fn resume(&self, mut app: AppCx) {
        app.create_window();
    }
}

pub fn main() {
    app::run(ExampleApp)
}
