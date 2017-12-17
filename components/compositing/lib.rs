/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

#![deny(unsafe_code)]
#![feature(box_syntax)]
#![feature(nonzero)]

extern crate crossbeam_channel;
extern crate embedder_traits;
extern crate euclid;
extern crate gfx_traits;
#[cfg(feature = "gleam")]
extern crate gleam;
#[cfg(feature = "gleam")]
extern crate image;
extern crate ipc_channel;
extern crate libc;
#[macro_use]
extern crate log;
extern crate msg;
extern crate net_traits;
extern crate profile_traits;
extern crate script_traits;
extern crate servo_config;
extern crate servo_geometry;
extern crate servo_url;
extern crate style_traits;
extern crate time;
extern crate webrender;
extern crate webrender_api;

pub use compositor_thread::CompositorProxy;
pub use compositor::IOCompositor;
pub use compositor::RenderNotifier;
pub use compositor::ShutdownState;
use euclid::TypedSize2D;
use ipc_channel::ipc::IpcSender;
use msg::constellation_msg::PipelineId;
use msg::constellation_msg::TopLevelBrowsingContextId;
use script_traits::{ConstellationControlMsg, LayoutControlMsg};
use style_traits::CSSPixel;

mod compositor;
pub mod compositor_thread;
#[cfg(feature = "gleam")]
mod gl;
mod touch;
pub mod windowing;

pub struct SendableFrameTree {
    pub pipeline: CompositionPipeline,
    pub size: Option<TypedSize2D<f32, CSSPixel>>,
    pub children: Vec<SendableFrameTree>,
}

/// The subset of the pipeline that is needed for layer composition.
#[derive(Clone)]
pub struct CompositionPipeline {
    pub id: PipelineId,
    pub top_level_browsing_context_id: TopLevelBrowsingContextId,
    pub script_chan: IpcSender<ConstellationControlMsg>,
    pub layout_chan: IpcSender<LayoutControlMsg>,
}
