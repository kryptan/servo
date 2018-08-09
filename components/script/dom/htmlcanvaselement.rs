/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

use base64;
use canvas_traits::canvas::{CanvasMsg, CanvasId, FromScriptMsg};
use dom::attr::Attr;
use dom::bindings::cell::DomRefCell;
use dom::bindings::codegen::Bindings::CanvasRenderingContext2DBinding::CanvasRenderingContext2DMethods;
use dom::bindings::codegen::Bindings::HTMLCanvasElementBinding;
use dom::bindings::codegen::Bindings::HTMLCanvasElementBinding::{HTMLCanvasElementMethods, RenderingContext};
use dom::bindings::error::{Error, Fallible};
use dom::bindings::inheritance::Castable;
use dom::bindings::num::Finite;
use dom::bindings::reflector::DomObject;
use dom::bindings::root::{Dom, DomRoot, LayoutDom};
use dom::bindings::str::DOMString;
use dom::canvasrenderingcontext2d::{CanvasRenderingContext2D, LayoutCanvasRenderingContext2DHelpers};
use dom::document::Document;
use dom::element::{AttributeMutation, Element, RawLayoutElementHelpers};
use dom::globalscope::GlobalScope;
use dom::htmlelement::HTMLElement;
use dom::node::{Node, window_from_node};
use dom::virtualmethods::VirtualMethods;
use dom_struct::dom_struct;
use euclid::Size2D;
use html5ever::{LocalName, Prefix};
use image::ColorType;
use image::png::PNGEncoder;
use js::jsapi::JSContext;
use js::rust::HandleValue;
use profile_traits::ipc;
use script_layout_interface::{HTMLCanvasData, HTMLCanvasDataSource};
use std::iter::repeat;
use style::attr::{AttrValue, LengthOrPercentageOrAuto};

const DEFAULT_WIDTH: u32 = 300;
const DEFAULT_HEIGHT: u32 = 150;

#[must_root]
#[derive(Clone, JSTraceable, MallocSizeOf)]
pub enum CanvasContext {
    Context2d(Dom<CanvasRenderingContext2D>),
}

#[dom_struct]
pub struct HTMLCanvasElement {
    htmlelement: HTMLElement,
    context: DomRefCell<Option<CanvasContext>>,
}

impl HTMLCanvasElement {
    fn new_inherited(local_name: LocalName,
                     prefix: Option<Prefix>,
                     document: &Document) -> HTMLCanvasElement {
        HTMLCanvasElement {
            htmlelement: HTMLElement::new_inherited(local_name, prefix, document),
            context: DomRefCell::new(None),
        }
    }

    #[allow(unrooted_must_root)]
    pub fn new(local_name: LocalName,
               prefix: Option<Prefix>,
               document: &Document) -> DomRoot<HTMLCanvasElement> {
        Node::reflect_node(Box::new(HTMLCanvasElement::new_inherited(local_name, prefix, document)),
                           document,
                           HTMLCanvasElementBinding::Wrap)
    }

    fn recreate_contexts(&self) {
        let size = self.get_size();
        if let Some(ref context) = *self.context.borrow() {
            match *context {
                CanvasContext::Context2d(ref context) => context.set_bitmap_dimensions(size),
            }
        }
    }

    pub fn get_size(&self) -> Size2D<i32> {
        Size2D::new(self.Width() as i32, self.Height() as i32)
    }

    pub fn origin_is_clean(&self) -> bool {
        match *self.context.borrow() {
            Some(CanvasContext::Context2d(ref context)) => context.origin_is_clean(),
            _ => true,
        }
    }
}

pub trait LayoutHTMLCanvasElementHelpers {
    fn data(&self) -> HTMLCanvasData;
    fn get_width(&self) -> LengthOrPercentageOrAuto;
    fn get_height(&self) -> LengthOrPercentageOrAuto;
    fn get_canvas_id_for_layout(&self) -> CanvasId;
}

impl LayoutHTMLCanvasElementHelpers for LayoutDom<HTMLCanvasElement> {
    #[allow(unsafe_code)]
    fn data(&self) -> HTMLCanvasData {
        unsafe {
            let canvas = &*self.unsafe_get();
            let source = match canvas.context.borrow_for_layout().as_ref() {
                Some(&CanvasContext::Context2d(ref context)) => {
                    HTMLCanvasDataSource::Image(Some(context.to_layout().get_ipc_renderer()))
                },
                None => {
                    HTMLCanvasDataSource::Image(None)
                }
            };

            let width_attr = canvas.upcast::<Element>().get_attr_for_layout(&ns!(), &local_name!("width"));
            let height_attr = canvas.upcast::<Element>().get_attr_for_layout(&ns!(), &local_name!("height"));
            HTMLCanvasData {
                source: source,
                width: width_attr.map_or(DEFAULT_WIDTH, |val| val.as_uint()),
                height: height_attr.map_or(DEFAULT_HEIGHT, |val| val.as_uint()),
                canvas_id: self.get_canvas_id_for_layout(),
            }
        }
    }

    #[allow(unsafe_code)]
    fn get_width(&self) -> LengthOrPercentageOrAuto {
        unsafe {
            (&*self.upcast::<Element>().unsafe_get())
                .get_attr_for_layout(&ns!(), &local_name!("width"))
                .map(AttrValue::as_uint_px_dimension)
                .unwrap_or(LengthOrPercentageOrAuto::Auto)
        }
    }

    #[allow(unsafe_code)]
    fn get_height(&self) -> LengthOrPercentageOrAuto {
        unsafe {
            (&*self.upcast::<Element>().unsafe_get())
                .get_attr_for_layout(&ns!(), &local_name!("height"))
                .map(AttrValue::as_uint_px_dimension)
                .unwrap_or(LengthOrPercentageOrAuto::Auto)
        }
    }

