/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

//! A windowing implementation using winit.

use euclid::{Length, TypedPoint2D, TypedVector2D, TypedScale, TypedSize2D};
use gleam::gl;
use glutin::{Api, ContextBuilder, GlContext, GlRequest, GlWindow};
use servo::compositing::windowing::{AnimationState, MouseWindowEvent, WindowEvent};
use servo::compositing::windowing::{EmbedderCoordinates, WindowMethods};
use servo::embedder_traits::EventLoopWaker;
use servo::msg::constellation_msg::{Key, KeyState, KeyModifiers};
use servo::script_traits::TouchEventType;
use servo::servo_geometry::DeviceIndependentPixel;
use servo::style_traits::DevicePixel;
use servo::style_traits::cursor::CursorKind;
use servo::webrender_api::{DeviceIntPoint, DeviceUintRect, DeviceUintSize, ScrollLocation};
use std::cell::{Cell, RefCell};
#[cfg(any(target_os = "linux", target_os = "macos"))]
use std::ffi::CString;
use std::mem;
use std::rc::Rc;
use std::sync::Arc;
use super::keyutils;
use winit;
use winit::{ElementState, Event, ModifiersState, MouseButton, MouseScrollDelta, TouchPhase, VirtualKeyCode};
use winit::dpi::{LogicalPosition, LogicalSize, PhysicalSize};
#[cfg(target_os = "macos")]
use winit::os::macos::{ActivationPolicy, WindowBuilderExt};

// This should vary by zoom level and maybe actual text size (focused or under cursor)
pub const LINE_HEIGHT: f32 = 38.0;

const MULTISAMPLES: u16 = 16;

/// The type of a window.
pub struct Window {
    window: GlWindow,
    events_loop: RefCell<winit::EventsLoop>,
    screen_size: TypedSize2D<u32, DeviceIndependentPixel>,
    inner_size: Cell<TypedSize2D<u32, DeviceIndependentPixel>>,
    mouse_down_button: Cell<Option<winit::MouseButton>>,
    mouse_down_point: Cell<TypedPoint2D<i32, DevicePixel>>,
    event_queue: RefCell<Vec<WindowEvent>>,
    mouse_pos: Cell<TypedPoint2D<i32, DevicePixel>>,
    key_modifiers: Cell<KeyModifiers>,
    last_pressed_key: Cell<Option<Key>>,
    animation_state: Cell<AnimationState>,
    fullscreen: Cell<bool>,
    gl: Rc<gl::Gl>,
    suspended: Cell<bool>,
}

impl Window {
    pub fn new(mut window_builder: winit::WindowBuilder, vsync: bool, msaa: bool) -> Rc<Window> {
        let screen_size;
        let inner_size;
        let (glutin_window, events_loop) = {
            let events_loop = winit::EventsLoop::new();
            window_builder = window_builder.with_multitouch();

            #[cfg(any(target_os = "linux", target_os = "windows"))]
            {
                let icon_bytes = include_bytes!("../../../../resources/servo64.png");
                let icon = Some(winit::Icon::from_bytes(icon_bytes).expect("Failed to open icon"));
                window_builder = window_builder.with_window_icon(icon);
            }

            let mut context_builder = ContextBuilder::new()
                .with_gl(Window::gl_version())
                .with_vsync(vsync);

            if msaa {
                context_builder = context_builder.with_multisampling(MULTISAMPLES)
            }

            let glutin_window = GlWindow::new(window_builder, context_builder, &events_loop)
                .expect("Failed to create window.");

            unsafe {
                glutin_window.context().make_current().expect("Couldn't make window current");
            }

            let PhysicalSize {
                width: screen_width,
                height: screen_height,
            } = events_loop.get_primary_monitor().get_dimensions();
            screen_size = TypedSize2D::new(screen_width as u32, screen_height as u32);
            // TODO(ajeffrey): can this fail?
            let LogicalSize { width, height } =
                glutin_window.get_inner_size().expect("Failed to get window inner size.");
            inner_size = TypedSize2D::new(width as u32, height as u32);

            glutin_window.show();

            (glutin_window, RefCell::new(events_loop))
        };

        let gl = match gl::GlType::default() {
            gl::GlType::Gl => {
                unsafe {
                    gl::GlFns::load_with(|s| glutin_window.get_proc_address(s) as *const _)
                }
            }
            gl::GlType::Gles => {
                unsafe {
                    gl::GlesFns::load_with(|s| glutin_window.get_proc_address(s) as *const _)
                }
            }
        };

        gl.clear_color(0.6, 0.6, 0.6, 1.0);
        gl.clear(gl::COLOR_BUFFER_BIT);
        gl.finish();

        let window = Window {
            window: glutin_window,
            events_loop,
            event_queue: RefCell::new(vec!()),
            mouse_down_button: Cell::new(None),
            mouse_down_point: Cell::new(TypedPoint2D::new(0, 0)),

            mouse_pos: Cell::new(TypedPoint2D::new(0, 0)),
            key_modifiers: Cell::new(KeyModifiers::empty()),

            last_pressed_key: Cell::new(None),
            gl: gl.clone(),
            animation_state: Cell::new(AnimationState::Idle),
            fullscreen: Cell::new(false),
            inner_size: Cell::new(inner_size),
            screen_size,
            suspended: Cell::new(false),
        };

        window.present();

        Rc::new(window)
    }

