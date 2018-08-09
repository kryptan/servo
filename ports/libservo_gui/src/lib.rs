/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

extern crate euclid;
//#[cfg(target_os = "windows")] extern crate gdi32;
extern crate gleam;
extern crate glutin;
#[macro_use] extern crate lazy_static;
#[cfg(any(target_os = "linux", target_os = "macos"))] extern crate osmesa_sys;
extern crate servo;
#[cfg(any(target_os = "macos", target_os = "linux", target_os = "windows"))]
extern crate tinyfiledialogs;
extern crate winit;
//#[cfg(target_os = "windows")] extern crate winapi;
//#[cfg(target_os = "windows")] extern crate user32;

// The window backed by glutin
mod glutin_app;
mod resources;
mod browser;

use servo::{Servo, BrowserId};
use servo::compositing::windowing::WindowEvent;
use servo::servo_url::ServoUrl;
pub use servo::{GuiApplication, GuiApplicationResponse};
pub use winit::WindowBuilder;

pub mod platform {
    #[cfg(target_os = "macos")]
    pub use platform::macos::deinit;

    #[cfg(target_os = "macos")]
    pub mod macos;

    #[cfg(not(target_os = "macos"))]
    pub fn deinit() {}
}

pub fn run(title: &str, app: Box<GuiApplication>) {
    let window_builder = winit::WindowBuilder::new().with_title(title.to_string());
    run_with_window_builder(window_builder, true, true, app);
}

pub fn run_with_window_builder(window_builder: winit::WindowBuilder, vsync: bool, msaa: bool, app: Box<GuiApplication>) {
    resources::init();

    let window = glutin_app::Window::new(window_builder, vsync, msaa);
    let mut browser = browser::Browser::new(window.clone());
    let target_url = ServoUrl::parse("app:index.xhtml").unwrap();
    let mut servo = Servo::new(window.clone(), Some(app));
    let browser_id = BrowserId::new();
    servo.handle_events(vec![WindowEvent::NewBrowser(target_url, browser_id)]);
    servo.setup_logging();

    window.run(|| {
        let win_events = window.get_events();

        // FIXME: this could be handled by Servo. We don't need
        // a repaint_synchronously function exposed.
        let need_resize = win_events.iter().any(|e| match *e {
            WindowEvent::Resize => true,
            _ => false,
        });

        browser.handle_window_events(win_events);

        let mut servo_events = servo.get_events();
        loop {
            browser.handle_servo_events(servo_events);
            servo.handle_events(browser.get_events());
            if browser.shutdown_requested() {
                return true;
            }
            servo_events = servo.get_events();
            if servo_events.is_empty() {
                break;
            }
        }

        if need_resize {
            servo.repaint_synchronously();
        }
        false
    });

    servo.deinit();

    platform::deinit()
}
