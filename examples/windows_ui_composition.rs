use skia_safe::{colors, Paint};
use skia_winit_renderer::windows_ui_composition::WindowsUiCompositionWindowManager;
use winit::{
    event::{Event, WindowEvent},
    event_loop::EventLoopBuilder,
    window::WindowBuilder,
};

fn main() {
    let event_loop = EventLoopBuilder::new().build().unwrap();

    let mut window_manager = WindowsUiCompositionWindowManager::new().unwrap();

    event_loop
        .run(|event, elwt| match event {
            Event::Resumed => {
                let window_id = window_manager
                    .create_window(elwt, WindowBuilder::new().with_transparent(true))
                    .unwrap();
                window_manager
                    .draw(&window_id, |canvas, window| {
                        canvas.clear(colors::TRANSPARENT);

                        let size = window.inner_size();

                        canvas.draw_circle(
                            ((size.width / 2) as i32, (size.height / 2) as i32),
                            size.width.min(size.height) as f32 / 2.0,
                            &Paint::new(colors::CYAN, None),
                        );
                    })
                    .unwrap();
            }
            Event::WindowEvent { window_id, event } => match event {
                WindowEvent::CloseRequested => {
                    window_manager.remove_window(&window_id);
                    elwt.exit();
                }
                WindowEvent::RedrawRequested => {
                    // window_manager
                    //     .draw(&window_id, |canvas, window| {
                    //         canvas.clear(colors::TRANSPARENT);

                    //         let size = window.inner_size();

                    //         canvas.draw_circle(
                    //             ((size.width / 2) as i32, (size.height / 2) as i32),
                    //             size.width.min(size.height) as f32 / 2.0,
                    //             &Paint::new(colors::CYAN, None),
                    //         );
                    //     })
                    //     .unwrap();
                    window_manager
                        .draw(&window_id, |canvas, window| {
                            canvas.clear(colors::TRANSPARENT);

                            let size = window.inner_size();

                            canvas.draw_circle(
                                ((size.width / 2) as i32, (size.height / 2) as i32),
                                size.width.min(size.height) as f32 / 2.0,
                                &Paint::new(colors::CYAN, None),
                            );
                        })
                        .unwrap();
                }
                WindowEvent::Resized(size) => {
                    window_manager.resize_window(&window_id, size).unwrap();
                }
                _ => {}
            },
            _ => {}
        })
        .unwrap();
}