    #[allow(unsafe_code)]
    fn get_canvas_id_for_layout(&self) -> CanvasId {
        unsafe {
            let canvas = &*self.unsafe_get();
            if let &Some(CanvasContext::Context2d(ref context)) = canvas.context.borrow_for_layout() {
                context.to_layout().get_canvas_id()
            } else {
                CanvasId(0)
            }
        }
    }
}


impl HTMLCanvasElement {
    pub fn get_or_init_2d_context(&self) -> Option<DomRoot<CanvasRenderingContext2D>> {
        if self.context.borrow().is_none() {
            let window = window_from_node(self);
            let size = self.get_size();
            let context = CanvasRenderingContext2D::new(window.upcast::<GlobalScope>(), self, size);
            *self.context.borrow_mut() = Some(CanvasContext::Context2d(Dom::from_ref(&*context)));
        }

        match *self.context.borrow().as_ref().unwrap() {
            CanvasContext::Context2d(ref context) => Some(DomRoot::from_ref(&*context)),
            //_   => None,
        }
    }

    pub fn is_valid(&self) -> bool {
        self.Height() != 0 && self.Width() != 0
    }

    pub fn fetch_all_data(&self) -> Option<(Vec<u8>, Size2D<i32>)> {
        let size = self.get_size();

        if size.width == 0 || size.height == 0 {
            return None
        }

        let data = match self.context.borrow().as_ref() {
            Some(&CanvasContext::Context2d(ref context)) => {
                let (sender, receiver) = ipc::channel(self.global().time_profiler_chan().clone()).unwrap();
                let msg = CanvasMsg::FromScript(FromScriptMsg::SendPixels(sender), context.get_canvas_id());
                context.get_ipc_renderer().send(msg).unwrap();

                receiver.recv().unwrap()?.into()
            },
            None => {
                repeat(0xffu8).take((size.height as usize) * (size.width as usize) * 4).collect()
            }
        };

        Some((data, size))
    }
}

impl HTMLCanvasElementMethods for HTMLCanvasElement {
    // https://html.spec.whatwg.org/multipage/#dom-canvas-width
    make_uint_getter!(Width, "width", DEFAULT_WIDTH);

    // https://html.spec.whatwg.org/multipage/#dom-canvas-width
    make_uint_setter!(SetWidth, "width", DEFAULT_WIDTH);

    // https://html.spec.whatwg.org/multipage/#dom-canvas-height
    make_uint_getter!(Height, "height", DEFAULT_HEIGHT);

    // https://html.spec.whatwg.org/multipage/#dom-canvas-height
    make_uint_setter!(SetHeight, "height", DEFAULT_HEIGHT);

    #[allow(unsafe_code)]
    // https://html.spec.whatwg.org/multipage/#dom-canvas-getcontext
    unsafe fn GetContext(&self,
                  _cx: *mut JSContext,
                  id: DOMString,
                  _attributes: Vec<HandleValue>)
        -> Option<RenderingContext> {
        match &*id {
            "2d" => {
                self.get_or_init_2d_context()
                  /*  .map(RenderingContext::CanvasRenderingContext2D) */
            }
            _ => None
        }
    }

    #[allow(unsafe_code)]
    // https://html.spec.whatwg.org/multipage/#dom-canvas-todataurl
    unsafe fn ToDataURL(&self,
                 _context: *mut JSContext,
                 _mime_type: Option<DOMString>,
                 _arguments: Vec<HandleValue>) -> Fallible<DOMString> {
        // Step 1.
        if let Some(CanvasContext::Context2d(ref context)) = *self.context.borrow() {
            if !context.origin_is_clean() {
                return Err(Error::Security);
            }
        }

        // Step 2.
        if self.Width() == 0 || self.Height() == 0 {
            return Ok(DOMString::from("data:,"));
        }

        // Step 3.
        let raw_data = match *self.context.borrow() {
            Some(CanvasContext::Context2d(ref context)) => {
                let image_data = context.GetImageData(Finite::wrap(0f64), Finite::wrap(0f64),
                                                           Finite::wrap(self.Width() as f64),
                                                           Finite::wrap(self.Height() as f64))?;
                image_data.get_data_array()
            }
            None => {
                // Each pixel is fully-transparent black.
                vec![0; (self.Width() * self.Height() * 4) as usize]
            }
        };

        // Only handle image/png for now.
        let mime_type = "image/png";

        let mut encoded = Vec::new();
        {
            let encoder: PNGEncoder<&mut Vec<u8>> = PNGEncoder::new(&mut encoded);
            encoder.encode(&raw_data, self.Width(), self.Height(), ColorType::RGBA(8)).unwrap();
        }

        let encoded = base64::encode(&encoded);
        Ok(DOMString::from(format!("data:{};base64,{}", mime_type, encoded)))
    }
}

impl VirtualMethods for HTMLCanvasElement {
    fn super_type(&self) -> Option<&VirtualMethods> {
        Some(self.upcast::<HTMLElement>() as &VirtualMethods)
    }

    fn attribute_mutated(&self, attr: &Attr, mutation: AttributeMutation) {
        self.super_type().unwrap().attribute_mutated(attr, mutation);
        match attr.local_name() {
            &local_name!("width") | &local_name!("height") => self.recreate_contexts(),
            _ => (),
        };
    }

    fn parse_plain_attribute(&self, name: &LocalName, value: DOMString) -> AttrValue {
        match name {
            &local_name!("width") => AttrValue::from_u32(value.into(), DEFAULT_WIDTH),
            &local_name!("height") => AttrValue::from_u32(value.into(), DEFAULT_HEIGHT),
            _ => self.super_type().unwrap().parse_plain_attribute(name, value),
        }
    }
}
