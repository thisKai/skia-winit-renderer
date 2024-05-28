use crate::{
    gl::{GlWindowManagerState, SkiaGlRenderer},
    skia::SkiaRenderer,
    software::SkiaSoftwareRenderer,
    window::SkiaWindow,
};
use glutin::config::Config;
use raw_window_handle::{HasRawDisplayHandle, HasRawWindowHandle};
use skia_safe::Canvas;
use std::{collections::HashMap, iter};
use winit::{
    dpi::PhysicalSize,
    error::OsError,
    event_loop::EventLoopWindowTarget,
    window::{Window, WindowBuilder, WindowId},
};

pub struct WindowManager<State = ()> {
    state: WindowManagerState<State>,
}
impl<State> WindowManager<State> {
    pub fn new() -> Self {
        Self {
            state: WindowManagerState::Init,
        }
    }

    pub fn draw(&mut self, id: &WindowId, mut f: impl FnMut(&Canvas, &Window, &mut State)) {
        let window = self.get_window_mut(id).unwrap();

        window.draw(&mut f);
    }
    pub fn resize(&mut self, id: &WindowId, size: PhysicalSize<u32>) {
        let winit_window = match &mut self.state {
            WindowManagerState::Init => {
                panic!("Uninitialized window manager");
            }
            WindowManagerState::Software { windows } => {
                let window = windows.get_mut(id).unwrap();

                window.window.resize(size);

                window.winit_window()
            }
            WindowManagerState::Gl { state, windows } => {
                let window = windows.get_mut(id).unwrap();

                window.window.resize_dependent(state, size);

                window.winit_window()
            }
        };

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

    pub fn get_window(&self, id: &WindowId) -> Option<&dyn ManagedWindow<State>> {
        match &self.state {
            WindowManagerState::Init => None,
            WindowManagerState::Software { windows } => windows.get(id).map(|window| window as _),
            WindowManagerState::Gl { windows, .. } => windows.get(id).map(|window| window as _),
        }
    }
    pub fn get_window_mut(&mut self, id: &WindowId) -> Option<&mut dyn ManagedWindow<State>> {
        match &mut self.state {
            WindowManagerState::Init => None,
            WindowManagerState::Software { windows } => {
                windows.get_mut(id).map(|window| window as _)
            }
            WindowManagerState::Gl { windows, .. } => windows.get_mut(id).map(|window| window as _),
        }
    }
    pub fn iter_windows(&self) -> Box<dyn Iterator<Item = &dyn ManagedWindow<State>> + '_> {
        match &self.state {
            WindowManagerState::Init => Box::new(iter::empty()),
            WindowManagerState::Software { windows } => {
                Box::new(windows.values().map(|window| window as _))
            }
            WindowManagerState::Gl { windows, .. } => {
                Box::new(windows.values().map(|window| window as _))
            }
        }
    }
    pub fn iter_windows_mut(
        &mut self,
    ) -> Box<dyn Iterator<Item = &mut dyn ManagedWindow<State>> + '_> {
        match &mut self.state {
            WindowManagerState::Init => Box::new(iter::empty()),
            WindowManagerState::Software { windows } => {
                Box::new(windows.values_mut().map(|window| window as _))
            }
            WindowManagerState::Gl { windows, .. } => {
                Box::new(windows.values_mut().map(|window| window as _))
            }
        }
    }

    pub fn create_window<T>(
        &mut self,
        window_target: &EventLoopWindowTarget<T>,
        window_builder: WindowBuilder,
        window_state: State,
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
                // let gl_state_and_first_window: Result<(GlWindowManagerState, GlWindow), _> = Err((
                //     <Box<dyn Error>>::from(String::from("blah")),
                //     window_builder.clone().build(window_target).ok(),
                // ));

                match gl_state_and_first_window {
                    Ok((gl_state, window)) => {
                        let id = window.id();

                        let mut windows = HashMap::new();

                        Self::init_window(&window.window);
                        windows.insert(id, StatefulWindow::new(window, window_state));

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

                        Self::init_window(&window.window);
                        windows.insert(id, StatefulWindow::new(window, window_state));

                        *state = WindowManagerState::Software { windows };
                        id
                    }
                }
            }
            WindowManagerState::Software { windows } => {
                let window =
                    Self::create_software_window(window_target, InitWindow::Other(window_builder));
                let id = window.id();

                Self::init_window(&window.window);
                windows.insert(id, StatefulWindow::new(window, window_state));

                id
            }
            WindowManagerState::Gl { state, windows } => {
                let window =
                    Self::create_gl_window(window_target, state, InitWindow::Other(window_builder))
                        .unwrap();
                let id = window.id();

                Self::init_window(&window.window);
                windows.insert(id, StatefulWindow::new(window, window_state));

                id
            }
        }
    }

    fn init_window(winit_window: &Window) {
        winit_window.set_visible(true);
    }

    fn create_software_window<T>(
        window_target: &EventLoopWindowTarget<T>,
        window: InitWindow,
    ) -> SkiaWindow<SkiaSoftwareRenderer> {
        let window = window.init_software(window_target).unwrap();
        let size = window.inner_size();

        let skia = SkiaSoftwareRenderer::new(
            window_target.raw_display_handle(),
            window.raw_window_handle(),
            size.width.try_into().unwrap(),
            size.height.try_into().unwrap(),
        );

        SkiaWindow::software(skia, window)
    }

    fn create_gl_window<T>(
        window_target: &EventLoopWindowTarget<T>,
        gl_state: &GlWindowManagerState,
        window: InitWindow,
    ) -> Result<SkiaWindow<SkiaGlRenderer>, (glutin::error::Error, Window)> {
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
            Ok(skia) => Ok(SkiaWindow::gl(skia, window)),
            Err(err) => Err((err, window)),
        }
    }
}