    pub fn get_events(&self) -> Vec<WindowEvent> {
        mem::replace(&mut *self.event_queue.borrow_mut(), Vec::new())
    }

    pub fn page_height(&self) -> f32 {
        let dpr = self.device_hidpi_factor();
        let size = self.window.get_inner_size().expect("Failed to get window inner size.");
        size.height as f32 * dpr.get()
    }

    pub fn set_title(&self, title: &str) {
        self.window.set_title(title);
    }

    pub fn set_inner_size(&self, size: DeviceUintSize) {
        let size = size.to_f32() / self.device_hidpi_factor();
        self.window.set_inner_size(LogicalSize::new(size.width.into(), size.height.into()));
    }

    pub fn set_position(&self, point: DeviceIntPoint) {
        let point = point.to_f32() / self.device_hidpi_factor();
        self.window.set_position(LogicalPosition::new(point.x.into(), point.y.into()));
    }

    pub fn set_fullscreen(&self, state: bool) {
        if self.fullscreen.get() != state {
            self.window.set_fullscreen(None);
        }
        self.fullscreen.set(state);
    }

    fn is_animating(&self) -> bool {
        self.animation_state.get() == AnimationState::Animating && !self.suspended.get()
    }

    pub fn run<T>(&self, mut servo_callback: T) where T: FnMut() -> bool {
        let mut stop = false;
        loop {
            if self.is_animating() {
                // We block on compositing (servo_callback ends up calling swap_buffers)
                self.events_loop.borrow_mut().poll_events(|e| {
                    self.winit_event_to_servo_event(e);
                });
                stop = servo_callback();
            } else {
                // We block on winit's event loop (window events)
                self.events_loop.borrow_mut().run_forever(|e| {
                    self.winit_event_to_servo_event(e);
                    if !self.event_queue.borrow().is_empty() {
                        if !self.suspended.get() {
                            stop = servo_callback();
                        }
                    }
                    if stop || self.is_animating() {
                        winit::ControlFlow::Break
                    } else {
                        winit::ControlFlow::Continue
                    }
                });
            }
            if stop {
                break;
            }
        }
    }

    #[cfg(not(any(target_arch = "arm", target_arch = "aarch64")))]
    fn gl_version() -> GlRequest {
        return GlRequest::Specific(Api::OpenGl, (3, 2));
    }

    #[cfg(any(target_arch = "arm", target_arch = "aarch64"))]
    fn gl_version() -> GlRequest {
        GlRequest::Specific(Api::OpenGlEs, (3, 0))
    }

    fn handle_received_character(&self, ch: char) {
        let last_key = if let Some(key) = self.last_pressed_key.get() {
            key
        } else {
            return;
        };

        self.last_pressed_key.set(None);

        let (key, ch) = if let Some(key) = keyutils::char_to_script_key(ch) {
            (key, Some(ch))
        } else {
            (last_key, None)
        };

        let modifiers = self.key_modifiers.get();
        let event = WindowEvent::KeyEvent(ch, key, KeyState::Pressed, modifiers);
        self.event_queue.borrow_mut().push(event);
    }

