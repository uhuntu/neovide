use std::thread::sleep;
use std::time::{Duration, Instant};

use log::{debug, info, trace};

use skulpin::sdl2;
use skulpin::sdl2::event::{Event, WindowEvent};
use skulpin::sdl2::keyboard::Keycode;
use skulpin::sdl2::video::FullscreenType;
use skulpin::sdl2::Sdl;
use skulpin::{
    CoordinateSystem, LogicalSize, PhysicalSize, PresentMode, Renderer as SkulpinRenderer,
    RendererBuilder, Sdl2Window, Window,
};

use crate::events::*;
use crate::keyboard::*;
use crate::redraw_scheduler::*;

#[cfg(target_os = "windows")]
fn windows_fix_dpi() {
    use winapi::shared::windef::DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2;
    use winapi::um::winuser::SetProcessDpiAwarenessContext;
    unsafe {
        SetProcessDpiAwarenessContext(DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2);
    }
}

struct WindowWrapper<Handler: UiEventHandler> {
    context: Sdl,
    event_handler: Handler,
    window: sdl2::video::Window,
    skulpin_renderer: SkulpinRenderer,
    mouse_down: bool,
    mouse_position: LogicalSize,
    title: String,
    previous_size: LogicalSize,
    transparency: f32,
    fullscreen: bool,
    cached_size: (u32, u32),
    cached_position: (i32, i32),
}

impl<Handler: UiEventHandler> WindowWrapper<Handler> {
    pub fn new(event_handler: Handler, size: (u32, u32)) -> WindowWrapper<Handler> {
        let context = sdl2::init().expect("Failed to initialize sdl2");
        let video_subsystem = context
            .video()
            .expect("Failed to create sdl video subsystem");
        video_subsystem.text_input().start();

        let (width, height) = size;
        let logical_size = LogicalSize {
            // width: (width as f32 * renderer.font_width) as u32,
            // height: (height as f32 * renderer.font_height + 1.0) as u32,
            width: (width as f32 * 10.0) as u32,
            height: (height as f32 * 10.0 + 1.0) as u32,
        };

        #[cfg(target_os = "windows")]
        windows_fix_dpi();
        sdl2::hint::set("SDL_MOUSE_FOCUS_CLICKTHROUGH", "1");

        let sdl_window = video_subsystem
            .window("Neovide", logical_size.width, logical_size.height)
            .position_centered()
            .allow_highdpi()
            .resizable()
            .vulkan()
            .build()
            .expect("Failed to create window");
        info!("window created");

        let skulpin_renderer = {
            let sdl_window_wrapper = Sdl2Window::new(&sdl_window);
            RendererBuilder::new()
                .prefer_integrated_gpu()
                .use_vulkan_debug_layer(false)
                .present_mode_priority(vec![PresentMode::Immediate])
                .coordinate_system(CoordinateSystem::Logical)
                .build(&sdl_window_wrapper)
                .expect("Failed to create renderer")
        };

        WindowWrapper {
            context,
            event_handler,
            window: sdl_window,
            skulpin_renderer,
            mouse_down: false,
            mouse_position: LogicalSize {
                width: 0,
                height: 0,
            },
            title: String::from("Neovide"),
            previous_size: logical_size,
            transparency: 1.0,
            fullscreen: false,
            cached_size: (0, 0),
            cached_position: (0, 0),
        }
    }

    pub fn toggle_fullscreen(&mut self) {
        if self.fullscreen {
            if cfg!(target_os = "windows") {
                unsafe {
                    let raw_handle = self.window.raw();
                    sdl2::sys::SDL_SetWindowResizable(raw_handle, sdl2::sys::SDL_bool::SDL_TRUE);
                }
            } else {
                self.window.set_fullscreen(FullscreenType::Off).ok();
            }

            // Use cached size and position
            self.window
                .set_size(self.cached_size.0, self.cached_size.1)
                .unwrap();
            self.window.set_position(
                sdl2::video::WindowPos::Positioned(self.cached_position.0),
                sdl2::video::WindowPos::Positioned(self.cached_position.1),
            );
        } else {
            self.cached_size = self.window.size();
            self.cached_position = self.window.position();

            if cfg!(target_os = "windows") {
                let video_subsystem = self.window.subsystem();
                if let Ok(rect) = self
                    .window
                    .display_index()
                    .and_then(|index| video_subsystem.display_bounds(index))
                {
                    // Set window to fullscreen
                    unsafe {
                        let raw_handle = self.window.raw();
                        sdl2::sys::SDL_SetWindowResizable(
                            raw_handle,
                            sdl2::sys::SDL_bool::SDL_FALSE,
                        );
                    }
                    self.window.set_size(rect.width(), rect.height()).unwrap();
                    self.window.set_position(
                        sdl2::video::WindowPos::Positioned(rect.x()),
                        sdl2::video::WindowPos::Positioned(rect.y()),
                    );
                }
            } else {
                self.window.set_fullscreen(FullscreenType::Desktop).ok();
            }
        }

        self.fullscreen = !self.fullscreen;
    }

    pub fn handle_quit(&mut self) {
        self.event_handler.handle_ui_event(UiEvent::Quit);
    }

