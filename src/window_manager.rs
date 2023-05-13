use crate::{
    gl::{GlWindowManagerState, SkiaGlRenderer},
    software::SkiaSoftwareRenderer,
    window::{GlWindow, SkiaWinitWindow, SoftwareWindow, Window, WindowCx},
};
use glutin::config::Config;
use raw_window_handle::HasRawWindowHandle;
use softbuffer::GraphicsContext;
use std::{collections::HashMap, iter};
use winit::{
    dpi::{PhysicalPosition, PhysicalSize},
    error::OsError,
    event::{ElementState, MouseButton, MouseScrollDelta, TouchPhase, WindowEvent},
    event_loop::{ControlFlow, EventLoopWindowTarget},
    window::{Window as WinitWindow, WindowBuilder, WindowId},
};

enum WindowManagerState {
    Init,
    Software {
        windows: WindowMap<SoftwareWindow>,
    },
    Gl {
        state: GlWindowManagerState,
        windows: WindowMap<GlWindow>,
    },
}

type WindowMap<W> = HashMap<WindowId, (W, Box<dyn Window>)>;

pub struct WindowManager {
    state: WindowManagerState,
}
impl WindowManager {
    pub(crate) fn new() -> Self {
        Self {
            state: WindowManagerState::Init,
        }
    }

    pub(crate) fn draw(&mut self, id: &WindowId) {
        let (window, window_state) = self.get_window_mut(id).unwrap();

        window.draw(&mut |canvas, window| window_state.draw(canvas, &WindowCx { window }));
    }
    pub fn redraw_events_cleared(&mut self, control_flow: &mut ControlFlow) {
        for (window, window_state) in self.iter_windows_mut() {
            window_state.after_draw(
                &WindowCx {
                    window: window.winit_window(),
                },
                control_flow,
            )
        }
    }
    pub fn handle_window_event(
        &mut self,
        window_id: WindowId,
        event: WindowEvent,
        _window_target: &EventLoopWindowTarget<()>,
        control_flow: &mut ControlFlow,
    ) {
        match event {
            WindowEvent::Resized(size) => self.resize(&window_id, size),
            WindowEvent::CloseRequested => {
                if self.close_window(&window_id) {
                    control_flow.set_exit();
                }
            }
            WindowEvent::CursorEntered { .. } => self.cursor_enter(&window_id),
            WindowEvent::CursorLeft { .. } => self.cursor_leave(&window_id),
            WindowEvent::CursorMoved { position, .. } => self.cursor_move(&window_id, position),
            WindowEvent::MouseInput { state, button, .. } => {
                self.mouse_input(&window_id, state, button)
            }
            WindowEvent::MouseWheel { delta, phase, .. } => {
                self.mouse_wheel(&window_id, delta, phase)
            }
            _ => (),
        }
    }
    pub fn resize(&mut self, id: &WindowId, size: PhysicalSize<u32>) {
        let (winit_window, window_state) = match &mut self.state {
            WindowManagerState::Init => {
                panic!("Uninitialized window manager");
            }
            WindowManagerState::Software { windows } => {
                let (window, window_state) = windows.get_mut(id).unwrap();

                window.resize(size);

                (window.winit_window(), &mut **window_state)
            }
            WindowManagerState::Gl { state, windows } => {
                let (window, window_state) = windows.get_mut(id).unwrap();

                window.resize(state, size);

                (window.winit_window(), &mut **window_state)
            }
        };

        window_state.resize(
            size,
            &WindowCx {
                window: winit_window,
            },
        );

        if size.width != 0 && size.height != 0 {
            winit_window.request_redraw();
        }
    }

    pub fn close_window(&mut self, id: &WindowId) -> bool {
        match &mut self.state {
            WindowManagerState::Init => panic!("Uninitialized window manager"),
            WindowManagerState::Software { windows } => {
                windows.remove(&id);
                dbg!("close");
                windows.is_empty()
            }
            WindowManagerState::Gl { windows, .. } => {
                windows.remove(&id);
                dbg!("close");
                windows.is_empty()
            }
        }
    }

    pub fn cursor_enter(&mut self, id: &WindowId) {
        let (window, state) = self.get_window_mut(id).unwrap();
        state.cursor_enter(&WindowCx {
            window: window.winit_window(),
        });
    }
    pub fn cursor_leave(&mut self, id: &WindowId) {
        let (window, state) = self.get_window_mut(id).unwrap();
        state.cursor_leave(&WindowCx {
            window: window.winit_window(),
        });
    }
    pub fn cursor_move(&mut self, id: &WindowId, position: PhysicalPosition<f64>) {
        let (window, state) = self.get_window_mut(id).unwrap();
        state.cursor_move(
            position,
            &WindowCx {
                window: window.winit_window(),
            },
        )
    }
    pub fn mouse_input(&mut self, id: &WindowId, button_state: ElementState, button: MouseButton) {
        let (window, state) = self.get_window_mut(id).unwrap();
        state.mouse_input(
            button_state,
            button,
            &WindowCx {
                window: window.winit_window(),
            },
        );
    }
    pub fn mouse_wheel(&mut self, id: &WindowId, delta: MouseScrollDelta, phase: TouchPhase) {
        let (window, state) = self.get_window_mut(id).unwrap();
        state.mouse_wheel(
            delta,
            phase,
            &WindowCx {
                window: window.winit_window(),
            },
        );
    }