    fn toggle_keyboard_modifiers(&self, mods: ModifiersState) {
        self.toggle_modifier(KeyModifiers::CONTROL, mods.ctrl);
        self.toggle_modifier(KeyModifiers::SHIFT, mods.shift);
        self.toggle_modifier(KeyModifiers::ALT, mods.alt);
        self.toggle_modifier(KeyModifiers::SUPER, mods.logo);
    }

    fn handle_keyboard_input(&self, element_state: ElementState, code: VirtualKeyCode, mods: ModifiersState) {
        self.toggle_keyboard_modifiers(mods);

        if let Ok(key) = keyutils::winit_key_to_script_key(code) {
            let state = match element_state {
                ElementState::Pressed => KeyState::Pressed,
                ElementState::Released => KeyState::Released,
            };
            if element_state == ElementState::Pressed && keyutils::is_printable(code) {
                // If pressed and printable, we expect a ReceivedCharacter event.
                self.last_pressed_key.set(Some(key));
            } else {
                self.last_pressed_key.set(None);
                let modifiers = self.key_modifiers.get();
                self.event_queue.borrow_mut().push(WindowEvent::KeyEvent(None, key, state, modifiers));
            }
        }
    }

    fn winit_event_to_servo_event(&self, event: winit::Event) {
        match event {
            Event::WindowEvent {
                event: winit::WindowEvent::ReceivedCharacter(ch),
                ..
            } => self.handle_received_character(ch),
            Event::WindowEvent {
                event: winit::WindowEvent::KeyboardInput {
                    input: winit::KeyboardInput {
                        state, virtual_keycode: Some(virtual_keycode), modifiers, ..
                    }, ..
                }, ..
            } => self.handle_keyboard_input(state, virtual_keycode, modifiers),
            Event::WindowEvent {
                event: winit::WindowEvent::MouseInput {
                    state, button, ..
                }, ..
            } => {
                if button == MouseButton::Left || button == MouseButton::Right {
                    self.handle_mouse(button, state, self.mouse_pos.get());
                }
            },
            Event::WindowEvent {
                event: winit::WindowEvent::CursorMoved {
                    position,
                    ..
                },
                ..
            } => {
                let pos = position.to_physical(self.device_hidpi_factor().get() as f64);
                let (x, y): (i32, i32) = pos.into();
                self.mouse_pos.set(TypedPoint2D::new(x, y));
                self.event_queue.borrow_mut().push(
                    WindowEvent::MouseWindowMoveEventClass(TypedPoint2D::new(x as f32, y as f32)));
            }
            Event::WindowEvent {
                event: winit::WindowEvent::MouseWheel { delta, phase, .. },
                ..
            } => {
                let (mut dx, mut dy) = match delta {
                    MouseScrollDelta::LineDelta(dx, dy) => (dx, dy * LINE_HEIGHT),
                    MouseScrollDelta::PixelDelta(position) => {
                        let position = position.to_physical(self.device_hidpi_factor().get() as f64);
                        (position.x as f32, position.y as f32)
                    }
                };
                // Scroll events snap to the major axis of movement, with vertical
                // preferred over horizontal.
                if dy.abs() >= dx.abs() {
                    dx = 0.0;
                } else {
                    dy = 0.0;
                }

                let scroll_location = ScrollLocation::Delta(TypedVector2D::new(dx, dy));
                let phase = winit_phase_to_touch_event_type(phase);
                let event = WindowEvent::Scroll(scroll_location, self.mouse_pos.get(), phase);
                self.event_queue.borrow_mut().push(event);
            },
            Event::WindowEvent {
                event: winit::WindowEvent::Touch(touch),
                ..
            } => {
                use servo::script_traits::TouchId;

                let phase = winit_phase_to_touch_event_type(touch.phase);
                let id = TouchId(touch.id as i32);
                let position = touch.location.to_physical(self.device_hidpi_factor().get() as f64);
                let point = TypedPoint2D::new(position.x as f32, position.y as f32);
                self.event_queue.borrow_mut().push(WindowEvent::Touch(phase, id, point));
            }
            Event::WindowEvent {
                event: winit::WindowEvent::Refresh,
                ..
            } => self.event_queue.borrow_mut().push(WindowEvent::Refresh),
            Event::WindowEvent {
                event: winit::WindowEvent::CloseRequested,
                ..
            } => {
                self.event_queue.borrow_mut().push(WindowEvent::Quit);
            }
            Event::WindowEvent {
                event: winit::WindowEvent::Resized(size),
                ..
            } => {
                // size is DeviceIndependentPixel.
                // window.resize() takes DevicePixel.
                let size = size.to_physical(self.device_hidpi_factor().get() as f64);
                self.window.resize(size);
                // window.set_inner_size() takes DeviceIndependentPixel.
                let (width, height) = size.into();
                let new_size = TypedSize2D::new(width, height);
                if self.inner_size.get() != new_size {
                    self.inner_size.set(new_size);
                    self.event_queue.borrow_mut().push(WindowEvent::Resize);
                }
            }
            Event::Suspended(suspended) => {
                self.suspended.set(suspended);
                if !suspended {
                    self.event_queue.borrow_mut().push(WindowEvent::Idle);
                }
            }
            Event::Awakened => {
                self.event_queue.borrow_mut().push(WindowEvent::Idle);
            }
            _ => {}
        }
    }

