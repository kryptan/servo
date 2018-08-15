/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

//! Everything required for using Servo as a GUI library.

use std::cell::RefCell;
use mime::Mime;
use dom::bindings::trace::JSTraceable;
use dom::eventtarget::EventTarget;
use dom::event::Event;
use js::jsapi::JSTracer;

pub mod dom {
    pub use dom::bindings::codegen::Bindings::HTMLAnchorElementBinding::HTMLAnchorElementMethods;
    pub use dom::bindings::codegen::Bindings::HTMLElementBinding::HTMLElementMethods;
    pub use dom::bindings::codegen::Bindings::EventTargetBinding::EventTargetMethods;
    pub use dom::htmlanchorelement::HTMLAnchorElement;
    pub use dom::htmlelement::HTMLElement;
    pub use dom::eventtarget::EventTarget;
    pub use dom::event::Event;
}

/// Trait implemented by the application that uses Servo as a GUI library.
pub trait GuiApplication: Send {
    /// Get resource from the application.
    fn get_resource(&mut self, uri: &str) -> Option<GuiApplicationResponse>;
    /// Handle event. Name is the handler method specified in XHTML.
    fn handle_event(&mut self, name: &str, event_target: &EventTarget, event: &Event);
}

/// Resource provided by the application.
pub struct GuiApplicationResponse {
    /// Content-type of the resource.
    pub content_type: Mime,
    /// Entire resource data.
    pub data: Vec<u8>,
}

#[allow(unsafe_code)]
unsafe impl JSTraceable for RefCell<Box<GuiApplication>> {
    #[inline]
    unsafe fn trace(&self, _: *mut JSTracer) {
        // Do nothing
    }
}