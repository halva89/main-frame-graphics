use winit::{
    window::Window,
    event_loop::ActiveEventLoop,
    dpi::LogicalSize,
    event::WindowEvent,
};
use raw_window_handle::{HasWindowHandle, HasDisplayHandle, RawWindowHandle, RawDisplayHandle};
use crate::scene_parser::SceneSettings;
use crate::platform::input::Input;
use crate::platform::time::Time;

pub struct CustomWindow {
    pub raw: Option<Window>,
    pub input: Input,
    pub time: Time,
    pub should_close: bool,
    pub width: u32,
    pub height: u32,
    pub title: String,
    pub initialized: bool,
    pub decorated: bool,
}

impl CustomWindow {
    pub fn new(settings: &SceneSettings) -> Self {
        Self {
            raw: None,
            input: Input::new(),
            time: Time::new(),
            should_close: false,
            width: settings.width,
            height: settings.height,
            title: settings.title.clone(),
            initialized: false,
            decorated: settings.decorated,
        }
    }

    pub fn init_window(&mut self, elwt: &ActiveEventLoop) {
        let window = elwt.create_window(
            Window::default_attributes()
                .with_title(&self.title)
                .with_inner_size(LogicalSize::new(self.width, self.height))
                .with_decorations(self.decorated),
        ).unwrap();
        let size = window.inner_size();
        self.width = size.width;
        self.height = size.height;
        self.raw = Some(window);
        self.initialized = true;
    }

    pub fn handle_event(&mut self, event: &WindowEvent) {
        match event {
            WindowEvent::CloseRequested => self.should_close = true,
            WindowEvent::Resized(size) => {
                self.width = size.width;
                self.height = size.height;
            }
            _ => self.input.handle_event(event),
        }
    }

    pub fn should_close(&self) -> bool {
        self.should_close
    }

    pub fn get_raw_window_handle(&self) -> RawWindowHandle {
        self.raw.as_ref().unwrap().window_handle().unwrap().as_raw()
    }

    pub fn get_raw_display_handle(&self) -> RawDisplayHandle {
        self.raw.as_ref().unwrap().display_handle().unwrap().as_raw()
    }

    pub fn dummy(w: u32, h: u32) -> Self {
        Self {
            raw: None,
            input: Input::new(),
            time: Time::new(),
            should_close: false,
            width: w,
            height: h,
            title: String::new(),
            initialized: false,
            decorated: true,
        }
    }
}