    fn toggle_modifier(&self, modifier: KeyModifiers, pressed: bool) {
        let mut modifiers = self.key_modifiers.get();
        if pressed {
            modifiers.insert(modifier);
        } else {
            modifiers.remove(modifier);
        }
        self.key_modifiers.set(modifiers);
    }

    /// Helper function to handle a click
    fn handle_mouse(&self, button: winit::MouseButton,
                    action: winit::ElementState,
                    coords: TypedPoint2D<i32, DevicePixel>) {
        use servo::script_traits::MouseButton;

        let max_pixel_dist = 10.0 * self.device_hidpi_factor().get();
        let event = match action {
            ElementState::Pressed => {
                self.mouse_down_point.set(coords);
                self.mouse_down_button.set(Some(button));
                MouseWindowEvent::MouseDown(MouseButton::Left, coords.to_f32())
            }
            ElementState::Released => {
                let mouse_up_event = MouseWindowEvent::MouseUp(MouseButton::Left, coords.to_f32());
                match self.mouse_down_button.get() {
                    None => mouse_up_event,
                    Some(but) if button == but => {
                        let pixel_dist = self.mouse_down_point.get() - coords;
                        let pixel_dist = ((pixel_dist.x * pixel_dist.x +
                                           pixel_dist.y * pixel_dist.y) as f32).sqrt();
                        if pixel_dist < max_pixel_dist {
                            self.event_queue.borrow_mut().push(WindowEvent::MouseWindowEventClass(mouse_up_event));
                            MouseWindowEvent::Click(MouseButton::Left, coords.to_f32())
                        } else {
                            mouse_up_event
                        }
                    },
                    Some(_) => mouse_up_event,
                }
            }
        };
        self.event_queue.borrow_mut().push(WindowEvent::MouseWindowEventClass(event));
    }

    fn device_hidpi_factor(&self) -> TypedScale<f32, DeviceIndependentPixel, DevicePixel> {
        TypedScale::new(self.window.get_hidpi_factor() as f32)
    }

    pub fn set_cursor(&self, cursor: CursorKind) {
        use winit::MouseCursor;

        let winit_cursor = match cursor {
            CursorKind::Auto => MouseCursor::Default,
            CursorKind::Default => MouseCursor::Default,
            CursorKind::Pointer => MouseCursor::Hand,
            CursorKind::ContextMenu => MouseCursor::ContextMenu,
            CursorKind::Help => MouseCursor::Help,
            CursorKind::Progress => MouseCursor::Progress,
            CursorKind::Wait => MouseCursor::Wait,
            CursorKind::Cell => MouseCursor::Cell,
            CursorKind::Crosshair => MouseCursor::Crosshair,
            CursorKind::Text => MouseCursor::Text,
            CursorKind::VerticalText => MouseCursor::VerticalText,
            CursorKind::Alias => MouseCursor::Alias,
            CursorKind::Copy => MouseCursor::Copy,
            CursorKind::Move => MouseCursor::Move,
            CursorKind::NoDrop => MouseCursor::NoDrop,
            CursorKind::NotAllowed => MouseCursor::NotAllowed,
            CursorKind::Grab => MouseCursor::Grab,
            CursorKind::Grabbing => MouseCursor::Grabbing,
            CursorKind::EResize => MouseCursor::EResize,
            CursorKind::NResize => MouseCursor::NResize,
            CursorKind::NeResize => MouseCursor::NeResize,
            CursorKind::NwResize => MouseCursor::NwResize,
            CursorKind::SResize => MouseCursor::SResize,
            CursorKind::SeResize => MouseCursor::SeResize,
            CursorKind::SwResize => MouseCursor::SwResize,
            CursorKind::WResize => MouseCursor::WResize,
            CursorKind::EwResize => MouseCursor::EwResize,
            CursorKind::NsResize => MouseCursor::NsResize,
            CursorKind::NeswResize => MouseCursor::NeswResize,
            CursorKind::NwseResize => MouseCursor::NwseResize,
            CursorKind::ColResize => MouseCursor::ColResize,
            CursorKind::RowResize => MouseCursor::RowResize,
            CursorKind::AllScroll => MouseCursor::AllScroll,
            CursorKind::ZoomIn => MouseCursor::ZoomIn,
            CursorKind::ZoomOut => MouseCursor::ZoomOut,
            _ => MouseCursor::Default
        };
        self.window.set_cursor(winit_cursor);
    }
}

