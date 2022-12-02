use std::num::NonZeroU32;

use glutin::{
    config::{Config, ConfigTemplateBuilder},
    context::{ContextApi, ContextAttributesBuilder, NotCurrentContext, PossiblyCurrentContext},
    display::{Display, GetGlDisplay},
    prelude::{GlConfig, GlDisplay, NotCurrentGlContextSurfaceAccessor, PossiblyCurrentGlContext},
    surface::{GlSurface, SwapInterval},
};
use glutin_winit::DisplayBuilder;
use raw_window_handle::HasRawWindowHandle;
use skia_safe::{colors, Paint};
use winit::{
    event::{Event, WindowEvent},
    event_loop::{EventLoop, EventLoopBuilder},
    window::{Window, WindowBuilder},
};

use crate::{skia::SkiaGlRenderer, window::GlWindow};

pub struct SingleWindowApplication {
    gl_config: Config,
    gl_display: Display,
    renderer: Option<SkiaGlRenderer>,
    not_current_gl_context: Option<NotCurrentContext>,
    state: Option<(PossiblyCurrentContext, GlWindow)>,
    window: Option<Window>,
    event_loop: EventLoop<()>,
}
impl SingleWindowApplication {
    pub fn new() -> Self {
        let event_loop = EventLoopBuilder::new().build();

        // Only windows requires the window to be present before creating the display.
        // Other platforms don't really need one.
        //
        // XXX if you don't care about running on android or so you can safely remove
        // this condition and always pass the window builder.
        let window_builder = if cfg!(wgl_backend) {
            Some(WindowBuilder::new().with_transparent(true))
        } else {
            None
        };

        // The template will match only the configurations supporting rendering to
        // windows.
        let template = ConfigTemplateBuilder::new().with_alpha_size(8);

        let display_builder = DisplayBuilder::new().with_window_builder(window_builder);

        let (window, gl_config) = display_builder
            .build(&event_loop, template, |configs| {
                // Find the config with the maximum number of samples, so our triangle will
                // be smooth.
                configs
                    .reduce(|accum, config| {
                        let transparency_check = config.supports_transparency().unwrap_or(false)
                            & !accum.supports_transparency().unwrap_or(false);

                        if transparency_check || config.num_samples() > accum.num_samples() {
                            config
                        } else {
                            accum
                        }
                    })
                    .unwrap()
            })
            .unwrap();

        println!("Picked a config with {} samples", gl_config.num_samples());

        let raw_window_handle = window.as_ref().map(|window| window.raw_window_handle());

        // XXX The display could be obtained from the any object created by it, so we
        // can query it from the config.
        let gl_display = gl_config.display();

        // The context creation part. It can be created before surface and that's how
        // it's expected in multithreaded + multiwindow operation mode, since you
        // can send NotCurrentContext, but not Surface.
        let context_attributes = ContextAttributesBuilder::new().build(raw_window_handle);

        // Since glutin by default tries to create OpenGL core context, which may not be
        // present we should try gles.
        let fallback_context_attributes = ContextAttributesBuilder::new()
            .with_context_api(ContextApi::Gles(None))
            .build(raw_window_handle);
        let not_current_gl_context = Some(unsafe {
            gl_display
                .create_context(&gl_config, &context_attributes)
                .unwrap_or_else(|_| {
                    gl_display
                        .create_context(&gl_config, &fallback_context_attributes)
                        .expect("failed to create context")
                })
        });

        Self {
            gl_config,
            gl_display,
            renderer: None,
            not_current_gl_context,
            state: None,
            window,
            event_loop,
        }
    }

    pub fn run(self) -> ! {
        let SingleWindowApplication {
            gl_config,
            gl_display,
            mut renderer,
            mut not_current_gl_context,
            mut state,
            mut window,
            event_loop,
        } = self;

        event_loop.run(move |event, window_target, control_flow| {
            control_flow.set_wait();
            match event {
                Event::Resumed => {
                    #[cfg(target_os = "android")]
                    println!("Android window available");

                    let window = window.take().unwrap_or_else(|| {
                        let window_builder = WindowBuilder::new().with_transparent(true);
                        glutin_winit::finalize_window(window_target, window_builder, &gl_config)
                            .unwrap()
                    });

                    let gl_window = GlWindow::new(window, &gl_config);

                    // Make it current.
                    let gl_context = not_current_gl_context
                        .take()
                        .unwrap()
                        .make_current(&gl_window.surface)
                        .unwrap();

                    // The context needs to be current for the Renderer to set up shaders and
                    // buffers. It also performs function loading, which needs a current context on
                    // WGL.
                    renderer.get_or_insert_with(|| {
                        SkiaGlRenderer::new(&gl_config, &gl_display, gl_window.window.inner_size())
                    });

                    // Try setting vsync.
                    if let Err(res) = gl_window.surface.set_swap_interval(
                        &gl_context,
                        SwapInterval::Wait(NonZeroU32::new(1).unwrap()),
                    ) {
                        eprintln!("Error setting vsync: {:?}", res);
                    }

                    assert!(state.replace((gl_context, gl_window)).is_none());
                }
                Event::Suspended => {
                    // This event is only raised on Android, where the backing NativeWindow for a GL
                    // Surface can appear and disappear at any moment.
                    println!("Android window removed");

                    // Destroy the GL Surface and un-current the GL Context before ndk-glue releases
                    // the window back to the system.
                    let (gl_context, _) = state.take().unwrap();
                    assert!(not_current_gl_context
                        .replace(gl_context.make_not_current().unwrap())
                        .is_none());
                }
                Event::WindowEvent { event, .. } => match event {
                    WindowEvent::Resized(size) => {
                        if size.width != 0 && size.height != 0 {
                            // Some platforms like EGL require resizing GL surface to update the size
                            // Notable platforms here are Wayland and macOS, other don't require it
                            // and the function is no-op, but it's wise to resize it for portability
                            // reasons.
                            if let Some((gl_context, gl_window)) = &state {
                                gl_window.surface.resize(
                                    gl_context,
                                    NonZeroU32::new(size.width).unwrap(),
                                    NonZeroU32::new(size.height).unwrap(),
                                );
                                let renderer = renderer.as_mut().unwrap();
                                renderer.resize(&gl_config, size);
                            }
                        }
                    }
                    WindowEvent::CloseRequested => {
                        control_flow.set_exit();
                    }
                    _ => (),
                },
                Event::RedrawRequested(_) => {
                    if let Some((gl_context, gl_window)) = &state {
                        let renderer = renderer.as_mut().unwrap();
                        renderer.draw(|canvas| {
                            canvas.draw_circle((200, 200), 50., &Paint::new(colors::CYAN, None));
                        });

                        gl_window.surface.swap_buffers(gl_context).unwrap();
                    }
                }
                _ => (),
            }
        })
    }
}
