use skia_safe::{colors, Paint};
use skia_winit_renderer::{d3d12::D3d12Backend, generic::WindowManager};
use winit::{
    event::{Event, WindowEvent},
    event_loop::EventLoopBuilder,
    window::WindowBuilder,
};

fn main() {
    let event_loop = EventLoopBuilder::new().build().unwrap();

    let mut window_manager = WindowManager::new();

    event_loop
        .run(|event, elwt| match event {
            Event::Resumed => {
                window_manager
                    .create::<D3d12Backend, _>(elwt, WindowBuilder::new().with_transparent(true))
                    .unwrap();
            }
            Event::WindowEvent { window_id, event } => match event {
                WindowEvent::CloseRequested => {
                    window_manager.remove(&window_id);
                    elwt.exit();
                }
                WindowEvent::RedrawRequested => {
                    window_manager.draw(&window_id, |canvas, window| {
                        canvas.clear(colors::TRANSPARENT);

                        let size = window.inner_size();

                        canvas.draw_circle(
                            ((size.width / 2) as i32, (size.height / 2) as i32),
                            size.width.min(size.height) as f32 / 2.0,
                            &Paint::new(colors::CYAN, None),
                        );
                    });
                }
                WindowEvent::Resized(size) => {
                    window_manager.resize(&window_id, size);
                }
                _ => {}
            },
            _ => {}
        })
        .unwrap();
}
