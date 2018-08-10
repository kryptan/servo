/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

//! Machinery to initialise namespace objects.

use js::jsapi::JSClass;

/// The class of a namespace object.
#[derive(Clone, Copy)]
pub struct NamespaceObjectClass(JSClass);

unsafe impl Sync for NamespaceObjectClass {}
