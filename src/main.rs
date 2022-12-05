pub mod gl {
    #![allow(clippy::all)]
    include!(concat!(env!("OUT_DIR"), "/gl_bindings.rs"));

    pub use Gles2 as Gl;
}

mod app;
mod skia;
mod window;

use app::MultiWindowApplication;

pub fn main() {
    let app = MultiWindowApplication::new();
    app.run();
}