    fn get_window_mut(
        &mut self,
        id: &WindowId,
    ) -> Option<(&mut dyn SkiaWinitWindow, &mut dyn Window)> {
        match &mut self.state {
            WindowManagerState::Init => None,
            WindowManagerState::Software { windows } => {
                let (window, window_state) = windows.get_mut(id)?;

                Some((window, &mut **window_state))
            }
            WindowManagerState::Gl { windows, .. } => {
                let (window, window_state) = windows.get_mut(id)?;

                Some((window, &mut **window_state))
            }
        }
    }
    fn iter_windows_mut(
        &mut self,
    ) -> Box<dyn Iterator<Item = (&mut dyn SkiaWinitWindow, &mut dyn Window)> + '_> {
        match &mut self.state {
            WindowManagerState::Init => Box::new(iter::empty()),
            WindowManagerState::Software { windows } => Box::new(
                windows
                    .values_mut()
                    .map(|(window, window_state)| (window as _, &mut **window_state)),
            ),
            WindowManagerState::Gl { windows, .. } => Box::new(
                windows
                    .values_mut()
                    .map(|(window, window_state)| (window as _, &mut **window_state)),
            ),
        }
    }

    pub(crate) fn create_window(
        &mut self,
        window_target: &EventLoopWindowTarget<()>,
        window_builder: WindowBuilder,
        mut window_state: Box<dyn Window>,
    ) -> WindowId {
        match &mut self.state {
            state @ WindowManagerState::Init => {
                let gl_state_and_first_window =
                    GlWindowManagerState::create_with_first_winit_window(
                        window_target,
                        &window_builder,
                    )
                    .map_err(|err| (err, None))
                    .and_then(|(gl_state, first_window)| {
                        let window = Self::create_gl_window(
                            window_target,
                            &gl_state,
                            first_window
                                .map(InitWindow::First)
                                .unwrap_or(InitWindow::Other(window_builder.clone())),
                        )
                        .map_err(|(err, window)| (err.into(), Some(window)))?;

                        Ok((gl_state, window))
                    });

                match gl_state_and_first_window {
                    Ok((gl_state, window)) => {
                        let id = window.id();

                        let mut windows = HashMap::new();

                        Self::init_window(window.winit_window(), &mut *window_state);
                        windows.insert(id, (window, window_state));

                        *state = WindowManagerState::Gl {
                            state: gl_state,
                            windows,
                        };
                        id
                    }
                    Err((_err, window)) => {
                        let window = Self::create_software_window(
                            window_target,
                            window
                                .map(InitWindow::First)
                                .unwrap_or(InitWindow::Other(window_builder)),
                        );
                        let id = window.id();

                        let mut windows = HashMap::new();

                        Self::init_window(window.winit_window(), &mut *window_state);
                        windows.insert(id, (window, window_state));

                        *state = WindowManagerState::Software { windows };
                        id
                    }
                }
            }
            WindowManagerState::Software { windows } => {
                let window =
                    Self::create_software_window(window_target, InitWindow::Other(window_builder));
                let id = window.id();

                Self::init_window(window.winit_window(), &mut *window_state);
                windows.insert(id, (window, window_state));

                id
            }
            WindowManagerState::Gl { state, windows } => {
                let window =
                    Self::create_gl_window(window_target, state, InitWindow::Other(window_builder))
                        .unwrap();
                let id = window.id();

                Self::init_window(window.winit_window(), &mut *window_state);
                windows.insert(id, (window, window_state));

                id
            }
        }
    }

    fn init_window(winit_window: &WinitWindow, state: &mut dyn Window) {
        let size = winit_window.inner_size();

        let cx = WindowCx {
            window: winit_window,
        };
        state.open(&cx);
        state.resize(size, &cx);

        winit_window.set_visible(true);
    }

    fn create_software_window(
        window_target: &EventLoopWindowTarget<()>,
        window: InitWindow,
    ) -> SoftwareWindow {
        let window = window.init_software(window_target).unwrap();
        let size = window.inner_size();

        let gc = unsafe { GraphicsContext::new(&window, window_target).unwrap() };
        let skia = SkiaSoftwareRenderer::new(
            gc,
            size.width.try_into().unwrap(),
            size.height.try_into().unwrap(),
        );

        SoftwareWindow::new(skia, window)
    }

    fn create_gl_window(
        window_target: &EventLoopWindowTarget<()>,
        gl_state: &GlWindowManagerState,
        window: InitWindow,
    ) -> Result<GlWindow, (glutin::error::Error, WinitWindow)> {
        #[cfg(target_os = "android")]
        println!("Android window available");

        let window = window.init_gl(window_target, &gl_state.gl_config).unwrap();
        let size = window.inner_size();

        match SkiaGlRenderer::new(
            window.raw_window_handle(),
            size.width,
            size.height,
            &gl_state,
        ) {
            Ok(skia) => Ok(GlWindow::new(skia, window)),
            Err(err) => Err((err, window)),
        }
    }
}

enum InitWindow {
    First(WinitWindow),
    Other(WindowBuilder),
}
impl InitWindow {
    fn init_software(
        self,
        window_target: &EventLoopWindowTarget<()>,
    ) -> Result<WinitWindow, OsError> {
        match self {
            InitWindow::First(window) => Ok(window),
            InitWindow::Other(builder) => builder.build(window_target),
        }
    }
    fn init_gl(
        self,
        window_target: &EventLoopWindowTarget<()>,
        gl_config: &Config,
    ) -> Result<WinitWindow, OsError> {
        match self {
            InitWindow::First(window) => Ok(window),
            InitWindow::Other(builder) => {
                glutin_winit::finalize_window(window_target, builder, gl_config)
            }
        }
    }
}
