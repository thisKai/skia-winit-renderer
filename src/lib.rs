pub mod d3d12;
pub mod dcomp;
pub mod generic;
mod gl;
mod manager;
pub mod opengl;
mod skia;
pub mod softbuffer;
mod software;
mod window;
pub mod windows_ui_composition;

pub use skia_safe;
pub use {
    manager::{ManagedWindow, WindowManager},
    window::{Window, WindowCx},
};