    pub fn handle_keyboard_input(&mut self, keycode: Option<Keycode>, text: Option<String>) {
        let modifiers = self.context.keyboard().mod_state();

        if keycode.is_some() || text.is_some() {
            trace!(
                "Keyboard Input Received: keycode-{:?} modifiers-{:?} text-{:?}",
                keycode,
                modifiers,
                text
            );
        }

        if let Some(keybinding_string) = produce_keybinding_string(keycode, text, modifiers) {
            self.event_handler.handle_ui_event(UiEvent::KeyboardInput(keybinding_string));
        }
    }

    pub fn handle_pointer_motion(&mut self, x: i32, y: i32) {
        let previous_position = self.mouse_position;
        let physical_size = PhysicalSize::new(
            // (x as f32 / self.renderer.font_width) as u32,
            // (y as f32 / self.renderer.font_height) as u32,
            (x as f32 / 10.0) as u32,
            (y as f32 / 10.0) as u32,
        );

        let sdl_window_wrapper = Sdl2Window::new(&self.window);
        self.mouse_position = physical_size.to_logical(sdl_window_wrapper.scale_factor());
        if self.mouse_down && previous_position != self.mouse_position {
            self.event_handler.handle_ui_event(UiEvent::MouseDragged(
                self.mouse_position.width,
                self.mouse_position.height,
            ));
        }
    }

    pub fn handle_pointer_down(&mut self) {
        self.event_handler.handle_ui_event(UiEvent::MousePressed(
            self.mouse_position.width,
            self.mouse_position.height,
        ));
        self.mouse_down = true;
    }

    pub fn handle_pointer_up(&mut self) {
        self.event_handler.handle_ui_event(UiEvent::MouseReleased(
            self.mouse_position.width,
            self.mouse_position.height,
        ));
        self.mouse_down = false;
    }

    pub fn handle_mouse_wheel(&mut self, x: i32, y: i32) {
        let vertical_direction = if y > 0 {
            Some(Direction::Up)
        } else if y < 0 {
            Some(Direction::Down)
        } else {
            None
        };

        if let Some(direction) = vertical_direction {
            self.event_handler.handle_ui_event(UiEvent::Scroll(
                direction,
                self.mouse_position.width,
                self.mouse_position.height,
            ));
        }

        let horizontal_direction = if x > 0 {
            Some(Direction::Right)
        } else if x < 0 {
            Some(Direction::Left)
        } else {
            None
        };

        if let Some(direction) = horizontal_direction {
            self.event_handler.handle_ui_event(UiEvent::Scroll(
                direction,
                self.mouse_position.width,
                self.mouse_position.height,
            ));
        }
    }

    pub fn handle_focus_lost(&mut self) {
        self.event_handler.handle_ui_event(UiEvent::FocusLost);
    }

    pub fn handle_focus_gained(&mut self) {
        self.event_handler.handle_ui_event(UiEvent::FocusGained);
        REDRAW_SCHEDULER.queue_next_frame();
    }

    pub fn draw_frame(&mut self) -> bool {
        let sdl_window_wrapper = Sdl2Window::new(&self.window);
        let new_size = sdl_window_wrapper.logical_size();
        if self.previous_size != new_size {
            // handle_new_grid_size(new_size, &self.renderer);
            self.previous_size = new_size;
        }

        debug!("Render Triggered");

        let current_size = self.previous_size;

        if REDRAW_SCHEDULER.should_draw() {
        }

        return true;
    }
}

pub fn ui_loop<Handler: UiEventHandler>(event_handler: Handler, size: (u32, u32)) {
    let mut window = WindowWrapper::new(event_handler, size);

    info!("Starting window event loop");
    let mut event_pump = window
        .context
        .event_pump()
        .expect("Could not create sdl event pump");

    loop {
        let frame_start = Instant::now();

        let mut keycode = None;
        let mut keytext = None;
        let mut ignore_text_this_frame = false;

        for event in event_pump.poll_iter() {
            match event {
                Event::Quit { .. } => window.handle_quit(),
                Event::KeyDown {
                    keycode: received_keycode,
                    ..
                } => {
                    keycode = received_keycode;
                }
                Event::TextInput { text, .. } => keytext = Some(text),
                Event::MouseMotion { x, y, .. } => window.handle_pointer_motion(x, y),
                Event::MouseButtonDown { .. } => window.handle_pointer_down(),
                Event::MouseButtonUp { .. } => window.handle_pointer_up(),
                Event::MouseWheel { x, y, .. } => window.handle_mouse_wheel(x, y),
                Event::Window {
                    win_event: WindowEvent::FocusLost,
                    ..
                } => window.handle_focus_lost(),
                Event::Window {
                    win_event: WindowEvent::FocusGained,
                    ..
                } => {
                    ignore_text_this_frame = true; // Ignore any text events on the first frame when focus is regained. https://github.com/Kethku/neovide/issues/193
                    window.handle_focus_gained();
                },
                Event::Window { .. } => REDRAW_SCHEDULER.queue_next_frame(),
                _ => {}
            }
        }

        if !ignore_text_this_frame {
            window.handle_keyboard_input(keycode, keytext);
        }

        if !window.draw_frame() {
            break;
        }

        let elapsed = frame_start.elapsed();
        let frame_length = Duration::from_secs_f32(1.0 / 144.0);

        if elapsed < frame_length {
            sleep(frame_length - elapsed);
        }
    }

    std::process::exit(0);
}
