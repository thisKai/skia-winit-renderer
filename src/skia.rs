use std::num::NonZeroU32;

use skia_safe::Canvas;

pub(crate) trait SkiaRenderer {
    type ResizeDependency;

    fn draw(&mut self, f: &mut dyn FnMut(&Canvas));

    fn resize(
        &mut self,
        dependency: &Self::ResizeDependency,
        width: NonZeroU32,
        height: NonZeroU32,
    );
}