impl WindowMethods for Window {
    fn gl(&self) -> Rc<gl::Gl> {
        self.gl.clone()
    }

    fn get_coordinates(&self) -> EmbedderCoordinates {
            // TODO(ajeffrey): can this fail?
            let dpr = self.device_hidpi_factor();
            let LogicalSize { width, height } = self.window.get_outer_size().expect("Failed to get window outer size.");
            let LogicalPosition { x, y } = self.window.get_position().unwrap_or(LogicalPosition::new(0., 0.));
            let win_size = (TypedSize2D::new(width as f32, height as f32) * dpr).to_u32();
            let win_origin = (TypedPoint2D::new(x as f32, y as f32) * dpr).to_i32();
            let screen = (self.screen_size.to_f32() * dpr).to_u32();

            let LogicalSize { width, height } = self.window.get_inner_size().expect("Failed to get window inner size.");
            let inner_size = (TypedSize2D::new(width as f32, height as f32) * dpr).to_u32();

            let viewport = DeviceUintRect::new(TypedPoint2D::zero(), inner_size);

            EmbedderCoordinates {
                viewport,
                framebuffer: inner_size,
                window: (win_size, win_origin),
                screen,
                // FIXME: Glutin doesn't have API for available size. Fallback to screen size
                screen_avail: screen,
                hidpi_factor: self.device_hidpi_factor(),
            }
    }

    fn present(&self) {
        if let Err(_err) = self.window.swap_buffers() {
          //  warn!("Failed to swap window buffers ({}).", err);
        }
    }

    fn create_event_loop_waker(&self) -> Box<EventLoopWaker> {
        struct GlutinEventLoopWaker {
            proxy: Option<Arc<winit::EventsLoopProxy>>,
        }
        impl GlutinEventLoopWaker {
            fn new(window: &Window) -> GlutinEventLoopWaker {
                let proxy = Some(Arc::new(window.events_loop.borrow().create_proxy()));
                GlutinEventLoopWaker { proxy }
            }
        }
        impl EventLoopWaker for GlutinEventLoopWaker {
            fn wake(&self) {
                // kick the OS event loop awake.
                if let Some(ref proxy) = self.proxy {
                    let _ = proxy.wakeup();
                }
            }
            fn clone(&self) -> Box<EventLoopWaker + Send> {
                Box::new(GlutinEventLoopWaker {
                    proxy: self.proxy.clone(),
                })
            }
        }

        Box::new(GlutinEventLoopWaker::new(&self))
    }

    fn set_animation_state(&self, state: AnimationState) {
        self.animation_state.set(state);
    }

    fn prepare_for_composite(&self, _width: Length<u32, DevicePixel>, _height: Length<u32, DevicePixel>) -> bool {
        true
    }

    fn supports_clipboard(&self) -> bool {
        true
    }
}

fn winit_phase_to_touch_event_type(phase: TouchPhase) -> TouchEventType {
    match phase {
        TouchPhase::Started => TouchEventType::Down,
        TouchPhase::Moved => TouchEventType::Move,
        TouchPhase::Ended => TouchEventType::Up,
        TouchPhase::Cancelled => TouchEventType::Cancel,
    }
}
