use std::{num::NonZeroU32, rc::Rc};

use glutin::{
    config::{Config, ConfigTemplateBuilder},
    context::{ContextApi, ContextAttributesBuilder, NotCurrentContext},
    display::{Display, GetGlDisplay},
    prelude::*,
    surface::{GlSurface, SwapInterval},
};
use glutin_winit::DisplayBuilder;
use raw_window_handle::HasRawWindowHandle;
use skia_safe::{colors, Paint};
use winit::{
    event::{ElementState, Event, KeyboardInput, VirtualKeyCode, WindowEvent},
    event_loop::{EventLoop, EventLoopBuilder, EventLoopWindowTarget},
    window::{Window, WindowBuilder},
};

use crate::{
    skia::SkiaGlRenderer,
    window::{GlWindow, GlWindowManager, SkiaGlAppWindow},
};

pub struct SingleWindowApplication {
    gl_config: Config,
    gl_display: Display,
    renderer: Option<SkiaGlRenderer>,
    not_current_gl_context: Option<NotCurrentContext>,
    state: Option<GlWindow>,
    window: Option<Window>,
    event_loop: Option<EventLoop<()>>,
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
            event_loop: Some(event_loop),
        }
    }

    pub fn run(mut self) -> ! {
        self.event_loop
            .take()
            .unwrap()
            .run(move |event, window_target, control_flow| {
                control_flow.set_wait();
                match event {
                    Event::Resumed => {
                        #[cfg(target_os = "android")]
                        println!("Android window available");

                        let window = self.window.take().unwrap_or_else(|| {
                            let window_builder = WindowBuilder::new().with_transparent(true);
                            glutin_winit::finalize_window(
                                window_target,
                                window_builder,
                                &self.gl_config,
                            )
                            .unwrap()
                        });

                        let gl_window = GlWindow::new(
                            window,
                            &self.gl_config,
                            self.not_current_gl_context.take().unwrap(),
                        );

                        // Make it current.
                        // let gl_context = self
                        //     .not_current_gl_context
                        //     .take()
                        //     .unwrap()
                        //     .make_current(&gl_window.surface)
                        //     .unwrap();

                        // The context needs to be current for the Renderer to set up shaders and
                        // buffers. It also performs function loading, which needs a current context on
                        // WGL.
                        self.renderer.get_or_insert_with(|| {
                            SkiaGlRenderer::new(
                                &self.gl_config,
                                &self.gl_display,
                                gl_window.window.inner_size(),
                            )
                        });

                        // Try setting vsync.
                        if let Err(res) = gl_window.surface.set_swap_interval(
                            &gl_window.gl_context(),
                            SwapInterval::Wait(NonZeroU32::new(1).unwrap()),
                        ) {
                            eprintln!("Error setting vsync: {:?}", res);
                        }

                        assert!(self.state.replace(gl_window).is_none());
                    }
                    Event::Suspended => {
                        // This event is only raised on Android, where the backing NativeWindow for a GL
                        // Surface can appear and disappear at any moment.
                        println!("Android window removed");

                        // Destroy the GL Surface and un-current the GL Context before ndk-glue releases
                        // the window back to the system.
                        let gl_window = self.state.take().unwrap();
                        assert!(self
                            .not_current_gl_context
                            .replace(gl_window.make_not_current())
                            .is_none());
                    }
                    Event::WindowEvent { event, .. } => match event {
                        WindowEvent::Resized(size) => {
                            if size.width != 0 && size.height != 0 {
                                // Some platforms like EGL require resizing GL surface to update the size
                                // Notable platforms here are Wayland and macOS, other don't require it
                                // and the function is no-op, but it's wise to resize it for portability
                                // reasons.
                                if let Some(gl_window) = &self.state {
                                    gl_window.resize(
                                        NonZeroU32::new(size.width).unwrap(),
                                        NonZeroU32::new(size.height).unwrap(),
                                    );
                                    let renderer = self.renderer.as_mut().unwrap();
                                    renderer.resize(&self.gl_config, size);
                                }
                            }
                        }
                        WindowEvent::CloseRequested => {
                            control_flow.set_exit();
                        }
                        _ => (),
                    },
                    Event::RedrawRequested(_) => {
                        if let Some(gl_window) = &self.state {
                            let renderer = self.renderer.as_mut().unwrap();
                            renderer.draw(|canvas| {
                                canvas.draw_circle(
                                    (200, 200),
                                    50.,
                                    &Paint::new(colors::CYAN, None),
                                );
                            });

                            gl_window.swap_buffers();
                        }
                    }
                    _ => (),
                }
            })
    }
}

#[allow(unused_variables)]
pub trait App: 'static {
    fn resume(&self, app: AppCx) {}
}

pub fn run<T: App>(app: T) -> ! {
    let runtime = MultiWindowApplication::new();
    runtime.start(app)
}

pub struct MultiWindowApplication {
    window_manager: GlWindowManager,
    event_loop: Option<EventLoop<()>>,
}
impl MultiWindowApplication {
    fn new() -> Self {
        let event_loop = EventLoopBuilder::new().build();
        Self {
            window_manager: GlWindowManager::new(&event_loop),
            event_loop: Some(event_loop),
        }
    }
    fn context<'a>(&'a mut self, window_target: &'a EventLoopWindowTarget<()>) -> AppCx<'a> {
        AppCx {
            window_target,
            app: self,
        }
    }
    fn start<T: App>(mut self, app: T) -> ! {
        self.event_loop
            .take()
            .unwrap()
            .run(move |event, window_target, control_flow| {
                control_flow.set_wait();
                match event {
                    Event::Resumed => {
                        app.resume(self.context(window_target));
                    }

                    Event::WindowEvent { window_id, event } => match event {
                        WindowEvent::Resized(size) => self.window_manager.resize(&window_id, size),
                        WindowEvent::KeyboardInput {
                            device_id,
                            input:
                                KeyboardInput {
                                    virtual_keycode: Some(VirtualKeyCode::Return),
                                    state: ElementState::Released,
                                    ..
                                },
                            is_synthetic,
                        } => {
                            self.window_manager.create_window(window_target);
                        }
                        WindowEvent::CloseRequested => {
                            if self.window_manager.close_window(&window_id) {
                                control_flow.set_exit();
                            }
                        }
                        _ => (),
                    },
                    Event::RedrawRequested(window_id) => self.window_manager.draw(&window_id),
                    _ => (),
                }
            })
    }
}

pub struct AppCx<'a> {
    window_target: &'a EventLoopWindowTarget<()>,
    app: &'a mut MultiWindowApplication,
}
impl<'a> AppCx<'a> {
    pub fn create_window(&mut self) -> Rc<SkiaGlAppWindow> {
        self.app.window_manager.create_window(self.window_target)
    }
}
