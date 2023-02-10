pub(crate) mod gl {
    #![allow(clippy::all)]
    include!(concat!(env!("OUT_DIR"), "/gl_bindings.rs"));

    pub(crate) use Gles2 as Gl;
}

mod app;
mod skia;
mod window;

pub use skia_safe;
pub use {
    app::{run, App, AppCx},
    window::Window,
};