enum WindowManagerState<State> {
    Init,
    Software {
        windows: WindowMap<SkiaSoftwareRenderer, State>,
    },
    Gl {
        state: GlWindowManagerState,
        windows: WindowMap<SkiaGlRenderer, State>,
    },
}

type WindowMap<Skia, State> = HashMap<WindowId, StatefulWindow<Skia, State>>;

struct StatefulWindow<Skia, State> {
    state: State,
    window: SkiaWindow<Skia>,
}
impl<Skia, State> StatefulWindow<Skia, State> {
    fn new(window: SkiaWindow<Skia>, state: State) -> Self {
        Self { state, window }
    }
}

pub trait ManagedWindow<State> {
    fn state(&self) -> &State;
    fn state_mut(&mut self) -> &mut State;
    fn winit_window(&self) -> &Window;
    fn winit_window_and_state_mut(&mut self) -> (&mut Window, &mut State);
    fn draw(&mut self, f: &mut dyn FnMut(&Canvas, &Window, &mut State));
}
impl<Skia: SkiaRenderer, State> ManagedWindow<State> for StatefulWindow<Skia, State> {
    fn state(&self) -> &State {
        &self.state
    }
    fn state_mut(&mut self) -> &mut State {
        &mut self.state
    }
    fn winit_window(&self) -> &Window {
        &self.window.window
    }
    fn winit_window_and_state_mut(&mut self) -> (&mut Window, &mut State) {
        (&mut self.window.window, &mut self.state)
    }
    fn draw(&mut self, f: &mut dyn FnMut(&Canvas, &Window, &mut State)) {
        self.window
            .draw(&mut |canvas, window| f(canvas, window, &mut self.state))
    }
}

enum InitWindow {
    First(Window),
    Other(WindowBuilder),
}
impl InitWindow {
    fn init_software<T>(self, window_target: &EventLoopWindowTarget<T>) -> Result<Window, OsError> {
        match self {
            InitWindow::First(window) => Ok(window),
            InitWindow::Other(builder) => builder.build(window_target),
        }
    }
    fn init_gl<T>(
        self,
        window_target: &EventLoopWindowTarget<T>,
        gl_config: &Config,
    ) -> Result<Window, OsError> {
        match self {
            InitWindow::First(window) => Ok(window),
            InitWindow::Other(builder) => {
                glutin_winit::finalize_window(window_target, builder, gl_config)
            }
        }
    }
}
