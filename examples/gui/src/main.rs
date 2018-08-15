extern crate servo_gui;

use servo_gui::{GuiApplication, GuiApplicationResponse};
use servo_gui::dom::{Event, EventTarget};

fn main() {
    servo_gui::run("Servo Example Application", Box::new(App));
}

struct App;

impl GuiApplication for App {
    fn get_resource(&mut self, uri: &str) -> Option<GuiApplicationResponse> {
        match uri {
            "index.xhtml" => Some(GuiApplicationResponse {
                content_type: "application/xhtml+xml".parse().unwrap(),
                data: include_bytes!("index.xhtml").to_vec(),
            }),
            "servo.png" => Some(GuiApplicationResponse {
                content_type: "image/png".parse().unwrap(),
                data: include_bytes!("../../../resources/servo64.png").to_vec(),
            }),
            _ => None,
        }
    }

    fn handle_event(&mut self, name: &str, _event_target: &EventTarget, _event: &Event) {
        match name {
            "image_click" => {
                println!("You have clicked on the image, congratulations!");

            }
            _ => {}
        }
    }
}
